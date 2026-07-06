use agentdash_diagnostics::{DiagnosticErrorContext, Subsystem, diag, diag_error};
use std::sync::Arc;

use agentdash_application_ports::extension_runtime::{
    ExtensionBackendServiceTransport, ExtensionRuntimeChannelTransport,
};
use agentdash_application_runtime_gateway::{
    ExtensionRuntimeBackendServiceInvoker, ExtensionRuntimeChannelInvoker, RuntimeGateway,
};
use agentdash_contracts::workspace_module::{
    WorkspaceModuleOperationReadiness, WorkspaceModuleOperationReadinessKind,
};
use agentdash_domain::canvas::{CanvasRepository, CanvasRuntimeStateRepository};
use agentdash_domain::project::ProjectRepository;
use agentdash_domain::shared_library::ProjectExtensionInstallationRepository;
use agentdash_domain::workflow::RuntimeSessionExecutionAnchorRepository;
use agentdash_spi::platform::tool_capability::CAP_WORKSPACE_MODULE;
use agentdash_spi::{
    AgentTool, AgentToolError, AgentToolResult, ConnectorError, ContentPart, DynAgentTool,
    ExecutionContext, ToolCluster, ToolUpdateCallback, connector::RuntimeToolProvider,
};
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use crate::workspace_module::runtime_bridge::{
    SharedWorkspaceModuleAgentRunBridgeHandle, SharedWorkspaceModuleRuntimeGatewayHandle,
};
use crate::workspace_module::{
    WorkspaceModuleDescribeTool, WorkspaceModuleInvokeTool, WorkspaceModuleListTool,
    WorkspaceModuleOperateTool, WorkspaceModulePresentTool,
    delivery_runtime_session_id_from_context, project_authorization_context_from_identity,
    project_id_from_context, resolve_invocation_backend, shared_runtime_vfs_from_context,
};

#[derive(Clone)]
pub struct WorkspaceModuleRuntimeToolProvider {
    installation_repo: Arc<dyn ProjectExtensionInstallationRepository>,
    project_repo: Arc<dyn ProjectRepository>,
    canvas_repo: Arc<dyn CanvasRepository>,
    canvas_runtime_state_repo: Arc<dyn CanvasRuntimeStateRepository>,
    execution_anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository>,
    agent_run_bridge_handle: SharedWorkspaceModuleAgentRunBridgeHandle,
    runtime_gateway_handle: SharedWorkspaceModuleRuntimeGatewayHandle,
    extension_channel_transport: Option<Arc<dyn ExtensionRuntimeChannelTransport>>,
    extension_backend_service_transport: Option<Arc<dyn ExtensionBackendServiceTransport>>,
}

impl WorkspaceModuleRuntimeToolProvider {
    pub fn new(
        installation_repo: Arc<dyn ProjectExtensionInstallationRepository>,
        project_repo: Arc<dyn ProjectRepository>,
        canvas_repo: Arc<dyn CanvasRepository>,
        canvas_runtime_state_repo: Arc<dyn CanvasRuntimeStateRepository>,
        execution_anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository>,
        agent_run_bridge_handle: SharedWorkspaceModuleAgentRunBridgeHandle,
        runtime_gateway_handle: SharedWorkspaceModuleRuntimeGatewayHandle,
    ) -> Self {
        Self {
            installation_repo,
            project_repo,
            canvas_repo,
            canvas_runtime_state_repo,
            execution_anchor_repo,
            agent_run_bridge_handle,
            runtime_gateway_handle,
            extension_channel_transport: None,
            extension_backend_service_transport: None,
        }
    }

    pub fn with_extension_channel_transport(
        mut self,
        transport: Arc<dyn ExtensionRuntimeChannelTransport>,
    ) -> Self {
        self.extension_channel_transport = Some(transport);
        self
    }

    pub fn with_extension_backend_service_transport(
        mut self,
        transport: Arc<dyn ExtensionBackendServiceTransport>,
    ) -> Self {
        self.extension_backend_service_transport = Some(transport);
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
            diag!(
                Warn,
                Subsystem::AgentRun,
                "workspace module tools 注入失败：无法从 hook session 解析 project_id"
            );
            return Ok(Vec::new());
        };

