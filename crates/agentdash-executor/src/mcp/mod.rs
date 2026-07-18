//! 通用 MCP 工具发现与适配层
//!
//! 此模块将 MCP server 声明转化为 SPI `AgentTool` 实例，供任意 connector 使用。
//! 包含两种模式：
//! - **direct**: 云端直连 HTTP MCP server（Streamable HTTP 传输）
//! - **relay**: 通过 relay 信道代理到本机 backend 上的 MCP server

use std::sync::Arc;

use agentdash_application_ports::mcp_discovery::{
    DiscoveredMcpTool, McpToolDiscovery, McpToolDiscoveryOutcome, McpToolDiscoveryRequest,
};
use agentdash_platform_spi::{PlatformRuntimeError, McpRelayProvider};
use async_trait::async_trait;

mod common;
pub mod direct;
pub mod naming;
pub mod relay;

pub use direct::{discover_mcp_tool_entries, discover_mcp_tools};
pub use naming::namespaced_tool_name;
pub use relay::{discover_relay_mcp_tool_entries, discover_relay_mcp_tools};

#[derive(Clone, Default)]
pub struct ExecutorMcpToolDiscovery {
    relay_provider: Option<Arc<dyn McpRelayProvider>>,
}

impl ExecutorMcpToolDiscovery {
    pub fn new(relay_provider: Option<Arc<dyn McpRelayProvider>>) -> Self {
        Self { relay_provider }
    }
}

#[async_trait]
impl McpToolDiscovery for ExecutorMcpToolDiscovery {
    async fn discover_tool_entries(
        &self,
        request: McpToolDiscoveryRequest,
    ) -> Result<McpToolDiscoveryOutcome, PlatformRuntimeError> {
        let (relay_servers, direct_servers) =
            agentdash_platform_spi::partition_runtime_mcp_servers(&request.servers);
        let mut outcome =
            direct::discover_mcp_tool_outcome(&direct_servers, &request.capability_state).await?;

        if let Some(relay_provider) = &self.relay_provider {
            let relay_outcome = relay::discover_relay_mcp_tool_outcome(
                relay_provider.clone(),
                &relay_servers,
                &request.capability_state,
                request.call_context,
            )
            .await;
            outcome.tools.extend(relay_outcome.tools);
            outcome.sources.extend(relay_outcome.sources);
        } else {
            outcome
                .sources
                .extend(relay_servers.into_iter().map(|server| {
                    agentdash_application_ports::mcp_discovery::McpToolSourceOutcome::unavailable(
                        server,
                        "relay_provider_missing",
                        "relay MCP discovery requires a relay provider",
                    )
                }));
        }

        Ok(outcome)
    }
}
