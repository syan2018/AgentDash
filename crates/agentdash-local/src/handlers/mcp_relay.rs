//! MCP Relay 命令处理——probe / list_tools / call_tool / close

use agentdash_relay::*;
use rmcp::transport::child_process::TokioChildProcess;
use rmcp::transport::streamable_http_client::{
    StreamableHttpClientTransportConfig, StreamableHttpClientWorker,
};

use super::CommandHandler;

/// 一次性 probe 超时（秒）——覆盖进程 spawn + MCP 握手 + tools/list 全过程。
const PROBE_TIMEOUT_SECS: u64 = 15;

impl CommandHandler {
    /// 一次性 probe：临时连接指定 transport → tools/list → 关闭，不入连接池。
    pub(super) async fn handle_mcp_probe_transport(
        &self,
        id: String,
        payload: CommandMcpProbeTransportPayload,
    ) -> RelayMessage {
        use agentdash_domain::mcp_preset::McpTransportConfig;
        use std::time::{Duration, Instant};

        let start = Instant::now();
        let transport = payload.transport;

        let probe_fut = async {
            match &transport {
                McpTransportConfig::Stdio { command, args, env } => {
                    let mut cmd = tokio::process::Command::new(command);
                    cmd.args(args);
                    for var in env {
                        cmd.env(&var.name, &var.value);
                    }
                    let child = TokioChildProcess::new(cmd)
                        .map_err(|e| format!("spawn stdio 进程失败: {e}"))?;
                    let client = rmcp::ServiceExt::serve((), child)
                        .await
                        .map_err(|e| format!("MCP 握手失败: {e}"))?;
                    let tools = client
                        .list_all_tools()
                        .await
                        .map_err(|e| format!("list_tools 失败: {e}"))?;
                    let _ = client.cancel().await;
                    Ok::<Vec<rmcp::model::Tool>, String>(tools)
                }
                McpTransportConfig::Http { url, .. } | McpTransportConfig::Sse { url, .. } => {
                    let worker = StreamableHttpClientWorker::new(
                        reqwest::Client::new(),
                        StreamableHttpClientTransportConfig::with_uri(url.clone()),
                    );
                    let client = rmcp::ServiceExt::serve((), worker)
                        .await
                        .map_err(|e| format!("连接 MCP Server 失败: {e}"))?;
                    let tools = client
                        .list_all_tools()
                        .await
                        .map_err(|e| format!("list_tools 失败: {e}"))?;
                    let _ = client.cancel().await;
                    Ok(tools)
                }
            }
        };

        match tokio::time::timeout(Duration::from_secs(PROBE_TIMEOUT_SECS), probe_fut).await {
            Ok(Ok(tools)) => {
                let latency_ms = start.elapsed().as_millis() as u64;
                let tool_infos: Vec<McpToolInfoRelay> = tools
                    .into_iter()
                    .map(|t| McpToolInfoRelay {
                        name: t.name.to_string(),
                        description: t.description.as_deref().unwrap_or("").to_string(),
                        parameters_schema: serde_json::Value::Object((*t.input_schema).clone()),
                    })
                    .collect();
                RelayMessage::ResponseMcpProbeTransport {
                    id,
                    payload: Some(ResponseMcpProbeTransportPayload {
                        status: "ok".to_string(),
                        latency_ms: Some(latency_ms),
                        tools: Some(tool_infos),
                        error: None,
                    }),
                    error: None,
                }
            }
            Ok(Err(err)) => RelayMessage::ResponseMcpProbeTransport {
                id,
                payload: Some(ResponseMcpProbeTransportPayload {
                    status: "error".to_string(),
                    latency_ms: None,
                    tools: None,
                    error: Some(err),
                }),
                error: None,
            },
            Err(_) => RelayMessage::ResponseMcpProbeTransport {
                id,
                payload: Some(ResponseMcpProbeTransportPayload {
                    status: "error".to_string(),
                    latency_ms: None,
                    tools: None,
                    error: Some(format!("探测超时（{PROBE_TIMEOUT_SECS}s）")),
                }),
                error: None,
            },
        }
    }

    pub(super) async fn handle_mcp_list_tools(
        &self,
        id: String,
        payload: CommandMcpListToolsPayload,
    ) -> RelayMessage {
        let mgr = match &self.mcp_manager {
            Some(m) => m,
            None => {
                return RelayMessage::ResponseMcpListTools {
                    id,
                    payload: None,
                    error: Some(RelayError::runtime_error("MCP 管理器未初始化")),
                };
            }
        };
        match mgr.list_tools(&payload.server_name).await {
            Ok(tools) => RelayMessage::ResponseMcpListTools {
                id,
                payload: Some(ResponseMcpListToolsPayload {
                    server_name: payload.server_name,
                    tools,
                }),
                error: None,
            },
            Err(e) => RelayMessage::ResponseMcpListTools {
                id,
                payload: None,
                error: Some(RelayError::runtime_error(e.to_string())),
            },
        }
    }

    pub(super) async fn handle_mcp_call_tool(
        &self,
        id: String,
        payload: CommandMcpCallToolPayload,
    ) -> RelayMessage {
        let mgr = match &self.mcp_manager {
            Some(m) => m,
            None => {
                return RelayMessage::ResponseMcpCallTool {
                    id,
                    payload: None,
                    error: Some(RelayError::runtime_error("MCP 管理器未初始化")),
                };
            }
        };
        match mgr
            .call_tool(&payload.server_name, &payload.tool_name, payload.arguments)
            .await
        {
            Ok(result) => RelayMessage::ResponseMcpCallTool {
                id,
                payload: Some(result),
                error: None,
            },
            Err(e) => RelayMessage::ResponseMcpCallTool {
                id,
                payload: None,
                error: Some(RelayError::runtime_error(e.to_string())),
            },
        }
    }

    pub(super) async fn handle_mcp_close(
        &self,
        id: String,
        payload: CommandMcpClosePayload,
    ) -> RelayMessage {
        let mgr = match &self.mcp_manager {
            Some(m) => m,
            None => {
                return RelayMessage::ResponseMcpClose {
                    id,
                    payload: None,
                    error: Some(RelayError::runtime_error("MCP 管理器未初始化")),
                };
            }
        };
        match mgr.close(&payload.server_name).await {
            Ok(()) => RelayMessage::ResponseMcpClose {
                id,
                payload: Some(ResponseMcpClosePayload {
                    server_name: payload.server_name,
                    status: "closed".to_string(),
                }),
                error: None,
            },
            Err(e) => RelayMessage::ResponseMcpClose {
                id,
                payload: None,
                error: Some(RelayError::runtime_error(e.to_string())),
            },
        }
    }
}
