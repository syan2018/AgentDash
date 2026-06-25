use std::sync::Arc;

use tokio::sync::broadcast;

use crate::relay::registry::BackendRegistry;

pub(crate) struct RelayBootstrapOutput {
    pub backend_registry: Arc<BackendRegistry>,
    pub backend_runtime_events: broadcast::Sender<String>,
    pub mcp_probe_relay: Arc<dyn agentdash_spi::McpRelayProvider>,
    pub setup_action_transport:
        Arc<dyn agentdash_application_ports::backend_transport::BackendTransport>,
    pub shell_output_registry: Arc<agentdash_relay::ShellOutputRegistry>,
    pub terminal_cache:
        Arc<agentdash_application_runtime_session::session::terminal_cache::SessionTerminalCache>,
}

pub(crate) fn build_relay_runtime(channel_capacity: usize) -> RelayBootstrapOutput {
    let backend_registry = BackendRegistry::new();
    let (backend_runtime_events, _) = broadcast::channel(channel_capacity);
    let mcp_probe_relay: Arc<dyn agentdash_spi::McpRelayProvider> = backend_registry.clone();
    let setup_action_transport: Arc<
        dyn agentdash_application_ports::backend_transport::BackendTransport,
    > = backend_registry.clone();
    let shell_output_registry = agentdash_relay::ShellOutputRegistry::new();
    let terminal_cache =
        agentdash_application_runtime_session::session::terminal_cache::SessionTerminalCache::new();

    RelayBootstrapOutput {
        backend_registry,
        backend_runtime_events,
        mcp_probe_relay,
        setup_action_transport,
        shell_output_registry,
        terminal_cache,
    }
}
