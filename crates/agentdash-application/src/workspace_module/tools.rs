//! Workspace Module Agent 工具：`workspace_module_list` / `workspace_module_describe`。
//!
//! 二者挂在 `RelayRuntimeToolProvider`，用 `project_id_from_context` + repos 现取
//! 现算（样板 `ListCanvasesTool`）：每次调用拉 enabled installations + visible
//! canvases，经聚合层 `build_workspace_modules` 投影，再按 capability 的
//! `WorkspaceModuleDimension` 过滤（可见性裁切的唯一来源，D4）。

use std::sync::Arc;

use agentdash_agent_protocol::{
    BackboneEnvelope, BackboneEvent, PlatformEvent, SourceInfo, TraceInfo,
};
use agentdash_domain::canvas::{Canvas, CanvasRepository};
use agentdash_domain::shared_library::ProjectExtensionInstallationRepository;
use agentdash_spi::WorkspaceModuleDimension;
use agentdash_spi::context::tool_schema_sanitizer::schema_value;
use agentdash_spi::{AgentTool, AgentToolError, AgentToolResult, ContentPart, ToolUpdateCallback};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::extension_runtime::extension_runtime_projection_from_installations;
use crate::runtime_gateway::{
    ExtensionRuntimeChannelConsumer, ExtensionRuntimeChannelInvokeRequest,
    ExtensionRuntimeChannelInvoker, RuntimeActionKey, RuntimeActor, RuntimeContext, RuntimeGateway,
    RuntimeInvocationError, RuntimeInvocationErrorKind, RuntimeInvocationRequest,
    RuntimeInvocationResult, RuntimeTarget, RuntimeTrace,
};
use crate::vfs::tools::SharedSessionToolServicesHandle;
use crate::workspace_module::{
    ResolvedInvocationBackend, WorkspaceModuleDescriptor, WorkspaceModuleOperation,
    WorkspaceModuleOperationDispatch, build_workspace_modules, validate_input_against_schema,
};

/// 现取现算：拉 enabled extension projection + visible canvas，聚合 + capability 过滤。
async fn resolve_visible_modules(
    installation_repo: &Arc<dyn ProjectExtensionInstallationRepository>,
    canvas_repo: &Arc<dyn CanvasRepository>,
    project_id: Uuid,
    visibility: &WorkspaceModuleDimension,
) -> Result<Vec<WorkspaceModuleDescriptor>, AgentToolError> {
    let installations = installation_repo
        .list_enabled_by_project(project_id)
        .await
        .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;
    let projection = extension_runtime_projection_from_installations(installations)
        .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;
    let canvases: Vec<Canvas> = canvas_repo
        .list_by_project(project_id)
        .await
        .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;

    let modules = build_workspace_modules(&projection, &canvases)
        .into_iter()
        .filter(|module| visibility.allows(&module.summary.module_id))
        .collect();
    Ok(modules)
}

#[derive(Clone)]
pub struct WorkspaceModuleListTool {
    installation_repo: Arc<dyn ProjectExtensionInstallationRepository>,
    canvas_repo: Arc<dyn CanvasRepository>,
    project_id: Uuid,
    visibility: WorkspaceModuleDimension,
}

