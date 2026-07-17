//! Workspace Module Agent 工具：`workspace_module_list` / `workspace_module_describe`。
//!
//! 二者由 session runtime tool composer 通过 workspace-module provider 装配，
//! 用当前 project context + repos 现取现算：每次调用拉 enabled installations + Canvas 候选，
//! 先按 AgentRun effective capability view 过滤，再按当前用户 Canvas access 重投影。

use std::sync::Arc;

use agentdash_application_ports::agent_run_runtime::AgentRunRuntimeBindingRepository;
use agentdash_application_ports::agent_run_surface::AgentRunEffectiveCapabilityView;
use agentdash_application_runtime_gateway::{
    ExtensionRuntimeBackendServiceInvokeResult, ExtensionRuntimeBackendServiceInvoker,
    ExtensionRuntimeChannelInvokeResult, ExtensionRuntimeChannelInvoker, RuntimeGateway,
    RuntimeInvocationResult,
};
use agentdash_application_vfs::tools::SharedRuntimeVfs;
use agentdash_contracts::workspace_module::{
    WorkspaceModuleDescriptor, WorkspaceModuleOperationReadiness,
    WorkspaceModuleOperationReadinessKind,
};
#[cfg(test)]
use agentdash_domain::canvas::CanvasScope;
use agentdash_domain::canvas::{CanvasRepository, CanvasRuntimeStateRepository};
use agentdash_domain::project::{ProjectAuthorizationContext, ProjectRepository};
use agentdash_domain::shared_library::ProjectExtensionInstallationRepository;
use agentdash_spi::context::tool_schema_sanitizer::schema_value;
use agentdash_spi::{AgentTool, AgentToolError, AgentToolResult, ContentPart, ToolUpdateCallback};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::workspace_module::runtime_bridge::{
    SharedWorkspaceModuleAgentRunBridgeHandle, SharedWorkspaceModulePresentationAppendHandle,
    SharedWorkspaceModuleRuntimeGatewayHandle,
};
use crate::workspace_module::{
    ResolvedInvocationBackend, WorkspaceModuleAgentSurface, WorkspaceModuleAgentSurfaceCommand,
    WorkspaceModuleCanvasBindingResult, WorkspaceModuleCommandDiagnostic,
    WorkspaceModuleInvokeCommand, WorkspaceModuleOperateCommand, WorkspaceModuleOperationOutcome,
    WorkspaceModuleOperationRuntimeSource, WorkspaceModulePresentCommand,
    WorkspaceModuleResolveContext, WorkspaceModuleRuntimeContext, WorkspaceModuleSurfaceError,
    WorkspaceModuleVisibilitySource,
};

fn surface_error_to_tool_error(error: WorkspaceModuleSurfaceError) -> AgentToolError {
    match error {
        WorkspaceModuleSurfaceError::InvalidArguments(message) => {
            AgentToolError::InvalidArguments(message)
        }
        WorkspaceModuleSurfaceError::ExecutionFailed(message) => {
            AgentToolError::ExecutionFailed(message)
        }
    }
}

async fn resolve_surface_modules_for_adapter(
    installation_repo: &Arc<dyn ProjectExtensionInstallationRepository>,
    canvas_repo: &Arc<dyn CanvasRepository>,
    project_id: Uuid,
    visibility_source: &WorkspaceModuleVisibilitySource,
    operation_runtime_source: &WorkspaceModuleOperationRuntimeSource,
) -> Result<Vec<WorkspaceModuleDescriptor>, AgentToolError> {
    Ok(
        WorkspaceModuleAgentSurface::resolve(WorkspaceModuleResolveContext {
            installation_repo,
            canvas_repo,
            project_id,
            visibility_source,
            operation_runtime_source,
        })
        .await
        .map_err(surface_error_to_tool_error)?
        .modules,
    )
}