        let shared_vfs = shared_runtime_vfs_from_context(context)?;
        let delivery_runtime_session_id = delivery_runtime_session_id_from_context(context);
        let current_user = context
            .session
            .identity
            .as_ref()
            .map(project_authorization_context_from_identity);
        let channel_transport_available = self.extension_channel_transport.is_some();
        let backend_readiness = operation_backend_readiness(context, &delivery_runtime_session_id);
        let backend_service_readiness = operation_backend_service_readiness(
            &backend_readiness,
            self.extension_backend_service_transport.is_some(),
        );
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
                .with_agent_run_visibility(
                    self.agent_run_bridge_handle.clone(),
                    delivery_runtime_session_id.clone(),
                )
                .with_runtime_dependencies(
                    self.runtime_gateway_handle.clone(),
                    delivery_runtime_session_id.clone(),
                    channel_transport_available,
                    backend_readiness.clone(),
                    backend_service_readiness.clone(),
                ),
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
                .with_agent_run_visibility(
                    self.agent_run_bridge_handle.clone(),
                    delivery_runtime_session_id.clone(),
                )
                .with_runtime_dependencies(
                    self.runtime_gateway_handle.clone(),
                    delivery_runtime_session_id.clone(),
                    channel_transport_available,
                    backend_readiness.clone(),
                    backend_service_readiness.clone(),
                ),
            ));
        }

        if flow.is_capability_tool_enabled(
            CAP_WORKSPACE_MODULE,
            "workspace_module_operate",
            Some(ToolCluster::WorkspaceModule),
        ) {
            tools.push(Arc::new(
                WorkspaceModuleOperateTool::new(
                    self.project_repo.clone(),
                    self.canvas_repo.clone(),
                    project_id,
                    shared_vfs.clone(),
                    self.agent_run_bridge_handle.clone(),
                    Some(delivery_runtime_session_id.clone()),
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
                &delivery_runtime_session_id,
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
                    self.execution_anchor_repo.clone(),
                    project_id,
                    shared_vfs,
                    self.agent_run_bridge_handle.clone(),
                    delivery_runtime_session_id,
                    context.session.turn_id.clone(),
                )
                .with_current_user(current_user.clone())
                .with_runtime_dependencies(
                    self.runtime_gateway_handle.clone(),
                    channel_transport_available,
                    backend_readiness.clone(),
                    backend_service_readiness.clone(),
                ),
            ));
        }

        Ok(tools)
    }
}

fn operation_backend_readiness(
    context: &ExecutionContext,
    delivery_runtime_session_id: &str,
) -> WorkspaceModuleOperationReadiness {
    match context.session.require_runtime_backend_anchor(
        "workspace_module_operations",
        Some(delivery_runtime_session_id),
    ) {
        Ok(anchor) => {
            if resolve_invocation_backend(context.session.vfs.as_ref(), Some(anchor)).is_some() {
                WorkspaceModuleOperationReadiness::ready()
            } else {
                WorkspaceModuleOperationReadiness::unavailable(
                    WorkspaceModuleOperationReadinessKind::BackendUnavailable,
                    "runtime backend target could not be resolved for workspace module operations",
                )
            }
        }
        Err(error) => WorkspaceModuleOperationReadiness::unavailable(
            WorkspaceModuleOperationReadinessKind::MissingRuntimeBackendAnchor,
            error.to_string(),
        ),
    }
}

fn operation_backend_service_readiness(
    backend_readiness: &WorkspaceModuleOperationReadiness,
    transport_available: bool,
) -> WorkspaceModuleOperationReadiness {
    if !backend_readiness.is_ready() {
        return backend_readiness.clone();
    }
    if transport_available {
        WorkspaceModuleOperationReadiness::ready()
    } else {
        WorkspaceModuleOperationReadiness::unavailable(
            WorkspaceModuleOperationReadinessKind::BackendServiceUnavailable,
            "backendService bridge transport is not attached to this runtime",
        )
    }
}

impl WorkspaceModuleRuntimeToolProvider {
    async fn push_invoke_tool(
        &self,
        context: &ExecutionContext,
        project_id: uuid::Uuid,
        delivery_runtime_session_id: &str,
        current_user: Option<agentdash_domain::project::ProjectAuthorizationContext>,
        tools: &mut Vec<DynAgentTool>,
    ) {
        let (gateway, transport) = match self.invoke_runtime_deps().await {
            Ok(deps) => deps,
            Err(missing_dependencies) => {
                let missing_names = missing_dependencies
                    .iter()
                    .map(|dependency| dependency.as_str())
                    .collect::<Vec<_>>();
                diag!(Warn, Subsystem::AgentRun,

                    missing_dependencies = ?missing_names,
                    "workspace_module_invoke 装配为诊断工具：缺少 RuntimeGateway 或 channel transport 注入"
                );
                tools.push(Arc::new(WorkspaceModuleInvokeUnavailableTool::new(
                    missing_dependencies,
                )));
                return;
            }
        };

        let backend_anchor = match context.session.require_runtime_backend_anchor(
            "workspace_module_invoke",
            Some(delivery_runtime_session_id),
        ) {
            Ok(anchor) => anchor,
            Err(error) => {
                let diagnostic_context = DiagnosticErrorContext::new(
                    "workspace_module.runtime_tool_provider",
                    "runtime_backend_anchor",
                );
                diag_error!(Warn, Subsystem::AgentRun,
                    context = &diagnostic_context,
                    error = &error,
                    delivery_runtime_session_id = %delivery_runtime_session_id,
                    project_id = %project_id,
                    tool_name = "workspace_module_invoke",
                    "workspace_module_invoke runtime backend anchor resolution failed"
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
        let backend_service_invoker =
            self.extension_backend_service_transport
                .as_ref()
                .map(|transport| {
                    Arc::new(ExtensionRuntimeBackendServiceInvoker::new(
                        self.installation_repo.clone(),
                        transport.clone(),
                    ))
                });
        tools.push(Arc::new(
            WorkspaceModuleInvokeTool::new(
                self.installation_repo.clone(),
                self.canvas_repo.clone(),
                self.canvas_runtime_state_repo.clone(),
                self.execution_anchor_repo.clone(),
                project_id,
                delivery_runtime_session_id.to_string(),
                None,
                backend,
                gateway,
                channel_invoker,
                backend_service_invoker,
            )
            .with_current_user(current_user)
            .with_agent_run_visibility(self.agent_run_bridge_handle.clone()),
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