impl WorkspaceModuleListTool {
    pub fn new(
        installation_repo: Arc<dyn ProjectExtensionInstallationRepository>,
        canvas_repo: Arc<dyn CanvasRepository>,
        project_id: Uuid,
        visibility: WorkspaceModuleDimension,
    ) -> Self {
        Self {
            installation_repo,
            canvas_repo,
            project_id,
            visibility,
        }
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
        let modules = resolve_visible_modules(
            &self.installation_repo,
            &self.canvas_repo,
            self.project_id,
            &self.visibility,
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
    visibility: WorkspaceModuleDimension,
}

impl WorkspaceModuleDescribeTool {
    pub fn new(
        installation_repo: Arc<dyn ProjectExtensionInstallationRepository>,
        canvas_repo: Arc<dyn CanvasRepository>,
        project_id: Uuid,
        visibility: WorkspaceModuleDimension,
    ) -> Self {
        Self {
            installation_repo,
            canvas_repo,
            project_id,
            visibility,
        }
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

        let modules = resolve_visible_modules(
            &self.installation_repo,
            &self.canvas_repo,
            self.project_id,
            &self.visibility,
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
    visibility: WorkspaceModuleDimension,
    session_id: String,
    agent_id: Option<String>,
    backend: Option<ResolvedInvocationBackend>,
    gateway: Arc<RuntimeGateway>,
    channel_invoker: Arc<ExtensionRuntimeChannelInvoker>,
}

impl WorkspaceModuleInvokeTool {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        installation_repo: Arc<dyn ProjectExtensionInstallationRepository>,
        canvas_repo: Arc<dyn CanvasRepository>,
        project_id: Uuid,
        visibility: WorkspaceModuleDimension,
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
            visibility,
            session_id,
            agent_id,
            backend,
            gateway,
            channel_invoker,
        }
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

        // 现取现算：聚合 + 可见性裁切（与 list/describe 同源，capability 通道 D4）。
        let modules = resolve_visible_modules(
            &self.installation_repo,
            &self.canvas_repo,
            self.project_id,
            &self.visibility,
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
            WorkspaceModuleOperationDispatch::Canvas { canvas_action } => {
                // canvas runtime action：以 UserCanvas actor 走同一 RuntimeGateway（复用
                // research/03 三行路径，不另起 canvas service）。本轮聚合层不投影 canvas
                // operation（design §4），因此正常路径下到不了这里；保留分支以待 canvas
                // runtime action surface 落地。
                let backend = match self.require_backend() {
                    Ok(backend) => backend,
                    Err(result) => return Ok(result),
                };
                let action_key =
                    RuntimeActionKey::parse(canvas_action.clone()).map_err(|error| {
                        AgentToolError::ExecutionFailed(format!(
                            "canvas operation `{operation_key}` 的 action_key 非法: {error}"
                        ))
                    })?;
                let mut request = RuntimeInvocationRequest::new(
                    action_key,
                    RuntimeActor::UserCanvas {
                        session_id: self.session_id.clone(),
                        canvas_id: None,
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
    pub payload: Option<serde_json::Value>,
}

/// `workspace_module_present`：best-effort 请求宿主向前端 panel 展示某 module 的 UI 入口。
///
/// 复用 `PlatformEvent::SessionMetaUpdate{ key: "workspace_module_presented" }` +
/// inject_notification（模板 PresentCanvasTool），不新增 PlatformEvent 变体（D2-5）。
/// 无可展示目标（module 不可见 / view_key 不存在）时返回**可操作诊断**结构化错误（R4）。
#[derive(Clone)]
pub struct WorkspaceModulePresentTool {
    installation_repo: Arc<dyn ProjectExtensionInstallationRepository>,
    canvas_repo: Arc<dyn CanvasRepository>,
    project_id: Uuid,
    visibility: WorkspaceModuleDimension,
    session_services_handle: SharedSessionToolServicesHandle,
    session_id: String,
    turn_id: String,
}

impl WorkspaceModulePresentTool {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        installation_repo: Arc<dyn ProjectExtensionInstallationRepository>,
        canvas_repo: Arc<dyn CanvasRepository>,
        project_id: Uuid,
        visibility: WorkspaceModuleDimension,
        session_services_handle: SharedSessionToolServicesHandle,
        session_id: String,
        turn_id: String,
    ) -> Self {
        Self {
            installation_repo,
            canvas_repo,
            project_id,
            visibility,
            session_services_handle,
            session_id,
            turn_id,
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

        let modules = resolve_visible_modules(
            &self.installation_repo,
            &self.canvas_repo,
            self.project_id,
            &self.visibility,
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

        let Some(ui_entry) = module
            .ui_entries
            .iter()
            .find(|entry| entry.view_key == view_key)
        else {
            // 无可展示目标 → 可操作诊断（R4），仍 inject 一条诊断事件，不静默。
            let diagnostic = serde_json::json!({
                "module_id": module_id,
                "view_key": view_key,
                "reason": "no_matching_ui_entry",
                "available_views": module
                    .ui_entries
                    .iter()
                    .map(|entry| entry.view_key.clone())
                    .collect::<Vec<_>>(),
            });
            self.inject_present_diagnostic(&diagnostic).await;
            return Ok(structured_tool_error(
                "view_not_found",
                format!("module `{module_id}` 无名为 `{view_key}` 的 UI view"),
                diagnostic,
            ));
        };

        let value = serde_json::json!({
            "module_id": module_id,
            "view_key": view_key,
            "renderer_kind": ui_entry.renderer_kind,
            "uri": ui_entry.uri_scheme,
            "title": ui_entry.title,
            "payload": params.payload,
        });

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
                ui_entry.renderer_kind
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use agentdash_domain::DomainError;
    use agentdash_domain::extension_package::ExtensionPackageMetadata;
    use agentdash_domain::shared_library::{
        ExtensionBundleKind, ExtensionBundleRef, ExtensionRuntimeActionDefinition,
        ExtensionRuntimeActionKind, ExtensionTemplatePayload, ProjectExtensionInstallation,
        ProjectExtensionInstallationRepository,
    };
    use tokio::sync::RwLock;

    use super::*;
    use crate::canvas::build_canvas;

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
        async fn find_by_mount_id(&self, mount_id: &str) -> Result<Option<Canvas>, DomainError> {
            Ok(self
                .canvases
                .read()
                .await
                .values()
                .find(|c| c.mount_id == mount_id)
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
            Some("dashboard-a".to_string()),
            "Dashboard A".to_string(),
            "demo canvas".to_string(),
            Default::default(),
        )
        .expect("canvas");
        canvas_repo.create(&canvas).await.expect("create canvas");
        (install_repo, canvas_repo, project_id)
    }

    #[tokio::test]
    async fn list_returns_extension_and_canvas_summaries() {
        let (install_repo, canvas_repo, project_id) = fixtures().await;
        let tool = WorkspaceModuleListTool::new(
            install_repo,
            canvas_repo,
            project_id,
            WorkspaceModuleDimension::default(),
        );
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
        let tool = WorkspaceModuleDescribeTool::new(
            install_repo,
            canvas_repo,
            project_id,
            WorkspaceModuleDimension::default(),
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
        let tool = WorkspaceModuleDescribeTool::new(
            install_repo,
            canvas_repo,
            project_id,
            WorkspaceModuleDimension::default(),
        );
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
            mode: agentdash_spi::WorkspaceModuleVisibilityMode::Allowlist,
            allowed_module_ids: vec!["ext:demo".to_string()],
        };
        let tool = WorkspaceModuleListTool::new(install_repo, canvas_repo, project_id, visibility);
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

    // ---- invoke tool tests ----

    use crate::runtime_gateway::{RuntimeActionKind, RuntimeInvocationOutput, RuntimeProvider};
    use agentdash_application_ports::extension_runtime::{
        ExtensionChannelInvokeRequest, ExtensionChannelInvokeResponse,
        ExtensionRuntimeActionTransportError, ExtensionRuntimeChannelTransport,
    };

    struct EchoActionProvider {
        action_key: RuntimeActionKey,
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
        let gateway = Arc::new(
            RuntimeGateway::new().with_provider(Arc::new(EchoActionProvider {
                action_key: RuntimeActionKey::parse("demo.profile").expect("valid action key"),
            })),
        );
        let channel_invoker = Arc::new(ExtensionRuntimeChannelInvoker::new(
            install_repo.clone(),
            Arc::new(NoopChannelTransport),
        ));
        WorkspaceModuleInvokeTool::new(
            install_repo,
            canvas_repo,
            project_id,
            WorkspaceModuleDimension::default(),
            "session-1".to_string(),
            None,
            backend,
            gateway,
            channel_invoker,
        )
    }

    fn backend(id: &str) -> Option<ResolvedInvocationBackend> {
        Some(ResolvedInvocationBackend {
            backend_id: id.to_string(),
            workspace: None,
        })
    }

    #[tokio::test]
    async fn invoke_runtime_action_routes_to_gateway() {
        let (install_repo, canvas_repo, project_id) = fixtures().await;
        let tool =
            invoke_tool_with_backend(install_repo, canvas_repo, project_id, backend("backend-1"));
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
