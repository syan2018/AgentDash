//! McpRelayProvider 实现 — 基于 BackendRegistry 的 MCP relay 工具发现与调用

use agentdash_diagnostics::{DiagnosticErrorContext, Subsystem, diag, diag_error};
use async_trait::async_trait;

use agentdash_application::mcp_relay_adapter;
use agentdash_platform_spi::PlatformRuntimeError;
use agentdash_platform_spi::RuntimeMcpServer;
use agentdash_platform_spi::platform::mcp_relay::{
    McpRelayProvider, RelayMcpCallContext, RelayMcpCallResult, RelayMcpListOutcome,
    RelayMcpSourceOutcome, RelayMcpToolInfo, RelayProbeResult, RelayProbeTarget, RelayProbeTool,
};
use agentdash_relay::RelayMessage;

use super::registry::{BackendRegistry, relay_message_kind};

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
                    let context =
                        DiagnosticErrorContext::new("relay.mcp.list_tools", "resolve_backend")
                            .with_field("server", server_name);
                    diag_error!(
                        Warn,
                        Subsystem::Relay,
                        context = &context,
                        error = &error,
                        server = %server_name,
                        "relay MCP list_tools 缺少可用 runtime backend anchor，跳过 server"
                    );
                    let message = format!(
                        "无法解析 relay MCP server '{server_name}' 的 runtime backend anchor: {error}"
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
            let request_id = cmd.id().to_string();
            let message_kind = relay_message_kind(&cmd);

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
                    let context =
                        DiagnosticErrorContext::new("relay.mcp.list_tools", "relay_response_error")
                            .with_field("backend_id", &backend_id)
                            .with_field("server", server_name)
                            .with_field("request_id", &request_id)
                            .with_field("message_kind", message_kind)
                            .with_field("error_code", err.code.as_str());
                    diag_error!(
                        Warn,
                        Subsystem::Relay,
                        context = &context,
                        error = &err,
                        backend_id = %backend_id,
                        server = %server_name,
                        request_id = %request_id,
                        message_kind = %message_kind,
                        error_code = err.code.as_str(),
                        "relay MCP list_tools 失败"
                    );
                    sources.push(RelayMcpSourceOutcome::unavailable(
                        server.clone(),
                        "list_tools_failed",
                        err.message,
                    ));
                }
                Ok(other) => {
                    let response_kind = relay_message_kind(&other);
                    diag!(
                        Warn,
                        Subsystem::Relay,
                        operation = "relay.mcp.list_tools",
                        stage = "unexpected_response",
                        backend_id = %backend_id,
                        server = %server_name,
                        request_id = %request_id,
                        message_kind = %message_kind,
                        response_kind = %response_kind,
                        "relay MCP list_tools 返回意外消息类型"
                    );
                    sources.push(RelayMcpSourceOutcome::unavailable(
                        server.clone(),
                        "unexpected_response",
                        "relay MCP list_tools returned an unexpected response type",
                    ));
                }
                Err(e) => {
                    let context =
                        DiagnosticErrorContext::new("relay.mcp.list_tools", "send_command")
                            .with_field("backend_id", &backend_id)
                            .with_field("server", server_name)
                            .with_field("request_id", &request_id)
                            .with_field("message_kind", message_kind);
                    diag_error!(
                        Warn,
                        Subsystem::Relay,
                        context = &context,
                        error = &e,
                        backend_id = %backend_id,
                        server = %server_name,
                        request_id = %request_id,
                        message_kind = %message_kind,
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
    ) -> Result<RelayMcpCallResult, PlatformRuntimeError> {
        let server_name = server.name.as_str();
        let backend_id = self
            .resolve_backend_for_relay_mcp(server_name, context.as_ref())
            .await
            .map_err(|error| {
                let diagnostic_context =
                    DiagnosticErrorContext::new("relay.mcp.call_tool", "resolve_backend")
                        .with_field("server", server_name)
                        .with_field("tool_name", tool_name);
                diag_error!(
                    Warn,
                    Subsystem::Relay,
                    context = &diagnostic_context,
                    error = &error,
                    server = %server_name,
                    tool_name = %tool_name,
                    "relay MCP call_tool 缺少可用 runtime backend anchor"
                );
                PlatformRuntimeError::ConnectionFailed(format!(
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
        let request_id = cmd.id().to_string();
        let message_kind = relay_message_kind(&cmd);

        let resp = self
            .send_command_with_timeout(&backend_id, cmd, std::time::Duration::from_secs(120))
            .await
            .map_err(|e| {
                let context = DiagnosticErrorContext::new("relay.mcp.call_tool", "send_command")
                    .with_field("backend_id", &backend_id)
                    .with_field("server", server_name)
                    .with_field("tool_name", tool_name)
                    .with_field("request_id", &request_id)
                    .with_field("message_kind", message_kind);
                diag_error!(
                    Warn,
                    Subsystem::Relay,
                    context = &context,
                    error = &e,
                    backend_id = %backend_id,
                    server = %server_name,
                    tool_name = %tool_name,
                    request_id = %request_id,
                    message_kind = %message_kind,
                    "relay MCP call_tool 通信失败"
                );
                PlatformRuntimeError::ConnectionFailed(e.to_string())
            })?;

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
            } => {
                let context =
                    DiagnosticErrorContext::new("relay.mcp.call_tool", "relay_response_error")
                        .with_field("backend_id", &backend_id)
                        .with_field("server", server_name)
                        .with_field("tool_name", tool_name)
                        .with_field("request_id", &request_id)
                        .with_field("message_kind", message_kind)
                        .with_field("error_code", err.code.as_str());
                diag_error!(
                    Warn,
                    Subsystem::Relay,
                    context = &context,
                    error = &err,
                    backend_id = %backend_id,
                    server = %server_name,
                    tool_name = %tool_name,
                    request_id = %request_id,
                    message_kind = %message_kind,
                    error_code = err.code.as_str(),
                    "relay MCP call_tool 失败"
                );
                Err(PlatformRuntimeError::Runtime(err.message))
            }
            other => {
                let response_kind = relay_message_kind(&other);
                diag!(
                    Warn,
                    Subsystem::Relay,
                    operation = "relay.mcp.call_tool",
                    stage = "unexpected_response",
                    backend_id = %backend_id,
                    server = %server_name,
                    tool_name = %tool_name,
                    request_id = %request_id,
                    message_kind = %message_kind,
                    response_kind = %response_kind,
                    "relay MCP call_tool 返回意外消息类型"
                );
                Err(PlatformRuntimeError::Runtime(
                    "MCP relay 返回意外响应类型".to_string(),
                ))
            }
        }
    }

    async fn probe_transport(
        &self,
        transport: &agentdash_domain::mcp_preset::McpTransportConfig,
        target: RelayProbeTarget,
    ) -> Result<RelayProbeResult, PlatformRuntimeError> {
        let backend_id = target.backend_id;

        let cmd = RelayMessage::CommandMcpProbeTransport {
            id: RelayMessage::new_id("mcp-probe"),
            payload: agentdash_relay::CommandMcpProbeTransportPayload {
                transport: mcp_relay_adapter::mcp_transport_to_relay(transport),
            },
        };
        let request_id = cmd.id().to_string();
        let message_kind = relay_message_kind(&cmd);

        let resp = self
            .send_command_with_timeout(&backend_id, cmd, std::time::Duration::from_secs(20))
            .await
            .map_err(|e| {
                let context =
                    DiagnosticErrorContext::new("relay.mcp.probe_transport", "send_command")
                        .with_field("backend_id", &backend_id)
                        .with_field("request_id", &request_id)
                        .with_field("message_kind", message_kind);
                diag_error!(
                    Warn,
                    Subsystem::Relay,
                    context = &context,
                    error = &e,
                    backend_id = %backend_id,
                    request_id = %request_id,
                    message_kind = %message_kind,
                    "relay MCP probe_transport 通信失败"
                );
                PlatformRuntimeError::ConnectionFailed(e.to_string())
            })?;

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
            } => {
                let context = DiagnosticErrorContext::new(
                    "relay.mcp.probe_transport",
                    "relay_response_error",
                )
                .with_field("backend_id", &backend_id)
                .with_field("request_id", &request_id)
                .with_field("message_kind", message_kind)
                .with_field("error_code", err.code.as_str());
                diag_error!(
                    Warn,
                    Subsystem::Relay,
                    context = &context,
                    error = &err,
                    backend_id = %backend_id,
                    request_id = %request_id,
                    message_kind = %message_kind,
                    error_code = err.code.as_str(),
                    "relay MCP probe_transport 失败"
                );
                Ok(RelayProbeResult {
                    status: "error".to_string(),
                    latency_ms: None,
                    tools: None,
                    error: Some(err.message),
                })
            }
            other => {
                let response_kind = relay_message_kind(&other);
                diag!(
                    Warn,
                    Subsystem::Relay,
                    operation = "relay.mcp.probe_transport",
                    stage = "unexpected_response",
                    backend_id = %backend_id,
                    request_id = %request_id,
                    message_kind = %message_kind,
                    response_kind = %response_kind,
                    "relay MCP probe_transport 返回意外消息类型"
                );
                Err(PlatformRuntimeError::Runtime(
                    "MCP probe relay 返回意外响应类型".to_string(),
                ))
            }
        }
    }
}
