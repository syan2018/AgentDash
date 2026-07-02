//! McpRelayProvider 实现 — 基于 BackendRegistry 的 MCP relay 工具发现与调用

use agentdash_diagnostics::{Subsystem, diag};
use async_trait::async_trait;

use agentdash_application::mcp_relay_adapter;
use agentdash_relay::RelayMessage;
use agentdash_spi::ConnectorError;
use agentdash_spi::RuntimeMcpServer;
use agentdash_spi::platform::mcp_relay::{
    McpRelayProvider, RelayMcpCallContext, RelayMcpCallResult, RelayMcpListOutcome,
    RelayMcpSourceOutcome, RelayMcpToolInfo, RelayProbeResult, RelayProbeTool,
};

use super::registry::BackendRegistry;

#[async_trait]
impl McpRelayProvider for BackendRegistry {
    async fn list_relay_tools(
        &self,
        requested_servers: &[RuntimeMcpServer],
        context: Option<RelayMcpCallContext>,
    ) -> RelayMcpListOutcome {
        let mut tools = Vec::new();
        let mut sources = Vec::new();

        for server in requested_servers {
            let server_name = &server.name;
            let backend_id = match self
                .resolve_backend_for_relay_mcp(server_name, context.as_ref())
                .await
            {
                Ok(id) => id,
                Err(error) => {
                    let message = format!(
                        "无法解析 relay MCP server '{server_name}' 的 runtime backend anchor: {error}"
                    );
                    diag!(Warn, Subsystem::Relay,

                        server = %server_name,
                        error = %error,
                        "relay MCP list_tools 缺少可用 runtime backend anchor，跳过 server"
                    );
                    sources.push(RelayMcpSourceOutcome::unavailable(
                        server.clone(),
                        "backend_anchor_unavailable",
                        message,
                    ));
                    continue;
                }
            };

            let cmd = RelayMessage::CommandMcpListTools {
                id: RelayMessage::new_id("mcp-list"),
                payload: agentdash_relay::CommandMcpListToolsPayload {
                    server: mcp_relay_adapter::runtime_mcp_server_to_relay(server),
                },
            };

            match self
                .send_command_with_timeout(&backend_id, cmd, std::time::Duration::from_secs(30))
                .await
            {
                Ok(RelayMessage::ResponseMcpListTools {
                    payload: Some(resp),
                    error: None,
                    ..
                }) => {
                    let tool_count = resp.tools.len();
                    for tool in &resp.tools {
                        tools.push(RelayMcpToolInfo {
                            server_name: server_name.clone(),
                            server: server.clone(),
                            tool_name: tool.name.clone(),
                            description: tool.description.clone(),
                            parameters_schema: tool.parameters_schema.clone(),
                        });
                    }
                    sources.push(RelayMcpSourceOutcome::ready(server.clone(), tool_count));
                }
                Ok(RelayMessage::ResponseMcpListTools {
                    error: Some(err), ..
                }) => {
                    diag!(Warn, Subsystem::Relay,

                        server = %server_name,
                        error = %err.message,
                        "relay MCP list_tools 失败"
                    );
                    sources.push(RelayMcpSourceOutcome::unavailable(
                        server.clone(),
                        "list_tools_failed",
                        err.message,
                    ));
                }
                Ok(_) => {
                    diag!(Warn, Subsystem::Relay,

                        server = %server_name,
                        "relay MCP list_tools 返回意外消息类型"
                    );
                    sources.push(RelayMcpSourceOutcome::unavailable(
                        server.clone(),
                        "unexpected_response",
                        "relay MCP list_tools returned an unexpected response type",
                    ));
                }
                Err(e) => {
                    diag!(Warn, Subsystem::Relay,

                        server = %server_name,
                        error = %e,
                        "relay MCP list_tools 通信失败"
                    );
                    sources.push(RelayMcpSourceOutcome::unavailable(
                        server.clone(),
                        "relay_unreachable",
                        e.to_string(),
                    ));
                }
            }
        }

        RelayMcpListOutcome { tools, sources }
    }

    async fn call_relay_tool(
        &self,
        server: &RuntimeMcpServer,
        tool_name: &str,
        arguments: Option<serde_json::Map<String, serde_json::Value>>,
        context: Option<RelayMcpCallContext>,
    ) -> Result<RelayMcpCallResult, ConnectorError> {
        let server_name = server.name.as_str();
        let backend_id = self
            .resolve_backend_for_relay_mcp(server_name, context.as_ref())
            .await
            .map_err(|error| {
                ConnectorError::ConnectionFailed(format!(
                    "无法解析 relay MCP server '{server_name}' 的 runtime backend anchor: {error}"
                ))
            })?;

        let cmd = RelayMessage::CommandMcpCallTool {
            id: RelayMessage::new_id("mcp-call"),
            payload: agentdash_relay::CommandMcpCallToolPayload {
                server: mcp_relay_adapter::runtime_mcp_server_to_relay(server),
                tool_name: tool_name.to_string(),
                arguments,
            },
        };

        let resp = self
            .send_command_with_timeout(&backend_id, cmd, std::time::Duration::from_secs(120))
            .await
            .map_err(|e| ConnectorError::ConnectionFailed(e.to_string()))?;

        match resp {
            RelayMessage::ResponseMcpCallTool {
                payload: Some(p),
                error: None,
                ..
            } => Ok(RelayMcpCallResult {
                content: p.content,
                is_error: p.is_error,
            }),
            RelayMessage::ResponseMcpCallTool {
                error: Some(err), ..
            } => Err(ConnectorError::Runtime(err.message)),
            _ => Err(ConnectorError::Runtime(
                "MCP relay 返回意外响应类型".to_string(),
            )),
        }
    }

    async fn probe_transport(
        &self,
        transport: &agentdash_domain::mcp_preset::McpTransportConfig,
    ) -> Result<RelayProbeResult, ConnectorError> {
        let backend_id = self
            .find_any_online_backend_for_setup_probe()
            .await
            .ok_or_else(|| ConnectorError::ConnectionFailed("无在线本机后端".to_string()))?;

        let cmd = RelayMessage::CommandMcpProbeTransport {
            id: RelayMessage::new_id("mcp-probe"),
            payload: agentdash_relay::CommandMcpProbeTransportPayload {
                transport: mcp_relay_adapter::mcp_transport_to_relay(transport),
            },
        };

        let resp = self
            .send_command_with_timeout(&backend_id, cmd, std::time::Duration::from_secs(20))
            .await
            .map_err(|e| ConnectorError::ConnectionFailed(e.to_string()))?;

        match resp {
            RelayMessage::ResponseMcpProbeTransport {
                payload: Some(p),
                error: None,
                ..
            } => Ok(RelayProbeResult {
                status: p.status,
                latency_ms: p.latency_ms,
                tools: p.tools.map(|ts| {
                    ts.into_iter()
                        .map(|t| RelayProbeTool {
                            name: t.name,
                            description: t.description,
                        })
                        .collect()
                }),
                error: p.error,
            }),
            RelayMessage::ResponseMcpProbeTransport {
                error: Some(err), ..
            } => Ok(RelayProbeResult {
                status: "error".to_string(),
                latency_ms: None,
                tools: None,
                error: Some(err.message),
            }),
            _ => Err(ConnectorError::Runtime(
                "MCP probe relay 返回意外响应类型".to_string(),
            )),
        }
    }
}
