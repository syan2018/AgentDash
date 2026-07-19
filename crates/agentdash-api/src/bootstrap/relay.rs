use std::sync::Arc;

use tokio::sync::broadcast;

use crate::relay::registry::BackendRegistry;
use crate::relay::runtime_wire::CloudRuntimeWirePlacementRegistry;

pub(crate) struct RelayBootstrapOutput {
    pub backend_registry: Arc<BackendRegistry>,
    pub backend_runtime_events: broadcast::Sender<String>,
    pub mcp_probe_relay: Arc<dyn agentdash_platform_spi::McpRelayProvider>,
    pub setup_action_transport:
        Arc<dyn agentdash_application_ports::backend_transport::BackendTransport>,
    pub shell_output_registry: Arc<agentdash_relay::ShellOutputRegistry>,
    pub runtime_wire_placements: Arc<CloudRuntimeWirePlacementRegistry>,
}

pub(crate) fn build_relay_runtime(channel_capacity: usize) -> RelayBootstrapOutput {
    let backend_registry = BackendRegistry::new();
    let (backend_runtime_events, _) = broadcast::channel(channel_capacity);
    let mcp_probe_relay: Arc<dyn agentdash_platform_spi::McpRelayProvider> =
        backend_registry.clone();
    let setup_action_transport: Arc<
        dyn agentdash_application_ports::backend_transport::BackendTransport,
    > = backend_registry.clone();
    let shell_output_registry = agentdash_relay::ShellOutputRegistry::new();
    let runtime_wire_placements = CloudRuntimeWirePlacementRegistry::new();

    RelayBootstrapOutput {
        backend_registry,
        backend_runtime_events,
        mcp_probe_relay,
        setup_action_transport,
        shell_output_registry,
        runtime_wire_placements,
    }
}
