//! Workspace Module Agent 工具：`workspace_module_list` / `workspace_module_describe`。
//!
//! 二者由 session runtime tool composer 通过 workspace-module provider 装配，
//! 用当前 project context + repos 现取现算：每次调用拉 enabled installations + Canvas 候选，
//! 先按 AgentRun effective capability view 过滤，再按当前用户 Canvas access 重投影。

use std::sync::Arc;

use agentdash_agent_protocol::{
    BackboneEnvelope, BackboneEvent, PlatformEvent, SourceInfo, TraceInfo,
};
use agentdash_application_ports::agent_frame_materialization::{
    CanvasVisibilityReason, RuntimeSurfaceUpdateRequest,
};
use agentdash_application_ports::agent_run_surface::AgentRunEffectiveCapabilityView;
use agentdash_application_runtime_gateway::{
    ExtensionRuntimeChannelConsumer, ExtensionRuntimeChannelInvokeRequest,
    ExtensionRuntimeChannelInvoker, RuntimeActionKey, RuntimeActor, RuntimeContext, RuntimeGateway,
    RuntimeInvocationError, RuntimeInvocationErrorKind, RuntimeInvocationRequest,
    RuntimeInvocationResult, RuntimeTarget, RuntimeTrace, attach_extension_invocation_workspace,
};
use agentdash_application_vfs::tools::SharedRuntimeVfs;
use agentdash_contracts::workspace_module::{
    WorkspaceModuleCanvasHostAction, WorkspaceModuleDescriptor, WorkspaceModuleKind,
    WorkspaceModuleOperation, WorkspaceModuleOperationDispatch,
};
use agentdash_domain::canvas::{
    CANVAS_SYSTEM_SKILL_NAME, Canvas, CanvasAccessProjection, CanvasDataBinding, CanvasRepository,
    CanvasRuntimeStateRepository, CanvasScope, canvas_access_projection,
};
use agentdash_domain::project::ProjectRepository;
use agentdash_domain::project::{ProjectAuthorization, ProjectAuthorizationContext};
use agentdash_domain::shared_library::ProjectExtensionInstallationRepository;
use agentdash_domain::workflow::{
    RuntimeSessionExecutionAnchor, RuntimeSessionExecutionAnchorRepository,
};
use agentdash_spi::context::tool_schema_sanitizer::schema_value;
use agentdash_spi::{AgentTool, AgentToolError, AgentToolResult, ContentPart, ToolUpdateCallback};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::canvas::{
    CANVAS_BIND_DATA_OPERATION_KEY, CANVAS_RENDERER_KIND, CanvasMutationInput, CanvasRepositorySet,
    CopyCanvasInput, CreatePersonalCanvasInput, canvas_module_id, canvas_presentation_uri,
    canvas_vfs_mount_id, copy_canvas_to_personal, create_personal_canvas,
    load_canvas_by_project_mount_id, normalize_canvas_mount_id, upsert_canvas_data_binding,
    validate_canvas_data_bindings,
};
use crate::workspace_module::runtime_bridge::SharedWorkspaceModuleAgentRunBridgeHandle;
use crate::workspace_module::{
    ResolvedInvocationBackend, build_canvas_workspace_module, build_workspace_module_presentation,
    request_existing_canvas_visibility_for_runtime, resolve_workspace_module_visibility,
    submit_canvas_runtime_surface_update, validate_input_against_schema,
};

#[derive(Clone, Default)]
struct WorkspaceModuleVisibilitySource {
    agent_run_bridge_handle: Option<SharedWorkspaceModuleAgentRunBridgeHandle>,
    delivery_runtime_session_id: Option<String>,
    current_user: Option<ProjectAuthorizationContext>,
    #[cfg(test)]
    effective_view: Option<AgentRunEffectiveCapabilityView>,
}

impl WorkspaceModuleVisibilitySource {
    fn with_agent_run_delivery(
        mut self,
        agent_run_bridge_handle: SharedWorkspaceModuleAgentRunBridgeHandle,
        delivery_runtime_session_id: impl Into<String>,
    ) -> Self {
        self.agent_run_bridge_handle = Some(agent_run_bridge_handle);
        self.delivery_runtime_session_id = Some(delivery_runtime_session_id.into());
        self
    }

    fn with_current_user(mut self, current_user: Option<ProjectAuthorizationContext>) -> Self {
        self.current_user = current_user;
        self
    }

    fn current_user(&self) -> Option<&ProjectAuthorizationContext> {
        self.current_user.as_ref()
    }

    #[cfg(test)]
    fn with_effective_view(mut self, view: AgentRunEffectiveCapabilityView) -> Self {
        self.effective_view = Some(view);
        self
    }

    async fn effective_view(&self) -> Result<AgentRunEffectiveCapabilityView, AgentToolError> {
        #[cfg(test)]
        if let Some(view) = self.effective_view.clone() {
            return Ok(view);
        }

        let (Some(handle), Some(delivery_runtime_session_id)) = (
            self.agent_run_bridge_handle.as_ref(),
            self.delivery_runtime_session_id.as_deref(),
        ) else {
            return Err(AgentToolError::ExecutionFailed(
                "AgentRun effective capability view unavailable for workspace module visibility"
                    .to_string(),
            ));
        };
        let Some(agent_run_bridge) = handle.get().await else {
            return Err(AgentToolError::ExecutionFailed(
                "Workspace module AgentRun bridge 尚未完成初始化".to_string(),
            ));
        };
        agent_run_bridge
            .effective_capability_view_for_agent_run_delivery(delivery_runtime_session_id)
            .await
            .map_err(AgentToolError::ExecutionFailed)
    }
}

async fn resolve_visible_modules_for_tool(
    installation_repo: &Arc<dyn ProjectExtensionInstallationRepository>,
    canvas_repo: &Arc<dyn CanvasRepository>,
    project_id: Uuid,
    visibility_source: &WorkspaceModuleVisibilitySource,
) -> Result<Vec<WorkspaceModuleDescriptor>, AgentToolError> {
    let view = visibility_source.effective_view().await?;
    let projection =
        resolve_workspace_module_visibility(installation_repo, canvas_repo, project_id, &view)
            .await
            .map_err(AgentToolError::ExecutionFailed)?;
    for diagnostic in &projection.diagnostics {
        tracing::warn!(
            code = %diagnostic.code,
            module_ref = diagnostic.module_ref.as_deref().unwrap_or(""),
            "workspace module visibility diagnostic: {}",
            diagnostic.message
        );
    }
    reproject_canvas_modules_for_access(
        canvas_repo,
        project_id,
        projection.modules,
        visibility_source.current_user(),
    )
    .await
}

async fn reproject_canvas_modules_for_access(
    canvas_repo: &Arc<dyn CanvasRepository>,
    project_id: Uuid,
    modules: Vec<WorkspaceModuleDescriptor>,
    current_user: Option<&ProjectAuthorizationContext>,
) -> Result<Vec<WorkspaceModuleDescriptor>, AgentToolError> {
    let Some(current_user) = current_user else {
        return Ok(modules
            .into_iter()
            .filter(|module| module.summary.kind != WorkspaceModuleKind::Canvas)
            .collect());
    };

    let mut visible = Vec::with_capacity(modules.len());
    for module in modules {
        if module.summary.kind != WorkspaceModuleKind::Canvas {
            visible.push(module);
            continue;
        }

        let canvas = load_canvas_by_project_mount_id_for_tool(
            canvas_repo.as_ref(),
            project_id,
            &module.summary.source,
        )
        .await?;
        let access = canvas_access_for_workspace_module(&canvas, current_user);
        if access.can_view {
            visible.push(build_canvas_workspace_module(&canvas, &access));
        }
    }
    Ok(visible)
}

async fn load_canvas_by_project_mount_id_for_tool(
    canvas_repo: &dyn CanvasRepository,
    project_id: Uuid,
    raw_canvas_mount_id: &str,
) -> Result<Canvas, AgentToolError> {
    let canvas_mount_id = normalize_canvas_mount_id(raw_canvas_mount_id)
        .map_err(|error| AgentToolError::InvalidArguments(error.to_string()))?;
    let canvas = canvas_repo
        .get_by_mount_id(project_id, &canvas_mount_id)
        .await
        .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;
    canvas
        .ok_or_else(|| AgentToolError::ExecutionFailed(format!("Canvas 不存在: {canvas_mount_id}")))
}

fn canvas_access_for_workspace_module(
    canvas: &Canvas,
    current_user: &ProjectAuthorizationContext,
) -> CanvasAccessProjection {
    canvas_access_projection(
        canvas,
        current_user,
        &workspace_module_project_access(canvas, current_user),
    )
}

fn workspace_module_project_access(
    canvas: &Canvas,
    current_user: &ProjectAuthorizationContext,
) -> ProjectAuthorization {
    ProjectAuthorization {
        role: None,
        via_admin_bypass: current_user.is_admin,
        via_template_visibility: canvas.scope == CanvasScope::Project,
    }
}

#[derive(Clone)]
pub struct WorkspaceModuleListTool {
    installation_repo: Arc<dyn ProjectExtensionInstallationRepository>,
    canvas_repo: Arc<dyn CanvasRepository>,
    project_id: Uuid,
    visibility_source: WorkspaceModuleVisibilitySource,
}

impl WorkspaceModuleListTool {
    pub fn new(
        installation_repo: Arc<dyn ProjectExtensionInstallationRepository>,
        canvas_repo: Arc<dyn CanvasRepository>,
        project_id: Uuid,
    ) -> Self {
        Self {
            installation_repo,
            canvas_repo,
            project_id,
            visibility_source: WorkspaceModuleVisibilitySource::default(),
        }
    }

    pub fn with_agent_run_visibility(
        mut self,
        agent_run_bridge_handle: SharedWorkspaceModuleAgentRunBridgeHandle,
        delivery_runtime_session_id: String,
    ) -> Self {
        self.visibility_source = self
            .visibility_source
            .with_agent_run_delivery(agent_run_bridge_handle, delivery_runtime_session_id);
        self
    }

    pub fn with_current_user(mut self, current_user: Option<ProjectAuthorizationContext>) -> Self {
        self.visibility_source = self.visibility_source.with_current_user(current_user);
        self
    }

    #[cfg(test)]
    fn with_effective_capability_view(mut self, view: AgentRunEffectiveCapabilityView) -> Self {
        self.visibility_source = self.visibility_source.with_effective_view(view);
        self
    }
}

#[async_trait]
impl AgentTool for WorkspaceModuleListTool {
    fn name(&self) -> &str {
        "workspace_module_list"
    }

