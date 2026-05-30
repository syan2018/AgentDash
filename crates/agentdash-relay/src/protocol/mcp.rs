use serde::{Deserialize, Serialize};

use super::McpToolInfoRelay;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct McpHttpHeaderRelay {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct McpEnvVarRelay {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum McpTransportConfigRelay {
    Http {
        url: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        headers: Vec<McpHttpHeaderRelay>,
    },
    Sse {
        url: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        headers: Vec<McpHttpHeaderRelay>,
    },
    Stdio {
        command: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        args: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        env: Vec<McpEnvVarRelay>,
    },
}

/// 一次性 probe 命令——临时连接任意 transport 并探测工具列表（不入连接池）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandMcpProbeTransportPayload {
    pub transport: McpTransportConfigRelay,
}

/// 一次性 probe 响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseMcpProbeTransportPayload {
    /// "ok" | "error" | "unsupported"
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<McpToolInfoRelay>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandMcpListToolsPayload {
    pub server_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandMcpCallToolPayload {
    pub server_name: String,
    pub tool_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<serde_json::Map<String, serde_json::Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandMcpClosePayload {
    pub server_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseMcpListToolsPayload {
    pub server_name: String,
    pub tools: Vec<McpToolInfoRelay>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseMcpCallToolPayload {
    pub server_name: String,
    pub tool_name: String,
    pub content: String,
    #[serde(default)]
    pub is_error: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseMcpClosePayload {
    pub server_name: String,
    pub status: String,
}
