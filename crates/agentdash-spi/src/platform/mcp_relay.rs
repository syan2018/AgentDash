//! MCP Relay Provider SPI — 云端通过 relay 信道调用本机 MCP 工具的抽象层

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::{AuthIdentity, ConnectorError, RuntimeMcpServerDeclaration, Vfs};

/// relay MCP 工具描述
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayMcpToolInfo {
    pub server_name: String,
    pub server: RuntimeMcpServerDeclaration,
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

/// relay MCP 调用时由 application 注入的 session/VFS 上下文。
#[derive(Debug, Clone)]
pub struct RelayMcpCallContext {
    pub session_id: String,
    pub turn_id: Option<String>,
    pub tool_call_id: Option<String>,
    pub vfs: Option<Vfs>,
    pub identity: Option<AuthIdentity>,
}

/// relay probe 结果
#[derive(Debug, Clone)]
pub struct RelayProbeResult {
    pub status: String,
    pub latency_ms: Option<u64>,
    pub tools: Option<Vec<RelayProbeTool>>,
    pub error: Option<String>,
}

/// relay probe 发现的单个工具
#[derive(Debug, Clone)]
pub struct RelayProbeTool {
    pub name: String,
    pub description: String,
}

/// 通过 relay 信道发现和调用本机 MCP 工具的提供者。
///
/// 由 API 层实现（基于 BackendRegistry），由 executor 层消费（RelayMcpToolAdapter）。
#[async_trait]
pub trait McpRelayProvider: Send + Sync {
    /// 列出指定 server 的 MCP 工具（通过 relay 信道）
    async fn list_relay_tools(
        &self,
        requested_servers: &[RuntimeMcpServerDeclaration],
        context: Option<RelayMcpCallContext>,
    ) -> Vec<RelayMcpToolInfo>;

    /// 调用指定 MCP server 上的工具
    async fn call_relay_tool(
        &self,
        server: &RuntimeMcpServerDeclaration,
        tool_name: &str,
        arguments: Option<serde_json::Map<String, serde_json::Value>>,
        context: Option<RelayMcpCallContext>,
    ) -> Result<RelayMcpCallResult, ConnectorError>;

    /// 一次性 probe：通过 relay 下发 transport 配置，探测连通性和工具列表。
    /// 失败返回 Err（relay 通道离线等）。
    async fn probe_transport(
        &self,
        transport: &agentdash_domain::mcp_preset::McpTransportConfig,
    ) -> Result<RelayProbeResult, ConnectorError>;
}
