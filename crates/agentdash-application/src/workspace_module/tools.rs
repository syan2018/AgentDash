//! Workspace Module Agent 工具：`workspace_module_list` / `workspace_module_describe`。
//!
//! 二者由 session runtime tool composer 通过 workspace-module provider 装配，
//! 用当前 project context + repos 现取现算：每次调用拉 enabled installations + visible canvases，经聚合层
//! `build_workspace_modules` 投影，再按 capability 的
//! AgentRun effective capability view 过滤。

use std::sync::Arc;

use agentdash_agent_protocol::{
    BackboneEnvelope, BackboneEvent, PlatformEvent, SourceInfo, TraceInfo,
};
use agentdash_contracts::workspace_module::{
    WorkspaceModuleCanvasHostAction, WorkspaceModuleDescriptor, WorkspaceModuleOperation,
    WorkspaceModuleOperationDispatch,
};
use agentdash_domain::canvas::{Canvas, CanvasRepository};
use agentdash_domain::shared_library::ProjectExtensionInstallationRepository;
use agentdash_spi::context::tool_schema_sanitizer::schema_value;
use agentdash_spi::{AgentTool, AgentToolError, AgentToolResult, ContentPart, ToolUpdateCallback};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::agent_run::AgentRunEffectiveCapabilityView;
use crate::canvas::{
    BindCanvasDataParams, StartCanvasParams, bind_canvas_data_for_project,
    create_or_attach_canvas_for_session, expose_existing_canvas_for_session,
};
use crate::extension_runtime::ExtensionRuntimeProjection;
use crate::runtime_gateway::{
    ExtensionRuntimeChannelConsumer, ExtensionRuntimeChannelInvokeRequest,
    ExtensionRuntimeChannelInvoker, RuntimeActionKey, RuntimeActor, RuntimeContext, RuntimeGateway,
    RuntimeInvocationError, RuntimeInvocationErrorKind, RuntimeInvocationRequest,
    RuntimeInvocationResult, RuntimeTarget, RuntimeTrace,
};
use crate::runtime_tools::SharedSessionToolServicesHandle;
use crate::workspace_module::{
    ResolvedInvocationBackend, build_workspace_module_presentation, build_workspace_modules,
    resolve_workspace_module_visibility, validate_input_against_schema,
};

#[derive(Clone, Default)]
struct WorkspaceModuleVisibilitySource {
    session_services_handle: Option<SharedSessionToolServicesHandle>,
    session_id: Option<String>,
    #[cfg(test)]
    effective_view: Option<AgentRunEffectiveCapabilityView>,
}

