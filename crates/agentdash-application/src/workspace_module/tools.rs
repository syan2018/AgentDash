//! Workspace Module Agent 工具：`workspace_module_list` / `workspace_module_describe`。
//!
//! 二者由 session runtime tool composer 通过 workspace-module provider 装配，
//! 用当前 project context + repos 现取现算：每次调用拉 enabled installations + visible canvases，经聚合层
//! `build_workspace_modules` 投影，再按 capability 的
//! `WorkspaceModuleDimension` 过滤（可见性裁切的唯一来源，D4）。

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
use agentdash_spi::WorkspaceModuleDimension;
use agentdash_spi::context::tool_schema_sanitizer::schema_value;
use agentdash_spi::{AgentTool, AgentToolError, AgentToolResult, ContentPart, ToolUpdateCallback};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::canvas::{
    BindCanvasDataParams, StartCanvasParams, bind_canvas_data_for_project,
    create_or_attach_canvas_for_session, expose_existing_canvas_for_session,
};
use crate::extension_runtime::{
    ExtensionRuntimeProjection, extension_runtime_projection_from_installations,
};
use crate::runtime_gateway::{
    ExtensionRuntimeChannelConsumer, ExtensionRuntimeChannelInvokeRequest,
    ExtensionRuntimeChannelInvoker, RuntimeActionKey, RuntimeActor, RuntimeContext, RuntimeGateway,
    RuntimeInvocationError, RuntimeInvocationErrorKind, RuntimeInvocationRequest,
    RuntimeInvocationResult, RuntimeTarget, RuntimeTrace,
};
use crate::runtime_tools::SharedSessionToolServicesHandle;
use crate::workspace_module::{
    ResolvedInvocationBackend, build_workspace_module_presentation, build_workspace_modules,
    validate_input_against_schema,
};

/// 现取现算：拉 enabled extension projection + visible canvas，聚合 + capability 过滤。
async fn resolve_visible_modules(
    installation_repo: &Arc<dyn ProjectExtensionInstallationRepository>,
    canvas_repo: &Arc<dyn CanvasRepository>,
    project_id: Uuid,
    visibility: &WorkspaceModuleDimension,
    dynamic_module_refs: &[String],
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
        .filter(|module| {
            visibility.allows(&module.summary.module_id)
                || dynamic_module_refs
                    .iter()
                    .any(|module_ref| module_ref == &module.summary.module_id)
        })
        .collect();
    Ok(modules)
}

async fn runtime_visible_module_refs(
    session_services_handle: Option<&SharedSessionToolServicesHandle>,
    session_id: Option<&str>,
) -> Vec<String> {
    let (Some(handle), Some(session_id)) = (session_services_handle, session_id) else {
        return Vec::new();
    };
    let Some(session_services) = handle.get().await else {
        return Vec::new();
    };
    match session_services
        .capability
        .visible_workspace_module_refs_from_frame(session_id)
        .await
    {
        Ok(refs) => refs,
        Err(error) => {
            tracing::warn!(%error, "读取运行时 workspace module grant 失败，降级为 base 可见性");
            Vec::new()
        }
    }
}

