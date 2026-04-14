//! McpRelayProvider 实现 — 基于 BackendRegistry 的 MCP relay 工具发现与调用

use async_trait::async_trait;

use agentdash_relay::RelayMessage;
use agentdash_spi::mcp_relay::{McpRelayProvider, RelayMcpCallResult, RelayMcpToolInfo};
use agentdash_spi::ConnectorError;

use super::registry::BackendRegistry;

#[async_trait]
impl McpRelayProvider for BackendRegistry {
    async fn list_relay_tools(&self) -> Vec<RelayMcpToolInfo> {
        let all_servers = self.list_all_mcp_servers().await;
        let mut result = Vec::new();

        for (_, server_info) in &all_servers {
            // 向 backend 发 list_tools 获取工具列表
            let backend_id = match self.find_backend_for_mcp_server(&server_info.name).await {
                Some(id) => id,
                None => continue,
            };

            let cmd = RelayMessage::CommandMcpListTools {
                id: RelayMessage::new_id("mcp-list"),
                payload: agentdash_relay::CommandMcpListToolsPayload {
                    server_name: server_info.name.clone(),
                },
            };

            match self
                .send_command_with_timeout(
                    &backend_id,
                    cmd,
                    std::time::Duration::from_secs(30),
                )
                .await
            {
                Ok(RelayMessage::ResponseMcpListTools {
                    payload: Some(resp),
                    error: None,
                    ..
                }) => {
                    for tool in &resp.tools {
                        result.push(RelayMcpToolInfo {
                            server_name: server_info.name.clone(),
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
                        server = %server_info.name,
                        error = %err.message,
                        "relay MCP list_tools 失败"
                    );
                }
                Ok(_) => {
                    tracing::warn!(
                        server = %server_info.name,
                        "relay MCP list_tools 返回意外消息类型"
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        server = %server_info.name,
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
            .send_command_with_timeout(
                &backend_id,
                cmd,
                std::time::Duration::from_secs(120),
            )
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
}
