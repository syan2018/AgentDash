//! McpRelayProvider 实现 — 基于 BackendRegistry 的 MCP relay 工具发现与调用

use async_trait::async_trait;

use agentdash_relay::{McpEnvVarRelay, McpHttpHeaderRelay, McpTransportConfigRelay, RelayMessage};
use agentdash_spi::ConnectorError;
use agentdash_spi::platform::mcp_relay::{
    McpRelayProvider, RelayMcpCallContext, RelayMcpCallResult, RelayMcpToolInfo, RelayProbeResult,
    RelayProbeTool,
};

use super::registry::BackendRegistry;

#[async_trait]
impl McpRelayProvider for BackendRegistry {
    async fn list_relay_tools(&self, requested_servers: &[String]) -> Vec<RelayMcpToolInfo> {
        let mut result = Vec::new();

        for server_name in requested_servers {
            let backend_id = match self.find_backend_for_mcp_server(server_name).await {
                Some(id) => id,
                None => {
                    tracing::debug!(
                        server = %server_name,
                        "无在线 backend 提供该 MCP server，跳过"
                    );
                    continue;
                }
            };

            let cmd = RelayMessage::CommandMcpListTools {
                id: RelayMessage::new_id("mcp-list"),
                payload: agentdash_relay::CommandMcpListToolsPayload {
                    server_name: server_name.clone(),
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
                    for tool in &resp.tools {
                        result.push(RelayMcpToolInfo {
                            server_name: server_name.clone(),
                            tool_name: tool.name.clone(),
                            description: tool.description.clone(),
                            parameters_schema: tool.parameters_schema.clone(),
                        });
                    }
                }
                Ok(RelayMessage::ResponseMcpListTools {
                    error: Some(err), ..
                }) => {
                    tracing::warn!(
                        server = %server_name,
                        error = %err.message,
                        "relay MCP list_tools 失败"
                    );
                }
                Ok(_) => {
                    tracing::warn!(
                        server = %server_name,
                        "relay MCP list_tools 返回意外消息类型"
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        server = %server_name,
                        error = %e,
                        "relay MCP list_tools 通信失败"
                    );
                }
            }
        }

        result
    }

    async fn call_relay_tool(
        &self,
        server_name: &str,
        tool_name: &str,
        arguments: Option<serde_json::Map<String, serde_json::Value>>,
        _context: Option<RelayMcpCallContext>,
    ) -> Result<RelayMcpCallResult, ConnectorError> {
        let backend_id = self
            .find_backend_for_mcp_server(server_name)
            .await
            .ok_or_else(|| {
                ConnectorError::ConnectionFailed(format!(
                    "无在线 backend 提供 MCP server '{server_name}'"
                ))
            })?;

        let cmd = RelayMessage::CommandMcpCallTool {
            id: RelayMessage::new_id("mcp-call"),
            payload: agentdash_relay::CommandMcpCallToolPayload {
                server_name: server_name.to_string(),
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
            .find_any_online_backend()
            .await
            .ok_or_else(|| ConnectorError::ConnectionFailed("无在线本机后端".to_string()))?;

        let cmd = RelayMessage::CommandMcpProbeTransport {
            id: RelayMessage::new_id("mcp-probe"),
            payload: agentdash_relay::CommandMcpProbeTransportPayload {
                transport: mcp_transport_to_relay(transport),
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

fn mcp_transport_to_relay(
    transport: &agentdash_domain::mcp_preset::McpTransportConfig,
) -> McpTransportConfigRelay {
    match transport {
        agentdash_domain::mcp_preset::McpTransportConfig::Http { url, headers } => {
            McpTransportConfigRelay::Http {
                url: url.clone(),
                headers: headers
                    .iter()
                    .map(|header| McpHttpHeaderRelay {
                        name: header.name.clone(),
                        value: header.value.clone(),
                    })
                    .collect(),
            }
        }
        agentdash_domain::mcp_preset::McpTransportConfig::Sse { url, headers } => {
            McpTransportConfigRelay::Sse {
                url: url.clone(),
                headers: headers
                    .iter()
                    .map(|header| McpHttpHeaderRelay {
                        name: header.name.clone(),
                        value: header.value.clone(),
                    })
                    .collect(),
            }
        }
        agentdash_domain::mcp_preset::McpTransportConfig::Stdio { command, args, env } => {
            McpTransportConfigRelay::Stdio {
                command: command.clone(),
                args: args.clone(),
                env: env
                    .iter()
                    .map(|var| McpEnvVarRelay {
                        name: var.name.clone(),
                        value: var.value.clone(),
                    })
                    .collect(),
            }
        }
    }
}