#[derive(Clone)]
pub struct WorkspaceModuleListTool {
    installation_repo: Arc<dyn ProjectExtensionInstallationRepository>,
    canvas_repo: Arc<dyn CanvasRepository>,
    project_id: Uuid,
    visibility: WorkspaceModuleDimension,
    session_services_handle: Option<SharedSessionToolServicesHandle>,
    session_id: Option<String>,
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
            session_services_handle: None,
            session_id: None,
        }
    }

    pub fn with_runtime_visibility(
        mut self,
        session_services_handle: SharedSessionToolServicesHandle,
        session_id: String,
    ) -> Self {
        self.session_services_handle = Some(session_services_handle);
        self.session_id = Some(session_id);
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
        let dynamic_module_refs = runtime_visible_module_refs(
            self.session_services_handle.as_ref(),
            self.session_id.as_deref(),
        )
        .await;
        let modules = resolve_visible_modules(
            &self.installation_repo,
            &self.canvas_repo,
            self.project_id,
            &self.visibility,
            &dynamic_module_refs,
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
    session_services_handle: Option<SharedSessionToolServicesHandle>,
    session_id: Option<String>,
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
            session_services_handle: None,
            session_id: None,
        }
    }

    pub fn with_runtime_visibility(
        mut self,
        session_services_handle: SharedSessionToolServicesHandle,
        session_id: String,
    ) -> Self {
        self.session_services_handle = Some(session_services_handle);
        self.session_id = Some(session_id);
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

        let dynamic_module_refs = runtime_visible_module_refs(
            self.session_services_handle.as_ref(),
            self.session_id.as_deref(),
        )
        .await;
        let modules = resolve_visible_modules(
            &self.installation_repo,
            &self.canvas_repo,
            self.project_id,
            &self.visibility,
            &dynamic_module_refs,
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
        "Create or attach a workspace module instance. Currently supports kind=`canvas`: pass input.canvas_id? + input.title? + input.description?; returns the materialized canvas:{mount_id} descriptor and exposes its Canvas VFS mount to the current session."
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
            "created workspace module\nmodule_id={module_id}\ncanvas_id={}\nmount={}://\nskill_path={}",
            canvas.mount_id, canvas_result.mount_id, canvas_result.skill_path
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
    visibility: WorkspaceModuleDimension,
    session_id: String,
    agent_id: Option<String>,
    backend: Option<ResolvedInvocationBackend>,
    gateway: Arc<RuntimeGateway>,
    channel_invoker: Arc<ExtensionRuntimeChannelInvoker>,
    session_services_handle: Option<SharedSessionToolServicesHandle>,
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
            session_services_handle: None,
        }
    }

    pub fn with_runtime_visibility(
        mut self,
        session_services_handle: SharedSessionToolServicesHandle,
    ) -> Self {
        self.session_services_handle = Some(session_services_handle);
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
        let dynamic_module_refs = runtime_visible_module_refs(
            self.session_services_handle.as_ref(),
            Some(&self.session_id),
        )
        .await;
        let modules = resolve_visible_modules(
            &self.installation_repo,
            &self.canvas_repo,
            self.project_id,
            &self.visibility,
            &dynamic_module_refs,
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
                        "canvas_id".to_string(),
                        serde_json::Value::String(module.summary.source.clone()),
                    );
                    let bind_params: BindCanvasDataParams =
                        serde_json::from_value(input).map_err(|error| {
                            AgentToolError::InvalidArguments(format!(
                                "invalid canvas.bind_data input: {error}"
                            ))
                        })?;
                    let result = bind_canvas_data_for_project(
                        self.canvas_repo.as_ref(),
                        self.project_id,
                        bind_params,
                    )
                    .await?;
                    let content = format!(
                        "canvas_id={}\nmount={}://\nalias={}\nsource_uri={}\ncontent_type={}",
                        result.canvas_id,
                        result.mount_id,
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
    visibility: WorkspaceModuleDimension,
    vfs: crate::vfs::tools::fs::SharedRuntimeVfs,
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
        vfs: crate::vfs::tools::fs::SharedRuntimeVfs,
        session_services_handle: SharedSessionToolServicesHandle,
        session_id: String,
        turn_id: String,
    ) -> Self {
        Self {
            installation_repo,
            canvas_repo,
            project_id,
            visibility,
            vfs,
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

        let dynamic_module_refs = runtime_visible_module_refs(
            Some(&self.session_services_handle),
            Some(&self.session_id),
        )
        .await;
        let modules = resolve_visible_modules(
            &self.installation_repo,
            &self.canvas_repo,
            self.project_id,
            &self.visibility,
            &dynamic_module_refs,
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
        ToolDefinition,
    };
    use futures::stream;
    use tokio::sync::RwLock;

    use super::*;
    use crate::agent_run::frame::builder::AgentFrameBuilder;
    use crate::agent_run::frame::surface::FrameSurfaceDraft;
    use crate::canvas::build_canvas;
    use crate::runtime_tools::{
        SessionToolServices, SharedRuntimeGatewayHandle, SharedSessionToolServicesHandle,
    };
    use crate::session::construction::{
        ConstructionResolutionPlan, OwnerResolutionTrace, ResolvedSessionOwner,
        RuntimeContextInspectionPlan,
    };
    use crate::session::hub::SessionRuntimeInner;
    use crate::session::{MemorySessionPersistence, UserPromptInput, local_workspace_vfs};
    use crate::test_support::{
        MemoryAgentFrameRepository, MemoryLifecycleAgentRepository, MemoryLifecycleGateRepository,
        MemoryRuntimeSessionExecutionAnchorRepository,
    };
    use crate::vfs::{CanvasFsMountProvider, MountProviderRegistry, VfsService};
    use crate::workspace_module::WorkspaceModuleRuntimeToolProvider;

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
            WorkspaceModuleDimension::all(),
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
            WorkspaceModuleDimension::all(),
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
            WorkspaceModuleDimension::all(),
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

    #[tokio::test]
    async fn runtime_visible_refs_extend_workspace_module_allowlist() {
        let (install_repo, canvas_repo, project_id) = fixtures().await;
        let visibility = WorkspaceModuleDimension {
            mode: agentdash_spi::WorkspaceModuleVisibilityMode::Allowlist,
            allowed_module_ids: vec!["ext:demo".to_string()],
        };
        let modules = resolve_visible_modules(
            &install_repo,
            &canvas_repo,
            project_id,
            &visibility,
            &["canvas:dashboard-a".to_string()],
        )
        .await
        .expect("resolve modules");
        let module_ids = modules
            .iter()
            .map(|module| module.summary.module_id.as_str())
            .collect::<Vec<_>>();

        assert_eq!(module_ids.len(), 2);
        assert!(module_ids.contains(&"ext:demo"));
        assert!(module_ids.contains(&"canvas:dashboard-a"));
    }

    #[tokio::test]
    async fn create_canvas_returns_module_descriptor_and_exposes_vfs_mount() {
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
            .await
            .expect("create canvas module");

        assert!(!result.is_error, "expected success, got {result:?}");
        let details = result.details.expect("details");
        assert_eq!(
            details.get("module_id").and_then(serde_json::Value::as_str),
            Some("canvas:sales-board")
        );
        assert_eq!(
            details
                .pointer("/descriptor/summary/module_id")
                .and_then(serde_json::Value::as_str),
            Some("canvas:sales-board")
        );
        assert_eq!(
            details
                .pointer("/descriptor/ui_entries/0/presentation_uri")
                .and_then(serde_json::Value::as_str),
            Some("canvas://sales-board")
        );
        assert!(details.get("presentation").is_none());

        let vfs = shared_vfs.snapshot().await;
        assert!(
            vfs.mounts.iter().any(|mount| mount.id == "cvs-sales-board"),
            "workspace_module_create should expose the Canvas VFS mount"
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
        let switched_frame = AgentFrameBuilder::new(agent_id)
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
                capability: hub.capability_service(),
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
            Some("canvas:sales-board")
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
            vec!["canvas:sales-board".to_string()]
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
                        && value.get("kind").and_then(|v| v.as_str()) == Some("capability_state_update")
                )
            })
            .expect("should write capability_state_update context_frame");
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

        let describe_visibility = WorkspaceModuleDimension {
            mode: agentdash_spi::WorkspaceModuleVisibilityMode::Allowlist,
            allowed_module_ids: vec!["ext:demo".to_string()],
        };
        let describe_tool = WorkspaceModuleDescribeTool::new(
            install_repo,
            canvas_repo,
            project_id,
            describe_visibility,
        )
        .with_runtime_visibility(handle, session.id.clone());
        let describe = describe_tool
            .execute(
                "tool-describe",
                serde_json::json!({"module_id": "canvas:sales-board"}),
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
            Some("canvas:sales-board".to_string())
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
        let switched_frame = AgentFrameBuilder::new(agent_id)
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
                capability: hub.capability_service(),
            })
            .await;

        let shared_vfs =
            crate::vfs::tools::fs::SharedRuntimeVfs::new(local_workspace_vfs(base.path()));
        let present_tool = WorkspaceModulePresentTool::new(
            install_repo,
            canvas_repo,
            project_id,
            WorkspaceModuleDimension::all(),
            shared_vfs,
            handle,
            session.id.clone(),
            turn_id,
        );

        present_tool
            .execute(
                "tool-present",
                serde_json::json!({
                    "module_id": "canvas:dashboard-a",
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
            vec!["dashboard-a".to_string()]
        );
        assert_eq!(
            updated_frame.visible_workspace_module_refs(),
            vec!["canvas:dashboard-a".to_string()]
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
        assert!(state.skill.skills.iter().any(|skill| {
            skill.name == "canvas-system"
                && skill.file_path == "cvs-dashboard-a://skills/canvas-system/SKILL.md"
        }));

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
                        && value.get("kind").and_then(|v| v.as_str()) == Some("capability_state_update")
                )
            })
            .expect("should write capability_state_update context_frame");
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
            Some("canvas://dashboard-a")
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
            WorkspaceModuleDimension::all(),
            "session-1".to_string(),
            None,
            backend,
            gateway,
            channel_invoker,
        );
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
                    "module_id": "canvas:dashboard-a",
                    "operation_key": "canvas.bind_data",
                    "input": {
                        "alias": "stats",
                        "source_uri": "project://data/stats.json"
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
            Some("application/json")
        );

        let saved = canvas_repo
            .get_by_mount_id(project_id, "dashboard-a")
            .await
            .expect("load canvas")
            .expect("canvas");
        let binding = saved
            .bindings
            .iter()
            .find(|binding| binding.alias == "stats")
            .expect("binding should be saved");
        assert_eq!(binding.source_uri, "project://data/stats.json");
        assert_eq!(binding.content_type, "application/json");
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
