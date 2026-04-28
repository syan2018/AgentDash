//! MCP Preset probe — 临时连接 MCP Server 并获取工具列表，用于连通性检查 + 工具发现。
//!
//! 行为：
//! - Http / Sse transport：使用 `rmcp` StreamableHttp client 建立临时连接 → `list_all_tools()` → 关闭
//! - Stdio transport：返回 `Unsupported`（需要通过 relay 下发给 local 端，暂不支持）
//!
//! 所有连接操作都在 15 秒超时下执行。

use std::time::{Duration, Instant};

use agentdash_domain::mcp_preset::McpTransportConfig;
use rmcp::{
    ServiceExt,
    transport::streamable_http_client::{
        StreamableHttpClientTransportConfig, StreamableHttpClientWorker,
    },
};
use serde::Serialize;
use tokio::time::timeout;

/// Probe 超时（秒）——覆盖连接 + tools/list 全过程。
const PROBE_TIMEOUT_SECS: u64 = 15;

/// Probe 结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ProbeResult {
    /// 连接成功并获取到工具列表。
    Ok {
        latency_ms: u64,
        tools: Vec<ProbeTool>,
    },
    /// 连接失败或超时。
    Error { error: String },
    /// 当前 transport 不支持云端 probe（stdio）。
    Unsupported { reason: String },
}

/// Probe 发现的单个工具信息。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ProbeTool {
    pub name: String,
    pub description: String,
}

/// 根据 transport 配置执行 probe（连通性检查 + 工具发现合一）。
///
/// 行为：
/// - Http / Sse：建立临时连接 → `tools/list` → 关闭
/// - Stdio：返回 `Unsupported`（后续通过 relay 下发给 local 端）
pub async fn probe_transport(transport: &McpTransportConfig) -> ProbeResult {
    match transport {
        McpTransportConfig::Http { url, .. } | McpTransportConfig::Sse { url, .. } => {
            probe_http(url).await
        }
        McpTransportConfig::Stdio { .. } => ProbeResult::Unsupported {
            reason: "Stdio transport 需要通过本地 relay 探测，当前暂不支持".to_string(),
        },
    }
}

async fn probe_http(url: &str) -> ProbeResult {
    let start = Instant::now();
    let fut = async {
        let worker = StreamableHttpClientWorker::new(
            reqwest::Client::new(),
            StreamableHttpClientTransportConfig::with_uri(url.to_string()),
        );
        let client = ().serve(worker).await.map_err(|e| format!("连接 MCP Server 失败: {e}"))?;
        let tools = client
            .list_all_tools()
            .await
            .map_err(|e| format!("list_tools 失败: {e}"))?;
        let _ = client.cancel().await;
        Ok::<Vec<rmcp::model::Tool>, String>(tools)
    };

    match timeout(Duration::from_secs(PROBE_TIMEOUT_SECS), fut).await {
        Ok(Ok(tools)) => {
            let latency_ms = start.elapsed().as_millis() as u64;
            let tools = tools
                .into_iter()
                .map(|t| ProbeTool {
                    name: t.name.to_string(),
                    description: t.description.as_deref().unwrap_or("").to_string(),
                })
                .collect();
            ProbeResult::Ok { latency_ms, tools }
        }
        Ok(Err(err)) => ProbeResult::Error { error: err },
        Err(_) => ProbeResult::Error {
            error: format!("probe 超时（{PROBE_TIMEOUT_SECS}s）"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::mcp_preset::McpEnvVar;

    #[tokio::test]
    async fn stdio_transport_returns_unsupported() {
        let transport = McpTransportConfig::Stdio {
            command: "npx".to_string(),
            args: vec!["@modelcontextprotocol/server-filesystem".to_string()],
            env: vec![McpEnvVar {
                name: "FOO".to_string(),
                value: "bar".to_string(),
            }],
        };
        match probe_transport(&transport).await {
            ProbeResult::Unsupported { reason } => {
                assert!(reason.contains("Stdio"));
            }
            other => panic!("expected Unsupported, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn http_probe_fails_gracefully_on_unreachable() {
        // 使用 localhost 上一个不存在的端口，预期快速失败
        let transport = McpTransportConfig::Http {
            url: "http://127.0.0.1:1/mcp".to_string(),
            headers: vec![],
        };
        match probe_transport(&transport).await {
            ProbeResult::Error { error } => {
                assert!(!error.is_empty(), "error 信息不应为空");
            }
            other => panic!("expected Error, got: {other:?}"),
        }
    }

    #[test]
    fn probe_result_serialize_ok_shape() {
        let result = ProbeResult::Ok {
            latency_ms: 123,
            tools: vec![ProbeTool {
                name: "read_file".to_string(),
                description: "read".to_string(),
            }],
        };
        let json = serde_json::to_value(&result).expect("serialize");
        assert_eq!(json["status"], "ok");
        assert_eq!(json["latency_ms"], 123);
        assert_eq!(json["tools"][0]["name"], "read_file");
    }

    #[test]
    fn probe_result_serialize_error_shape() {
        let result = ProbeResult::Error {
            error: "连接失败".to_string(),
        };
        let json = serde_json::to_value(&result).expect("serialize");
        assert_eq!(json["status"], "error");
        assert_eq!(json["error"], "连接失败");
    }

    #[test]
    fn probe_result_serialize_unsupported_shape() {
        let result = ProbeResult::Unsupported {
            reason: "Stdio 暂不支持".to_string(),
        };
        let json = serde_json::to_value(&result).expect("serialize");
        assert_eq!(json["status"], "unsupported");
        assert_eq!(json["reason"], "Stdio 暂不支持");
    }
}
