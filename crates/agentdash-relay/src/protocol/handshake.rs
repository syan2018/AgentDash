use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmptyPayload {}

// ── 注册 ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterPayload {
    pub backend_id: String,
    pub name: String,
    pub version: String,
    pub capabilities: CapabilitiesPayload,
    pub workspace_roots: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterAckPayload {
    pub backend_id: String,
    pub status: String,
    pub server_time: i64,
}

// ── 心跳 ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PingPayload {
    pub server_time: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PongPayload {
    pub client_time: i64,
}

// ── 能力 ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilitiesPayload {
    pub executors: Vec<AgentInfoRelay>,
    #[serde(default)]
    pub supports_cancel: bool,
    #[serde(default)]
    pub supports_discover_options: bool,
    /// 该 backend 实例可提供的 MCP server 列表（relay 用）
    #[serde(default)]
    pub mcp_servers: Vec<McpServerInfoRelay>,
}

/// backend 上报的 MCP server 能力描述
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerInfoRelay {
    pub name: String,
    /// "stdio" | "http" | "sse"
    pub transport: String,
}

/// MCP 工具描述（用于 relay 协议传输）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolInfoRelay {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub parameters_schema: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInfoRelay {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub variants: Vec<String>,
    #[serde(default = "default_true")]
    pub available: bool,
}

fn default_true() -> bool {
    true
}
