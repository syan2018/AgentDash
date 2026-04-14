//! MCP Relay Provider SPI — 云端通过 relay 信道调用本机 MCP 工具的抽象层

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::ConnectorError;

/// relay MCP 工具描述
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayMcpToolInfo {
    pub server_name: String,
    pub tool_name: String,
    pub description: String,
    #[serde(default)]
    pub parameters_schema: serde_json::Value,
}

/// relay MCP 工具调用结果
#[derive(Debug, Clone)]
pub struct RelayMcpCallResult {
    pub content: String,
    pub is_error: bool,
}

/// 通过 relay 信道发现和调用本机 MCP 工具的提供者。
///
/// 由 API 层实现（基于 BackendRegistry），由 executor 层消费（RelayMcpToolAdapter）。
#[async_trait]
pub trait McpRelayProvider: Send + Sync {
    /// 列出所有在线 backend 上报的 MCP 工具
    async fn list_relay_tools(&self) -> Vec<RelayMcpToolInfo>;

    /// 调用指定 MCP server 上的工具
    async fn call_relay_tool(
        &self,
        server_name: &str,
        tool_name: &str,
        arguments: Option<serde_json::Map<String, serde_json::Value>>,
    ) -> Result<RelayMcpCallResult, ConnectorError>;
}