    fn description(&self) -> &str {
        "List workspace modules visible to the current project (enabled extensions + visible canvases). Returns summaries only (module_id, kind, title, operation keys, status) — call workspace_module_describe to get a module's UI entries and operation schemas."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {},
            "required": [],
            "additionalProperties": false
        })
    }

    async fn execute(
        &self,
        _: &str,
        _: serde_json::Value,
        _: CancellationToken,
        _: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let modules = resolve_visible_modules_for_tool(
            &self.installation_repo,
            &self.canvas_repo,
            self.project_id,
            &self.visibility_source,
        )
        .await?;

        let summaries = modules
            .iter()
            .map(|module| module.summary.clone())
            .collect::<Vec<_>>();

        let body = if summaries.is_empty() {
            "No workspace modules visible in the current project.".to_string()
        } else {
            format!(
                "module_count: {}\n{}",
                summaries.len(),
                summaries
                    .iter()
                    .map(|summary| format!(
                        "- module_id={}  kind={:?}  title={}  operations={}",
                        summary.module_id,
                        summary.kind,
                        summary.title,
                        summary.operation_summary.len()
                    ))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        };

        let details = serde_json::json!({
            "module_count": summaries.len(),
            "modules": summaries,
        });

        Ok(AgentToolResult {
            content: vec![ContentPart::text(body)],
            is_error: false,
            details: Some(details),
        })
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WorkspaceModuleDescribeParams {
    /// Stable module id from workspace_module_list, e.g. `ext:<extension_key>` or `canvas:<mount_id>`.
    pub module_id: String,
}

#[derive(Clone)]
pub struct WorkspaceModuleDescribeTool {
    installation_repo: Arc<dyn ProjectExtensionInstallationRepository>,
    canvas_repo: Arc<dyn CanvasRepository>,
    project_id: Uuid,
    visibility_source: WorkspaceModuleVisibilitySource,
}

impl WorkspaceModuleDescribeTool {
    pub fn new(
        installation_repo: Arc<dyn ProjectExtensionInstallationRepository>,
        canvas_repo: Arc<dyn CanvasRepository>,
        project_id: Uuid,
    ) -> Self {
        Self {
            installation_repo,
            canvas_repo,
            project_id,
            visibility_source: WorkspaceModuleVisibilitySource::default(),
        }
    }

    pub fn with_agent_run_visibility(
        mut self,
        agent_run_bridge_handle: SharedWorkspaceModuleAgentRunBridgeHandle,
        delivery_runtime_session_id: String,
    ) -> Self {
        self.visibility_source = self
            .visibility_source
            .with_agent_run_delivery(agent_run_bridge_handle, delivery_runtime_session_id);
        self
    }

    pub fn with_current_user(mut self, current_user: Option<ProjectAuthorizationContext>) -> Self {
        self.visibility_source = self.visibility_source.with_current_user(current_user);
        self
    }

    #[cfg(test)]
    fn with_effective_capability_view(mut self, view: AgentRunEffectiveCapabilityView) -> Self {
        self.visibility_source = self.visibility_source.with_effective_view(view);
        self
    }
}

#[async_trait]
impl AgentTool for WorkspaceModuleDescribeTool {
    fn name(&self) -> &str {
        "workspace_module_describe"
    }

    fn description(&self) -> &str {
        "Describe a single workspace module by module_id. Returns the module's UI entries and operations, where extension runtime actions and protocol channel methods are presented uniformly as operations (with input/output schemas)."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        schema_value::<WorkspaceModuleDescribeParams>()
    }

    async fn execute(
        &self,
        _: &str,
        args: serde_json::Value,
        _: CancellationToken,
        _: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let params: WorkspaceModuleDescribeParams =
            serde_json::from_value(args).map_err(|error| {
                AgentToolError::InvalidArguments(format!("invalid arguments: {error}"))
            })?;
        let module_id = params.module_id.trim();
        if module_id.is_empty() {
            return Err(AgentToolError::InvalidArguments(
                "module_id 不能为空".to_string(),
            ));
        }

        let modules = resolve_visible_modules_for_tool(
            &self.installation_repo,
            &self.canvas_repo,
            self.project_id,
            &self.visibility_source,
        )
        .await?;

        let Some(descriptor) = modules
            .into_iter()
            .find(|module| module.summary.module_id == module_id)
        else {
            return Ok(AgentToolResult {
                content: vec![ContentPart::text(format!(
                    "workspace module not found or not visible: {module_id}"
                ))],
                is_error: true,
                details: Some(serde_json::json!({
                    "error": "module_not_found",
                    "module_id": module_id,
                })),
            });
        };

        let body = format!(
            "module_id={}\nkind={:?}\ntitle={}\nui_entries={}\noperations={}",
            descriptor.summary.module_id,
            descriptor.summary.kind,
            descriptor.summary.title,
            descriptor.ui_entries.len(),
            descriptor.operations.len()
        );
        let details = serde_json::to_value(&descriptor).map_err(|error| {
            AgentToolError::ExecutionFailed(format!("failed to serialize descriptor: {error}"))
        })?;

        Ok(AgentToolResult {
            content: vec![ContentPart::text(body)],
            is_error: false,
            details: Some(details),
        })
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WorkspaceModuleOperateParams {
    /// Operation to apply, e.g. `canvas.create`, `canvas.attach`, or `canvas.copy`.
    pub operation: String,
    /// Operation-specific payload.
    #[serde(default)]
    #[schemars(schema_with = "json_object_payload_schema")]
    pub input: serde_json::Value,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreatePersonalCanvasModuleParams {
    /// Optional stable Canvas VFS mount identifier (`cvs-...`) for the new personal Canvas.
    pub canvas_mount_id: Option<String>,
    /// Title for the new personal Canvas.
    pub title: Option<String>,
    /// Optional description for the new personal Canvas.
    pub description: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AttachExistingCanvasModuleParams {
    /// Existing Canvas VFS mount identifier (`cvs-...`) to expose in the current runtime.
    pub canvas_mount_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CopyCanvasToPersonalModuleParams {
    /// Existing source Canvas VFS mount identifier (`cvs-...`) to copy.
    pub source_canvas_mount_id: Option<String>,
    /// Optional target Canvas VFS mount identifier. If omitted, the copy uses `{source}-copy-{xxxx}`.
    pub canvas_mount_id: Option<String>,
    /// Optional title override for the copied personal Canvas.
    pub title: Option<String>,
    /// Optional description override for the copied personal Canvas.
    pub description: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BindCanvasDataParams {
    pub canvas_mount_id: String,
    pub alias: String,
    pub source_uri: String,
    pub content_type: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
struct WorkspaceModuleCanvasToolResult {
    action: String,
    canvas_id: String,
    canvas_mount_id: String,
    vfs_mount_id: String,
    module_id: String,
    presentation_uri: String,
    title: String,
    entry_file: String,
    skill_name: String,
    skill_path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
struct WorkspaceModuleCanvasBindingResult {
    canvas_id: String,
    canvas_mount_id: String,
    vfs_mount_id: String,
    bindings: Vec<CanvasDataBinding>,
    alias: String,
    source_uri: String,
    content_type: String,
}

struct WorkspaceModuleCanvasRepos<'a> {
    project_repo: &'a dyn ProjectRepository,
    canvas_repo: &'a dyn CanvasRepository,
}

impl CanvasRepositorySet for WorkspaceModuleCanvasRepos<'_> {
    fn project_repo(&self) -> &dyn ProjectRepository {
        self.project_repo
    }

    fn canvas_repo(&self) -> &dyn CanvasRepository {
        self.canvas_repo
    }
}

#[derive(Clone)]
pub struct WorkspaceModuleOperateTool {
    project_repo: Arc<dyn ProjectRepository>,
    canvas_repo: Arc<dyn CanvasRepository>,
    project_id: Uuid,
    vfs: SharedRuntimeVfs,
    agent_run_bridge_handle: SharedWorkspaceModuleAgentRunBridgeHandle,
    delivery_runtime_session_id: Option<String>,
    current_user: Option<ProjectAuthorizationContext>,
}

impl WorkspaceModuleOperateTool {
    pub fn new(
        project_repo: Arc<dyn ProjectRepository>,
        canvas_repo: Arc<dyn CanvasRepository>,
        project_id: Uuid,
        vfs: SharedRuntimeVfs,
        agent_run_bridge_handle: SharedWorkspaceModuleAgentRunBridgeHandle,
        delivery_runtime_session_id: Option<String>,
    ) -> Self {
        Self {
            project_repo,
            canvas_repo,
            project_id,
            vfs,
            agent_run_bridge_handle,
            delivery_runtime_session_id,
            current_user: None,
        }
    }

    pub fn with_turn_id(self, _turn_id: impl Into<String>) -> Self {
        self
    }

    pub fn with_current_user(mut self, current_user: Option<ProjectAuthorizationContext>) -> Self {
        self.current_user = current_user;
        self
    }
}

#[async_trait]
impl AgentTool for WorkspaceModuleOperateTool {
    fn name(&self) -> &str {
        "workspace_module_operate"
    }

    fn description(&self) -> &str {
        "Operate on workspace modules. For Canvas, pass operation=`canvas.create`, `canvas.attach`, or `canvas.copy` with operation-specific input; returns the materialized canvas:{canvas_mount_id} descriptor and exposes its Canvas VFS mount to the current session."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        schema_value::<WorkspaceModuleOperateParams>()
    }

    async fn execute(
        &self,
        _: &str,
        args: serde_json::Value,
        _: CancellationToken,
        _: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let params: WorkspaceModuleOperateParams =
            serde_json::from_value(args).map_err(|error| {
                AgentToolError::InvalidArguments(format!("invalid arguments: {error}"))
            })?;
        let WorkspaceModuleOperateParams { operation, input } = params;
        let operation = operation.trim().to_string();
        let current_user = self.current_user.as_ref().ok_or_else(|| {
            AgentToolError::ExecutionFailed(
                "workspace_module_operate 需要当前 runtime identity".to_string(),
            )
        })?;
        let repos = WorkspaceModuleCanvasRepos {
            project_repo: self.project_repo.as_ref(),
            canvas_repo: self.canvas_repo.as_ref(),
        };
        let (canvas, canvas_result) = match operation.as_str() {
            "canvas.create" => {
                let params: CreatePersonalCanvasModuleParams = serde_json::from_value(input)
                    .map_err(|error| {
                        AgentToolError::InvalidArguments(format!(
                            "invalid canvas.create input: {error}"
                        ))
                    })?;
                operate_create_personal_canvas_for_workspace_module(
                    &repos,
                    self.project_id,
                    current_user,
                    &self.vfs,
                    &self.agent_run_bridge_handle,
                    self.delivery_runtime_session_id.as_deref(),
                    params,
                )
                .await?
            }
            "canvas.attach" => {
                let params: AttachExistingCanvasModuleParams = serde_json::from_value(input)
                    .map_err(|error| {
                        AgentToolError::InvalidArguments(format!(
                            "invalid canvas.attach input: {error}"
                        ))
                    })?;
                operate_attach_existing_canvas_for_workspace_module(
                    &repos,
                    self.project_id,
                    current_user,
                    &self.vfs,
                    &self.agent_run_bridge_handle,
                    self.delivery_runtime_session_id.as_deref(),
                    params,
                )
                .await?
            }
            "canvas.copy" => {
                let params: CopyCanvasToPersonalModuleParams = serde_json::from_value(input)
                    .map_err(|error| {
                        AgentToolError::InvalidArguments(format!(
                            "invalid canvas.copy input: {error}"
                        ))
                    })?;
                operate_copy_canvas_to_personal_for_workspace_module(
                    &repos,
                    self.project_id,
                    current_user,
                    &self.vfs,
                    &self.agent_run_bridge_handle,
                    self.delivery_runtime_session_id.as_deref(),
                    params,
                )
                .await?
            }
            _ => {
                return Ok(structured_tool_error(
                    "unsupported_workspace_module_operation",
                    format!("workspace_module_operate 暂不支持 operation `{operation}`"),
                    serde_json::json!({
                        "operation": operation,
                        "supported_operations": [
                            "canvas.create",
                            "canvas.attach",
                            "canvas.copy"
                        ],
                    }),
                ));
            }
        };
        let access = canvas_access_for_workspace_module(&canvas, current_user);
        let descriptor = build_canvas_workspace_module(&canvas, &access);
        let module_id = descriptor.summary.module_id.clone();
        let content = format!(
            "operated workspace module\noperation={operation}\nmodule_id={module_id}\ncanvas_id={}\ncanvas_mount_id={}\nvfs_mount={}://\nskill_path={}",
            canvas_result.canvas_id,
            canvas_result.canvas_mount_id,
            canvas_result.vfs_mount_id,
            canvas_result.skill_path
        );
        let details = serde_json::json!({
            "operation": operation,
            "module_id": module_id,
            "descriptor": descriptor,
            "canvas": canvas_result,
        });

        Ok(AgentToolResult {
            content: vec![ContentPart::text(content)],
            is_error: false,
            details: Some(details),
        })
    }
}

async fn operate_create_personal_canvas_for_workspace_module(
    repos: &dyn CanvasRepositorySet,
    project_id: Uuid,
    current_user: &ProjectAuthorizationContext,
    vfs: &SharedRuntimeVfs,
    agent_run_bridge_handle: &SharedWorkspaceModuleAgentRunBridgeHandle,
    delivery_runtime_session_id: Option<&str>,
    params: CreatePersonalCanvasModuleParams,
) -> Result<(Canvas, WorkspaceModuleCanvasToolResult), AgentToolError> {
    let title = params
        .title
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            AgentToolError::InvalidArguments("title is required for canvas.create".to_string())
        })?;
    let canvas = create_personal_canvas(
        repos,
        current_user,
        CreatePersonalCanvasInput {
            project_id,
            title: title.to_string(),
            description: params.description,
            mount_id: params.canvas_mount_id,
            mutation: CanvasMutationInput::default(),
        },
    )
    .await
    .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?
    .canvas;
    expose_canvas_for_workspace_module(
        vfs,
        agent_run_bridge_handle,
        delivery_runtime_session_id,
        current_user,
        canvas,
        "created",
        CanvasVisibilityReason::Created,
    )
    .await
}

async fn operate_attach_existing_canvas_for_workspace_module(
    repos: &dyn CanvasRepositorySet,
    project_id: Uuid,
    current_user: &ProjectAuthorizationContext,
    vfs: &SharedRuntimeVfs,
    agent_run_bridge_handle: &SharedWorkspaceModuleAgentRunBridgeHandle,
    delivery_runtime_session_id: Option<&str>,
    params: AttachExistingCanvasModuleParams,
) -> Result<(Canvas, WorkspaceModuleCanvasToolResult), AgentToolError> {
    let canvas_mount_id = required_canvas_mount_id(
        params.canvas_mount_id.as_deref(),
        "canvas.attach input.canvas_mount_id",
    )?;
    let canvas = load_canvas_by_project_mount_id(repos, project_id, &canvas_mount_id)
        .await
        .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;
    ensure_canvas_visible_to_current_user(&canvas, current_user)?;
    expose_canvas_for_workspace_module(
        vfs,
        agent_run_bridge_handle,
        delivery_runtime_session_id,
        current_user,
        canvas,
        "attached",
        CanvasVisibilityReason::Presented,
    )
    .await
}

async fn operate_copy_canvas_to_personal_for_workspace_module(
    repos: &dyn CanvasRepositorySet,
    project_id: Uuid,
    current_user: &ProjectAuthorizationContext,
    vfs: &SharedRuntimeVfs,
    agent_run_bridge_handle: &SharedWorkspaceModuleAgentRunBridgeHandle,
    delivery_runtime_session_id: Option<&str>,
    params: CopyCanvasToPersonalModuleParams,
) -> Result<(Canvas, WorkspaceModuleCanvasToolResult), AgentToolError> {
    let source_canvas_mount_id = required_canvas_mount_id(
        params.source_canvas_mount_id.as_deref(),
        "canvas.copy input.source_canvas_mount_id",
    )?;
    let source = load_canvas_by_project_mount_id(repos, project_id, &source_canvas_mount_id)
        .await
        .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;
    let copy = copy_canvas_to_personal(
        repos,
        current_user,
        source.id,
        CopyCanvasInput {
            mount_id: params.canvas_mount_id,
            title: params.title,
            description: params.description,
        },
    )
    .await
    .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?
    .canvas;
    expose_canvas_for_workspace_module(
        vfs,
        agent_run_bridge_handle,
        delivery_runtime_session_id,
        current_user,
        copy,
        "copied",
        CanvasVisibilityReason::Created,
    )
    .await
}

async fn expose_canvas_for_workspace_module(
    vfs: &SharedRuntimeVfs,
    agent_run_bridge_handle: &SharedWorkspaceModuleAgentRunBridgeHandle,
    delivery_runtime_session_id: Option<&str>,
    current_user: &ProjectAuthorizationContext,
    canvas: Canvas,
    action: &str,
    reason: CanvasVisibilityReason,
) -> Result<(Canvas, WorkspaceModuleCanvasToolResult), AgentToolError> {
    submit_canvas_runtime_surface_update(
        Some(vfs),
        agent_run_bridge_handle,
        delivery_runtime_session_id,
        Some(current_user),
        &canvas,
        RuntimeSurfaceUpdateRequest::CanvasVisibilityRequested {
            canvas_mount_id: canvas.mount_id.clone(),
            reason,
        },
    )
    .await?;

    let result = WorkspaceModuleCanvasToolResult {
        action: action.to_string(),
        canvas_id: canvas.id.to_string(),
        canvas_mount_id: canvas.mount_id.clone(),
        vfs_mount_id: canvas_vfs_mount_id(&canvas.mount_id),
        module_id: canvas_module_id(&canvas.mount_id),
        presentation_uri: canvas_presentation_uri(&canvas.mount_id),
        title: canvas.title.clone(),
        entry_file: canvas.entry_file.clone(),
        skill_name: CANVAS_SYSTEM_SKILL_NAME.to_string(),
        skill_path: format!("lifecycle://skills/{CANVAS_SYSTEM_SKILL_NAME}/SKILL.md"),
    };
    Ok((canvas, result))
}

fn required_canvas_mount_id(
    value: Option<&str>,
    field_name: &str,
) -> Result<String, AgentToolError> {
    let raw = value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AgentToolError::InvalidArguments(format!("{field_name} is required")))?;
    normalize_canvas_mount_id(raw)
        .map_err(|error| AgentToolError::InvalidArguments(error.to_string()))
}

fn ensure_canvas_visible_to_current_user(
    canvas: &Canvas,
    current_user: &ProjectAuthorizationContext,
) -> Result<(), AgentToolError> {
    let access = canvas_access_for_workspace_module(canvas, current_user);
    if access.can_view {
        Ok(())
    } else {
        Err(AgentToolError::ExecutionFailed(format!(
            "当前用户无权查看 Canvas {}",
            canvas.id
        )))
    }
}

fn bind_canvas_data_for_loaded_canvas(
    project_id: Uuid,
    canvas: Canvas,
    params: BindCanvasDataParams,
) -> Result<
    (
        Canvas,
        CanvasDataBinding,
        WorkspaceModuleCanvasBindingResult,
    ),
    AgentToolError,
> {
    if canvas.project_id != project_id {
        return Err(AgentToolError::ExecutionFailed(
            "当前 session 无权操作其它 Project 的 Canvas".to_string(),
        ));
    }
    let requested_mount_id = normalize_canvas_mount_id(&params.canvas_mount_id)
        .map_err(|error| AgentToolError::InvalidArguments(error.to_string()))?;
    if requested_mount_id != canvas.mount_id {
        return Err(AgentToolError::InvalidArguments(format!(
            "canvas.bind_data target `{requested_mount_id}` does not match Canvas `{}`",
            canvas.mount_id
        )));
    }

    let binding =
        CanvasDataBinding::with_content_type(params.alias, params.source_uri, params.content_type);
    let alias = binding.alias.clone();
    let source_uri = binding.source_uri.clone();
    let content_type = binding.content_type.clone();
    let mut runtime_bindings = Vec::new();
    upsert_canvas_data_binding(&mut runtime_bindings, binding.clone())
        .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;
    validate_canvas_data_bindings(&canvas, &runtime_bindings)
        .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;

    let result = WorkspaceModuleCanvasBindingResult {
        canvas_id: canvas.id.to_string(),
        canvas_mount_id: canvas.mount_id.clone(),
        vfs_mount_id: canvas_vfs_mount_id(&canvas.mount_id),
        bindings: runtime_bindings,
        alias,
        source_uri,
        content_type,
    };
    Ok((canvas, binding, result))
}

/// 在已聚合（且 capability 过滤后）的 module 列表里定位 module + operation。
/// 返回结构化错误码：`module_not_found` / `operation_not_found`。
fn locate_operation<'a>(
    modules: &'a [WorkspaceModuleDescriptor],
    module_id: &str,
    operation_key: &str,
) -> Result<(&'a WorkspaceModuleDescriptor, &'a WorkspaceModuleOperation), AgentToolResult> {
    let Some(module) = modules
        .iter()
        .find(|module| module.summary.module_id == module_id)
    else {
        return Err(structured_tool_error(
            "module_not_found",
            format!("workspace module not found or not visible: {module_id}"),
            serde_json::json!({ "module_id": module_id }),
        ));
    };
    let Some(operation) = module
        .operations
        .iter()
        .find(|operation| operation.operation_key == operation_key)
    else {
        return Err(structured_tool_error(
            "operation_not_found",
            format!("unknown operation `{operation_key}` for module `{module_id}`"),
            serde_json::json!({
                "module_id": module_id,
                "operation_key": operation_key,
                "available_operations": module
                    .operations
                    .iter()
                    .map(|op| op.operation_key.clone())
                    .collect::<Vec<_>>(),
            }),
        ));
    };
    Ok((module, operation))
}

/// 构造一个 `is_error` 的结构化工具结果（带 error code + details，便于 agent 还原）。
fn structured_tool_error(
    code: &str,
    message: String,
    mut extra: serde_json::Value,
) -> AgentToolResult {
    if let Some(obj) = extra.as_object_mut() {
        obj.insert("error".to_string(), serde_json::json!(code));
        obj.insert("message".to_string(), serde_json::json!(message.clone()));
    }
    AgentToolResult {
        content: vec![ContentPart::text(message)],
        is_error: true,
        details: Some(extra),
    }
}

fn json_tool_result(output: serde_json::Value) -> AgentToolResult {
    let rendered = serde_json::to_string_pretty(&output).unwrap_or_else(|_| output.to_string());
    AgentToolResult {
        content: vec![ContentPart::text(rendered)],
        is_error: false,
        details: Some(serde_json::json!({ "output": output })),
    }
}

fn runtime_error_to_tool_error(error: RuntimeInvocationError) -> AgentToolError {
    match error.kind() {
        RuntimeInvocationErrorKind::InvalidRequest => {
            AgentToolError::InvalidArguments(error.to_string())
        }
        RuntimeInvocationErrorKind::CapabilityDenied
        | RuntimeInvocationErrorKind::Conflict
        | RuntimeInvocationErrorKind::ProviderUnavailable
        | RuntimeInvocationErrorKind::ProviderFailed
        | RuntimeInvocationErrorKind::Timeout => AgentToolError::ExecutionFailed(error.to_string()),
    }
}

/// 把 RuntimeGateway 结果整形为 agent 工具结果（参照 RuntimeActionToolAdapter）。
/// `provenance` 携带 module source / operation provenance，落进 details 供审计（R5）。
fn invocation_result_to_tool_result(
    result: RuntimeInvocationResult,
    provenance: serde_json::Value,
) -> AgentToolResult {
    let trace = serde_json::to_value(&result.trace).unwrap_or(serde_json::Value::Null);
    let action_key = serde_json::to_value(&result.action_key).unwrap_or(serde_json::Value::Null);

    if let Ok(mut tool_result) =
        serde_json::from_value::<AgentToolResult>(result.output.output.clone())
    {
        let provider_details = tool_result.details.take();
        tool_result.details = Some(serde_json::json!({
            "provenance": provenance,
            "runtime_action": action_key,
            "runtime_trace": trace,
            "provider_details": provider_details,
        }));
        return tool_result;
    }

    let rendered = serde_json::to_string_pretty(&result.output.output)
        .unwrap_or_else(|_| result.output.output.to_string());
    AgentToolResult {
        content: vec![ContentPart::text(rendered)],
        is_error: false,
        details: Some(serde_json::json!({
            "provenance": provenance,
            "runtime_action": action_key,
            "runtime_trace": trace,
            "output": result.output.output,
        })),
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WorkspaceModuleInvokeParams {
    /// Stable module id from workspace_module_list, e.g. `ext:<extension_key>`.
    pub module_id: String,
    /// operation_key from workspace_module_describe (the module's operation list).
    pub operation_key: String,
    /// Operation input payload; must satisfy the operation's input_schema from describe.
    #[serde(default)]
    #[schemars(schema_with = "json_object_payload_schema")]
    pub input: serde_json::Value,
}

/// `workspace_module_invoke`：按 operation 的结构化 `dispatch` 分支派发的统一调用入口。
///
/// Agent 只传 `module_id + operation_key + input`；project / backend / session / workspace
/// 全部由宿主从 ExecutionContext 解析（R3）。服务端裁决：operation 归属 module、可见性、
/// input schema（R2）；runtime_action 的 extension permission 由 provider 内裁决，不重复。
#[derive(Clone)]
pub struct WorkspaceModuleInvokeTool {
    installation_repo: Arc<dyn ProjectExtensionInstallationRepository>,
    canvas_repo: Arc<dyn CanvasRepository>,
    canvas_runtime_state_repo: Arc<dyn CanvasRuntimeStateRepository>,
    execution_anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository>,
    project_id: Uuid,
    delivery_runtime_session_id: String,
    agent_id: Option<String>,
    backend: Option<ResolvedInvocationBackend>,
    gateway: Arc<RuntimeGateway>,
    channel_invoker: Arc<ExtensionRuntimeChannelInvoker>,
    visibility_source: WorkspaceModuleVisibilitySource,
    agent_run_bridge_handle: Option<SharedWorkspaceModuleAgentRunBridgeHandle>,
    current_user: Option<ProjectAuthorizationContext>,
}

impl WorkspaceModuleInvokeTool {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        installation_repo: Arc<dyn ProjectExtensionInstallationRepository>,
        canvas_repo: Arc<dyn CanvasRepository>,
        canvas_runtime_state_repo: Arc<dyn CanvasRuntimeStateRepository>,
        execution_anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository>,
        project_id: Uuid,
        delivery_runtime_session_id: String,
        agent_id: Option<String>,
        backend: Option<ResolvedInvocationBackend>,
        gateway: Arc<RuntimeGateway>,
        channel_invoker: Arc<ExtensionRuntimeChannelInvoker>,
    ) -> Self {
        Self {
            installation_repo,
            canvas_repo,
            canvas_runtime_state_repo,
            execution_anchor_repo,
            project_id,
            delivery_runtime_session_id,
            agent_id,
            backend,
            gateway,
            channel_invoker,
            visibility_source: WorkspaceModuleVisibilitySource::default(),
            agent_run_bridge_handle: None,
            current_user: None,
        }
    }

    pub fn with_agent_run_visibility(
        mut self,
        agent_run_bridge_handle: SharedWorkspaceModuleAgentRunBridgeHandle,
    ) -> Self {
        self.agent_run_bridge_handle = Some(agent_run_bridge_handle.clone());
        self.visibility_source = self.visibility_source.with_agent_run_delivery(
            agent_run_bridge_handle,
            self.delivery_runtime_session_id.clone(),
        );
        self
    }

    pub fn with_current_user(mut self, current_user: Option<ProjectAuthorizationContext>) -> Self {
        self.current_user = current_user.clone();
        self.visibility_source = self.visibility_source.with_current_user(current_user);
        self
    }

    #[cfg(test)]
    fn with_effective_capability_view(mut self, view: AgentRunEffectiveCapabilityView) -> Self {
        self.visibility_source = self.visibility_source.with_effective_view(view);
        self
    }

    fn require_backend(&self) -> Result<&ResolvedInvocationBackend, AgentToolResult> {
        self.backend.as_ref().ok_or_else(|| {
            structured_tool_error(
                "backend_unavailable",
                "当前 AgentRun delivery 无可用 backend target（既无 remote backend execution，vfs 也无 default mount backend），无法执行该 operation".to_string(),
                serde_json::json!({}),
            )
        })
    }

    async fn submit_canvas_runtime_surface_request(
        &self,
        canvas: &Canvas,
        request: RuntimeSurfaceUpdateRequest,
    ) -> Result<(), AgentToolError> {
        let Some(handle) = self.agent_run_bridge_handle.as_ref() else {
            return Ok(());
        };
        submit_canvas_runtime_surface_update(
            None,
            handle,
            Some(&self.delivery_runtime_session_id),
            self.current_user.as_ref(),
            canvas,
            request,
        )
        .await
    }

    async fn load_canvas_for_runtime_binding(
        &self,
        canvas_mount_id: &str,
    ) -> Result<Canvas, AgentToolResult> {
        let Some(current_user) = self.current_user.as_ref() else {
            return Err(structured_tool_error(
                "runtime_identity_required",
                "canvas.bind_data 需要当前 runtime identity".to_string(),
                serde_json::json!({
                    "canvas_mount_id": canvas_mount_id,
                    "required_action": "runtime_binding",
                }),
            ));
        };
        let canvas = match load_canvas_by_project_mount_id_for_tool(
            self.canvas_repo.as_ref(),
            self.project_id,
            canvas_mount_id,
        )
        .await
        {
            Ok(canvas) => canvas,
            Err(error) => {
                return Err(structured_tool_error(
                    "canvas_not_found",
                    error.to_string(),
                    serde_json::json!({
                        "canvas_mount_id": canvas_mount_id,
                    }),
                ));
            }
        };
        let access = canvas_access_for_workspace_module(&canvas, current_user);
        if access.can_view {
            Ok(canvas)
        } else {
            Err(structured_tool_error(
                "canvas_not_viewable",
                format!(
                    "当前用户无权查看 Canvas `{}`，无法挂接运行期数据",
                    canvas.mount_id
                ),
                serde_json::json!({
                    "canvas_id": canvas.id,
                    "canvas_mount_id": canvas.mount_id,
                    "scope": canvas.scope,
                    "required_action": "runtime_binding",
                }),
            ))
        }
    }

    async fn current_anchor(&self) -> Result<RuntimeSessionExecutionAnchor, AgentToolResult> {
        match self
            .execution_anchor_repo
            .find_by_session(&self.delivery_runtime_session_id)
            .await
        {
            Ok(Some(anchor)) => Ok(anchor),
            Ok(None) => Err(structured_tool_error(
                "runtime_anchor_not_found",
                "当前 AgentRun delivery runtime 无 execution anchor，无法解析 Canvas 诊断状态归属"
                    .to_string(),
                serde_json::json!({
                    "delivery_runtime_session_id": self.delivery_runtime_session_id,
                }),
            )),
            Err(error) => Err(structured_tool_error(
                "runtime_anchor_query_failed",
                format!("查询 runtime execution anchor 失败: {error}"),
                serde_json::json!({
                    "delivery_runtime_session_id": self.delivery_runtime_session_id,
                }),
            )),
        }
    }

    async fn inspect_canvas_render_state(
        &self,
        canvas_mount_id: &str,
    ) -> Result<AgentToolResult, AgentToolError> {
        let anchor = match self.current_anchor().await {
            Ok(anchor) => anchor,
            Err(result) => return Ok(result),
        };
        let observation = self
            .canvas_runtime_state_repo
            .latest_runtime_observation(anchor.run_id, anchor.agent_id, canvas_mount_id)
            .await
            .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;
        Ok(json_tool_result(serde_json::json!({
            "canvas_mount_id": canvas_mount_id,
            "run_id": anchor.run_id,
            "agent_id": anchor.agent_id,
            "observation": observation,
        })))
    }

    async fn get_canvas_interaction_state(
        &self,
        canvas_mount_id: &str,
    ) -> Result<AgentToolResult, AgentToolError> {
        let anchor = match self.current_anchor().await {
            Ok(anchor) => anchor,
            Err(result) => return Ok(result),
        };
        let snapshot = self
            .canvas_runtime_state_repo
            .latest_interaction_snapshot(anchor.run_id, anchor.agent_id, canvas_mount_id)
            .await
            .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;
        Ok(json_tool_result(serde_json::json!({
            "canvas_mount_id": canvas_mount_id,
            "run_id": anchor.run_id,
            "agent_id": anchor.agent_id,
            "snapshot": snapshot,
        })))
    }
}

#[async_trait]
impl AgentTool for WorkspaceModuleInvokeTool {
    fn name(&self) -> &str {
        "workspace_module_invoke"
    }

    fn description(&self) -> &str {
        "Invoke an operation on a workspace module. Pass module_id + operation_key (from workspace_module_describe) + input. The host resolves all internal routing (project/backend/session) and dispatches to the right runtime path based on the operation's declared dispatch kind."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        schema_value::<WorkspaceModuleInvokeParams>()
    }

    async fn execute(
        &self,
        _: &str,
        args: serde_json::Value,
        _: CancellationToken,
        _: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let params: WorkspaceModuleInvokeParams =
            serde_json::from_value(args).map_err(|error| {
                AgentToolError::InvalidArguments(format!("invalid arguments: {error}"))
            })?;
        let module_id = params.module_id.trim();
        let operation_key = params.operation_key.trim();
        if module_id.is_empty() || operation_key.is_empty() {
            return Err(AgentToolError::InvalidArguments(
                "module_id 与 operation_key 不能为空".to_string(),
            ));
        }

        let modules = resolve_visible_modules_for_tool(
            &self.installation_repo,
            &self.canvas_repo,
            self.project_id,
            &self.visibility_source,
        )
        .await?;

        // operation 归属 module + 未知 operation 拒绝（R2）。
        let (module, operation) = match locate_operation(&modules, module_id, operation_key) {
            Ok(found) => found,
            Err(result) => {
                if operation_key == CANVAS_BIND_DATA_OPERATION_KEY
                    && let Some(module) = modules.iter().find(|module| {
                        module.summary.kind == WorkspaceModuleKind::Canvas
                            && module.summary.module_id == module_id
                    })
                    && let Err(guard_result) = self
                        .load_canvas_for_runtime_binding(&module.summary.source)
                        .await
                {
                    return Ok(guard_result);
                }
                return Ok(result);
            }
        };

        // input schema 校验（R2，describe 暴露的 schema 与此成对）。
        if let Some(schema) = operation.input_schema.as_ref()
            && let Err(reason) = validate_input_against_schema(schema, &params.input)
        {
            return Ok(structured_tool_error(
                "input_schema_mismatch",
                format!("input 不满足 operation `{operation_key}` 的 input_schema：{reason}"),
                serde_json::json!({
                    "module_id": module_id,
                    "operation_key": operation_key,
                }),
            ));
        }

        let provenance = serde_json::json!({
            "module_id": module_id,
            "module_kind": module.summary.kind,
            "module_source": module.summary.source,
            "operation_key": operation_key,
            "operation_origin": operation.origin,
            "runtime_backing": module.runtime_backing,
        });

        match &operation.dispatch {
            WorkspaceModuleOperationDispatch::RuntimeAction { action_key } => {
                let backend = match self.require_backend() {
                    Ok(backend) => backend,
                    Err(result) => return Ok(result),
                };
                let action_key = RuntimeActionKey::parse(action_key.clone()).map_err(|error| {
                    AgentToolError::ExecutionFailed(format!(
                        "operation `{operation_key}` 的 action_key 非法: {error}"
                    ))
                })?;
                let mut request = RuntimeInvocationRequest::new(
                    action_key,
                    RuntimeActor::AgentSession {
                        session_id: self.delivery_runtime_session_id.clone(),
                        agent_id: self.agent_id.clone(),
                    },
                    RuntimeContext::Session {
                        session_id: self.delivery_runtime_session_id.clone(),
                        project_id: Some(self.project_id),
                        workspace_id: None,
                    },
                    params.input,
                );
                request.target = Some(RuntimeTarget::Backend {
                    backend_id: backend.backend_id.clone(),
                });
                if let Some(workspace) = backend.workspace.clone() {
                    attach_extension_invocation_workspace(&mut request, Some(workspace));
                }
                let mut provenance = provenance;
                if let Some(obj) = provenance.as_object_mut() {
                    obj.insert("backend".to_string(), serde_json::json!(backend.backend_id));
                }
                let result = self
                    .gateway
                    .invoke(request)
                    .await
                    .map_err(runtime_error_to_tool_error)?;
                Ok(invocation_result_to_tool_result(result, provenance))
            }
            WorkspaceModuleOperationDispatch::ProtocolChannel {
                channel_key,
                method_name,
            } => {
                let backend = match self.require_backend() {
                    Ok(backend) => backend,
                    Err(result) => return Ok(result),
                };
                let trace = RuntimeTrace::new();
                let result = self
                    .channel_invoker
                    .invoke(ExtensionRuntimeChannelInvokeRequest {
                        project_id: self.project_id,
                        session_id: self.delivery_runtime_session_id.clone(),
                        backend_id: backend.backend_id.clone(),
                        workspace: backend.workspace.clone(),
                        consumer: ExtensionRuntimeChannelConsumer::SessionUser,
                        channel_key: channel_key.clone(),
                        dependency_alias: None,
                        method: method_name.clone(),
                        input: params.input,
                        trace,
                    })
                    .await
                    .map_err(runtime_error_to_tool_error)?;

                let trace_value =
                    serde_json::to_value(&result.trace).unwrap_or(serde_json::Value::Null);
                let mut provenance = provenance;
                if let Some(obj) = provenance.as_object_mut() {
                    obj.insert("backend".to_string(), serde_json::json!(backend.backend_id));
                    obj.insert(
                        "channel_key".to_string(),
                        serde_json::json!(result.channel_key),
                    );
                    obj.insert("method".to_string(), serde_json::json!(result.method));
                }
                let rendered = serde_json::to_string_pretty(&result.output.output)
                    .unwrap_or_else(|_| result.output.output.to_string());
                Ok(AgentToolResult {
                    content: vec![ContentPart::text(rendered)],
                    is_error: false,
                    details: Some(serde_json::json!({
                        "provenance": provenance,
                        "runtime_trace": trace_value,
                        "output": result.output.output,
                    })),
                })
            }
            WorkspaceModuleOperationDispatch::HostCanvas { canvas_action } => match canvas_action {
                WorkspaceModuleCanvasHostAction::BindData => {
                    let editable_canvas = match self
                        .load_canvas_for_runtime_binding(&module.summary.source)
                        .await
                    {
                        Ok(canvas) => canvas,
                        Err(result) => return Ok(result),
                    };
                    let mut input = params.input;
                    let Some(obj) = input.as_object_mut() else {
                        return Ok(structured_tool_error(
                            "invalid_canvas_input",
                            "canvas.bind_data input 必须是 object".to_string(),
                            serde_json::json!({
                                "module_id": module_id,
                                "operation_key": operation_key,
                            }),
                        ));
                    };
                    obj.insert(
                        "canvas_mount_id".to_string(),
                        serde_json::Value::String(module.summary.source.clone()),
                    );
                    let bind_params: BindCanvasDataParams =
                        serde_json::from_value(input).map_err(|error| {
                            AgentToolError::InvalidArguments(format!(
                                "invalid canvas.bind_data input: {error}"
                            ))
                        })?;
                    let (canvas, binding, result) = bind_canvas_data_for_loaded_canvas(
                        self.project_id,
                        editable_canvas,
                        bind_params,
                    )?;
                    self.submit_canvas_runtime_surface_request(
                        &canvas,
                        RuntimeSurfaceUpdateRequest::CanvasBindingChanged {
                            canvas_mount_id: canvas.mount_id.clone(),
                            binding,
                        },
                    )
                    .await?;
                    let content = format!(
                        "canvas_id={}\ncanvas_mount_id={}\nvfs_mount={}://\nalias={}\nsource_uri={}\ncontent_type={}",
                        result.canvas_id,
                        result.canvas_mount_id,
                        result.vfs_mount_id,
                        result.alias,
                        result.source_uri,
                        result.content_type
                    );
                    let details = serde_json::json!({
                        "provenance": provenance,
                        "output": result,
                    });
                    Ok(AgentToolResult {
                        content: vec![ContentPart::text(content)],
                        is_error: false,
                        details: Some(details),
                    })
                }
                WorkspaceModuleCanvasHostAction::InspectRenderState => {
                    self.inspect_canvas_render_state(&module.summary.source)
                        .await
                }
                WorkspaceModuleCanvasHostAction::GetInteractionState => {
                    self.get_canvas_interaction_state(&module.summary.source)
                        .await
                }
            },
            WorkspaceModuleOperationDispatch::Builtin { builtin_key } => Ok(structured_tool_error(
                "operation_unimplemented",
                format!("builtin operation `{builtin_key}` 暂未实装"),
                serde_json::json!({
                    "module_id": module_id,
                    "operation_key": operation_key,
                    "builtin_key": builtin_key,
                }),
            )),
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WorkspaceModulePresentParams {
    /// Stable module id from workspace_module_list.
    pub module_id: String,
    /// view_key of a UI entry from workspace_module_describe (ui_entries[].view_key).
    pub view_key: String,
    /// Optional payload forwarded to the frontend view.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(schema_with = "json_object_payload_schema")]
    pub payload: Option<serde_json::Value>,
}

/// `workspace_module_present`：best-effort 请求宿主向前端 panel 展示某 module 的 UI 入口。
///
/// 复用 `PlatformEvent::SessionMetaUpdate{ key: "workspace_module_presented" }` +
/// AgentRun notification bridge，不新增 PlatformEvent 变体（D2-5）。
/// 无可展示目标（module 不可见 / view_key 不存在）时返回**可操作诊断**结构化错误（R4）。
#[derive(Clone)]
pub struct WorkspaceModulePresentTool {
    installation_repo: Arc<dyn ProjectExtensionInstallationRepository>,
    canvas_repo: Arc<dyn CanvasRepository>,
    project_id: Uuid,
    vfs: SharedRuntimeVfs,
    agent_run_bridge_handle: SharedWorkspaceModuleAgentRunBridgeHandle,
    delivery_runtime_session_id: String,
    turn_id: String,
    visibility_source: WorkspaceModuleVisibilitySource,
}

impl WorkspaceModulePresentTool {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        installation_repo: Arc<dyn ProjectExtensionInstallationRepository>,
        canvas_repo: Arc<dyn CanvasRepository>,
        project_id: Uuid,
        vfs: SharedRuntimeVfs,
        agent_run_bridge_handle: SharedWorkspaceModuleAgentRunBridgeHandle,
        delivery_runtime_session_id: String,
        turn_id: String,
    ) -> Self {
        let visibility_source = WorkspaceModuleVisibilitySource::default().with_agent_run_delivery(
            agent_run_bridge_handle.clone(),
            delivery_runtime_session_id.clone(),
        );
        Self {
            installation_repo,
            canvas_repo,
            project_id,
            vfs,
            agent_run_bridge_handle,
            delivery_runtime_session_id,
            turn_id,
            visibility_source,
        }
    }

    pub fn with_current_user(mut self, current_user: Option<ProjectAuthorizationContext>) -> Self {
        self.visibility_source = self.visibility_source.with_current_user(current_user);
        self
    }
}

#[async_trait]
impl AgentTool for WorkspaceModulePresentTool {
    fn name(&self) -> &str {
        "workspace_module_present"
    }

    fn description(&self) -> &str {
        "Request the frontend to open/activate a workspace module's UI view (extension webview or canvas panel). Pass module_id + view_key (from workspace_module_describe ui_entries) + optional payload. Returns a diagnostic when no presentable target exists."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        schema_value::<WorkspaceModulePresentParams>()
    }

    async fn execute(
        &self,
        _: &str,
        args: serde_json::Value,
        _: CancellationToken,
        _: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let params: WorkspaceModulePresentParams =
            serde_json::from_value(args).map_err(|error| {
                AgentToolError::InvalidArguments(format!("invalid arguments: {error}"))
            })?;
        let module_id = params.module_id.trim();
        let view_key = params.view_key.trim();
        if module_id.is_empty() || view_key.is_empty() {
            return Err(AgentToolError::InvalidArguments(
                "module_id 与 view_key 不能为空".to_string(),
            ));
        }

        let modules = resolve_visible_modules_for_tool(
            &self.installation_repo,
            &self.canvas_repo,
            self.project_id,
            &self.visibility_source,
        )
        .await?;

        let Some(module) = modules
            .iter()
            .find(|module| module.summary.module_id == module_id)
        else {
            return Ok(structured_tool_error(
                "module_not_found",
                format!("workspace module not found or not visible: {module_id}"),
                serde_json::json!({ "module_id": module_id }),
            ));
        };

        let presentation =
            match build_workspace_module_presentation(module, view_key, params.payload, None) {
                Ok(presentation) => presentation,
                Err(error) => {
                    let diagnostic = error.diagnostics();
                    self.inject_present_diagnostic(&diagnostic).await;
                    return Ok(structured_tool_error(
                        "view_not_found",
                        error.to_string(),
                        diagnostic,
                    ));
                }
            };

        if presentation.renderer_kind == CANVAS_RENDERER_KIND {
            request_existing_canvas_visibility_for_runtime(
                self.canvas_repo.as_ref(),
                self.project_id,
                &module.summary.source,
                Some(&self.vfs),
                &self.agent_run_bridge_handle,
                Some(&self.delivery_runtime_session_id),
                self.visibility_source.current_user.as_ref(),
            )
            .await?;
        }

        let value = serde_json::to_value(&presentation).map_err(|error| {
            AgentToolError::ExecutionFailed(format!(
                "failed to serialize workspace module presentation: {error}"
            ))
        })?;

        let notification = build_present_notification(
            &self.delivery_runtime_session_id,
            &self.turn_id,
            "workspace_module_presented",
            value.clone(),
        );
        let agent_run_bridge = self.agent_run_bridge_handle.get().await.ok_or_else(|| {
            AgentToolError::ExecutionFailed(
                "Workspace module AgentRun bridge 尚未完成初始化".to_string(),
            )
        })?;
        agent_run_bridge
            .inject_agent_run_notification(&self.delivery_runtime_session_id, notification)
            .await
            .map_err(AgentToolError::ExecutionFailed)?;

        Ok(AgentToolResult {
            content: vec![ContentPart::text(format!(
                "presented module={module_id} view={view_key} renderer={}",
                presentation.renderer_kind
            ))],
            is_error: false,
            details: Some(value),
        })
    }
}

impl WorkspaceModulePresentTool {
    /// 无可展示目标时也发一条诊断 meta（best-effort，失败仅 warn）。
    async fn inject_present_diagnostic(&self, value: &serde_json::Value) {
        let Some(agent_run_bridge) = self.agent_run_bridge_handle.get().await else {
            return;
        };
        let notification = build_present_notification(
            &self.delivery_runtime_session_id,
            &self.turn_id,
            "workspace_module_present_failed",
            value.clone(),
        );
        if let Err(error) = agent_run_bridge
            .inject_agent_run_notification(&self.delivery_runtime_session_id, notification)
            .await
        {
            tracing::warn!(%error, "workspace_module_present 诊断事件注入失败");
        }
    }
}

fn build_present_notification(
    session_id: &str,
    turn_id: &str,
    key: &str,
    value: serde_json::Value,
) -> BackboneEnvelope {
    let source = SourceInfo {
        connector_id: "agentdash-workspace-module".to_string(),
        connector_type: "runtime_tool".to_string(),
        executor_id: None,
    };
    BackboneEnvelope::new(
        BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate {
            key: key.to_string(),
            value,
        }),
        session_id,
        source,
    )
    .with_trace(TraceInfo {
        turn_id: Some(turn_id.to_string()),
        entry_index: None,
    })
}

fn json_object_payload_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
        "type": "object",
        "additionalProperties": true
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    use agentdash_application_ports::agent_run_surface::AgentRunGrantProjection;
    use agentdash_application_ports::runtime_surface_adoption::AgentFrameRuntimeTarget;
    use agentdash_application_vfs::tools::SharedRuntimeVfs;
    use agentdash_domain::DomainError;
    use agentdash_domain::canvas::{Canvas, CanvasFile};
    use agentdash_domain::extension_package::ExtensionPackageMetadata;
    use agentdash_domain::project::{
        Project, ProjectAuthorizationContext, ProjectRepository, ProjectRole, ProjectSubjectGrant,
        ProjectSubjectType,
    };
    use agentdash_domain::shared_library::{
        ExtensionBundleKind, ExtensionBundleRef, ExtensionRuntimeActionDefinition,
        ExtensionRuntimeActionKind, ExtensionTemplatePayload, ProjectExtensionInstallation,
        ProjectExtensionInstallationRepository,
    };
    use agentdash_domain::workflow::{
        RuntimeSessionExecutionAnchor, RuntimeSessionExecutionAnchorRepository,
    };
    use agentdash_spi::connector::RuntimeToolProvider;
    use agentdash_spi::platform::tool_capability::CAP_WORKSPACE_MODULE;
    use agentdash_spi::{
        AgentConfig, CapabilityState, ExecutionContext, ExecutionSessionFrame, ExecutionTurnFrame,
        ToolCapability, ToolCluster, ToolDefinition, Vfs, WorkspaceModuleDimension,
        WorkspaceModuleVisibilityMode,
    };
    use tokio::sync::RwLock;

    use super::*;
    use crate::canvas::{build_canvas, build_personal_canvas};
    use crate::workspace_module::WorkspaceModuleRuntimeToolProvider;
    use crate::workspace_module::runtime_bridge::{
        SharedWorkspaceModuleAgentRunBridgeHandle, SharedWorkspaceModuleRuntimeGatewayHandle,
        WorkspaceModuleAgentRunBridge,
    };

    fn manifest(extension_id: &str) -> ExtensionTemplatePayload {
        ExtensionTemplatePayload {
            manifest_version: "2".to_string(),
            extension_id: extension_id.to_string(),
            package: ExtensionPackageMetadata {
                name: extension_id.to_string(),
                version: "0.1.0".to_string(),
            },
            asset_version: "0.1.0".to_string(),
            commands: vec![],
            flags: vec![],
            message_renderers: vec![],
            capability_directives: vec![],
            asset_refs: vec![],
            runtime_actions: vec![ExtensionRuntimeActionDefinition {
                action_key: format!("{extension_id}.profile"),
                kind: ExtensionRuntimeActionKind::SessionRuntime,
                description: "read profile".to_string(),
                input_schema: serde_json::json!({"type": "object"}),
                output_schema: serde_json::json!({"type": "object"}),
                permissions: vec!["local.profile.read".to_string()],
            }],
            protocol_channels: vec![],
            extension_dependencies: vec![],
            workspace_tabs: vec![],
            permissions: vec![],
            bundles: vec![ExtensionBundleRef {
                kind: ExtensionBundleKind::ExtensionHost,
                entry: "dist/extension.js".to_string(),
                digest: "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                    .to_string(),
            }],
        }
    }

    const TEST_USER_ID: &str = "user-1";

    fn canvas_file_pairs(files: &[CanvasFile]) -> Vec<(&str, &str)> {
        files
            .iter()
            .map(|file| (file.path.as_str(), file.content.as_str()))
            .collect()
    }

    #[derive(Default)]
    struct FakeCanvasRuntimeStateRepository {
        observation: Mutex<Option<agentdash_domain::canvas::CanvasRuntimeObservation>>,
        snapshot: Mutex<Option<agentdash_domain::canvas::CanvasInteractionSnapshot>>,
    }

    #[async_trait]
    impl CanvasRuntimeStateRepository for FakeCanvasRuntimeStateRepository {
        async fn upsert_runtime_observation(
            &self,
            observation: agentdash_domain::canvas::CanvasRuntimeObservation,
        ) -> Result<agentdash_domain::canvas::CanvasRuntimeObservation, DomainError> {
            *self.observation.lock().expect("observation lock") = Some(observation.clone());
            Ok(observation)
        }

        async fn latest_runtime_observation(
            &self,
            _run_id: Uuid,
            _agent_id: Uuid,
            _canvas_mount_id: &str,
        ) -> Result<Option<agentdash_domain::canvas::CanvasRuntimeObservation>, DomainError>
        {
            Ok(self.observation.lock().expect("observation lock").clone())
        }

        async fn upsert_interaction_snapshot(
            &self,
            snapshot: agentdash_domain::canvas::CanvasInteractionSnapshot,
        ) -> Result<agentdash_domain::canvas::CanvasInteractionSnapshot, DomainError> {
            *self.snapshot.lock().expect("snapshot lock") = Some(snapshot.clone());
            Ok(snapshot)
        }

        async fn latest_interaction_snapshot(
            &self,
            _run_id: Uuid,
            _agent_id: Uuid,
            _canvas_mount_id: &str,
        ) -> Result<Option<agentdash_domain::canvas::CanvasInteractionSnapshot>, DomainError>
        {
            Ok(self.snapshot.lock().expect("snapshot lock").clone())
        }
    }

    fn fake_canvas_runtime_state_repo() -> Arc<dyn CanvasRuntimeStateRepository> {
        Arc::new(FakeCanvasRuntimeStateRepository::default())
    }

    #[derive(Default)]
    struct FakeAgentRunBridge {
        exposed_canvas_mount_ids: Mutex<Vec<String>>,
        requests: Mutex<Vec<RuntimeSurfaceUpdateRequest>>,
    }

    #[async_trait]
    impl WorkspaceModuleAgentRunBridge for FakeAgentRunBridge {
        async fn effective_capability_view_for_agent_run_delivery(
            &self,
            delivery_runtime_session_id: &str,
        ) -> Result<AgentRunEffectiveCapabilityView, String> {
            Ok(test_effective_capability_view(
                WorkspaceModuleDimension::all(),
                vec![delivery_runtime_session_id.to_string()],
            ))
        }

        async fn apply_canvas_runtime_surface_update_to_agent_run(
            &self,
            _delivery_runtime_session_id: &str,
            canvas: &Canvas,
            _current_user: Option<&ProjectAuthorizationContext>,
            request: RuntimeSurfaceUpdateRequest,
        ) -> Result<agentdash_domain::common::Vfs, String> {
            self.exposed_canvas_mount_ids
                .lock()
                .expect("exposed canvas lock")
                .push(canvas.mount_id.clone());
            self.requests.lock().expect("requests lock").push(request);
            Ok(agentdash_domain::common::Vfs::default())
        }

        async fn inject_agent_run_notification(
            &self,
            _delivery_runtime_session_id: &str,
            _notification: BackboneEnvelope,
        ) -> Result<(), String> {
            Ok(())
        }
    }

    fn test_current_user() -> ProjectAuthorizationContext {
        ProjectAuthorizationContext::new(TEST_USER_ID.to_string(), Vec::new(), false)
    }

    fn installation(project_id: Uuid, key: &str) -> ProjectExtensionInstallation {
        ProjectExtensionInstallation::new(
            project_id,
            key,
            format!("{key} Extension"),
            manifest(key),
            agentdash_domain::shared_library::InstalledAssetSource::new(
                Uuid::new_v4(),
                "integration:test:extension_template:demo",
                "0.1.0",
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            ),
        )
        .expect("valid installation")
    }

    #[derive(Default)]
    struct FakeInstallationRepo {
        installations: Mutex<Vec<ProjectExtensionInstallation>>,
    }

    #[async_trait]
    impl ProjectExtensionInstallationRepository for FakeInstallationRepo {
        async fn create(&self, item: &ProjectExtensionInstallation) -> Result<(), DomainError> {
            self.installations.lock().unwrap().push(item.clone());
            Ok(())
        }
        async fn update(&self, _item: &ProjectExtensionInstallation) -> Result<(), DomainError> {
            Ok(())
        }
        async fn get_by_project_and_key(
            &self,
            project_id: Uuid,
            extension_key: &str,
        ) -> Result<Option<ProjectExtensionInstallation>, DomainError> {
            Ok(self
                .installations
                .lock()
                .unwrap()
                .iter()
                .find(|i| i.project_id == project_id && i.extension_key == extension_key)
                .cloned())
        }
        async fn get_by_project_and_id(
            &self,
            project_id: Uuid,
            installation_id: Uuid,
        ) -> Result<Option<ProjectExtensionInstallation>, DomainError> {
            Ok(self
                .installations
                .lock()
                .unwrap()
                .iter()
                .find(|i| i.project_id == project_id && i.id == installation_id)
                .cloned())
        }
        async fn list_by_project(
            &self,
            project_id: Uuid,
        ) -> Result<Vec<ProjectExtensionInstallation>, DomainError> {
            Ok(self
                .installations
                .lock()
                .unwrap()
                .iter()
                .filter(|i| i.project_id == project_id)
                .cloned()
                .collect())
        }
        async fn list_enabled_by_project(
            &self,
            project_id: Uuid,
        ) -> Result<Vec<ProjectExtensionInstallation>, DomainError> {
            Ok(self
                .installations
                .lock()
                .unwrap()
                .iter()
                .filter(|i| i.project_id == project_id && i.enabled)
                .cloned()
                .collect())
        }
        async fn delete(
            &self,
            _project_id: Uuid,
            _installation_id: Uuid,
        ) -> Result<bool, DomainError> {
            Ok(true)
        }
    }

    #[derive(Default)]
    struct FakeProjectRepo {
        projects: RwLock<HashMap<Uuid, Project>>,
        grants: RwLock<Vec<ProjectSubjectGrant>>,
    }

    #[async_trait]
    impl ProjectRepository for FakeProjectRepo {
        async fn create(&self, project: &Project) -> Result<(), DomainError> {
            self.projects
                .write()
                .await
                .insert(project.id, project.clone());
            Ok(())
        }

        async fn get_by_id(&self, id: Uuid) -> Result<Option<Project>, DomainError> {
            Ok(self.projects.read().await.get(&id).cloned())
        }

        async fn list_all(&self) -> Result<Vec<Project>, DomainError> {
            Ok(self.projects.read().await.values().cloned().collect())
        }

        async fn update(&self, project: &Project) -> Result<(), DomainError> {
            self.projects
                .write()
                .await
                .insert(project.id, project.clone());
            Ok(())
        }

        async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
            self.projects.write().await.remove(&id);
            Ok(())
        }

        async fn list_subject_grants(
            &self,
            project_id: Uuid,
        ) -> Result<Vec<ProjectSubjectGrant>, DomainError> {
            Ok(self
                .grants
                .read()
                .await
                .iter()
                .filter(|grant| grant.project_id == project_id)
                .cloned()
                .collect())
        }

        async fn upsert_subject_grant(
            &self,
            grant: &ProjectSubjectGrant,
        ) -> Result<(), DomainError> {
            let mut grants = self.grants.write().await;
            grants.retain(|item| {
                item.project_id != grant.project_id
                    || item.subject_type != grant.subject_type
                    || item.subject_id != grant.subject_id
            });
            grants.push(grant.clone());
            Ok(())
        }

        async fn delete_subject_grant(
            &self,
            project_id: Uuid,
            subject_type: ProjectSubjectType,
            subject_id: &str,
        ) -> Result<(), DomainError> {
            self.grants.write().await.retain(|grant| {
                grant.project_id != project_id
                    || grant.subject_type != subject_type
                    || grant.subject_id != subject_id
            });
            Ok(())
        }
    }

    async fn fake_project_repo(project_id: Uuid) -> Arc<dyn ProjectRepository> {
        let repo = Arc::new(FakeProjectRepo::default());
        let mut project = Project::new_with_creator(
            "Test Project".to_string(),
            String::new(),
            TEST_USER_ID.to_string(),
        );
        project.id = project_id;
        repo.create(&project).await.expect("create project");
        repo.upsert_subject_grant(&ProjectSubjectGrant::new(
            project_id,
            ProjectSubjectType::User,
            TEST_USER_ID.to_string(),
            ProjectRole::Editor,
            TEST_USER_ID.to_string(),
        ))
        .await
        .expect("grant project access");
        repo
    }

    #[derive(Default)]
    struct FakeCanvasRepo {
        canvases: RwLock<HashMap<Uuid, Canvas>>,
    }

    #[async_trait]
    impl CanvasRepository for FakeCanvasRepo {
        async fn create(&self, canvas: &Canvas) -> Result<(), DomainError> {
            self.canvases
                .write()
                .await
                .insert(canvas.id, canvas.clone());
            Ok(())
        }
        async fn get_by_id(&self, id: Uuid) -> Result<Option<Canvas>, DomainError> {
            Ok(self.canvases.read().await.get(&id).cloned())
        }
        async fn get_by_mount_id(
            &self,
            project_id: Uuid,
            mount_id: &str,
        ) -> Result<Option<Canvas>, DomainError> {
            Ok(self
                .canvases
                .read()
                .await
                .values()
                .find(|c| c.project_id == project_id && c.mount_id == mount_id)
                .cloned())
        }
        async fn list_by_project(&self, project_id: Uuid) -> Result<Vec<Canvas>, DomainError> {
            Ok(self
                .canvases
                .read()
                .await
                .values()
                .filter(|c| c.project_id == project_id)
                .cloned()
                .collect())
        }
        async fn update(&self, canvas: &Canvas) -> Result<(), DomainError> {
            self.canvases
                .write()
                .await
                .insert(canvas.id, canvas.clone());
            Ok(())
        }
        async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
            self.canvases.write().await.remove(&id);
            Ok(())
        }
    }

    #[derive(Default)]
    struct FakeRuntimeSessionExecutionAnchorRepository {
        anchors: RwLock<HashMap<String, RuntimeSessionExecutionAnchor>>,
    }

    #[async_trait]
    impl RuntimeSessionExecutionAnchorRepository for FakeRuntimeSessionExecutionAnchorRepository {
        async fn upsert(&self, anchor: &RuntimeSessionExecutionAnchor) -> Result<(), DomainError> {
            self.anchors
                .write()
                .await
                .insert(anchor.runtime_session_id.clone(), anchor.clone());
            Ok(())
        }

        async fn delete_by_session(&self, runtime_session_id: &str) -> Result<(), DomainError> {
            self.anchors.write().await.remove(runtime_session_id);
            Ok(())
        }

        async fn find_by_session(
            &self,
            runtime_session_id: &str,
        ) -> Result<Option<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(self.anchors.read().await.get(runtime_session_id).cloned())
        }

        async fn list_by_run(
            &self,
            run_id: Uuid,
        ) -> Result<Vec<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(self
                .anchors
                .read()
                .await
                .values()
                .filter(|anchor| anchor.run_id == run_id)
                .cloned()
                .collect())
        }

        async fn list_by_agent(
            &self,
            agent_id: Uuid,
        ) -> Result<Vec<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(self
                .anchors
                .read()
                .await
                .values()
                .filter(|anchor| anchor.agent_id == agent_id)
                .cloned()
                .collect())
        }

        async fn list_by_project_session_ids(
            &self,
            runtime_session_ids: &[String],
        ) -> Result<Vec<RuntimeSessionExecutionAnchor>, DomainError> {
            let anchors = self.anchors.read().await;
            Ok(runtime_session_ids
                .iter()
                .filter_map(|runtime_session_id| anchors.get(runtime_session_id).cloned())
                .collect())
        }

        async fn latest_updated_anchor_for_agent(
            &self,
            agent_id: Uuid,
        ) -> Result<Option<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(self
                .anchors
                .read()
                .await
                .values()
                .filter(|anchor| anchor.agent_id == agent_id)
                .max_by_key(|anchor| anchor.updated_at)
                .cloned())
        }
    }

    async fn fixtures() -> (
        Arc<dyn ProjectExtensionInstallationRepository>,
        Arc<dyn CanvasRepository>,
        Uuid,
    ) {
        let project_id = Uuid::new_v4();
        let install_repo = Arc::new(FakeInstallationRepo::default());
        install_repo
            .installations
            .lock()
            .unwrap()
            .push(installation(project_id, "demo"));
        let canvas_repo = Arc::new(FakeCanvasRepo::default());
        let canvas = build_personal_canvas(
            project_id,
            TEST_USER_ID.to_string(),
            Some("cvs-dashboard-a".to_string()),
            "Dashboard A".to_string(),
            "demo canvas".to_string(),
            Default::default(),
        )
        .expect("canvas");
        canvas_repo.create(&canvas).await.expect("create canvas");
        (install_repo, canvas_repo, project_id)
    }

    fn test_effective_capability_view(
        workspace_module: WorkspaceModuleDimension,
        runtime_refs: Vec<String>,
    ) -> AgentRunEffectiveCapabilityView {
        let mut capability_state = CapabilityState::from_clusters([ToolCluster::WorkspaceModule]);
        capability_state.workspace_module = workspace_module;
        AgentRunEffectiveCapabilityView {
            target: AgentFrameRuntimeTarget {
                frame_id: Uuid::new_v4(),
                delivery_runtime_session_id: "session-test".to_string(),
            },
            visible_capabilities: capability_state.tool.capabilities.clone(),
            vfs_surface: capability_state.vfs.active.clone().unwrap_or_default(),
            mcp_surface: Vec::new(),
            capability_state,
            visible_workspace_module_refs: runtime_refs,
            grant_projection: AgentRunGrantProjection::default(),
        }
    }

    #[tokio::test]
    async fn list_returns_extension_and_canvas_summaries() {
        let (install_repo, canvas_repo, project_id) = fixtures().await;
        let tool = WorkspaceModuleListTool::new(install_repo, canvas_repo, project_id)
            .with_current_user(Some(test_current_user()))
            .with_effective_capability_view(test_effective_capability_view(
                WorkspaceModuleDimension::all(),
                Vec::new(),
            ));
        let result = tool
            .execute("t", serde_json::json!({}), CancellationToken::new(), None)
            .await
            .expect("list");
        let details = result.details.expect("details");
        let modules = details
            .get("modules")
            .and_then(serde_json::Value::as_array)
            .expect("modules array");
        assert_eq!(modules.len(), 2);
        // 摘要不含完整 schema：summary 没有 operations 字段，只有 operation_summary
        assert!(modules[0].get("operations").is_none());
        assert!(modules[0].get("operation_summary").is_some());
    }

    #[tokio::test]
    async fn list_without_current_user_omits_canvas_modules() {
        let (install_repo, canvas_repo, project_id) = fixtures().await;
        let tool = WorkspaceModuleListTool::new(install_repo, canvas_repo, project_id)
            .with_effective_capability_view(test_effective_capability_view(
                WorkspaceModuleDimension::all(),
                Vec::new(),
            ));
        let result = tool
            .execute("t", serde_json::json!({}), CancellationToken::new(), None)
            .await
            .expect("list");
        let details = result.details.expect("details");
        let modules = details
            .get("modules")
            .and_then(serde_json::Value::as_array)
            .expect("modules array");

        assert_eq!(modules.len(), 1);
        assert_eq!(
            modules[0]
                .get("module_id")
                .and_then(serde_json::Value::as_str),
            Some("ext:demo")
        );
    }

    #[tokio::test]
    async fn describe_returns_full_descriptor_with_operations() {
        let (install_repo, canvas_repo, project_id) = fixtures().await;
        let tool = WorkspaceModuleDescribeTool::new(install_repo, canvas_repo, project_id)
            .with_effective_capability_view(test_effective_capability_view(
                WorkspaceModuleDimension::all(),
                Vec::new(),
            ));
        let result = tool
            .execute(
                "t",
                serde_json::json!({"module_id": "ext:demo"}),
                CancellationToken::new(),
                None,
            )
            .await
            .expect("describe");
        assert!(!result.is_error);
        let details = result.details.expect("details");
        let operations = details
            .get("operations")
            .and_then(serde_json::Value::as_array)
            .expect("operations");
        assert_eq!(operations.len(), 1);
        assert_eq!(
            operations[0]
                .get("origin")
                .and_then(serde_json::Value::as_str),
            Some("runtime_action")
        );
    }

    #[tokio::test]
    async fn describe_unknown_module_returns_structured_error() {
        let (install_repo, canvas_repo, project_id) = fixtures().await;
        let tool = WorkspaceModuleDescribeTool::new(install_repo, canvas_repo, project_id)
            .with_effective_capability_view(test_effective_capability_view(
                WorkspaceModuleDimension::all(),
                Vec::new(),
            ));
        let result = tool
            .execute(
                "t",
                serde_json::json!({"module_id": "ext:missing"}),
                CancellationToken::new(),
                None,
            )
            .await
            .expect("describe");
        assert!(result.is_error);
        assert_eq!(
            result
                .details
                .and_then(|d| d.get("error").and_then(|e| e.as_str()).map(str::to_string)),
            Some("module_not_found".to_string())
        );
    }

    #[tokio::test]
    async fn allowlist_visibility_filters_modules() {
        let (install_repo, canvas_repo, project_id) = fixtures().await;
        let visibility = WorkspaceModuleDimension {
            mode: WorkspaceModuleVisibilityMode::Allowlist,
            allowed_module_ids: vec!["ext:demo".to_string()],
        };
        let tool = WorkspaceModuleListTool::new(install_repo, canvas_repo, project_id)
            .with_effective_capability_view(test_effective_capability_view(visibility, Vec::new()));
        let result = tool
            .execute("t", serde_json::json!({}), CancellationToken::new(), None)
            .await
            .expect("list");
        let modules = result
            .details
            .and_then(|d| d.get("modules").and_then(|m| m.as_array()).cloned())
            .expect("modules");
        assert_eq!(modules.len(), 1);
        assert_eq!(
            modules[0]
                .get("module_id")
                .and_then(serde_json::Value::as_str),
            Some("ext:demo")
        );
    }

    #[tokio::test]
    async fn runtime_visible_refs_extend_workspace_module_allowlist() {
        let (install_repo, canvas_repo, project_id) = fixtures().await;
        let visibility = WorkspaceModuleDimension {
            mode: WorkspaceModuleVisibilityMode::Allowlist,
            allowed_module_ids: vec!["ext:demo".to_string()],
        };
        let view =
            test_effective_capability_view(visibility, vec!["canvas:cvs-dashboard-a".to_string()]);
        let projection =
            resolve_workspace_module_visibility(&install_repo, &canvas_repo, project_id, &view)
                .await
                .expect("resolve modules");
        let module_ids = projection
            .modules
            .iter()
            .map(|module| module.summary.module_id.as_str())
            .collect::<Vec<_>>();

        assert_eq!(module_ids.len(), 2);
        assert!(module_ids.contains(&"ext:demo"));
        assert!(module_ids.contains(&"canvas:cvs-dashboard-a"));
    }

    #[tokio::test]
    async fn operate_canvas_without_runtime_session_returns_diagnostic() {
        let project_id = Uuid::new_v4();
        let project_repo = fake_project_repo(project_id).await;
        let canvas_repo = Arc::new(FakeCanvasRepo::default());
        let shared_vfs = SharedRuntimeVfs::new(agentdash_spi::Vfs::default());
        let tool = WorkspaceModuleOperateTool::new(
            project_repo,
            canvas_repo,
            project_id,
            shared_vfs.clone(),
            SharedWorkspaceModuleAgentRunBridgeHandle::default(),
            None,
        )
        .with_current_user(Some(test_current_user()));

        let result = tool
            .execute(
                "t",
                serde_json::json!({
                    "operation": "canvas.create",
                    "input": {
                        "canvas_mount_id": "cvs-sales-board",
                        "title": "Sales Board",
                        "description": "test canvas"
                    }
                }),
                CancellationToken::new(),
                None,
            )
            .await;

        assert!(
            matches!(
                result,
                Err(AgentToolError::ExecutionFailed(ref message))
                    if message.contains("AgentRun bridge")
            ),
            "Canvas expose must fail explicitly without a runtime session, got {result:?}"
        );
    }

    #[tokio::test]
    async fn operate_copy_to_personal_materializes_editable_canvas_with_random_mount_suffix() {
        let project_id = Uuid::new_v4();
        let project_repo = fake_project_repo(project_id).await;
        let canvas_repo = Arc::new(FakeCanvasRepo::default());
        let source = Canvas::new_project_shared(
            project_id,
            "cvs-shared-dashboard".to_string(),
            "Shared Dashboard".to_string(),
            "project shared canvas".to_string(),
            None,
            Some("publisher-1".to_string()),
        );
        let source_id = source.id;
        canvas_repo
            .create(&source)
            .await
            .expect("create shared source");
        let bridge_handle = SharedWorkspaceModuleAgentRunBridgeHandle::default();
        let bridge = Arc::new(FakeAgentRunBridge::default());
        bridge_handle.set(bridge.clone()).await;
        let tool = WorkspaceModuleOperateTool::new(
            project_repo,
            canvas_repo.clone(),
            project_id,
            SharedRuntimeVfs::new(agentdash_spi::Vfs::default()),
            bridge_handle,
            Some("delivery-session-1".to_string()),
        )
        .with_current_user(Some(test_current_user()));

        let result = tool
            .execute(
                "t",
                serde_json::json!({
                    "operation": "canvas.copy",
                    "input": {
                        "source_canvas_mount_id": "cvs-shared-dashboard"
                    }
                }),
                CancellationToken::new(),
                None,
            )
            .await
            .expect("copy to personal");

        assert!(!result.is_error, "expected success, got {result:?}");
        let details = result.details.expect("details");
        assert_eq!(
            details
                .pointer("/canvas/action")
                .and_then(serde_json::Value::as_str),
            Some("copied")
        );
        let mount_id = details
            .pointer("/canvas/canvas_mount_id")
            .and_then(serde_json::Value::as_str)
            .expect("copied canvas mount id");
        let suffix = mount_id
            .strip_prefix("cvs-shared-dashboard-copy-")
            .expect("copy mount prefix");
        assert_eq!(suffix.len(), 4);
        assert!(
            suffix
                .chars()
                .all(|value| value.is_ascii_lowercase() || value.is_ascii_digit())
        );
        let operations = details
            .pointer("/descriptor/operations")
            .and_then(serde_json::Value::as_array)
            .expect("descriptor operations");
        assert!(operations.iter().any(|operation| {
            operation
                .get("operation_key")
                .and_then(serde_json::Value::as_str)
                == Some(CANVAS_BIND_DATA_OPERATION_KEY)
        }));
        let saved = canvas_repo
            .get_by_mount_id(project_id, mount_id)
            .await
            .expect("load copied canvas")
            .expect("copied canvas");
        assert_eq!(saved.scope, CanvasScope::Personal);
        assert_eq!(saved.owner_user_id.as_deref(), Some(TEST_USER_ID));
        assert_eq!(saved.cloned_from_canvas_id, Some(source_id));
        assert_eq!(
            bridge
                .exposed_canvas_mount_ids
                .lock()
                .expect("exposed canvas lock")
                .as_slice(),
            &[mount_id.to_string()]
        );
    }

    // ---- invoke tool tests ----

    use agentdash_application_ports::extension_runtime::{
        ExtensionChannelInvokeRequest, ExtensionChannelInvokeResponse,
        ExtensionRuntimeActionTransportError, ExtensionRuntimeChannelTransport,
    };
    use agentdash_application_runtime_gateway::{
        RuntimeActionKind, RuntimeInvocationOutput, RuntimeProvider,
    };

    struct EchoActionProvider {
        action_key: RuntimeActionKey,
        invoke_count: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl RuntimeProvider for EchoActionProvider {
        fn action_key(&self) -> &RuntimeActionKey {
            &self.action_key
        }
        fn action_kind(&self) -> RuntimeActionKind {
            RuntimeActionKind::SessionRuntime
        }
        async fn invoke(
            &self,
            request: RuntimeInvocationRequest,
        ) -> Result<RuntimeInvocationOutput, RuntimeInvocationError> {
            self.invoke_count.fetch_add(1, Ordering::SeqCst);
            Ok(RuntimeInvocationOutput::new(serde_json::json!({
                "echoed": request.input,
                "action": request.action_key.as_str(),
            })))
        }
    }

    struct NoopChannelTransport;

    #[async_trait]
    impl ExtensionRuntimeChannelTransport for NoopChannelTransport {
        async fn invoke_extension_channel(
            &self,
            _backend_id: &str,
            _payload: ExtensionChannelInvokeRequest,
        ) -> Result<ExtensionChannelInvokeResponse, ExtensionRuntimeActionTransportError> {
            Err(ExtensionRuntimeActionTransportError::Failed(
                "noop channel transport".to_string(),
            ))
        }
    }

    fn invoke_tool_with_backend(
        install_repo: Arc<dyn ProjectExtensionInstallationRepository>,
        canvas_repo: Arc<dyn CanvasRepository>,
        project_id: Uuid,
        backend: Option<ResolvedInvocationBackend>,
    ) -> WorkspaceModuleInvokeTool {
        let (tool, _invoke_count) =
            invoke_tool_with_backend_and_counter(install_repo, canvas_repo, project_id, backend);
        tool
    }

    fn invoke_tool_with_backend_and_counter(
        install_repo: Arc<dyn ProjectExtensionInstallationRepository>,
        canvas_repo: Arc<dyn CanvasRepository>,
        project_id: Uuid,
        backend: Option<ResolvedInvocationBackend>,
    ) -> (WorkspaceModuleInvokeTool, Arc<AtomicUsize>) {
        let invoke_count = Arc::new(AtomicUsize::new(0));
        let gateway = Arc::new(
            RuntimeGateway::new().with_provider(Arc::new(EchoActionProvider {
                action_key: RuntimeActionKey::parse("demo.profile").expect("valid action key"),
                invoke_count: invoke_count.clone(),
            })),
        );
        let channel_invoker = Arc::new(ExtensionRuntimeChannelInvoker::new(
            install_repo.clone(),
            Arc::new(NoopChannelTransport),
        ));
        let tool = WorkspaceModuleInvokeTool::new(
            install_repo,
            canvas_repo,
            fake_canvas_runtime_state_repo(),
            Arc::new(FakeRuntimeSessionExecutionAnchorRepository::default()),
            project_id,
            "session-1".to_string(),
            None,
            backend,
            gateway,
            channel_invoker,
        )
        .with_current_user(Some(test_current_user()))
        .with_effective_capability_view(test_effective_capability_view(
            WorkspaceModuleDimension::all(),
            Vec::new(),
        ));
        (tool, invoke_count)
    }

    fn backend(id: &str) -> Option<ResolvedInvocationBackend> {
        Some(ResolvedInvocationBackend {
            backend_id: id.to_string(),
            workspace: None,
        })
    }

    fn workspace_module_execution_context(project_id: Uuid) -> ExecutionContext {
        let working_directory = PathBuf::from(".");
        let mut vfs = Vfs::default();
        vfs.source_project_id = Some(project_id.to_string());
        let mut capability_state = CapabilityState::from_clusters([ToolCluster::WorkspaceModule]);
        capability_state.workspace_module = WorkspaceModuleDimension::all();
        capability_state
            .tool
            .capabilities
            .insert(ToolCapability::new(CAP_WORKSPACE_MODULE));
        ExecutionContext {
            session: ExecutionSessionFrame {
                turn_id: "turn-1".to_string(),
                working_directory,
                environment_variables: HashMap::new(),
                executor_config: AgentConfig::default(),
                mcp_servers: Vec::new(),
                vfs: Some(vfs),
                backend_execution: None,
                runtime_backend_anchor: None,
                identity: None,
            },
            turn: ExecutionTurnFrame {
                capability_state,
                ..Default::default()
            },
        }
    }

    #[tokio::test]
    async fn workspace_module_provider_declaration_does_not_invoke_runtime_gateway() {
        let (install_repo, canvas_repo, project_id) = fixtures().await;
        let invoke_count = Arc::new(AtomicUsize::new(0));
        let gateway = Arc::new(
            RuntimeGateway::new().with_provider(Arc::new(EchoActionProvider {
                action_key: RuntimeActionKey::parse("demo.profile").expect("valid action key"),
                invoke_count: invoke_count.clone(),
            })),
        );
        let gateway_handle = SharedWorkspaceModuleRuntimeGatewayHandle::default();
        gateway_handle.set(gateway).await;
        let project_repo = fake_project_repo(project_id).await;
        let provider = WorkspaceModuleRuntimeToolProvider::new(
            install_repo,
            project_repo,
            canvas_repo,
            fake_canvas_runtime_state_repo(),
            Arc::new(FakeRuntimeSessionExecutionAnchorRepository::default()),
            SharedWorkspaceModuleAgentRunBridgeHandle::default(),
            gateway_handle,
        )
        .with_extension_channel_transport(Arc::new(NoopChannelTransport));
        let context = workspace_module_execution_context(project_id);

        let tools = provider
            .build_tools(&context)
            .await
            .expect("workspace module tools should build");
        assert!(
            tools
                .iter()
                .any(|tool| tool.name() == "workspace_module_invoke"),
            "provider should declare invoke tool when runtime deps are present"
        );

        let definitions = tools
            .iter()
            .map(|tool| ToolDefinition::from_tool(tool.as_ref()))
            .collect::<Vec<_>>();
        assert!(
            definitions
                .iter()
                .any(|definition| definition.name == "workspace_module_invoke")
        );
        assert_eq!(invoke_count.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn workspace_module_provider_declares_diagnostic_invoke_tool_when_runtime_deps_missing() {
        let (install_repo, canvas_repo, project_id) = fixtures().await;
        let project_repo = fake_project_repo(project_id).await;
        let provider = WorkspaceModuleRuntimeToolProvider::new(
            install_repo,
            project_repo,
            canvas_repo,
            fake_canvas_runtime_state_repo(),
            Arc::new(FakeRuntimeSessionExecutionAnchorRepository::default()),
            SharedWorkspaceModuleAgentRunBridgeHandle::default(),
            SharedWorkspaceModuleRuntimeGatewayHandle::default(),
        );
        let context = workspace_module_execution_context(project_id);

        let tools = provider
            .build_tools(&context)
            .await
            .expect("workspace module tools should build with diagnostic tool");
        let invoke_tool = tools
            .iter()
            .find(|tool| tool.name() == "workspace_module_invoke")
            .expect("missing runtime deps should still expose invoke diagnostic tool");

        let result = invoke_tool
            .execute(
                "tool-call-1",
                serde_json::json!({
                    "module_id": "ext:demo",
                    "operation_key": "demo.profile",
                    "input": {}
                }),
                CancellationToken::new(),
                None,
            )
            .await
            .expect("diagnostic tool should return structured result");

        assert!(result.is_error);
        let details = result.details.expect("diagnostic details");
        assert_eq!(
            details.get("error").and_then(serde_json::Value::as_str),
            Some("workspace_module_runtime_dependencies_unavailable")
        );
        let missing = details
            .get("missing_dependencies")
            .and_then(serde_json::Value::as_array)
            .expect("missing dependencies array")
            .iter()
            .filter_map(serde_json::Value::as_str)
            .collect::<Vec<_>>();
        assert!(missing.contains(&"runtime_gateway"));
        assert!(missing.contains(&"extension_channel_transport"));
    }

    #[tokio::test]
    async fn workspace_module_tool_schemas_are_provider_safe() {
        let (install_repo, canvas_repo, project_id) = fixtures().await;
        let gateway_handle = SharedWorkspaceModuleRuntimeGatewayHandle::default();
        gateway_handle
            .set(Arc::new(RuntimeGateway::new().with_provider(Arc::new(
                EchoActionProvider {
                    action_key: RuntimeActionKey::parse("demo.profile").expect("valid action key"),
                    invoke_count: Arc::new(AtomicUsize::new(0)),
                },
            ))))
            .await;
        let project_repo = fake_project_repo(project_id).await;
        let provider = WorkspaceModuleRuntimeToolProvider::new(
            install_repo,
            project_repo,
            canvas_repo,
            fake_canvas_runtime_state_repo(),
            Arc::new(FakeRuntimeSessionExecutionAnchorRepository::default()),
            SharedWorkspaceModuleAgentRunBridgeHandle::default(),
            gateway_handle,
        )
        .with_extension_channel_transport(Arc::new(NoopChannelTransport));
        let context = workspace_module_execution_context(project_id);

        let tools = provider
            .build_tools(&context)
            .await
            .expect("workspace module tools should build");
        let definitions = tools
            .iter()
            .map(|tool| ToolDefinition::from_tool(tool.as_ref()))
            .collect::<Vec<_>>();

        for (tool_name, payload_field) in [
            ("workspace_module_operate", "input"),
            ("workspace_module_invoke", "input"),
            ("workspace_module_present", "payload"),
        ] {
            let definition = definitions
                .iter()
                .find(|definition| definition.name == tool_name)
                .unwrap_or_else(|| panic!("{tool_name} should be declared"));
            assert!(
                definition.parameters.get("$defs").is_none(),
                "{tool_name} schema should not expose recursive $defs"
            );
            assert!(
                definition.parameters.get("definitions").is_none(),
                "{tool_name} schema should not expose recursive definitions"
            );

            let payload_schema = &definition.parameters["properties"][payload_field];
            assert_eq!(
                payload_schema["type"], "object",
                "{tool_name} {payload_field} type"
            );
            assert_eq!(
                payload_schema["additionalProperties"], true,
                "{tool_name} {payload_field} should accept object payload properties"
            );
        }
    }

    #[tokio::test]
    async fn invoke_runtime_action_routes_to_gateway() {
        let (install_repo, canvas_repo, project_id) = fixtures().await;
        let (tool, invoke_count) = invoke_tool_with_backend_and_counter(
            install_repo,
            canvas_repo,
            project_id,
            backend("backend-1"),
        );
        let result = tool
            .execute(
                "t",
                serde_json::json!({
                    "module_id": "ext:demo",
                    "operation_key": "demo.profile",
                    "input": {"name": "alice"}
                }),
                CancellationToken::new(),
                None,
            )
            .await
            .expect("invoke");
        assert!(!result.is_error, "expected success, got {result:?}");
        let details = result.details.expect("details");
        // provenance 可还原 module source + operation provenance（R5）
        let provenance = details.get("provenance").expect("provenance");
        assert_eq!(
            provenance.get("operation_origin").and_then(|v| v.as_str()),
            Some("runtime_action")
        );
        assert_eq!(
            provenance.get("backend").and_then(|v| v.as_str()),
            Some("backend-1")
        );
        // gateway 实际收到 input
        let output = details.get("output").expect("output");
        assert_eq!(output["echoed"]["name"], "alice");
        assert_eq!(invoke_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn invoke_canvas_bind_data_routes_to_host_canvas_use_case() {
        let (install_repo, canvas_repo, project_id) = fixtures().await;
        let bridge_handle = SharedWorkspaceModuleAgentRunBridgeHandle::default();
        let bridge = Arc::new(FakeAgentRunBridge::default());
        bridge_handle.set(bridge.clone()).await;
        let tool = invoke_tool_with_backend(install_repo, canvas_repo.clone(), project_id, None)
            .with_agent_run_visibility(bridge_handle);
        let before = canvas_repo
            .get_by_mount_id(project_id, "cvs-dashboard-a")
            .await
            .expect("load canvas before bind")
            .expect("canvas before bind");
        let result = tool
            .execute(
                "t",
                serde_json::json!({
                    "module_id": "canvas:cvs-dashboard-a",
                    "operation_key": "canvas.bind_data",
                    "input": {
                        "alias": "stats",
                        "source_uri": "project://data/stats.csv"
                    }
                }),
                CancellationToken::new(),
                None,
            )
            .await
            .expect("invoke canvas bind data");

        assert!(!result.is_error, "expected success, got {result:?}");
        let details = result.details.expect("details");
        assert_eq!(
            details
                .pointer("/provenance/operation_origin")
                .and_then(serde_json::Value::as_str),
            Some("host_canvas")
        );
        assert_eq!(
            details
                .pointer("/output/content_type")
                .and_then(serde_json::Value::as_str),
            Some("text/csv")
        );

        let saved = canvas_repo
            .get_by_mount_id(project_id, "cvs-dashboard-a")
            .await
            .expect("load canvas")
            .expect("canvas");
        assert_eq!(saved.entry_file, before.entry_file);
        assert_eq!(
            canvas_file_pairs(&saved.files),
            canvas_file_pairs(&before.files)
        );
        let requests = bridge.requests.lock().expect("requests lock");
        assert_eq!(requests.len(), 1);
        let RuntimeSurfaceUpdateRequest::CanvasBindingChanged {
            canvas_mount_id,
            binding,
        } = &requests[0]
        else {
            panic!("expected CanvasBindingChanged request");
        };
        assert_eq!(canvas_mount_id, "cvs-dashboard-a");
        assert_eq!(binding.alias, "stats");
        assert_eq!(binding.source_uri, "project://data/stats.csv");
        assert_eq!(binding.content_type, "text/csv");
    }

    #[tokio::test]
    async fn invoke_canvas_bind_data_allows_shared_canvas_runtime_binding() {
        let project_id = Uuid::new_v4();
        let install_repo = Arc::new(FakeInstallationRepo::default());
        let canvas_repo = Arc::new(FakeCanvasRepo::default());
        let shared_canvas = build_canvas(
            project_id,
            Some("cvs-shared-dashboard".to_string()),
            "Shared Dashboard".to_string(),
            "project shared canvas".to_string(),
            Default::default(),
        )
        .expect("shared canvas");
        canvas_repo
            .create(&shared_canvas)
            .await
            .expect("create shared canvas");
        let bridge_handle = SharedWorkspaceModuleAgentRunBridgeHandle::default();
        let bridge = Arc::new(FakeAgentRunBridge::default());
        bridge_handle.set(bridge.clone()).await;
        let tool = invoke_tool_with_backend(install_repo, canvas_repo.clone(), project_id, None)
            .with_agent_run_visibility(bridge_handle);

        let result = tool
            .execute(
                "t",
                serde_json::json!({
                    "module_id": "canvas:cvs-shared-dashboard",
                    "operation_key": "canvas.bind_data",
                    "input": {
                        "alias": "stats",
                        "source_uri": "project://data/stats.csv"
                    }
                }),
                CancellationToken::new(),
                None,
            )
            .await
            .expect("invoke shared canvas bind data");

        assert!(!result.is_error, "expected shared Canvas bind to succeed");
        let saved = canvas_repo
            .get_by_mount_id(project_id, "cvs-shared-dashboard")
            .await
            .expect("load shared canvas")
            .expect("shared canvas");
        assert_eq!(saved.entry_file, shared_canvas.entry_file);
        assert_eq!(
            canvas_file_pairs(&saved.files),
            canvas_file_pairs(&shared_canvas.files)
        );
        let requests = bridge.requests.lock().expect("requests lock");
        assert_eq!(requests.len(), 1);
        let RuntimeSurfaceUpdateRequest::CanvasBindingChanged {
            canvas_mount_id,
            binding,
        } = &requests[0]
        else {
            panic!("expected CanvasBindingChanged request");
        };
        assert_eq!(canvas_mount_id, "cvs-shared-dashboard");
        assert_eq!(binding.alias, "stats");
        assert_eq!(binding.source_uri, "project://data/stats.csv");
    }

    #[tokio::test]
    async fn invoke_unknown_operation_returns_structured_error() {
        let (install_repo, canvas_repo, project_id) = fixtures().await;
        let tool =
            invoke_tool_with_backend(install_repo, canvas_repo, project_id, backend("backend-1"));
        let result = tool
            .execute(
                "t",
                serde_json::json!({
                    "module_id": "ext:demo",
                    "operation_key": "demo.nope",
                    "input": {}
                }),
                CancellationToken::new(),
                None,
            )
            .await
            .expect("invoke");
        assert!(result.is_error);
        assert_eq!(
            result
                .details
                .and_then(|d| d.get("error").and_then(|e| e.as_str()).map(str::to_string)),
            Some("operation_not_found".to_string())
        );
    }

    #[tokio::test]
    async fn invoke_unknown_module_returns_structured_error() {
        let (install_repo, canvas_repo, project_id) = fixtures().await;
        let tool =
            invoke_tool_with_backend(install_repo, canvas_repo, project_id, backend("backend-1"));
        let result = tool
            .execute(
                "t",
                serde_json::json!({
                    "module_id": "ext:missing",
                    "operation_key": "demo.profile",
                    "input": {}
                }),
                CancellationToken::new(),
                None,
            )
            .await
            .expect("invoke");
        assert!(result.is_error);
        assert_eq!(
            result
                .details
                .and_then(|d| d.get("error").and_then(|e| e.as_str()).map(str::to_string)),
            Some("module_not_found".to_string())
        );
    }

    #[tokio::test]
    async fn invoke_input_schema_mismatch_returns_structured_error() {
        let (install_repo, canvas_repo, project_id) = fixtures().await;
        let tool =
            invoke_tool_with_backend(install_repo, canvas_repo, project_id, backend("backend-1"));
        // demo.profile input_schema = {"type":"object"}; 传 array 触发类型不匹配
        let result = tool
            .execute(
                "t",
                serde_json::json!({
                    "module_id": "ext:demo",
                    "operation_key": "demo.profile",
                    "input": [1, 2, 3]
                }),
                CancellationToken::new(),
                None,
            )
            .await
            .expect("invoke");
        assert!(result.is_error);
        assert_eq!(
            result
                .details
                .and_then(|d| d.get("error").and_then(|e| e.as_str()).map(str::to_string)),
            Some("input_schema_mismatch".to_string())
        );
    }

    #[tokio::test]
    async fn invoke_missing_backend_returns_structured_error() {
        let (install_repo, canvas_repo, project_id) = fixtures().await;
        let tool = invoke_tool_with_backend(install_repo, canvas_repo, project_id, None);
        let result = tool
            .execute(
                "t",
                serde_json::json!({
                    "module_id": "ext:demo",
                    "operation_key": "demo.profile",
                    "input": {}
                }),
                CancellationToken::new(),
                None,
            )
            .await
            .expect("invoke");
        assert!(result.is_error);
        assert_eq!(
            result
                .details
                .and_then(|d| d.get("error").and_then(|e| e.as_str()).map(str::to_string)),
            Some("backend_unavailable".to_string())
        );
    }
}
