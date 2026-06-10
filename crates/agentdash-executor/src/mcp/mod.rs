//! 通用 MCP 工具发现与适配层
//!
//! 此模块将 MCP server 声明转化为 SPI `AgentTool` 实例，供任意 connector 使用。
//! 包含两种模式：
//! - **direct**: 云端直连 HTTP MCP server（Streamable HTTP 传输）
//! - **relay**: 通过 relay 信道代理到本机 backend 上的 MCP server

use agentdash_spi::DynAgentTool;

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

pub mod direct;
pub mod relay;

pub use direct::{discover_mcp_tool_entries, discover_mcp_tools, namespaced_tool_name};
pub use relay::{discover_relay_mcp_tool_entries, discover_relay_mcp_tools};
