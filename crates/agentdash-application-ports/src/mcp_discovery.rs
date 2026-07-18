use async_trait::async_trait;

use agentdash_agent_types::DynAgentTool;
use agentdash_platform_spi::{
    CapabilityState, PlatformRuntimeError, RelayMcpCallContext, RuntimeMcpServer,
    RuntimeMcpSourceReadiness,
};

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

#[derive(Clone)]
pub struct McpToolSourceOutcome {
    pub server: RuntimeMcpServer,
}

impl McpToolSourceOutcome {
    pub fn ready(mut server: RuntimeMcpServer, tool_count: usize) -> Self {
        server.readiness = RuntimeMcpSourceReadiness::ready(tool_count);
        Self { server }
    }

    pub fn unavailable(
        mut server: RuntimeMcpServer,
        reason_code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        server.readiness = RuntimeMcpSourceReadiness::unavailable(reason_code, message);
        Self { server }
    }
}

#[derive(Clone, Default)]
pub struct McpToolDiscoveryOutcome {
    pub tools: Vec<DiscoveredMcpTool>,
    pub sources: Vec<McpToolSourceOutcome>,
}

pub struct McpToolDiscoveryRequest {
    pub servers: Vec<RuntimeMcpServer>,
    pub capability_state: CapabilityState,
    pub call_context: Option<RelayMcpCallContext>,
}

#[async_trait]
pub trait McpToolDiscovery: Send + Sync {
    async fn discover_tool_entries(
        &self,
        request: McpToolDiscoveryRequest,
    ) -> Result<McpToolDiscoveryOutcome, PlatformRuntimeError>;
}
