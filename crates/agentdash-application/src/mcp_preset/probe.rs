//! MCP Preset probe — 临时连接 MCP Server 并获取工具列表，用于连通性检查 + 工具发现。
//!
//! 行为：
//! - Http / Sse transport：使用 `rmcp` StreamableHttp client 建立临时连接 → `list_all_tools()` → 关闭
//! - Stdio transport：通过 relay 下发给本机后端（`agentdash-local`）执行一次性 probe
//!
//! 所有连接操作都在 15 秒超时下执行。

use std::time::{Duration, Instant};

use agentdash_domain::mcp_preset::McpTransportConfig;
use agentdash_spi::platform::mcp_relay::McpRelayProvider;
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
/// - Stdio：通过 relay 下发给本机后端探测；relay 不可用时返回 error
pub async fn probe_transport(
    transport: &McpTransportConfig,
    relay: Option<&dyn McpRelayProvider>,
) -> ProbeResult {
    match transport {
        McpTransportConfig::Http { url, .. } | McpTransportConfig::Sse { url, .. } => {
            probe_http(url).await
        }
        McpTransportConfig::Stdio { .. } => match relay {
            Some(relay) => probe_via_relay(relay, transport).await,
            None => ProbeResult::Error {
                error: "本机 relay 未连接，无法探测 Stdio transport".to_string(),
            },
        },
    }
}

/// 通过 relay 信道下发 probe 指令给本机后端
async fn probe_via_relay(
    relay: &dyn McpRelayProvider,
    transport: &McpTransportConfig,
) -> ProbeResult {
    match relay.probe_transport(transport).await {
        Ok(result) => match result.status.as_str() {
            "ok" => ProbeResult::Ok {
                latency_ms: result.latency_ms.unwrap_or(0),
                tools: result
                    .tools
                    .unwrap_or_default()
                    .into_iter()
                    .map(|t| ProbeTool {
                        name: t.name,
                        description: t.description,
                    })
                    .collect(),
            },
            _ => ProbeResult::Error {
                error: result.error.unwrap_or_else(|| "探测失败".to_string()),
            },
        },
        Err(e) => ProbeResult::Error {
            error: format!("relay 通信失败: {e}"),
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
    async fn stdio_without_relay_returns_error() {
        let transport = McpTransportConfig::Stdio {
            command: "npx".to_string(),
            args: vec!["@modelcontextprotocol/server-filesystem".to_string()],
            env: vec![McpEnvVar {
                name: "FOO".to_string(),
                value: "bar".to_string(),
            }],
        };
        match probe_transport(&transport, None).await {
            ProbeResult::Error { error } => {
                assert!(error.contains("relay"), "应提示 relay 不可用: {error}");
            }
            other => panic!("expected Error, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn http_probe_fails_gracefully_on_unreachable() {
        let transport = McpTransportConfig::Http {
            url: "http://127.0.0.1:1/mcp".to_string(),
            headers: vec![],
        };
        match probe_transport(&transport, None).await {
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
