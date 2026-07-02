//! MCP Relay 命令处理——probe / list_tools / call_tool / close

use std::sync::Arc;

use agentdash_relay::*;

use super::CommandDispatchPlan;
use crate::mcp_client_manager::McpClientManager;

/// 一次性 probe 超时（秒）——覆盖进程 spawn + MCP 握手 + tools/list 全过程。
const PROBE_TIMEOUT_SECS: u64 = 15;
const PROBE_SERVER_NAME: &str = "__agentdash_probe__";

#[derive(Clone)]
pub(super) struct McpCommandHandler {
    mcp_manager: Option<Arc<McpClientManager>>,
}

impl McpCommandHandler {
    pub(super) fn new(mcp_manager: Option<Arc<McpClientManager>>) -> Self {
        Self { mcp_manager }
    }

    pub(super) fn dispatch_plan(msg: &RelayMessage) -> Option<CommandDispatchPlan> {
        match msg {
            RelayMessage::CommandMcpProbeTransport { .. } => Some(CommandDispatchPlan::BACKGROUND),
            RelayMessage::CommandMcpListTools { .. }
            | RelayMessage::CommandMcpCallTool { .. }
            | RelayMessage::CommandMcpClose { .. } => Some(CommandDispatchPlan::INLINE),
            _ => None,
        }
    }

    /// 一次性 probe：临时连接指定 transport → tools/list → 关闭，不入连接池。
    pub(super) async fn handle_mcp_probe_transport(
        &self,
        id: String,
        payload: CommandMcpProbeTransportPayload,
    ) -> RelayMessage {
        use std::time::{Duration, Instant};

        let start = Instant::now();
        let server = McpServerRelay {
            name: PROBE_SERVER_NAME.to_string(),
            transport: payload.transport,
        };
        let manager = McpClientManager::new(Vec::new(), false);
        let probe_fut = manager.probe_once(&server);

        match tokio::time::timeout(Duration::from_secs(PROBE_TIMEOUT_SECS), probe_fut).await {
            Ok(Ok(tools)) => {
                let latency_ms = start.elapsed().as_millis() as u64;
                RelayMessage::ResponseMcpProbeTransport {
                    id,
                    payload: Some(ResponseMcpProbeTransportPayload {
                        status: "ok".to_string(),
                        latency_ms: Some(latency_ms),
                        tools: Some(tools),
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
                    error: Some(err.to_string()),
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
        let server_name = payload.server.name.clone();
        match mgr.list_tools(&payload.server).await {
            Ok(tools) => RelayMessage::ResponseMcpListTools {
                id,
                payload: Some(ResponseMcpListToolsPayload { server_name, tools }),
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
            .call_tool(&payload.server, &payload.tool_name, payload.arguments)
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
