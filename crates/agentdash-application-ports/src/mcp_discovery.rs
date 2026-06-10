use async_trait::async_trait;

use agentdash_agent_types::DynAgentTool;
use agentdash_spi::{CapabilityState, ConnectorError, RelayMcpCallContext, SessionMcpServer};

#[derive(Clone)]
pub struct DiscoveredMcpTool {
    pub runtime_name: String,
    pub server_name: String,
    pub tool_name: String,
    pub uses_relay: bool,
    pub description: String,
    pub parameters_schema: serde_json::Value,
    pub tool: DynAgentTool,
}

pub struct McpToolDiscoveryRequest {
    pub servers: Vec<SessionMcpServer>,
    pub capability_state: CapabilityState,
    pub call_context: Option<RelayMcpCallContext>,
}

#[async_trait]
pub trait McpToolDiscovery: Send + Sync {
    async fn discover_tool_entries(
        &self,
        request: McpToolDiscoveryRequest,
    ) -> Result<Vec<DiscoveredMcpTool>, ConnectorError>;
}
