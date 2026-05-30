use std::sync::Arc;

use agentdash_application::runtime_gateway::{
    ExtensionRuntimeActionProvider, McpCallToolProvider, McpListToolsProvider,
    McpProbeTransportProvider, RuntimeGateway, RuntimeSessionMcpAccess,
    WorkspaceBrowseDirectoryProvider, WorkspaceDetectGitProvider, WorkspaceDetectProvider,
};
use agentdash_application_ports::extension_runtime::ExtensionRuntimeActionTransport;
use agentdash_domain::shared_library::ProjectExtensionInstallationRepository;

pub(crate) fn build_runtime_gateway(
    mcp_probe_relay: Arc<dyn agentdash_spi::McpRelayProvider>,
    setup_action_transport: Arc<
        dyn agentdash_application_ports::backend_transport::BackendTransport,
    >,
    session_mcp_access: Arc<dyn RuntimeSessionMcpAccess>,
    extension_installations: Arc<dyn ProjectExtensionInstallationRepository>,
    extension_action_transport: Arc<dyn ExtensionRuntimeActionTransport>,
) -> Arc<RuntimeGateway> {
    Arc::new(
        RuntimeGateway::new()
            .with_provider(Arc::new(McpProbeTransportProvider::new(
                Some(mcp_probe_relay),
                Arc::new(agentdash_infrastructure::RmcpProbeTransport::new()),
            )))
            .with_provider(Arc::new(WorkspaceDetectProvider::new(
                setup_action_transport.clone(),
            )))
            .with_provider(Arc::new(WorkspaceDetectGitProvider::new(
                setup_action_transport.clone(),
            )))
            .with_provider(Arc::new(WorkspaceBrowseDirectoryProvider::new(
                setup_action_transport,
            )))
            .with_provider(Arc::new(McpListToolsProvider::new(
                session_mcp_access.clone(),
            )))
            .with_provider(Arc::new(McpCallToolProvider::new(session_mcp_access)))
            .with_dynamic_provider(Arc::new(ExtensionRuntimeActionProvider::new(
                extension_installations,
                extension_action_transport,
            ))),
    )
}
