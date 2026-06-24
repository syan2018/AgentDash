use std::sync::Arc;

use agentdash_application_ports::extension_runtime::ExtensionRuntimeChannelTransport;
use agentdash_domain::canvas::CanvasRepository;
use agentdash_domain::shared_library::ProjectExtensionInstallationRepository;
use agentdash_spi::platform::tool_capability::CAP_WORKSPACE_MODULE;
use agentdash_spi::{
    AgentTool, AgentToolError, AgentToolResult, ConnectorError, ContentPart, DynAgentTool,
    ExecutionContext, ToolCluster, ToolUpdateCallback, connector::RuntimeToolProvider,
};
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use crate::project::project_authorization_context_from_identity;
use crate::runtime_gateway::{ExtensionRuntimeChannelInvoker, RuntimeGateway};
use crate::runtime_tools::provider::{
    SharedRuntimeGatewayHandle, SharedSessionToolServicesHandle, project_id_from_context,
    runtime_session_id_from_context, shared_runtime_vfs_from_context,
};
use crate::workspace_module::{
    WorkspaceModuleCreateTool, WorkspaceModuleDescribeTool, WorkspaceModuleInvokeTool,
    WorkspaceModuleListTool, WorkspaceModulePresentTool, resolve_invocation_backend,
};

#[derive(Clone)]
pub struct WorkspaceModuleRuntimeToolProvider {
    installation_repo: Arc<dyn ProjectExtensionInstallationRepository>,
    canvas_repo: Arc<dyn CanvasRepository>,
    session_services_handle: SharedSessionToolServicesHandle,
    runtime_gateway_handle: SharedRuntimeGatewayHandle,
    extension_channel_transport: Option<Arc<dyn ExtensionRuntimeChannelTransport>>,
}

impl WorkspaceModuleRuntimeToolProvider {
    pub fn new(
        installation_repo: Arc<dyn ProjectExtensionInstallationRepository>,
        canvas_repo: Arc<dyn CanvasRepository>,
        session_services_handle: SharedSessionToolServicesHandle,
        runtime_gateway_handle: SharedRuntimeGatewayHandle,
    ) -> Self {
        Self {
            installation_repo,
            canvas_repo,
            session_services_handle,
            runtime_gateway_handle,
            extension_channel_transport: None,
        }
    }

    pub fn with_extension_channel_transport(
        mut self,
        transport: Arc<dyn ExtensionRuntimeChannelTransport>,
    ) -> Self {
        self.extension_channel_transport = Some(transport);
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InvokeRuntimeDependency {
    RuntimeGateway,
    ExtensionChannelTransport,
    RuntimeBackendAnchor,
}

impl InvokeRuntimeDependency {
    fn as_str(self) -> &'static str {
        match self {
            Self::RuntimeGateway => "runtime_gateway",
            Self::ExtensionChannelTransport => "extension_channel_transport",
            Self::RuntimeBackendAnchor => "runtime_backend_anchor",
        }
    }
}

#[derive(Clone)]
struct WorkspaceModuleInvokeUnavailableTool {
    missing_dependencies: Vec<InvokeRuntimeDependency>,
}

impl WorkspaceModuleInvokeUnavailableTool {
    fn new(missing_dependencies: Vec<InvokeRuntimeDependency>) -> Self {
        Self {
            missing_dependencies,
        }
    }

    fn missing_dependency_names(&self) -> Vec<&'static str> {
        self.missing_dependencies
            .iter()
            .map(|dependency| dependency.as_str())
            .collect()
    }
}

#[async_trait]
impl AgentTool for WorkspaceModuleInvokeUnavailableTool {
    fn name(&self) -> &str {
        "workspace_module_invoke"
    }

    fn description(&self) -> &str {
        "Invoke a workspace module operation. This session currently exposes a diagnostic because the runtime invocation dependencies were not assembled."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "module_id": { "type": "string" },
                "operation_key": { "type": "string" },
                "input": { "type": "object", "additionalProperties": true }
            },
            "required": ["module_id", "operation_key"],
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
        let missing_dependencies = self.missing_dependency_names();
        let message = format!(
            "workspace_module_invoke unavailable: missing runtime dependencies ({})",
            missing_dependencies.join(", ")
        );
        Ok(AgentToolResult {
            content: vec![ContentPart::text(message.clone())],
            is_error: true,
            details: Some(serde_json::json!({
                "error": "workspace_module_runtime_dependencies_unavailable",
                "message": message,
                "missing_dependencies": missing_dependencies,
            })),
        })
    }
}