impl WorkspaceModuleVisibilitySource {
    fn with_runtime(
        mut self,
        session_services_handle: SharedSessionToolServicesHandle,
        session_id: impl Into<String>,
    ) -> Self {
        self.session_services_handle = Some(session_services_handle);
        self.session_id = Some(session_id.into());
        self
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

        let (Some(handle), Some(session_id)) = (
            self.session_services_handle.as_ref(),
            self.session_id.as_deref(),
        ) else {
            return Err(AgentToolError::ExecutionFailed(
                "AgentRun effective capability view unavailable for workspace module visibility"
                    .to_string(),
            ));
        };
        let Some(session_services) = handle.get().await else {
            return Err(AgentToolError::ExecutionFailed(
                "Session services 尚未完成初始化".to_string(),
            ));
        };
        session_services
            .runtime_surface_update
            .effective_capability_view_for_delivery_runtime(session_id)
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
    Ok(projection.modules)
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

    pub fn with_runtime_visibility(
        mut self,
        session_services_handle: SharedSessionToolServicesHandle,
        session_id: String,
    ) -> Self {
        self.visibility_source = self
            .visibility_source
            .with_runtime(session_services_handle, session_id);
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

    pub fn with_runtime_visibility(
        mut self,
        session_services_handle: SharedSessionToolServicesHandle,
        session_id: String,
    ) -> Self {
        self.visibility_source = self
            .visibility_source
            .with_runtime(session_services_handle, session_id);
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
pub struct WorkspaceModuleCreateParams {
    /// Module kind to materialize. Currently supports `canvas`.
    pub kind: String,
    /// Kind-specific creation payload.
    #[serde(default)]
    #[schemars(schema_with = "json_object_payload_schema")]
    pub input: serde_json::Value,
}

#[derive(Clone)]
pub struct WorkspaceModuleCreateTool {
    canvas_repo: Arc<dyn CanvasRepository>,
    project_id: Uuid,
    vfs: crate::vfs::tools::fs::SharedRuntimeVfs,
    session_services_handle: SharedSessionToolServicesHandle,
    session_id: Option<String>,
}

impl WorkspaceModuleCreateTool {
    pub fn new(
        canvas_repo: Arc<dyn CanvasRepository>,
        project_id: Uuid,
        vfs: crate::vfs::tools::fs::SharedRuntimeVfs,
        session_services_handle: SharedSessionToolServicesHandle,
        session_id: Option<String>,
    ) -> Self {
        Self {
            canvas_repo,
            project_id,
            vfs,
            session_services_handle,
            session_id,
        }
    }

    pub fn with_turn_id(self, _turn_id: impl Into<String>) -> Self {
        self
    }
}

#[async_trait]
impl AgentTool for WorkspaceModuleCreateTool {
    fn name(&self) -> &str {
        "workspace_module_create"
    }

    fn description(&self) -> &str {
        "Create or attach a workspace module instance. Currently supports kind=`canvas`: pass input.canvas_mount_id? + input.title? + input.description?; returns the materialized canvas:{canvas_mount_id} descriptor and exposes its Canvas VFS mount to the current session."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        schema_value::<WorkspaceModuleCreateParams>()
    }

    async fn execute(
        &self,
        _: &str,
        args: serde_json::Value,
        _: CancellationToken,
        _: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let params: WorkspaceModuleCreateParams =
            serde_json::from_value(args).map_err(|error| {
                AgentToolError::InvalidArguments(format!("invalid arguments: {error}"))
            })?;
        let kind = params.kind.trim();
        if kind != "canvas" {
            return Ok(structured_tool_error(
                "unsupported_module_kind",
                format!("workspace_module_create 暂不支持 kind `{kind}`"),
                serde_json::json!({
                    "kind": kind,
                    "supported_kinds": ["canvas"],
                }),
            ));
        }

        let canvas_params: StartCanvasParams =
            serde_json::from_value(params.input).map_err(|error| {
                AgentToolError::InvalidArguments(format!("invalid canvas create input: {error}"))
            })?;
        let (canvas, canvas_result) = create_or_attach_canvas_for_session(
            self.canvas_repo.as_ref(),
            self.project_id,
            &self.vfs,
            &self.session_services_handle,
            self.session_id.as_deref(),
            canvas_params,
        )
        .await?;
        let descriptor = build_workspace_modules(
            &ExtensionRuntimeProjection::default(),
            std::slice::from_ref(&canvas),
        )
        .into_iter()
        .next()
        .ok_or_else(|| {
            AgentToolError::ExecutionFailed(
                "failed to build canvas workspace module descriptor".to_string(),
            )
        })?;
        let module_id = descriptor.summary.module_id.clone();
        let content = format!(
            "created workspace module\nmodule_id={module_id}\ncanvas_id={}\ncanvas_mount_id={}\nvfs_mount={}://\nskill_path={}",
            canvas_result.canvas_id,
            canvas_result.canvas_mount_id,
            canvas_result.vfs_mount_id,
            canvas_result.skill_path
        );
        let details = serde_json::json!({
            "kind": kind,
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
    project_id: Uuid,
    session_id: String,
    agent_id: Option<String>,
    backend: Option<ResolvedInvocationBackend>,
    gateway: Arc<RuntimeGateway>,
    channel_invoker: Arc<ExtensionRuntimeChannelInvoker>,
    visibility_source: WorkspaceModuleVisibilitySource,
    session_services_handle: Option<SharedSessionToolServicesHandle>,
}

impl WorkspaceModuleInvokeTool {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        installation_repo: Arc<dyn ProjectExtensionInstallationRepository>,
        canvas_repo: Arc<dyn CanvasRepository>,
        project_id: Uuid,
        session_id: String,
        agent_id: Option<String>,
        backend: Option<ResolvedInvocationBackend>,
        gateway: Arc<RuntimeGateway>,
        channel_invoker: Arc<ExtensionRuntimeChannelInvoker>,
    ) -> Self {
        Self {
            installation_repo,
            canvas_repo,
            project_id,
            session_id,
            agent_id,
            backend,
            gateway,
            channel_invoker,
            visibility_source: WorkspaceModuleVisibilitySource::default(),
            session_services_handle: None,
        }
    }

    pub fn with_runtime_visibility(
        mut self,
        session_services_handle: SharedSessionToolServicesHandle,
    ) -> Self {
        self.session_services_handle = Some(session_services_handle.clone());
        self.visibility_source = self
            .visibility_source
            .with_runtime(session_services_handle, self.session_id.clone());
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
                "当前 session 无可用 backend target（既无 remote backend execution，vfs 也无 default mount backend），无法执行该 operation".to_string(),
                serde_json::json!({}),
            )
        })
    }

    async fn refresh_canvas_mount_for_runtime(
        &self,
        canvas: &Canvas,
    ) -> Result<(), AgentToolError> {
        let Some(handle) = self.session_services_handle.as_ref() else {
            return Ok(());
        };
        let Some(session_services) = handle.get().await else {
            return Err(AgentToolError::ExecutionFailed(
                "Session services 尚未完成初始化，无法刷新 Canvas mount".to_string(),
            ));
        };
        session_services
            .runtime_surface_update
            .expose_canvas_mount(&self.session_id, canvas)
            .await
            .map(|_| ())
            .map_err(AgentToolError::ExecutionFailed)
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
            Err(result) => return Ok(result),
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
                        session_id: self.session_id.clone(),
                        agent_id: self.agent_id.clone(),
                    },
                    RuntimeContext::Session {
                        session_id: self.session_id.clone(),
                        project_id: Some(self.project_id),
                        workspace_id: None,
                    },
                    params.input,
                );
                request.target = Some(RuntimeTarget::Backend {
                    backend_id: backend.backend_id.clone(),
                });
                if let Some(workspace) = backend.workspace.clone() {
                    crate::runtime_gateway::attach_extension_invocation_workspace(
                        &mut request,
                        Some(workspace),
                    );
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
                        session_id: self.session_id.clone(),
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
                    let (canvas, result) = bind_canvas_data_for_project(
                        self.canvas_repo.as_ref(),
                        self.project_id,
                        bind_params,
                    )
                    .await?;
                    self.refresh_canvas_mount_for_runtime(&canvas).await?;
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
/// inject_notification，不新增 PlatformEvent 变体（D2-5）。
/// 无可展示目标（module 不可见 / view_key 不存在）时返回**可操作诊断**结构化错误（R4）。
#[derive(Clone)]
pub struct WorkspaceModulePresentTool {
    installation_repo: Arc<dyn ProjectExtensionInstallationRepository>,
    canvas_repo: Arc<dyn CanvasRepository>,
    project_id: Uuid,
    vfs: crate::vfs::tools::fs::SharedRuntimeVfs,
    session_services_handle: SharedSessionToolServicesHandle,
    session_id: String,
    turn_id: String,
    visibility_source: WorkspaceModuleVisibilitySource,
}

impl WorkspaceModulePresentTool {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        installation_repo: Arc<dyn ProjectExtensionInstallationRepository>,
        canvas_repo: Arc<dyn CanvasRepository>,
        project_id: Uuid,
        vfs: crate::vfs::tools::fs::SharedRuntimeVfs,
        session_services_handle: SharedSessionToolServicesHandle,
        session_id: String,
        turn_id: String,
    ) -> Self {
        let visibility_source = WorkspaceModuleVisibilitySource::default()
            .with_runtime(session_services_handle.clone(), session_id.clone());
        Self {
            installation_repo,
            canvas_repo,
            project_id,
            vfs,
            session_services_handle,
            session_id,
            turn_id,
            visibility_source,
        }
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

        if presentation.renderer_kind == "canvas" {
            expose_existing_canvas_for_session(
                self.canvas_repo.as_ref(),
                self.project_id,
                &module.summary.source,
                &self.vfs,
                &self.session_services_handle,
                Some(&self.session_id),
            )
            .await?;
        }

        let value = serde_json::to_value(&presentation).map_err(|error| {
            AgentToolError::ExecutionFailed(format!(
                "failed to serialize workspace module presentation: {error}"
            ))
        })?;

        let notification = build_present_notification(
            &self.session_id,
            &self.turn_id,
            "workspace_module_presented",
            value.clone(),
        );
        let session_services = self.session_services_handle.get().await.ok_or_else(|| {
            AgentToolError::ExecutionFailed("Session services 尚未完成初始化".to_string())
        })?;
        session_services
            .eventing
            .inject_notification(&self.session_id, notification)
            .await
            .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;

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
        let Some(session_services) = self.session_services_handle.get().await else {
            return;
        };
        let notification = build_present_notification(
            &self.session_id,
            &self.turn_id,
            "workspace_module_present_failed",
            value.clone(),
        );
        if let Err(error) = session_services
            .eventing
            .inject_notification(&self.session_id, notification)
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

    use agentdash_domain::DomainError;
    use agentdash_domain::canvas::Canvas;
    use agentdash_domain::extension_package::ExtensionPackageMetadata;
    use agentdash_domain::shared_library::{
        ExtensionBundleKind, ExtensionBundleRef, ExtensionRuntimeActionDefinition,
        ExtensionRuntimeActionKind, ExtensionTemplatePayload, ProjectExtensionInstallation,
        ProjectExtensionInstallationRepository,
    };
    use agentdash_domain::workflow::{
        AgentFrame, AgentFrameRepository, AgentSource, LifecycleAgent, LifecycleAgentRepository,
        RuntimeSessionExecutionAnchor, RuntimeSessionExecutionAnchorRepository,
    };
    use agentdash_spi::connector::RuntimeToolProvider;
    use agentdash_spi::hooks::{
        ActiveWorkflowMeta, AgentFrameHookEvaluationQuery, AgentFrameHookRefreshQuery,
        AgentFrameHookSnapshot, AgentFrameHookSnapshotQuery, ExecutionHookProvider, HookResolution,
        SessionSnapshotMetadata,
    };
    use agentdash_spi::platform::tool_capability::CAP_WORKSPACE_MODULE;
    use agentdash_spi::{
        AgentConfig, AgentConnector, CapabilityState, ConnectorError, ExecutionContext,
        ExecutionSessionFrame, ExecutionTurnFrame, PromptPayload, ToolCapability, ToolCluster,
        ToolDefinition, WorkspaceModuleDimension, WorkspaceModuleVisibilityMode,
    };
    use futures::stream;
    use tokio::sync::RwLock;

    use super::*;
    use crate::agent_run::frame::surface::{AgentFrameSurfaceExt, FrameSurfaceDraft};
    use crate::agent_run::runtime_capability::project_capability_state_from_frame;
    use crate::agent_run::{
        AgentRunEffectiveCapabilityService, AgentRunRuntimeSurface, AgentRunRuntimeSurfaceClosure,
        AgentRunRuntimeSurfaceProvenance, AgentRunRuntimeSurfaceQueryError,
        AgentRunRuntimeSurfaceQueryPort, AgentRunRuntimeSurfaceUpdateDeps,
        AgentRunRuntimeSurfaceUpdateService, AgentRunRuntimeSurfaceWithBackend,
        RuntimeSurfaceQueryPurpose, frame::builder::AgentFrameBuilder,
    };
    use crate::canvas::build_canvas;
    use crate::lifecycle::AgentRunRuntimeAddress;
    use crate::runtime_tools::{
        SessionToolServices, SharedRuntimeGatewayHandle, SharedSessionToolServicesHandle,
    };
    use crate::session::construction::{
        ConstructionResolutionPlan, OwnerResolutionTrace, ResolvedSessionOwner,
        RuntimeContextInspectionPlan,
    };
    use crate::session::hub::SessionRuntimeInner;
    use crate::session::{
        AgentFrameRuntimeTarget, MemorySessionPersistence, UserPromptInput, local_workspace_vfs,
    };
    use crate::test_support::{
        MemoryAgentFrameRepository, MemoryLifecycleAgentRepository, MemoryLifecycleGateRepository,
        MemoryRuntimeSessionExecutionAnchorRepository,
    };
    use crate::vfs::{CanvasFsMountProvider, MountProviderRegistry, VfsService};
    use crate::workspace_module::WorkspaceModuleRuntimeToolProvider;

    struct TestRuntimeSurfaceQuery {
        frame_repo: Arc<MemoryAgentFrameRepository>,
        run_id: Uuid,
        project_id: Uuid,
        agent_id: Uuid,
    }

    #[async_trait]
    impl AgentRunRuntimeSurfaceQueryPort for TestRuntimeSurfaceQuery {
        async fn current_runtime_surface(
            &self,
            runtime_session_id: &str,
            _purpose: RuntimeSurfaceQueryPurpose,
        ) -> Result<AgentRunRuntimeSurface, AgentRunRuntimeSurfaceQueryError> {
            let frame = self
                .frame_repo
                .get_current(self.agent_id)
                .await
                .map_err(|error| AgentRunRuntimeSurfaceQueryError::Repository {
                    purpose: RuntimeSurfaceQueryPurpose::new("test_workspace_module"),
                    operation: "current AgentFrame",
                    message: error.to_string(),
                })?
                .expect("test current frame should exist");
            let capability_state = project_capability_state_from_frame(&frame);
            let vfs = capability_state.vfs.active.clone().unwrap_or_default();
            Ok(AgentRunRuntimeSurface {
                runtime_session_id: runtime_session_id.to_string(),
                run_id: self.run_id,
                project_id: self.project_id,
                agent_id: self.agent_id,
                runtime_address: AgentRunRuntimeAddress {
                    run_id: self.run_id,
                    agent_id: self.agent_id,
                    frame_id: frame.id,
                },
                surface_frame_id: frame.id,
                surface_revision: frame.revision,
                capability_state,
                vfs,
                mcp_servers: frame.typed_mcp_servers(),
                runtime_backend_anchor: None,
                active_turn_id: None,
                identity: None,
                provenance: AgentRunRuntimeSurfaceProvenance {
                    launch_frame_id: frame.id,
                    launch_created_by_kind: frame.created_by_kind.clone(),
                    surface_frame_id: frame.id,
                    surface_revision: frame.revision,
                    surface_created_by_kind: frame.created_by_kind.clone(),
                    anchor_updated_at: chrono::Utc::now(),
                    orchestration_id: None,
                    node_path: None,
                    node_attempt: None,
                },
                closure: AgentRunRuntimeSurfaceClosure {
                    capability_field_present: frame.effective_capability_json.is_some(),
                    vfs_field_present: frame.vfs_surface_json.is_some(),
                    mcp_field_present: frame.mcp_surface_json.is_some(),
                },
            })
        }

        async fn current_runtime_surface_with_backend(
            &self,
            runtime_session_id: &str,
            purpose: RuntimeSurfaceQueryPurpose,
        ) -> Result<AgentRunRuntimeSurfaceWithBackend, AgentRunRuntimeSurfaceQueryError> {
            let surface = self
                .current_runtime_surface(runtime_session_id, purpose.clone())
                .await?;
            let runtime_backend_anchor =
                surface.runtime_backend_anchor.clone().ok_or_else(|| {
                    AgentRunRuntimeSurfaceQueryError::MissingRuntimeBackendAnchor {
                        purpose,
                        runtime_session_id: runtime_session_id.to_string(),
                        turn_id: None,
                    }
                })?;
            Ok(AgentRunRuntimeSurfaceWithBackend {
                surface,
                runtime_backend_anchor,
            })
        }
    }

    fn runtime_surface_update_for_test(
        hub: &SessionRuntimeInner,
        frame_repo: Arc<MemoryAgentFrameRepository>,
        run_id: Uuid,
        project_id: Uuid,
        agent_id: Uuid,
        vfs_service: Option<Arc<VfsService>>,
    ) -> AgentRunRuntimeSurfaceUpdateService {
        AgentRunRuntimeSurfaceUpdateService::new(AgentRunRuntimeSurfaceUpdateDeps {
            surface_query: Arc::new(TestRuntimeSurfaceQuery {
                frame_repo: frame_repo.clone(),
                run_id,
                project_id,
                agent_id,
            }),
            frame_repo,
            vfs_service,
            active_adopter: Arc::new(hub.clone()),
            extra_skill_dirs: Vec::new(),
            skill_discovery_providers: Vec::new(),
        })
    }

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
    struct PendingConnector;

    #[async_trait]
    impl AgentConnector for PendingConnector {
        fn connector_id(&self) -> &'static str {
            "pending"
        }

        fn connector_type(&self) -> agentdash_spi::ConnectorType {
            agentdash_spi::ConnectorType::LocalExecutor
        }

        fn capabilities(&self) -> agentdash_spi::ConnectorCapabilities {
            agentdash_spi::ConnectorCapabilities::default()
        }

        fn list_executors(&self) -> Vec<agentdash_spi::AgentInfo> {
            Vec::new()
        }

        async fn discover_options_stream(
            &self,
            _executor: &str,
            _working_dir: Option<PathBuf>,
        ) -> Result<futures::stream::BoxStream<'static, json_patch::Patch>, ConnectorError>
        {
            Ok(Box::pin(stream::empty()))
        }

        async fn prompt(
            &self,
            _session_id: &str,
            _follow_up_session_id: Option<&str>,
            _prompt: &PromptPayload,
            _context: agentdash_spi::ExecutionContext,
        ) -> Result<agentdash_spi::ExecutionStream, ConnectorError> {
            Ok(Box::pin(stream::pending()))
        }

        async fn cancel(&self, _session_id: &str) -> Result<(), ConnectorError> {
            Ok(())
        }

        async fn approve_tool_call(
            &self,
            _session_id: &str,
            _tool_call_id: &str,
        ) -> Result<(), ConnectorError> {
            Ok(())
        }

        async fn reject_tool_call(
            &self,
            _session_id: &str,
            _tool_call_id: &str,
            _reason: Option<String>,
        ) -> Result<(), ConnectorError> {
            Ok(())
        }
    }

    struct EmptyHookProvider {
        active_run_id: Uuid,
    }

    impl EmptyHookProvider {
        fn snapshot(&self, session_id: String) -> AgentFrameHookSnapshot {
            AgentFrameHookSnapshot {
                runtime_adapter_session_id: session_id,
                metadata: Some(SessionSnapshotMetadata {
                    active_workflow: Some(ActiveWorkflowMeta {
                        run_id: Some(self.active_run_id),
                        ..ActiveWorkflowMeta::default()
                    }),
                    ..SessionSnapshotMetadata::default()
                }),
                ..AgentFrameHookSnapshot::default()
            }
        }
    }

    #[async_trait]
    impl ExecutionHookProvider for EmptyHookProvider {
        async fn load_frame_snapshot(
            &self,
            query: AgentFrameHookSnapshotQuery,
        ) -> Result<AgentFrameHookSnapshot, agentdash_spi::hooks::HookError> {
            Ok(self.snapshot(query.provenance.runtime_session_id.unwrap_or_default()))
        }

        async fn refresh_frame_snapshot(
            &self,
            query: AgentFrameHookRefreshQuery,
        ) -> Result<AgentFrameHookSnapshot, agentdash_spi::hooks::HookError> {
            Ok(self.snapshot(query.provenance.runtime_session_id.unwrap_or_default()))
        }

        async fn evaluate_frame_hook(
            &self,
            _query: AgentFrameHookEvaluationQuery,
        ) -> Result<HookResolution, agentdash_spi::hooks::HookError> {
            Ok(HookResolution::default())
        }
    }

    fn prompt_construction(
        session_id: &str,
        project_id: Uuid,
        working_dir: &std::path::Path,
    ) -> RuntimeContextInspectionPlan {
        let user_input = UserPromptInput {
            executor_config: Some(agentdash_spi::AgentConfig::new("PI_AGENT")),
            ..UserPromptInput::from_text("present workspace module")
        };
        let owner = ResolvedSessionOwner {
            owner_type: agentdash_spi::CapabilityScope::Project,
            project_id: Some(project_id),
            trace: OwnerResolutionTrace {
                selected_reason: "test".to_string(),
            },
        };
        let mut construction =
            RuntimeContextInspectionPlan::from_source_input(session_id, owner, &user_input);
        let vfs = local_workspace_vfs(working_dir);
        let mut capability_state =
            CapabilityState::from_clusters([agentdash_spi::ToolCluster::WorkspaceModule]);
        capability_state.workspace_module = WorkspaceModuleDimension::all();
        capability_state.vfs.active = Some(vfs.clone());
        construction.workspace.working_directory = Some(working_dir.to_path_buf());
        construction.execution_profile.executor_config = user_input.executor_config;
        construction.surface.vfs = Some(vfs.clone());
        construction.projections.frame_surface_draft = Some(FrameSurfaceDraft {
            capability_state: Some(capability_state),
            vfs: Some(vfs),
            mcp_servers: Vec::new(),
            context_bundle_summary: None,
            execution_profile: construction.execution_profile.executor_config.clone(),
        });
        construction.resolution = ConstructionResolutionPlan {
            vfs_source: Some("test.local_workspace_vfs".to_string()),
            mcp_source: Some("test.empty".to_string()),
            capability_source: Some("test.capability_state".to_string()),
            executor_source: Some("test.executor_config".to_string()),
            working_directory_source: Some("test.working_dir".to_string()),
            pending_overlay_applied: false,
            runtime_base_capability_state: None,
        };
        construction
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
        let canvas = build_canvas(
            project_id,
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
        AgentRunEffectiveCapabilityService::effective_view_from_state(
            AgentFrameRuntimeTarget {
                frame_id: Uuid::new_v4(),
                delivery_runtime_session_id: "session-test".to_string(),
            },
            capability_state,
            runtime_refs,
        )
    }

    #[tokio::test]
    async fn list_returns_extension_and_canvas_summaries() {
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
        assert_eq!(modules.len(), 2);
        // 摘要不含完整 schema：summary 没有 operations 字段，只有 operation_summary
        assert!(modules[0].get("operations").is_none());
        assert!(modules[0].get("operation_summary").is_some());
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
    async fn create_canvas_without_runtime_session_returns_diagnostic() {
        let project_id = Uuid::new_v4();
        let canvas_repo = Arc::new(FakeCanvasRepo::default());
        let shared_vfs =
            crate::vfs::tools::fs::SharedRuntimeVfs::new(agentdash_spi::Vfs::default());
        let tool = WorkspaceModuleCreateTool::new(
            canvas_repo,
            project_id,
            shared_vfs.clone(),
            SharedSessionToolServicesHandle::default(),
            None,
        );

        let result = tool
            .execute(
                "t",
                serde_json::json!({
                    "kind": "canvas",
                    "input": {
                        "canvas_id": "sales-board",
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
                    if message.contains("Session services")
            ),
            "Canvas expose must fail explicitly without a runtime session, got {result:?}"
        );
    }

    #[tokio::test]
    async fn create_canvas_runtime_grant_extends_allowlist_session_visibility() {
        let (install_repo, canvas_repo, project_id) = fixtures().await;

        let mut registry = MountProviderRegistry::new();
        registry.register(Arc::new(CanvasFsMountProvider::new(canvas_repo.clone())));
        let vfs_service = Arc::new(VfsService::new(Arc::new(registry)));
        let base = tempfile::tempdir().expect("tempdir");
        let active_run_id = Uuid::new_v4();
        let frame_repo = Arc::new(MemoryAgentFrameRepository::default());
        let frame = AgentFrame::new_initial(Uuid::new_v4());
        let frame_id = frame.id;
        let agent_id = frame.agent_id;
        frame_repo.create(&frame).await.expect("frame should save");
        let gate_repo = Arc::new(MemoryLifecycleGateRepository::default());
        let agent_repo = Arc::new(MemoryLifecycleAgentRepository::default());
        let anchor_repo = Arc::new(MemoryRuntimeSessionExecutionAnchorRepository::default());
        let mut agent =
            LifecycleAgent::new_root(active_run_id, Uuid::new_v4(), AgentSource::Unknown);
        agent.id = agent_id;
        agent_repo.create(&agent).await.expect("agent should save");
        let hub = SessionRuntimeInner::new_with_hooks_and_persistence(
            Arc::new(PendingConnector),
            Some(Arc::new(EmptyHookProvider { active_run_id })),
            Arc::new(MemorySessionPersistence::default()),
        )
        .with_vfs_service(vfs_service)
        .with_agent_frame_repo(frame_repo.clone())
        .with_lifecycle_gate_repo(gate_repo)
        .with_lifecycle_agent_repo(agent_repo)
        .with_execution_anchor_repo(anchor_repo.clone());
        let session = hub
            .create_session("create-workspace-module")
            .await
            .expect("session should create");
        anchor_repo
            .upsert(&RuntimeSessionExecutionAnchor::new_dispatch(
                &session.id,
                active_run_id,
                frame_id,
                agent_id,
            ))
            .await
            .expect("runtime anchor should save");
        hub.ensure_session(&session.id).await;
        let turn_id = hub
            .start_prompt(
                &session.id,
                prompt_construction(&session.id, project_id, base.path()),
            )
            .await
            .expect("prompt should start");
        let stale_runtime = hub
            .hook_service()
            .ensure_hook_runtime_for_target(
                &crate::session::types::AgentFrameRuntimeTarget {
                    frame_id,
                    delivery_runtime_session_id: session.id.clone(),
                },
                Some(&turn_id),
            )
            .await
            .expect("hook runtime should reload")
            .expect("hook runtime should exist");
        let stale_target = stale_runtime.control_target();
        let mut frame_state =
            CapabilityState::from_clusters([agentdash_spi::ToolCluster::WorkspaceModule]);
        frame_state.workspace_module = WorkspaceModuleDimension {
            mode: WorkspaceModuleVisibilityMode::Allowlist,
            allowed_module_ids: vec!["ext:demo".to_string()],
        };
        frame_state.vfs.active = Some(local_workspace_vfs(base.path()));
        let switched_frame = AgentFrameBuilder::new(agent_id)
            .with_capability_state(&frame_state)
            .with_created_by("test_frame_switch", Some("canvas_create".to_string()))
            .build(frame_repo.as_ref())
            .await
            .expect("test frame switch should save");
        assert_ne!(
            stale_target.frame_id, switched_frame.id,
            "test setup should leave the cached hook runtime on a stale frame"
        );

        let handle = SharedSessionToolServicesHandle::default();
        handle
            .set(SessionToolServices {
                core: hub.core_service(),
                eventing: hub.eventing_service(),
                control: hub.control_service(),
                launch: hub.launch_service(),
                hooks: hub.hook_service(),
                runtime_transition: hub.runtime_transition_service(),
                runtime_surface_update: runtime_surface_update_for_test(
                    &hub,
                    frame_repo.clone(),
                    active_run_id,
                    project_id,
                    agent_id,
                    None,
                ),
            })
            .await;

        let shared_vfs =
            crate::vfs::tools::fs::SharedRuntimeVfs::new(local_workspace_vfs(base.path()));
        let create_tool = WorkspaceModuleCreateTool::new(
            canvas_repo.clone(),
            project_id,
            shared_vfs,
            handle.clone(),
            Some(session.id.clone()),
        )
        .with_turn_id(turn_id.clone());

        let result = create_tool
            .execute(
                "tool-create",
                serde_json::json!({
                    "kind": "canvas",
                    "input": {
                        "canvas_id": "sales-board",
                        "title": "Sales Board",
                        "description": "test canvas"
                    }
                }),
                CancellationToken::new(),
                None,
            )
            .await
            .expect("workspace_module_create should succeed");
        assert!(!result.is_error, "expected success, got {result:?}");
        assert_eq!(
            result
                .details
                .as_ref()
                .and_then(|details| details.get("module_id"))
                .and_then(serde_json::Value::as_str),
            Some("canvas:cvs-sales-board")
        );
        assert!(
            result
                .details
                .as_ref()
                .and_then(|details| details.get("presentation"))
                .is_none(),
            "workspace_module_create should register the Canvas without presenting it"
        );

        let updated_frame = frame_repo
            .get_current(agent_id)
            .await
            .expect("frame query should succeed")
            .expect("frame should exist");
        let refreshed_runtime = hub
            .hook_service()
            .get_hook_runtime_for_target(&crate::session::types::AgentFrameRuntimeTarget {
                frame_id: updated_frame.id,
                delivery_runtime_session_id: session.id.clone(),
            })
            .await
            .expect("hook runtime lookup should succeed")
            .expect("hook runtime should exist for updated frame");
        assert_eq!(
            refreshed_runtime.control_target().frame_id,
            updated_frame.id,
            "workspace_module_create should align hook runtime to the AgentFrame revision produced by Canvas capability sync"
        );
        assert_ne!(
            refreshed_runtime.control_target().frame_id,
            stale_target.frame_id,
            "workspace_module_create should not keep using the stale cached hook runtime target"
        );
        assert_eq!(
            updated_frame.visible_workspace_module_refs(),
            vec!["canvas:cvs-sales-board".to_string()]
        );

        let state = hub
            .get_current_capability_state(&session.id)
            .await
            .expect("current capability state should exist");
        let active_vfs = state.vfs.active.expect("active VFS should exist");
        assert!(
            active_vfs
                .mounts
                .iter()
                .any(|mount| mount.id == "cvs-sales-board")
        );
        let events = hub
            .eventing_service()
            .list_event_page(&session.id, 0, 100)
            .await
            .expect("events should list")
            .events;
        events
            .iter()
            .position(|event| {
                matches!(
                    &event.notification.event,
                    agentdash_agent_protocol::BackboneEvent::Platform(
                        agentdash_agent_protocol::PlatformEvent::SessionMetaUpdate { key, value }
                    ) if key == "context_frame"
                        && value.get("kind").and_then(|v| v.as_str()) == Some("capability_state_delta")
                )
            })
            .expect("should write capability_state_delta context_frame");
        assert!(
            events.iter().all(|event| {
                !matches!(
                    &event.notification.event,
                    agentdash_agent_protocol::BackboneEvent::Platform(
                        agentdash_agent_protocol::PlatformEvent::SessionMetaUpdate { key, .. }
                    ) if key == "workspace_module_presented"
                )
            }),
            "workspace_module_create should not open the Canvas tab"
        );

        let describe_tool = WorkspaceModuleDescribeTool::new(install_repo, canvas_repo, project_id)
            .with_runtime_visibility(handle, session.id.clone());
        let describe = describe_tool
            .execute(
                "tool-describe",
                serde_json::json!({"module_id": "canvas:cvs-sales-board"}),
                CancellationToken::new(),
                None,
            )
            .await
            .expect("workspace_module_describe should succeed");
        assert!(
            !describe.is_error,
            "runtime grant should make created Canvas visible through allowlist describe"
        );
        assert_eq!(
            describe
                .details
                .and_then(|details| details.pointer("/summary/module_id").cloned())
                .and_then(|value| value.as_str().map(str::to_string)),
            Some("canvas:cvs-sales-board".to_string())
        );
    }

    #[tokio::test]
    async fn canvas_module_present_refreshes_session_exposure_before_event() {
        let (install_repo, canvas_repo, project_id) = fixtures().await;

        let mut registry = MountProviderRegistry::new();
        registry.register(Arc::new(CanvasFsMountProvider::new(canvas_repo.clone())));
        let vfs_service = Arc::new(VfsService::new(Arc::new(registry)));
        let base = tempfile::tempdir().expect("tempdir");
        let active_run_id = Uuid::new_v4();
        let frame_repo = Arc::new(MemoryAgentFrameRepository::default());
        let frame = AgentFrame::new_initial(Uuid::new_v4());
        let frame_id = frame.id;
        let agent_id = frame.agent_id;
        frame_repo.create(&frame).await.expect("frame should save");
        let gate_repo = Arc::new(MemoryLifecycleGateRepository::default());
        let agent_repo = Arc::new(MemoryLifecycleAgentRepository::default());
        let anchor_repo = Arc::new(MemoryRuntimeSessionExecutionAnchorRepository::default());
        let mut agent =
            LifecycleAgent::new_root(active_run_id, Uuid::new_v4(), AgentSource::Unknown);
        agent.id = agent_id;
        agent_repo.create(&agent).await.expect("agent should save");
        let hub = SessionRuntimeInner::new_with_hooks_and_persistence(
            Arc::new(PendingConnector),
            Some(Arc::new(EmptyHookProvider { active_run_id })),
            Arc::new(MemorySessionPersistence::default()),
        )
        .with_vfs_service(vfs_service)
        .with_agent_frame_repo(frame_repo.clone())
        .with_lifecycle_gate_repo(gate_repo)
        .with_lifecycle_agent_repo(agent_repo)
        .with_execution_anchor_repo(anchor_repo.clone());
        let session = hub
            .create_session("present-workspace-module")
            .await
            .expect("session should create");
        anchor_repo
            .upsert(&RuntimeSessionExecutionAnchor::new_dispatch(
                &session.id,
                active_run_id,
                frame_id,
                agent_id,
            ))
            .await
            .expect("runtime anchor should save");
        hub.ensure_session(&session.id).await;
        let turn_id = hub
            .start_prompt(
                &session.id,
                prompt_construction(&session.id, project_id, base.path()),
            )
            .await
            .expect("prompt should start");
        let stale_runtime = hub
            .hook_service()
            .ensure_hook_runtime_for_target(
                &crate::session::types::AgentFrameRuntimeTarget {
                    frame_id,
                    delivery_runtime_session_id: session.id.clone(),
                },
                Some(&turn_id),
            )
            .await
            .expect("hook runtime should reload")
            .expect("hook runtime should exist");
        let stale_target = stale_runtime.control_target();
        let mut frame_state =
            CapabilityState::from_clusters([agentdash_spi::ToolCluster::WorkspaceModule]);
        frame_state.workspace_module = WorkspaceModuleDimension::all();
        frame_state.vfs.active = Some(local_workspace_vfs(base.path()));
        let switched_frame = AgentFrameBuilder::new(agent_id)
            .with_capability_state(&frame_state)
            .with_created_by("test_frame_switch", Some("canvas_present".to_string()))
            .build(frame_repo.as_ref())
            .await
            .expect("test frame switch should save");
        assert_ne!(
            stale_target.frame_id, switched_frame.id,
            "test setup should leave the cached hook runtime on a stale frame"
        );

        let handle = SharedSessionToolServicesHandle::default();
        handle
            .set(SessionToolServices {
                core: hub.core_service(),
                eventing: hub.eventing_service(),
                control: hub.control_service(),
                launch: hub.launch_service(),
                hooks: hub.hook_service(),
                runtime_transition: hub.runtime_transition_service(),
                runtime_surface_update: runtime_surface_update_for_test(
                    &hub,
                    frame_repo.clone(),
                    active_run_id,
                    project_id,
                    agent_id,
                    None,
                ),
            })
            .await;

        let shared_vfs =
            crate::vfs::tools::fs::SharedRuntimeVfs::new(local_workspace_vfs(base.path()));
        let present_tool = WorkspaceModulePresentTool::new(
            install_repo,
            canvas_repo,
            project_id,
            shared_vfs,
            handle,
            session.id.clone(),
            turn_id,
        );

        present_tool
            .execute(
                "tool-present",
                serde_json::json!({
                    "module_id": "canvas:cvs-dashboard-a",
                    "view_key": "preview"
                }),
                CancellationToken::new(),
                None,
            )
            .await
            .expect("workspace_module_present should succeed");

        let updated_frame = frame_repo
            .get_current(agent_id)
            .await
            .expect("frame query should succeed")
            .expect("frame should exist");
        let refreshed_runtime = hub
            .hook_service()
            .get_hook_runtime_for_target(&crate::session::types::AgentFrameRuntimeTarget {
                frame_id: updated_frame.id,
                delivery_runtime_session_id: session.id.clone(),
            })
            .await
            .expect("hook runtime lookup should succeed")
            .expect("hook runtime should exist for updated frame");
        assert_eq!(
            refreshed_runtime.control_target().frame_id,
            updated_frame.id,
            "workspace_module_present should align hook runtime to the AgentFrame revision produced by Canvas capability sync"
        );
        assert_ne!(
            refreshed_runtime.control_target().frame_id,
            stale_target.frame_id,
            "workspace_module_present should not keep using the stale cached hook runtime target"
        );
        assert_eq!(
            updated_frame.visible_canvas_mount_ids(),
            vec!["cvs-dashboard-a".to_string()]
        );
        assert_eq!(
            updated_frame.visible_workspace_module_refs(),
            vec!["canvas:cvs-dashboard-a".to_string()]
        );

        let state = hub
            .get_current_capability_state(&session.id)
            .await
            .expect("current capability state should exist");
        let active_vfs = state.vfs.active.expect("active VFS should exist");
        assert!(
            active_vfs
                .mounts
                .iter()
                .any(|mount| mount.id == "cvs-dashboard-a")
        );
        assert!(
            state
                .skill
                .skills
                .iter()
                .all(|skill| skill.file_path != "cvs-dashboard-a://skills/canvas-system/SKILL.md"),
            "canvas-system must not be discovered from the Canvas VFS mount"
        );

        let events = hub
            .eventing_service()
            .list_event_page(&session.id, 0, 100)
            .await
            .expect("events should list")
            .events;
        let capability_index = events
            .iter()
            .position(|event| {
                matches!(
                    &event.notification.event,
                    agentdash_agent_protocol::BackboneEvent::Platform(
                        agentdash_agent_protocol::PlatformEvent::SessionMetaUpdate { key, value }
                    ) if key == "context_frame"
                        && value.get("kind").and_then(|v| v.as_str()) == Some("capability_state_delta")
                )
            })
            .expect("should write capability_state_delta context_frame");
        let presented = events
            .iter()
            .enumerate()
            .find_map(|(index, event)| {
                let agentdash_agent_protocol::BackboneEvent::Platform(
                    agentdash_agent_protocol::PlatformEvent::SessionMetaUpdate { key, value },
                ) = &event.notification.event
                else {
                    return None;
                };
                if key == "workspace_module_presented" {
                    Some((index, value))
                } else {
                    None
                }
            })
            .expect("should write workspace_module_presented event");
        assert!(capability_index < presented.0);
        assert_eq!(
            presented
                .1
                .get("presentation_uri")
                .and_then(serde_json::Value::as_str),
            Some("canvas://cvs-dashboard-a")
        );
    }

    // ---- invoke tool tests ----

    use crate::runtime_gateway::{RuntimeActionKind, RuntimeInvocationOutput, RuntimeProvider};
    use agentdash_application_ports::extension_runtime::{
        ExtensionChannelInvokeRequest, ExtensionChannelInvokeResponse,
        ExtensionRuntimeActionTransportError, ExtensionRuntimeChannelTransport,
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
            project_id,
            "session-1".to_string(),
            None,
            backend,
            gateway,
            channel_invoker,
        )
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
        let mut vfs = local_workspace_vfs(&working_directory);
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
        let gateway_handle = SharedRuntimeGatewayHandle::default();
        gateway_handle.set(gateway).await;
        let provider = WorkspaceModuleRuntimeToolProvider::new(
            install_repo,
            canvas_repo,
            SharedSessionToolServicesHandle::default(),
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
        let provider = WorkspaceModuleRuntimeToolProvider::new(
            install_repo,
            canvas_repo,
            SharedSessionToolServicesHandle::default(),
            SharedRuntimeGatewayHandle::default(),
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
        let gateway_handle = SharedRuntimeGatewayHandle::default();
        gateway_handle
            .set(Arc::new(RuntimeGateway::new().with_provider(Arc::new(
                EchoActionProvider {
                    action_key: RuntimeActionKey::parse("demo.profile").expect("valid action key"),
                    invoke_count: Arc::new(AtomicUsize::new(0)),
                },
            ))))
            .await;
        let provider = WorkspaceModuleRuntimeToolProvider::new(
            install_repo,
            canvas_repo,
            SharedSessionToolServicesHandle::default(),
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
            ("workspace_module_create", "input"),
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
        let tool = invoke_tool_with_backend(install_repo, canvas_repo.clone(), project_id, None);
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
        let binding = saved
            .bindings
            .iter()
            .find(|binding| binding.alias == "stats")
            .expect("binding should be saved");
        assert_eq!(binding.source_uri, "project://data/stats.csv");
        assert_eq!(binding.content_type, "text/csv");
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
