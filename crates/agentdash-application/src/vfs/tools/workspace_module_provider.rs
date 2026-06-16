use std::sync::Arc;

use agentdash_application_ports::extension_runtime::ExtensionRuntimeChannelTransport;
use agentdash_domain::canvas::CanvasRepository;
use agentdash_domain::shared_library::ProjectExtensionInstallationRepository;
use agentdash_spi::connector::RuntimeToolProvider;
use agentdash_spi::platform::tool_capability::CAP_WORKSPACE_MODULE;
use agentdash_spi::{ConnectorError, DynAgentTool, ExecutionContext, ToolCluster};
use async_trait::async_trait;

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
        let visibility = flow.workspace_module.clone();
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
                    visibility.clone(),
                )
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
                    visibility.clone(),
                )
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
                .with_turn_id(context.session.turn_id.clone()),
            ));
        }

        if flow.is_capability_tool_enabled(
            CAP_WORKSPACE_MODULE,
            "workspace_module_invoke",
            Some(ToolCluster::WorkspaceModule),
        ) {
            self.push_invoke_tool(context, project_id, &visibility, &session_id, &mut tools)
                .await;
        }

        if flow.is_capability_tool_enabled(
            CAP_WORKSPACE_MODULE,
            "workspace_module_present",
            Some(ToolCluster::WorkspaceModule),
        ) {
            tools.push(Arc::new(WorkspaceModulePresentTool::new(
                self.installation_repo.clone(),
                self.canvas_repo.clone(),
                project_id,
                visibility.clone(),
                shared_vfs,
                self.session_services_handle.clone(),
                session_id,
                context.session.turn_id.clone(),
            )));
        }

        Ok(tools)
    }
}

impl WorkspaceModuleRuntimeToolProvider {
    async fn push_invoke_tool(
        &self,
        context: &ExecutionContext,
        project_id: uuid::Uuid,
        visibility: &agentdash_spi::WorkspaceModuleDimension,
        session_id: &str,
        tools: &mut Vec<DynAgentTool>,
    ) {
        let Some((gateway, transport)) = self.invoke_runtime_deps().await else {
            tracing::warn!(
                "workspace_module_invoke 未装配：缺少 RuntimeGateway 或 channel transport 注入"
            );
            return;
        };

        let backend = resolve_invocation_backend(
            context.session.vfs.as_ref(),
            context
                .session
                .backend_execution
                .as_ref()
                .map(|placement| placement.backend_id.as_str()),
        );
        let channel_invoker = Arc::new(ExtensionRuntimeChannelInvoker::new(
            self.installation_repo.clone(),
            transport,
        ));
        tools.push(Arc::new(
            WorkspaceModuleInvokeTool::new(
                self.installation_repo.clone(),
                self.canvas_repo.clone(),
                project_id,
                visibility.clone(),
                session_id.to_string(),
                None,
                backend,
                gateway,
                channel_invoker,
            )
            .with_runtime_visibility(self.session_services_handle.clone()),
        ));
    }

    async fn invoke_runtime_deps(
        &self,
    ) -> Option<(
        Arc<RuntimeGateway>,
        Arc<dyn ExtensionRuntimeChannelTransport>,
    )> {
        Some((
            self.runtime_gateway_handle.get().await?,
            self.extension_channel_transport.as_ref()?.clone(),
        ))
    }
}