#[async_trait]
impl RuntimeToolProvider for WorkspaceModuleRuntimeToolProvider {
    async fn build_tools(
        &self,
        context: &ExecutionContext,
    ) -> Result<Vec<DynAgentTool>, ConnectorError> {
        let flow = &context.turn.capability_state;
        if !flow
            .tool
            .enabled_clusters
            .contains(&ToolCluster::WorkspaceModule)
        {
            return Ok(Vec::new());
        }

        let Some(project_id) = project_id_from_context(context) else {
            tracing::warn!("workspace module tools 注入失败：无法从 hook session 解析 project_id");
            return Ok(Vec::new());
        };

        let shared_vfs = shared_runtime_vfs_from_context(context)?;
        let session_id = runtime_session_id_from_context(context);
        let current_user = context
            .session
            .identity
            .as_ref()
            .map(project_authorization_context_from_identity);
        let mut tools: Vec<DynAgentTool> = Vec::new();

        if flow.is_capability_tool_enabled(
            CAP_WORKSPACE_MODULE,
            "workspace_module_list",
            Some(ToolCluster::WorkspaceModule),
        ) {
            tools.push(Arc::new(
                WorkspaceModuleListTool::new(
                    self.installation_repo.clone(),
                    self.canvas_repo.clone(),
                    project_id,
                )
                .with_current_user(current_user.clone())
                .with_runtime_visibility(self.session_services_handle.clone(), session_id.clone()),
            ));
        }

        if flow.is_capability_tool_enabled(
            CAP_WORKSPACE_MODULE,
            "workspace_module_describe",
            Some(ToolCluster::WorkspaceModule),
        ) {
            tools.push(Arc::new(
                WorkspaceModuleDescribeTool::new(
                    self.installation_repo.clone(),
                    self.canvas_repo.clone(),
                    project_id,
                )
                .with_current_user(current_user.clone())
                .with_runtime_visibility(self.session_services_handle.clone(), session_id.clone()),
            ));
        }

        if flow.is_capability_tool_enabled(
            CAP_WORKSPACE_MODULE,
            "workspace_module_create",
            Some(ToolCluster::WorkspaceModule),
        ) {
            tools.push(Arc::new(
                WorkspaceModuleCreateTool::new(
                    self.canvas_repo.clone(),
                    project_id,
                    shared_vfs.clone(),
                    self.session_services_handle.clone(),
                    Some(session_id.clone()),
                )
                .with_current_user(current_user.clone())
                .with_turn_id(context.session.turn_id.clone()),
            ));
        }

        if flow.is_capability_tool_enabled(
            CAP_WORKSPACE_MODULE,
            "workspace_module_invoke",
            Some(ToolCluster::WorkspaceModule),
        ) {
            self.push_invoke_tool(
                context,
                project_id,
                &session_id,
                current_user.clone(),
                &mut tools,
            )
            .await;
        }

        if flow.is_capability_tool_enabled(
            CAP_WORKSPACE_MODULE,
            "workspace_module_present",
            Some(ToolCluster::WorkspaceModule),
        ) {
            tools.push(Arc::new(
                WorkspaceModulePresentTool::new(
                    self.installation_repo.clone(),
                    self.canvas_repo.clone(),
                    project_id,
                    shared_vfs,
                    self.session_services_handle.clone(),
                    session_id,
                    context.session.turn_id.clone(),
                )
                .with_current_user(current_user.clone()),
            ));
        }

        Ok(tools)
    }
}

impl WorkspaceModuleRuntimeToolProvider {
    async fn push_invoke_tool(
        &self,
        context: &ExecutionContext,
        project_id: uuid::Uuid,
        session_id: &str,
        current_user: Option<crate::project::ProjectAuthorizationContext>,
        tools: &mut Vec<DynAgentTool>,
    ) {
        let (gateway, transport) = match self.invoke_runtime_deps().await {
            Ok(deps) => deps,
            Err(missing_dependencies) => {
                let missing_names = missing_dependencies
                    .iter()
                    .map(|dependency| dependency.as_str())
                    .collect::<Vec<_>>();
                tracing::warn!(
                    missing_dependencies = ?missing_names,
                    "workspace_module_invoke 装配为诊断工具：缺少 RuntimeGateway 或 channel transport 注入"
                );
                tools.push(Arc::new(WorkspaceModuleInvokeUnavailableTool::new(
                    missing_dependencies,
                )));
                return;
            }
        };

        let backend_anchor = match context
            .session
            .require_runtime_backend_anchor("workspace_module_invoke", Some(session_id))
        {
            Ok(anchor) => anchor,
            Err(error) => {
                tracing::warn!(
                    session_id = %session_id,
                    error = %error,
                    "workspace_module_invoke 装配为诊断工具：缺少 runtime backend anchor"
                );
                tools.push(Arc::new(WorkspaceModuleInvokeUnavailableTool::new(vec![
                    InvokeRuntimeDependency::RuntimeBackendAnchor,
                ])));
                return;
            }
        };
        let backend =
            resolve_invocation_backend(context.session.vfs.as_ref(), Some(backend_anchor));
        let channel_invoker = Arc::new(ExtensionRuntimeChannelInvoker::new(
            self.installation_repo.clone(),
            transport,
        ));
        tools.push(Arc::new(
            WorkspaceModuleInvokeTool::new(
                self.installation_repo.clone(),
                self.canvas_repo.clone(),
                project_id,
                session_id.to_string(),
                None,
                backend,
                gateway,
                channel_invoker,
            )
            .with_current_user(current_user)
            .with_runtime_visibility(self.session_services_handle.clone()),
        ));
    }

    async fn invoke_runtime_deps(
        &self,
    ) -> Result<
        (
            Arc<RuntimeGateway>,
            Arc<dyn ExtensionRuntimeChannelTransport>,
        ),
        Vec<InvokeRuntimeDependency>,
    > {
        let runtime_gateway = self.runtime_gateway_handle.get().await;
        let extension_channel_transport = self.extension_channel_transport.as_ref().cloned();
        let mut missing = Vec::new();
        if runtime_gateway.is_none() {
            missing.push(InvokeRuntimeDependency::RuntimeGateway);
        }
        if extension_channel_transport.is_none() {
            missing.push(InvokeRuntimeDependency::ExtensionChannelTransport);
        }
        if missing.is_empty() {
            Ok((
                runtime_gateway.expect("checked runtime gateway"),
                extension_channel_transport.expect("checked channel transport"),
            ))
        } else {
            Err(missing)
        }
    }
}
