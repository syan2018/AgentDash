//! MCP Preset probe — 临时连接 MCP Server 并获取工具列表，用于连通性检查 + 工具发现。
//!
//! 行为：
//! - Http / Sse transport：使用 `rmcp` StreamableHttp client 建立临时连接 → `list_all_tools()` → 关闭
//! - Stdio transport：通过 relay 下发给本机后端（`agentdash-local`）执行一次性 probe
//!
//! 所有连接操作都在 15 秒超时下执行。

use std::time::{Duration, Instant};

use agentdash_domain::mcp_preset::{
    McpHttpHeader, McpRoutePolicy, McpRuntimeBindingConfig, McpRuntimeBindingSource,
    McpTransportConfig,
};
use agentdash_platform_spi::platform::mcp_probe::McpProbeTransport;
use agentdash_platform_spi::platform::mcp_relay::{McpRelayProvider, RelayProbeTarget};
use serde::{Deserialize, Serialize};
use tokio::time::timeout;

/// Probe 超时（秒）——覆盖连接 + tools/list 全过程。
const PROBE_TIMEOUT_SECS: u64 = 15;

/// Probe 结果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProbeTool {
    pub name: String,
    pub description: String,
}

/// 根据 transport 配置执行 probe（连通性检查 + 工具发现合一）。
///
/// 行为：
/// - Relay policy：通过 relay 下发给本机后端探测；relay 不可用时返回 error
/// - Direct HTTP / SSE：建立临时连接 → `tools/list` → 关闭
/// - Direct Stdio：setup context 无本机进程语义，返回 unsupported
pub async fn probe_transport(
    transport: &McpTransportConfig,
    route_policy: McpRoutePolicy,
    relay_target: Option<RelayProbeTarget>,
    relay: Option<&dyn McpRelayProvider>,
    http_probe: &dyn McpProbeTransport,
) -> ProbeResult {
    if route_policy.uses_relay(transport) {
        let Some(target) = relay_target else {
            return ProbeResult::Unsupported {
                reason: "当前用户没有可用于 MCP relay 探测的在线本机 runtime".to_string(),
            };
        };
        return match relay {
            Some(relay) => probe_via_relay(relay, transport, target).await,
            None => ProbeResult::Error {
                error: "本机 relay 未连接，无法探测 relay MCP transport".to_string(),
            },
        };
    }

    match transport {
        McpTransportConfig::Http { url, headers } | McpTransportConfig::Sse { url, headers } => {
            probe_http(http_probe, url, headers).await
        }
        McpTransportConfig::Stdio { .. } => ProbeResult::Unsupported {
            reason: "Direct Stdio transport 需要在本机 relay 中探测".to_string(),
        },
    }
}

/// 普通 Preset probe 没有 runtime context；required runtime binding 不能被静态探测伪装成功。
pub async fn probe_transport_without_runtime_context(
    transport: &McpTransportConfig,
    route_policy: McpRoutePolicy,
    runtime_binding: Option<&McpRuntimeBindingConfig>,
    relay_target: Option<RelayProbeTarget>,
    relay: Option<&dyn McpRelayProvider>,
    http_probe: &dyn McpProbeTransport,
) -> ProbeResult {
    if let Some(reason) = required_runtime_binding_unsupported_reason(runtime_binding) {
        return ProbeResult::Unsupported { reason };
    }

    probe_transport(transport, route_policy, relay_target, relay, http_probe).await
}

fn required_runtime_binding_unsupported_reason(
    runtime_binding: Option<&McpRuntimeBindingConfig>,
) -> Option<String> {
    let binding = runtime_binding?;
    let required_sources = binding
        .bindings
        .iter()
        .filter(|rule| rule.required)
        .map(|rule| runtime_binding_source_path(&rule.source))
        .collect::<Vec<_>>();
    if required_sources.is_empty() {
        return None;
    }

    Some(format!(
        "该 Preset 包含 required runtime_binding，需要 runtime context 后才能探测：{}",
        required_sources.join(", ")
    ))
}

fn runtime_binding_source_path(source: &McpRuntimeBindingSource) -> String {
    match source {
        McpRuntimeBindingSource::VfsRootRef => "vfs.main.root_ref".to_string(),
        McpRuntimeBindingSource::RuntimeBackendAnchorBackendId => {
            "runtime_backend_anchor.backend_id".to_string()
        }
        McpRuntimeBindingSource::WorkspaceId => "workspace.id".to_string(),
        McpRuntimeBindingSource::WorkspaceBindingId => "workspace.binding_id".to_string(),
        McpRuntimeBindingSource::WorkspaceIdentity { path } => {
            format!("workspace.identity.{}", path.join("."))
        }
        McpRuntimeBindingSource::WorkspaceDetectedFact { path } => {
            format!("workspace.detected_facts.{}", path.join("."))
        }
    }
}