#[derive(Clone)]
pub struct WorkspaceModuleListTool {
    installation_repo: Arc<dyn ProjectExtensionInstallationRepository>,
    canvas_repo: Arc<dyn CanvasRepository>,
    project_id: Uuid,
    visibility_source: WorkspaceModuleVisibilitySource,
    operation_runtime_source: WorkspaceModuleOperationRuntimeSource,
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
            operation_runtime_source: WorkspaceModuleOperationRuntimeSource::default(),
        }
    }

    pub fn with_agent_run_visibility(
        mut self,
        agent_run_bridge_handle: SharedWorkspaceModuleAgentRunBridgeHandle,
        runtime_thread_id: String,
    ) -> Self {
        self.visibility_source = self
            .visibility_source
            .with_agent_run_delivery(agent_run_bridge_handle, runtime_thread_id);
        self
    }

    pub fn with_current_user(mut self, current_user: Option<ProjectAuthorizationContext>) -> Self {
        self.visibility_source = self.visibility_source.with_current_user(current_user);
        self
    }

    pub fn with_effective_capability_view(mut self, view: AgentRunEffectiveCapabilityView) -> Self {
        self.visibility_source = self.visibility_source.with_effective_view(view);
        self
    }

    pub fn with_runtime_dependencies(
        mut self,
        runtime_gateway_handle: SharedWorkspaceModuleRuntimeGatewayHandle,
        runtime_thread_id: String,
        channel_transport_available: bool,
        backend_readiness: WorkspaceModuleOperationReadiness,
        backend_service_readiness: WorkspaceModuleOperationReadiness,
    ) -> Self {
        self.operation_runtime_source = self.operation_runtime_source.with_gateway_handle(
            runtime_gateway_handle,
            runtime_thread_id,
            None,
            channel_transport_available,
            backend_readiness,
            backend_service_readiness,
        );
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
    fn protocol_projector(&self) -> Option<agentdash_spi::ToolProtocolProjector> {
        Some(agentdash_spi::ToolProtocolProjector::Dynamic { namespace: None })
    }

    fn protocol_fixture_id(&self) -> Option<String> {
        Some("main_tool_workspace_module_list_dynamic_lifecycle".to_string())
    }

    async fn execute(
        &self,
        _: &str,
        _: serde_json::Value,
        _: CancellationToken,
        _: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let modules = resolve_surface_modules_for_adapter(
            &self.installation_repo,
            &self.canvas_repo,
            self.project_id,
            &self.visibility_source,
            &self.operation_runtime_source,
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
    operation_runtime_source: WorkspaceModuleOperationRuntimeSource,
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
            operation_runtime_source: WorkspaceModuleOperationRuntimeSource::default(),
        }
    }

    pub fn with_agent_run_visibility(
        mut self,
        agent_run_bridge_handle: SharedWorkspaceModuleAgentRunBridgeHandle,
        runtime_thread_id: String,
    ) -> Self {
        self.visibility_source = self
            .visibility_source
            .with_agent_run_delivery(agent_run_bridge_handle, runtime_thread_id);
        self
    }

    pub fn with_current_user(mut self, current_user: Option<ProjectAuthorizationContext>) -> Self {
        self.visibility_source = self.visibility_source.with_current_user(current_user);
        self
    }

    pub fn with_runtime_dependencies(
        mut self,
        runtime_gateway_handle: SharedWorkspaceModuleRuntimeGatewayHandle,
        runtime_thread_id: String,
        channel_transport_available: bool,
        backend_readiness: WorkspaceModuleOperationReadiness,
        backend_service_readiness: WorkspaceModuleOperationReadiness,
    ) -> Self {
        self.operation_runtime_source = self.operation_runtime_source.with_gateway_handle(
            runtime_gateway_handle,
            runtime_thread_id,
            None,
            channel_transport_available,
            backend_readiness,
            backend_service_readiness,
        );
        self
    }

    pub fn with_effective_capability_view(mut self, view: AgentRunEffectiveCapabilityView) -> Self {
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
        "Describe a single workspace module by module_id. Returns the module's UI entries and Agent-visible operations from the generated operation catalog, including input/output schemas and dispatch metadata."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        schema_value::<WorkspaceModuleDescribeParams>()
    }
    fn protocol_projector(&self) -> Option<agentdash_spi::ToolProtocolProjector> {
        Some(agentdash_spi::ToolProtocolProjector::Dynamic { namespace: None })
    }

    fn protocol_fixture_id(&self) -> Option<String> {
        Some("main_tool_workspace_module_describe_dynamic_lifecycle".to_string())
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

        let modules = resolve_surface_modules_for_adapter(
            &self.installation_repo,
            &self.canvas_repo,
            self.project_id,
            &self.visibility_source,
            &self.operation_runtime_source,
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

#[derive(Clone)]
pub struct WorkspaceModuleOperateTool {
    project_repo: Arc<dyn ProjectRepository>,
    canvas_repo: Arc<dyn CanvasRepository>,
    project_id: Uuid,
    vfs: SharedRuntimeVfs,
    agent_run_bridge_handle: SharedWorkspaceModuleAgentRunBridgeHandle,
    runtime_thread_id: Option<String>,
    current_user: Option<ProjectAuthorizationContext>,
}

impl WorkspaceModuleOperateTool {
    pub fn new(
        project_repo: Arc<dyn ProjectRepository>,
        canvas_repo: Arc<dyn CanvasRepository>,
        project_id: Uuid,
        vfs: SharedRuntimeVfs,
        agent_run_bridge_handle: SharedWorkspaceModuleAgentRunBridgeHandle,
        runtime_thread_id: Option<String>,
    ) -> Self {
        Self {
            project_repo,
            canvas_repo,
            project_id,
            vfs,
            agent_run_bridge_handle,
            runtime_thread_id,
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
    fn protocol_projector(&self) -> Option<agentdash_spi::ToolProtocolProjector> {
        Some(agentdash_spi::ToolProtocolProjector::Dynamic { namespace: None })
    }

    fn protocol_fixture_id(&self) -> Option<String> {
        Some("main_tool_workspace_module_operate_dynamic_lifecycle".to_string())
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
        let runtime_thread_id = self.runtime_thread_id.clone().ok_or_else(|| {
            AgentToolError::ExecutionFailed(
                "workspace_module_operate 缺少 AgentRun delivery runtime id".to_string(),
            )
        })?;
        let runtime_context =
            WorkspaceModuleRuntimeContext::new(self.project_id, runtime_thread_id.clone())
                .with_vfs(self.vfs.clone())
                .with_current_user(self.current_user.clone())
                .with_agent_run_bridge(Some(self.agent_run_bridge_handle.clone()));
        project_operation_outcome(
            WorkspaceModuleAgentSurface::execute(WorkspaceModuleAgentSurfaceCommand::Operate(
                WorkspaceModuleOperateCommand {
                    project_repo: &self.project_repo,
                    canvas_repo: &self.canvas_repo,
                    project_id: self.project_id,
                    runtime_context: &runtime_context,
                    operation,
                    input,
                },
            ))
            .await
            .map_err(surface_error_to_tool_error)?,
        )
    }
}

fn project_operation_outcome(
    outcome: WorkspaceModuleOperationOutcome,
) -> Result<AgentToolResult, AgentToolError> {
    match outcome {
        WorkspaceModuleOperationOutcome::CanvasOperated {
            operation,
            module_id,
            descriptor,
            canvas,
        } => {
            let content = format!(
                "Canvas 操作完成\n\n- 操作：`{operation}`\n- 模块：`{module_id}`\n- Canvas ID：`{}`\n- Canvas mount：`{}`\n- VFS mount：`{}://`\n- 技能：`{}`",
                canvas.canvas_id, canvas.canvas_mount_id, canvas.vfs_mount_id, canvas.skill_path
            );
            let details = serde_json::json!({
                "operation": operation,
                "module_id": module_id,
                "descriptor": descriptor,
                "canvas": canvas,
            });

            Ok(AgentToolResult {
                content: vec![ContentPart::text(content)],
                is_error: false,
                details: Some(details),
            })
        }
        WorkspaceModuleOperationOutcome::RuntimeActionInvoked { result, provenance } => {
            Ok(runtime_action_invocation_to_tool_result(result, provenance))
        }
        WorkspaceModuleOperationOutcome::ProtocolChannelInvoked { result, provenance } => Ok(
            protocol_channel_invocation_to_tool_result(result, provenance),
        ),
        WorkspaceModuleOperationOutcome::BackendServiceInvoked { result, provenance } => Ok(
            backend_service_invocation_to_tool_result(result, provenance),
        ),
        WorkspaceModuleOperationOutcome::CanvasBindingApplied { result, provenance } => {
            Ok(canvas_binding_to_tool_result(result, provenance))
        }
        WorkspaceModuleOperationOutcome::CanvasRuntimeObservationRead {
            canvas_mount_id,
            run_id,
            agent_id,
            observation,
        } => Ok(json_output_tool_result(serde_json::json!({
            "canvas_mount_id": canvas_mount_id,
            "run_id": run_id,
            "agent_id": agent_id,
            "observation": observation,
        }))),
        WorkspaceModuleOperationOutcome::CanvasInteractionSnapshotRead {
            canvas_mount_id,
            run_id,
            agent_id,
            snapshot,
        } => Ok(json_output_tool_result(serde_json::json!({
            "canvas_mount_id": canvas_mount_id,
            "run_id": run_id,
            "agent_id": agent_id,
            "snapshot": snapshot,
        }))),
        WorkspaceModuleOperationOutcome::Presented { presentation } => {
            let value = serde_json::to_value(&presentation).map_err(|error| {
                AgentToolError::ExecutionFailed(format!(
                    "failed to serialize workspace module presentation: {error}"
                ))
            })?;
            Ok(AgentToolResult {
                content: vec![ContentPart::text(format!(
                    "模块展示请求已提交\n\n- 模块：`{}`\n- 视图：`{}`\n- 渲染器：`{}`",
                    presentation.module_id, presentation.view_key, presentation.renderer_kind
                ))],
                is_error: false,
                details: Some(value),
            })
        }
        WorkspaceModuleOperationOutcome::Diagnostic(diagnostic) => {
            Ok(diagnostic_to_tool_result(diagnostic))
        }
    }
}

fn runtime_action_invocation_to_tool_result(
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

fn protocol_channel_invocation_to_tool_result(
    result: ExtensionRuntimeChannelInvokeResult,
    provenance: serde_json::Value,
) -> AgentToolResult {
    let trace = serde_json::to_value(&result.trace).unwrap_or(serde_json::Value::Null);
    let rendered = serde_json::to_string_pretty(&result.output.output)
        .unwrap_or_else(|_| result.output.output.to_string());
    AgentToolResult {
        content: vec![ContentPart::text(rendered)],
        is_error: false,
        details: Some(serde_json::json!({
            "provenance": provenance,
            "runtime_trace": trace,
            "output": result.output.output,
        })),
    }
}

fn backend_service_invocation_to_tool_result(
    result: ExtensionRuntimeBackendServiceInvokeResult,
    provenance: serde_json::Value,
) -> AgentToolResult {
    let trace = serde_json::to_value(&result.trace).unwrap_or(serde_json::Value::Null);
    let service_key = result.metadata.service_key.clone();
    let route = result.metadata.route.clone();
    let status = result.response.as_ref().map(|response| response.status);
    let is_error = result.diagnostic.is_some();
    let rendered = serde_json::to_string_pretty(&result.output.output)
        .unwrap_or_else(|_| result.output.output.to_string());
    AgentToolResult {
        content: vec![ContentPart::text(rendered)],
        is_error,
        details: Some(serde_json::json!({
            "provenance": provenance,
            "runtime_trace": trace,
            "service_key": service_key,
            "route": route,
            "status": status,
            "output": result.output.output,
        })),
    }
}

fn canvas_binding_to_tool_result(
    result: WorkspaceModuleCanvasBindingResult,
    provenance: serde_json::Value,
) -> AgentToolResult {
    let content = format!(
        "canvas_id={}\ncanvas_mount_id={}\nvfs_mount={}://\nalias={}\nsource_uri={}\ncontent_type={}",
        result.canvas_id,
        result.canvas_mount_id,
        result.vfs_mount_id,
        result.alias,
        result.source_uri,
        result.content_type
    );
    AgentToolResult {
        content: vec![ContentPart::text(content)],
        is_error: false,
        details: Some(serde_json::json!({
            "provenance": provenance,
            "output": result,
        })),
    }
}

fn json_output_tool_result(output: serde_json::Value) -> AgentToolResult {
    let rendered = serde_json::to_string_pretty(&output).unwrap_or_else(|_| output.to_string());
    AgentToolResult {
        content: vec![ContentPart::text(rendered)],
        is_error: false,
        details: Some(serde_json::json!({ "output": output })),
    }
}

fn diagnostic_to_tool_result(diagnostic: WorkspaceModuleCommandDiagnostic) -> AgentToolResult {
    let mut details = diagnostic.details;
    if let Some(obj) = details.as_object_mut() {
        obj.insert("error".to_string(), serde_json::json!(diagnostic.code));
        obj.insert(
            "message".to_string(),
            serde_json::json!(diagnostic.message.clone()),
        );
    }
    AgentToolResult {
        content: vec![ContentPart::text(diagnostic.message)],
        is_error: true,
        details: Some(details),
    }
}

fn backend_readiness_for_optional_backend(
    backend: &Option<ResolvedInvocationBackend>,
) -> WorkspaceModuleOperationReadiness {
    if backend.is_some() {
        WorkspaceModuleOperationReadiness::ready()
    } else {
        WorkspaceModuleOperationReadiness::unavailable(
            WorkspaceModuleOperationReadinessKind::BackendUnavailable,
            "runtime backend target is unavailable for this operation",
        )
    }
}

fn backend_service_readiness_for_invoker(
    invoker: Option<&Arc<ExtensionRuntimeBackendServiceInvoker>>,
    backend_readiness: &WorkspaceModuleOperationReadiness,
) -> WorkspaceModuleOperationReadiness {
    if !backend_readiness.is_ready() {
        return backend_readiness.clone();
    }
    if invoker.is_some() {
        WorkspaceModuleOperationReadiness::ready()
    } else {
        WorkspaceModuleOperationReadiness::unavailable(
            WorkspaceModuleOperationReadinessKind::BackendServiceUnavailable,
            "backendService bridge transport is not attached to this runtime",
        )
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
    runtime_binding_repo: Arc<dyn AgentRunRuntimeBindingRepository>,
    project_id: Uuid,
    runtime_thread_id: String,
    agent_id: Option<String>,
    backend: Option<ResolvedInvocationBackend>,
    gateway: Arc<RuntimeGateway>,
    channel_invoker: Arc<ExtensionRuntimeChannelInvoker>,
    backend_service_invoker: Option<Arc<ExtensionRuntimeBackendServiceInvoker>>,
    visibility_source: WorkspaceModuleVisibilitySource,
    operation_runtime_source: WorkspaceModuleOperationRuntimeSource,
    agent_run_bridge_handle: Option<SharedWorkspaceModuleAgentRunBridgeHandle>,
    current_user: Option<ProjectAuthorizationContext>,
}

impl WorkspaceModuleInvokeTool {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        installation_repo: Arc<dyn ProjectExtensionInstallationRepository>,
        canvas_repo: Arc<dyn CanvasRepository>,
        canvas_runtime_state_repo: Arc<dyn CanvasRuntimeStateRepository>,
        runtime_binding_repo: Arc<dyn AgentRunRuntimeBindingRepository>,
        project_id: Uuid,
        runtime_thread_id: String,
        agent_id: Option<String>,
        backend: Option<ResolvedInvocationBackend>,
        gateway: Arc<RuntimeGateway>,
        channel_invoker: Arc<ExtensionRuntimeChannelInvoker>,
        backend_service_invoker: Option<Arc<ExtensionRuntimeBackendServiceInvoker>>,
    ) -> Self {
        let backend_readiness = backend_readiness_for_optional_backend(&backend);
        let backend_service_readiness = backend_service_readiness_for_invoker(
            backend_service_invoker.as_ref(),
            &backend_readiness,
        );
        let operation_runtime_source = WorkspaceModuleOperationRuntimeSource::default()
            .with_gateway(
                gateway.clone(),
                runtime_thread_id.clone(),
                agent_id.clone(),
                true,
                backend_readiness,
                backend_service_readiness,
            );
        Self {
            installation_repo,
            canvas_repo,
            canvas_runtime_state_repo,
            runtime_binding_repo,
            project_id,
            runtime_thread_id,
            agent_id,
            backend,
            gateway,
            channel_invoker,
            backend_service_invoker,
            visibility_source: WorkspaceModuleVisibilitySource::default(),
            operation_runtime_source,
            agent_run_bridge_handle: None,
            current_user: None,
        }
    }

    pub fn with_agent_run_visibility(
        mut self,
        agent_run_bridge_handle: SharedWorkspaceModuleAgentRunBridgeHandle,
    ) -> Self {
        self.agent_run_bridge_handle = Some(agent_run_bridge_handle.clone());
        self.visibility_source = self
            .visibility_source
            .with_agent_run_delivery(agent_run_bridge_handle, self.runtime_thread_id.clone());
        self
    }

    pub fn with_current_user(mut self, current_user: Option<ProjectAuthorizationContext>) -> Self {
        self.current_user = current_user.clone();
        self.visibility_source = self.visibility_source.with_current_user(current_user);
        self
    }

    pub fn with_effective_capability_view(mut self, view: AgentRunEffectiveCapabilityView) -> Self {
        self.visibility_source = self.visibility_source.with_effective_view(view);
        self
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
    fn protocol_projector(&self) -> Option<agentdash_spi::ToolProtocolProjector> {
        Some(agentdash_spi::ToolProtocolProjector::Dynamic { namespace: None })
    }

    fn protocol_fixture_id(&self) -> Option<String> {
        Some("main_tool_workspace_module_invoke_dynamic_lifecycle".to_string())
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
        let runtime_context =
            WorkspaceModuleRuntimeContext::new(self.project_id, self.runtime_thread_id.clone())
                .with_agent_id(self.agent_id.clone())
                .with_current_user(self.current_user.clone())
                .with_agent_run_bridge(self.agent_run_bridge_handle.clone())
                .with_backend(self.backend.clone());

        project_operation_outcome(
            WorkspaceModuleAgentSurface::execute(WorkspaceModuleAgentSurfaceCommand::Invoke(
                WorkspaceModuleInvokeCommand {
                    installation_repo: &self.installation_repo,
                    canvas_repo: &self.canvas_repo,
                    canvas_runtime_state_repo: &self.canvas_runtime_state_repo,
                    runtime_binding_repo: &self.runtime_binding_repo,
                    project_id: self.project_id,
                    gateway: &self.gateway,
                    channel_invoker: &self.channel_invoker,
                    backend_service_invoker: self.backend_service_invoker.as_deref(),
                    visibility_source: &self.visibility_source,
                    operation_runtime_source: &self.operation_runtime_source,
                    runtime_context: &runtime_context,
                    module_id: module_id.to_string(),
                    operation_key: operation_key.to_string(),
                    input: params.input,
                },
            ))
            .await
            .map_err(surface_error_to_tool_error)?,
        )
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
/// 通过 `ControlPlaneProjectionChanged(reason=workspace_module_presented)` 触发
/// AgentRun workspace/resource-surface refresh。
/// 无可展示目标（module 不可见 / view_key 不存在）时返回**可操作诊断**结构化错误（R4）。
#[derive(Clone)]
pub struct WorkspaceModulePresentTool {
    installation_repo: Arc<dyn ProjectExtensionInstallationRepository>,
    canvas_repo: Arc<dyn CanvasRepository>,
    runtime_binding_repo: Arc<dyn AgentRunRuntimeBindingRepository>,
    project_id: Uuid,
    vfs: SharedRuntimeVfs,
    agent_run_bridge_handle: SharedWorkspaceModuleAgentRunBridgeHandle,
    runtime_thread_id: String,
    turn_id: String,
    visibility_source: WorkspaceModuleVisibilitySource,
    operation_runtime_source: WorkspaceModuleOperationRuntimeSource,
    presentation_append_handle: Option<SharedWorkspaceModulePresentationAppendHandle>,
}

impl WorkspaceModulePresentTool {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        installation_repo: Arc<dyn ProjectExtensionInstallationRepository>,
        canvas_repo: Arc<dyn CanvasRepository>,
        runtime_binding_repo: Arc<dyn AgentRunRuntimeBindingRepository>,
        project_id: Uuid,
        vfs: SharedRuntimeVfs,
        agent_run_bridge_handle: SharedWorkspaceModuleAgentRunBridgeHandle,
        runtime_thread_id: String,
        turn_id: String,
    ) -> Self {
        let visibility_source = WorkspaceModuleVisibilitySource::default()
            .with_agent_run_delivery(agent_run_bridge_handle.clone(), runtime_thread_id.clone());
        Self {
            installation_repo,
            canvas_repo,
            runtime_binding_repo,
            project_id,
            vfs,
            agent_run_bridge_handle,
            runtime_thread_id,
            turn_id,
            visibility_source,
            operation_runtime_source: WorkspaceModuleOperationRuntimeSource::default(),
            presentation_append_handle: None,
        }
    }

    pub fn with_presentation_append_handle(
        mut self,
        handle: SharedWorkspaceModulePresentationAppendHandle,
    ) -> Self {
        self.presentation_append_handle = Some(handle);
        self
    }

    pub fn with_current_user(mut self, current_user: Option<ProjectAuthorizationContext>) -> Self {
        self.visibility_source = self.visibility_source.with_current_user(current_user);
        self
    }

    pub fn with_effective_capability_view(mut self, view: AgentRunEffectiveCapabilityView) -> Self {
        self.visibility_source = self.visibility_source.with_effective_view(view);
        self
    }

    pub fn with_runtime_dependencies(
        mut self,
        runtime_gateway_handle: SharedWorkspaceModuleRuntimeGatewayHandle,
        channel_transport_available: bool,
        backend_readiness: WorkspaceModuleOperationReadiness,
        backend_service_readiness: WorkspaceModuleOperationReadiness,
    ) -> Self {
        self.operation_runtime_source = self.operation_runtime_source.with_gateway_handle(
            runtime_gateway_handle,
            self.runtime_thread_id.clone(),
            None,
            channel_transport_available,
            backend_readiness,
            backend_service_readiness,
        );
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
    fn protocol_projector(&self) -> Option<agentdash_spi::ToolProtocolProjector> {
        Some(agentdash_spi::ToolProtocolProjector::Dynamic { namespace: None })
    }

    fn protocol_fixture_id(&self) -> Option<String> {
        Some("main_tool_workspace_module_present_dynamic_lifecycle".to_string())
    }

    async fn execute(
        &self,
        tool_call_id: &str,
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
        let presentation_append_handle =
            self.presentation_append_handle.clone().ok_or_else(|| {
                AgentToolError::ExecutionFailed(
                    "Workspace module canonical presentation append port 尚未完成初始化"
                        .to_string(),
                )
            })?;
        let runtime_context =
            WorkspaceModuleRuntimeContext::new(self.project_id, self.runtime_thread_id.clone())
                .with_vfs(self.vfs.clone())
                .with_agent_run_bridge(Some(self.agent_run_bridge_handle.clone()))
                .with_presentation_append(presentation_append_handle, tool_call_id)
                .with_current_user(self.visibility_source.current_user().cloned());

        project_operation_outcome(
            WorkspaceModuleAgentSurface::execute(WorkspaceModuleAgentSurfaceCommand::Present(
                WorkspaceModulePresentCommand {
                    installation_repo: &self.installation_repo,
                    canvas_repo: &self.canvas_repo,
                    runtime_binding_repo: &self.runtime_binding_repo,
                    project_id: self.project_id,
                    turn_id: &self.turn_id,
                    visibility_source: &self.visibility_source,
                    operation_runtime_source: &self.operation_runtime_source,
                    runtime_context: &runtime_context,
                    module_id: module_id.to_string(),
                    view_key: view_key.to_string(),
                    payload: params.payload,
                },
            ))
            .await
            .map_err(surface_error_to_tool_error)?,
        )
    }
}

fn json_object_payload_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
        "type": "object",
        "additionalProperties": true
    })
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashMap};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    use agentdash_agent_runtime_contract::RuntimeThreadId;
    use agentdash_application_ports::agent_frame_materialization::RuntimeSurfaceUpdateRequest;
    use agentdash_application_ports::agent_run_runtime::{
        AgentRunRuntimeBinding, AgentRunRuntimeBindingError, AgentRunRuntimeBindingRepository,
        AgentRunRuntimeTarget,
    };
    use agentdash_application_ports::runtime_surface_adoption::AgentFrameRuntimeTarget;
    use agentdash_application_runtime_gateway::{
        RuntimeActionKey, RuntimeInvocationError, RuntimeInvocationRequest,
    };
    use agentdash_application_vfs::tools::SharedRuntimeVfs;
    use agentdash_domain::DomainError;
    use agentdash_domain::canvas::{Canvas, CanvasFile};
    use agentdash_domain::extension_package::{
        ExtensionPackageArtifactRef, ExtensionPackageMetadata,
    };
    use agentdash_domain::project::{
        Project, ProjectAuthorizationContext, ProjectRepository, ProjectRole, ProjectSubjectGrant,
        ProjectSubjectType,
    };
    use agentdash_domain::shared_library::{
        ExtensionBackendServiceDefinition, ExtensionBundleKind, ExtensionBundleRef,
        ExtensionGeneratedOperationDefinition, ExtensionGeneratedOperationDispatch,
        ExtensionGeneratedOperationProvenance, ExtensionGeneratedOperationVisibility,
        ExtensionRuntimeActionDefinition, ExtensionRuntimeActionKind, ExtensionTemplatePayload,
        ProjectExtensionInstallation, ProjectExtensionInstallationRepository,
    };
    use agentdash_spi::connector::RuntimeToolProvider;
    use agentdash_spi::platform::tool_capability::CAP_WORKSPACE_MODULE;
    use agentdash_spi::{
        AgentConfig, CapabilityState, ExecutionContext, ExecutionSessionFrame, ExecutionTurnFrame,
        RuntimeVfsAccessPolicy, ToolCapability, ToolCluster, ToolDefinition, Vfs,
        WorkspaceModuleDimension, WorkspaceModuleVisibilityMode,
    };
    use tokio::sync::RwLock;

    use super::*;
    use crate::canvas::{build_canvas, build_personal_canvas};
    use crate::workspace_module::runtime_bridge::{
        SharedWorkspaceModuleAgentRunBridgeHandle, SharedWorkspaceModuleRuntimeGatewayHandle,
        WorkspaceModuleAgentRunBridge,
    };
    use crate::workspace_module::{
        WorkspaceModuleRuntimeToolProvider, resolve_workspace_module_visibility,
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
            fetch_routes: vec![],
            operation_catalog: vec![],
            backend_services: vec![],
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
    struct FixtureCanvasRuntimeStateRepository {
        observation: Mutex<Option<agentdash_domain::canvas::CanvasRuntimeObservation>>,
        snapshot: Mutex<Option<agentdash_domain::canvas::CanvasInteractionSnapshot>>,
    }

    #[async_trait]
    impl CanvasRuntimeStateRepository for FixtureCanvasRuntimeStateRepository {
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
        Arc::new(FixtureCanvasRuntimeStateRepository::default())
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
            runtime_thread_id: &str,
        ) -> Result<AgentRunEffectiveCapabilityView, String> {
            Ok(test_effective_capability_view(
                WorkspaceModuleDimension::all(),
                vec![runtime_thread_id.to_string()],
            ))
        }

        async fn apply_canvas_runtime_surface_update_to_agent_run(
            &self,
            _runtime_thread_id: &str,
            canvas: &Canvas,
            _current_user: Option<&ProjectAuthorizationContext>,
            request: RuntimeSurfaceUpdateRequest,
        ) -> Result<RuntimeVfsState, String> {
            self.exposed_canvas_mount_ids
                .lock()
                .expect("exposed canvas lock")
                .push(canvas.mount_id.clone());
            self.requests.lock().expect("requests lock").push(request);
            let vfs = agentdash_domain::common::Vfs::default();
            Ok(RuntimeVfsState::new(
                vfs.clone(),
                RuntimeVfsAccessPolicy::whole_mounts_from_vfs(&vfs),
            ))
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

    fn packaged_backend_service_installation(
        project_id: Uuid,
        visibility: ExtensionGeneratedOperationVisibility,
    ) -> ProjectExtensionInstallation {
        let mut manifest = manifest("demo");
        manifest
            .backend_services
            .push(ExtensionBackendServiceDefinition {
                service_key: "demo.api".to_string(),
                runtime: "node".to_string(),
                entry: "dist/backend/server.mjs".to_string(),
                routes: vec!["/api/**".to_string()],
                health_path: Some("/health".to_string()),
            });
        manifest
            .operation_catalog
            .push(ExtensionGeneratedOperationDefinition {
                operation_key: "demo.search".to_string(),
                description: "Search through the demo backend service".to_string(),
                visibility,
                input_schema: serde_json::json!({"type": "object"}),
                output_schema: serde_json::json!({"type": "object"}),
                permission_summary: vec!["backend_service:demo.api".to_string()],
                dispatch: ExtensionGeneratedOperationDispatch::BackendService {
                    service_key: "demo.api".to_string(),
                    route: "/api/search".to_string(),
                },
                provenance: ExtensionGeneratedOperationProvenance {
                    capability_key: "search".to_string(),
                    exposure_key: "search".to_string(),
                    generated_from: "backend_service".to_string(),
                },
            });
        ProjectExtensionInstallation::new_packaged(
            project_id,
            "demo",
            "Demo Extension",
            manifest,
            artifact_ref("demo"),
        )
        .expect("valid packaged backend service installation")
    }

    fn artifact_ref(extension_key: &str) -> ExtensionPackageArtifactRef {
        ExtensionPackageArtifactRef {
            artifact_id: Uuid::new_v4(),
            package_name: format!("@agentdash/{extension_key}"),
            package_version: "1.0.0".to_string(),
            asset_version: "1.0.0".to_string(),
            source_version: "1.0.0".to_string(),
            storage_ref: format!("extensions/{extension_key}.agentdash-extension.tgz"),
            archive_digest:
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                    .to_string(),
            manifest_digest:
                "sha256:abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"
                    .to_string(),
        }
    }

    #[derive(Default)]
    struct FixtureInstallationRepo {
        installations: Mutex<Vec<ProjectExtensionInstallation>>,
    }

    #[async_trait]
    impl ProjectExtensionInstallationRepository for FixtureInstallationRepo {
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
    struct FixtureProjectRepo {
        projects: RwLock<HashMap<Uuid, Project>>,
        grants: RwLock<Vec<ProjectSubjectGrant>>,
    }

    #[async_trait]
    impl ProjectRepository for FixtureProjectRepo {
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
        let repo = Arc::new(FixtureProjectRepo::default());
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
    struct FixtureCanvasRepo {
        canvases: RwLock<HashMap<Uuid, Canvas>>,
    }

    #[async_trait]
    impl CanvasRepository for FixtureCanvasRepo {
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
    struct FixtureAgentRunRuntimeBindingRepository;

    #[async_trait]
    impl AgentRunRuntimeBindingRepository for FixtureAgentRunRuntimeBindingRepository {
        async fn load(
            &self,
            _target: &AgentRunRuntimeTarget,
        ) -> Result<Option<AgentRunRuntimeBinding>, AgentRunRuntimeBindingError> {
            Ok(None)
        }

        async fn load_by_thread_id(
            &self,
            _thread_id: &RuntimeThreadId,
        ) -> Result<Option<AgentRunRuntimeBinding>, AgentRunRuntimeBindingError> {
            Ok(None)
        }

        async fn list_by_run(
            &self,
            _run_id: Uuid,
        ) -> Result<Vec<AgentRunRuntimeBinding>, AgentRunRuntimeBindingError> {
            Ok(Vec::new())
        }

        async fn list_by_agent(
            &self,
            _agent_id: Uuid,
        ) -> Result<Vec<AgentRunRuntimeBinding>, AgentRunRuntimeBindingError> {
            Ok(Vec::new())
        }

        async fn insert(
            &self,
            binding: AgentRunRuntimeBinding,
        ) -> Result<AgentRunRuntimeBinding, AgentRunRuntimeBindingError> {
            Ok(binding)
        }
    }

    async fn fixtures() -> (
        Arc<dyn ProjectExtensionInstallationRepository>,
        Arc<dyn CanvasRepository>,
        Uuid,
    ) {
        let project_id = Uuid::new_v4();
        let install_repo = Arc::new(FixtureInstallationRepo::default());
        install_repo
            .installations
            .lock()
            .unwrap()
            .push(installation(project_id, "demo"));
        let canvas_repo = Arc::new(FixtureCanvasRepo::default());
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
                runtime_thread_id: agentdash_agent_runtime_contract::RuntimeThreadId::new(
                    "session-test",
                )
                .unwrap(),
            },
            visible_capabilities: capability_state.tool.capabilities.clone(),
            vfs_surface: capability_state.vfs.active.clone().unwrap_or_default(),
            mcp_surface: Vec::new(),
            capability_state,
            visible_workspace_module_refs: runtime_refs,
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
    async fn describe_extension_without_operation_catalog_has_no_agent_operations() {
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
        assert!(operations.is_empty());
    }

    #[tokio::test]
    async fn describe_does_not_promote_gateway_catalog_without_generated_operation() {
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
        let tool = WorkspaceModuleDescribeTool::new(install_repo, canvas_repo, project_id)
            .with_effective_capability_view(test_effective_capability_view(
                WorkspaceModuleDimension::all(),
                Vec::new(),
            ))
            .with_runtime_dependencies(
                gateway_handle,
                "session-1".to_string(),
                true,
                WorkspaceModuleOperationReadiness::ready(),
                WorkspaceModuleOperationReadiness::ready(),
            );

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
        assert!(operations.is_empty());
    }

    #[tokio::test]
    async fn describe_canvas_exposes_inspect_operation() {
        let (install_repo, canvas_repo, project_id) = fixtures().await;
        let tool = WorkspaceModuleDescribeTool::new(install_repo, canvas_repo, project_id)
            .with_current_user(Some(test_current_user()))
            .with_effective_capability_view(test_effective_capability_view(
                WorkspaceModuleDimension::all(),
                Vec::new(),
            ));
        let result = tool
            .execute(
                "t",
                serde_json::json!({"module_id": "canvas:cvs-dashboard-a"}),
                CancellationToken::new(),
                None,
            )
            .await
            .expect("describe canvas");
        assert!(!result.is_error);
        let details = result.details.expect("details");
        let operation_keys = details
            .get("operations")
            .and_then(serde_json::Value::as_array)
            .expect("operations")
            .iter()
            .filter_map(|operation| {
                operation
                    .get("operation_key")
                    .and_then(serde_json::Value::as_str)
            })
            .collect::<Vec<_>>();

        assert!(operation_keys.contains(&"canvas.inspect"));
        assert!(!operation_keys.contains(&"canvas.inspect_render_state"));
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
        let canvas_repo = Arc::new(FixtureCanvasRepo::default());
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
                    if message.contains("delivery runtime id")
            ),
            "Canvas expose must fail explicitly without a runtime session, got {result:?}"
        );
    }

    #[tokio::test]
    async fn operate_copy_to_personal_materializes_editable_canvas_with_random_mount_suffix() {
        let project_id = Uuid::new_v4();
        let project_repo = fake_project_repo(project_id).await;
        let canvas_repo = Arc::new(FixtureCanvasRepo::default());
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
                        "source_mount_id": "cvs-shared-dashboard"
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
                == Some(crate::canvas::CANVAS_BIND_DATA_OPERATION_KEY)
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
        ExtensionBackendServiceHttpResponsePayload, ExtensionBackendServiceInvokeMetadataPayload,
        ExtensionBackendServiceInvokeRequest, ExtensionBackendServiceInvokeResponse,
        ExtensionBackendServiceTransport, ExtensionChannelInvokeRequest,
        ExtensionChannelInvokeResponse, ExtensionRuntimeActionTransportError,
        ExtensionRuntimeChannelTransport,
    };
    use agentdash_application_runtime_gateway::{
        RuntimeActionDescriptor, RuntimeActionKind, RuntimeInvocationOutput, RuntimePolicy,
        RuntimeProvider,
    };
    use agentdash_application_vfs::tools::RuntimeVfsState;

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
        fn describe_action(&self) -> RuntimeActionDescriptor {
            RuntimeActionDescriptor {
                action_key: self.action_key.clone(),
                kind: RuntimeActionKind::SessionRuntime,
                description: Some("gateway profile descriptor".to_string()),
                input_schema: Some(serde_json::json!({"type": "object"})),
                output_schema: Some(serde_json::json!({"type": "object"})),
                default_policy: RuntimePolicy {
                    required_capabilities: vec!["gateway.profile.read".to_string()],
                    ..RuntimePolicy::default()
                },
                metadata: Default::default(),
            }
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

    #[derive(Default)]
    struct CapturingBackendServiceTransport {
        last_payload: Mutex<Option<ExtensionBackendServiceInvokeRequest>>,
    }

    #[async_trait]
    impl ExtensionBackendServiceTransport for CapturingBackendServiceTransport {
        async fn invoke_extension_backend_service(
            &self,
            backend_id: &str,
            payload: ExtensionBackendServiceInvokeRequest,
        ) -> Result<ExtensionBackendServiceInvokeResponse, ExtensionRuntimeActionTransportError>
        {
            assert_eq!(backend_id, "backend-1");
            *self
                .last_payload
                .lock()
                .expect("backend service payload lock") = Some(payload.clone());
            Ok(ExtensionBackendServiceInvokeResponse {
                metadata: ExtensionBackendServiceInvokeMetadataPayload {
                    project_id: payload.project_id.clone(),
                    backend_id: backend_id.to_string(),
                    extension_key: payload.extension_key.clone(),
                    extension_id: payload.extension_id.clone(),
                    service_key: payload.service_key.clone(),
                    route: payload.route.clone(),
                    trace_id: payload.trace_id.clone(),
                    invocation_id: payload.invocation_id.clone(),
                },
                response: Some(ExtensionBackendServiceHttpResponsePayload {
                    status: 200,
                    headers: BTreeMap::from([(
                        "content-type".to_string(),
                        "application/json".to_string(),
                    )]),
                    body: Some(br#"{"ok":true}"#.to_vec()),
                }),
                diagnostic: None,
            })
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
            Arc::new(FixtureAgentRunRuntimeBindingRepository),
            project_id,
            "session-1".to_string(),
            None,
            backend,
            gateway,
            channel_invoker,
            None,
        )
        .with_current_user(Some(test_current_user()))
        .with_effective_capability_view(test_effective_capability_view(
            WorkspaceModuleDimension::all(),
            Vec::new(),
        ));
        (tool, invoke_count)
    }

    fn invoke_tool_with_backend_service_transport(
        install_repo: Arc<dyn ProjectExtensionInstallationRepository>,
        canvas_repo: Arc<dyn CanvasRepository>,
        project_id: Uuid,
        backend: Option<ResolvedInvocationBackend>,
        transport: Arc<CapturingBackendServiceTransport>,
    ) -> WorkspaceModuleInvokeTool {
        let gateway = Arc::new(RuntimeGateway::new());
        let channel_invoker = Arc::new(ExtensionRuntimeChannelInvoker::new(
            install_repo.clone(),
            Arc::new(NoopChannelTransport),
        ));
        let backend_service_invoker = Arc::new(ExtensionRuntimeBackendServiceInvoker::new(
            install_repo.clone(),
            transport,
        ));
        WorkspaceModuleInvokeTool::new(
            install_repo,
            canvas_repo,
            fake_canvas_runtime_state_repo(),
            Arc::new(FixtureAgentRunRuntimeBindingRepository),
            project_id,
            "session-1".to_string(),
            None,
            backend,
            gateway,
            channel_invoker,
            Some(backend_service_invoker),
        )
        .with_current_user(Some(test_current_user()))
        .with_effective_capability_view(test_effective_capability_view(
            WorkspaceModuleDimension::all(),
            Vec::new(),
        ))
    }

    fn backend(id: &str) -> Option<ResolvedInvocationBackend> {
        Some(ResolvedInvocationBackend {
            backend_id: id.to_string(),
            workspace: None,
        })
    }

    fn workspace_module_execution_context(project_id: Uuid) -> ExecutionContext {
        let working_directory = PathBuf::from(".");
        let vfs = Vfs {
            source_project_id: Some(project_id.to_string()),
            ..Default::default()
        };
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
                vfs_access_policy: Some(RuntimeVfsAccessPolicy::whole_mounts_from_vfs(&vfs)),
                vfs: Some(vfs),
                backend_execution: None,
                runtime_backend_anchor: None,
                identity: None,
            },
            turn: ExecutionTurnFrame {
                platform_tool_execution: Some(agentdash_spi::PlatformToolExecutionContext {
                    run_id: Uuid::new_v4(),
                    project_id,
                    agent_id: Uuid::new_v4(),
                    frame_id: Uuid::new_v4(),
                    runtime_thread_id: "thread-workspace-module-fixture"
                        .parse()
                        .expect("runtime thread"),
                    presentation_thread_id: "presentation-workspace-module-fixture"
                        .parse()
                        .expect("presentation thread"),
                    visible_workspace_module_refs: Vec::new(),
                    invocation: None,
                    launch_evidence_frame_id: Uuid::new_v4(),
                    current_surface_frame_id: Uuid::new_v4(),
                    orchestration_id: None,
                    node_path: None,
                    node_attempt: None,
                }),
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
            Arc::new(FixtureAgentRunRuntimeBindingRepository),
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
            Arc::new(FixtureAgentRunRuntimeBindingRepository),
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
            Arc::new(FixtureAgentRunRuntimeBindingRepository),
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
    async fn invoke_runtime_action_without_operation_catalog_is_not_exposed() {
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
        assert!(result.is_error);
        assert_eq!(
            result
                .details
                .and_then(|d| d.get("error").and_then(|e| e.as_str()).map(str::to_string)),
            Some("operation_not_found".to_string())
        );
        assert_eq!(invoke_count.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn invoke_panel_only_generated_operation_is_not_exposed_to_agent() {
        let project_id = Uuid::new_v4();
        let install_repo = Arc::new(FixtureInstallationRepo::default());
        let mut installed = installation(project_id, "demo");
        installed
            .manifest
            .operation_catalog
            .push(ExtensionGeneratedOperationDefinition {
                operation_key: "demo.profile".to_string(),
                description: "Panel-only profile helper".to_string(),
                visibility: ExtensionGeneratedOperationVisibility::PanelOnly,
                input_schema: serde_json::json!({"type": "object"}),
                output_schema: serde_json::json!({"type": "object"}),
                permission_summary: vec!["local.profile.read".to_string()],
                dispatch: ExtensionGeneratedOperationDispatch::RuntimeAction {
                    action_key: "demo.profile".to_string(),
                },
                provenance: ExtensionGeneratedOperationProvenance {
                    capability_key: "profile".to_string(),
                    exposure_key: "profile".to_string(),
                    generated_from: "capability_exposure".to_string(),
                },
            });
        install_repo.installations.lock().unwrap().push(installed);
        let canvas_repo = Arc::new(FixtureCanvasRepo::default());
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

        assert!(result.is_error);
        assert_eq!(
            result
                .details
                .and_then(|d| d.get("error").and_then(|e| e.as_str()).map(str::to_string)),
            Some("operation_not_found".to_string())
        );
        assert_eq!(invoke_count.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn invoke_backend_service_generated_operation_dispatches_to_bridge() {
        let project_id = Uuid::new_v4();
        let install_repo = Arc::new(FixtureInstallationRepo::default());
        install_repo
            .installations
            .lock()
            .expect("installations lock")
            .push(packaged_backend_service_installation(
                project_id,
                ExtensionGeneratedOperationVisibility::AgentAndPanel,
            ));
        let canvas_repo = Arc::new(FixtureCanvasRepo::default());
        let transport = Arc::new(CapturingBackendServiceTransport::default());
        let tool = invoke_tool_with_backend_service_transport(
            install_repo,
            canvas_repo,
            project_id,
            backend("backend-1"),
            transport.clone(),
        );

        let result = tool
            .execute(
                "t",
                serde_json::json!({
                    "module_id": "ext:demo",
                    "operation_key": "demo.search",
                    "input": {"query": "abc"}
                }),
                CancellationToken::new(),
                None,
            )
            .await
            .expect("invoke backend service");

        assert!(
            !result.is_error,
            "expected backendService success: {result:?}"
        );
        let details = result.details.expect("details");
        assert_eq!(
            details
                .pointer("/provenance/operation_origin")
                .and_then(serde_json::Value::as_str),
            Some("backend_service")
        );
        assert_eq!(
            details
                .get("service_key")
                .and_then(serde_json::Value::as_str),
            Some("demo.api")
        );
        assert_eq!(
            details.get("route").and_then(serde_json::Value::as_str),
            Some("/api/search")
        );
        assert_eq!(
            details.get("status").and_then(serde_json::Value::as_u64),
            Some(200)
        );
        assert_eq!(
            details
                .pointer("/output/response/status")
                .and_then(serde_json::Value::as_u64),
            Some(200)
        );

        let payload = transport
            .last_payload
            .lock()
            .expect("backend service payload lock")
            .clone()
            .expect("payload");
        assert_eq!(payload.extension_key, "demo");
        assert_eq!(payload.extension_id, "demo");
        assert_eq!(payload.service_key, "demo.api");
        assert_eq!(payload.route, "/api/search");
        assert_eq!(payload.method, "POST");
        assert_eq!(
            payload.headers.get("content-type").map(String::as_str),
            Some("application/json; charset=utf-8")
        );
        assert_eq!(
            payload.body.as_deref(),
            Some(br#"{"query":"abc"}"#.as_slice())
        );
    }

    #[tokio::test]
    async fn invoke_panel_only_backend_service_operation_is_not_exposed_to_agent() {
        let project_id = Uuid::new_v4();
        let install_repo = Arc::new(FixtureInstallationRepo::default());
        install_repo
            .installations
            .lock()
            .expect("installations lock")
            .push(packaged_backend_service_installation(
                project_id,
                ExtensionGeneratedOperationVisibility::PanelOnly,
            ));
        let canvas_repo = Arc::new(FixtureCanvasRepo::default());
        let transport = Arc::new(CapturingBackendServiceTransport::default());
        let tool = invoke_tool_with_backend_service_transport(
            install_repo,
            canvas_repo,
            project_id,
            backend("backend-1"),
            transport.clone(),
        );

        let result = tool
            .execute(
                "t",
                serde_json::json!({
                    "module_id": "ext:demo",
                    "operation_key": "demo.search",
                    "input": {"query": "abc"}
                }),
                CancellationToken::new(),
                None,
            )
            .await
            .expect("invoke panel-only backend service");

        assert!(result.is_error);
        assert_eq!(
            result
                .details
                .and_then(|d| d.get("error").and_then(|e| e.as_str()).map(str::to_string)),
            Some("operation_not_found".to_string())
        );
        assert!(
            transport
                .last_payload
                .lock()
                .expect("backend service payload lock")
                .is_none()
        );
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
        let install_repo = Arc::new(FixtureInstallationRepo::default());
        let canvas_repo = Arc::new(FixtureCanvasRepo::default());
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
        let tool = invoke_tool_with_backend(install_repo, canvas_repo, project_id, None);
        let result = tool
            .execute(
                "t",
                serde_json::json!({
                    "module_id": "canvas:cvs-dashboard-a",
                    "operation_key": "canvas.bind_data",
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
    async fn invoke_runtime_action_without_operation_catalog_returns_not_found_even_without_backend()
     {
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
            Some("operation_not_found".to_string())
        );
    }
}