/// 通过 relay 信道下发 probe 指令给本机后端
async fn probe_via_relay(
    relay: &dyn McpRelayProvider,
    transport: &McpTransportConfig,
    target: RelayProbeTarget,
) -> ProbeResult {
    match relay.probe_transport(transport, target).await {
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

async fn probe_http(
    http_probe: &dyn McpProbeTransport,
    url: &str,
    headers: &[McpHttpHeader],
) -> ProbeResult {
    let start = Instant::now();

    match timeout(
        Duration::from_secs(PROBE_TIMEOUT_SECS),
        http_probe.probe_http(url, headers),
    )
    .await
    {
        Ok(Ok(tools)) => {
            let latency_ms = start.elapsed().as_millis() as u64;
            let tools = tools
                .into_iter()
                .map(|t| ProbeTool {
                    name: t.name,
                    description: t.description,
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
    use agentdash_domain::mcp_preset::{McpEnvVar, McpRuntimeBindingRule, McpRuntimeBindingTarget};
    use agentdash_infrastructure::RmcpProbeTransport;
    use agentdash_platform_spi::{
        PlatformRuntimeError, RuntimeMcpServer,
        platform::mcp_relay::{
            RelayMcpCallContext, RelayMcpCallResult, RelayMcpListOutcome, RelayProbeResult,
            RelayProbeTarget, RelayProbeTool,
        },
    };
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct CapturingHttpProbe {
        headers: Arc<Mutex<Vec<McpHttpHeader>>>,
    }

    #[async_trait::async_trait]
    impl McpProbeTransport for CapturingHttpProbe {
        async fn probe_http(
            &self,
            _url: &str,
            headers: &[McpHttpHeader],
        ) -> Result<Vec<agentdash_platform_spi::platform::mcp_probe::McpProbedTool>, String> {
            *self.headers.lock().expect("headers lock") = headers.to_vec();
            Ok(Vec::new())
        }
    }

    #[derive(Clone, Default)]
    struct FakeRelayProbe {
        transports: Arc<Mutex<Vec<McpTransportConfig>>>,
    }

    #[async_trait::async_trait]
    impl McpRelayProvider for FakeRelayProbe {
        async fn list_relay_tools(
            &self,
            _requested_servers: &[RuntimeMcpServer],
            _context: Option<RelayMcpCallContext>,
        ) -> RelayMcpListOutcome {
            RelayMcpListOutcome::default()
        }

        async fn call_relay_tool(
            &self,
            _server: &RuntimeMcpServer,
            _tool_name: &str,
            _arguments: Option<serde_json::Map<String, serde_json::Value>>,
            _context: Option<RelayMcpCallContext>,
        ) -> Result<RelayMcpCallResult, PlatformRuntimeError> {
            Err(PlatformRuntimeError::Runtime(
                "not implemented in probe test".to_string(),
            ))
        }

        async fn probe_transport(
            &self,
            transport: &McpTransportConfig,
            _target: RelayProbeTarget,
        ) -> Result<RelayProbeResult, PlatformRuntimeError> {
            self.transports
                .lock()
                .expect("transports lock")
                .push(transport.clone());
            Ok(RelayProbeResult {
                status: "ok".to_string(),
                latency_ms: Some(7),
                tools: Some(vec![RelayProbeTool {
                    name: "relay_tool".to_string(),
                    description: "from relay".to_string(),
                }]),
                error: None,
            })
        }
    }

    fn relay_target() -> RelayProbeTarget {
        RelayProbeTarget {
            backend_id: "backend-a".to_string(),
        }
    }

    #[tokio::test]
    async fn stdio_without_relay_returns_error() {
        let transport = McpTransportConfig::Stdio {
            command: "npx".to_string(),
            args: vec!["@modelcontextprotocol/server-filesystem".to_string()],
            env: vec![McpEnvVar {
                name: "FOO".to_string(),
                value: "bar".to_string(),
            }],
            cwd: None,
        };
        match probe_transport(
            &transport,
            McpRoutePolicy::Auto,
            Some(relay_target()),
            None,
            &RmcpProbeTransport::new(),
        )
        .await
        {
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
        match probe_transport(
            &transport,
            McpRoutePolicy::Auto,
            None,
            None,
            &RmcpProbeTransport::new(),
        )
        .await
        {
            ProbeResult::Error { error } => {
                assert!(!error.is_empty(), "error 信息不应为空");
            }
            other => panic!("expected Error, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn http_probe_forwards_transport_headers() {
        let probe = CapturingHttpProbe::default();
        let captured = probe.headers.clone();
        let header = McpHttpHeader {
            name: "x-session".to_string(),
            value: "demo".to_string(),
        };
        let transport = McpTransportConfig::Http {
            url: "http://127.0.0.1:1/mcp".to_string(),
            headers: vec![header.clone()],
        };

        match probe_transport(&transport, McpRoutePolicy::Auto, None, None, &probe).await {
            ProbeResult::Ok { .. } => {}
            other => panic!("expected Ok from fake probe, got: {other:?}"),
        }
        assert_eq!(captured.lock().expect("headers lock").as_slice(), &[header]);
    }

    #[tokio::test]
    async fn relay_policy_http_probe_uses_relay_transport() {
        let relay = FakeRelayProbe::default();
        let captured = relay.transports.clone();
        let probe = CapturingHttpProbe::default();
        let header = McpHttpHeader {
            name: "x-session".to_string(),
            value: "demo".to_string(),
        };
        let transport = McpTransportConfig::Http {
            url: "http://127.0.0.1:7321/expose/mcp".to_string(),
            headers: vec![header],
        };

        match probe_transport(
            &transport,
            McpRoutePolicy::Relay,
            Some(relay_target()),
            Some(&relay),
            &probe,
        )
        .await
        {
            ProbeResult::Ok { latency_ms, tools } => {
                assert_eq!(latency_ms, 7);
                assert_eq!(tools[0].name, "relay_tool");
            }
            other => panic!("expected Ok from relay probe, got: {other:?}"),
        }

        assert_eq!(
            captured.lock().expect("transports lock").as_slice(),
            &[transport]
        );
        assert!(
            probe.headers.lock().expect("headers lock").is_empty(),
            "HTTP direct probe should not run for relay policy"
        );
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

    #[tokio::test]
    async fn required_runtime_binding_without_runtime_context_returns_unsupported() {
        let transport = McpTransportConfig::Http {
            url: "http://127.0.0.1:1/mcp".to_string(),
            headers: vec![],
        };
        let runtime_binding = McpRuntimeBindingConfig {
            mount_id: Some("main".to_string()),
            bindings: vec![McpRuntimeBindingRule {
                source: McpRuntimeBindingSource::WorkspaceDetectedFact {
                    path: vec!["p4".to_string(), "client_name".to_string()],
                },
                target: McpRuntimeBindingTarget::HttpQuery {
                    name: "p4_client".to_string(),
                },
                required: true,
            }],
        };

        match probe_transport_without_runtime_context(
            &transport,
            McpRoutePolicy::Auto,
            Some(&runtime_binding),
            None,
            None,
            &RmcpProbeTransport::new(),
        )
        .await
        {
            ProbeResult::Unsupported { reason } => {
                assert!(reason.contains("runtime context"));
                assert!(reason.contains("workspace.detected_facts.p4.client_name"));
            }
            other => panic!("expected Unsupported, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn optional_runtime_binding_without_runtime_context_keeps_static_probe() {
        let transport = McpTransportConfig::Http {
            url: "http://127.0.0.1:1/mcp".to_string(),
            headers: vec![],
        };
        let runtime_binding = McpRuntimeBindingConfig {
            mount_id: Some("main".to_string()),
            bindings: vec![McpRuntimeBindingRule {
                source: McpRuntimeBindingSource::WorkspaceDetectedFact {
                    path: vec!["p4".to_string(), "client_name".to_string()],
                },
                target: McpRuntimeBindingTarget::HttpQuery {
                    name: "p4_client".to_string(),
                },
                required: false,
            }],
        };

        match probe_transport_without_runtime_context(
            &transport,
            McpRoutePolicy::Auto,
            Some(&runtime_binding),
            None,
            None,
            &RmcpProbeTransport::new(),
        )
        .await
        {
            ProbeResult::Error { error } => {
                assert!(!error.is_empty(), "应继续执行静态 HTTP probe");
            }
            other => panic!("expected Error from static probe, got: {other:?}"),
        }
    }
}
