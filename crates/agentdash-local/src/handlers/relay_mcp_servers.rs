//! relay MCP Server typed DTO 转换。

use agentdash_relay::{McpServerDeclarationRelay, McpTransportConfigRelay};
use agentdash_spi::{McpEnvVar, McpHttpHeader, McpTransportConfig, RuntimeMcpServerDeclaration};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RelayMcpServerParseError {
    #[error("mcp_servers[{index}] name 字段必须是非空字符串")]
    InvalidName { index: usize },
}

/// 从中继 `CommandPromptPayload.mcp_servers` typed wire DTO 转换出 runtime MCP 声明。
pub fn relay_mcp_servers_to_runtime(
    servers: &[McpServerDeclarationRelay],
) -> Result<Vec<RuntimeMcpServerDeclaration>, RelayMcpServerParseError> {
    servers
        .iter()
        .enumerate()
        .map(|(index, server)| relay_mcp_server_to_runtime(index, server))
        .collect()
}

fn relay_mcp_server_to_runtime(
    index: usize,
    server: &McpServerDeclarationRelay,
) -> Result<RuntimeMcpServerDeclaration, RelayMcpServerParseError> {
    if server.name.trim().is_empty() {
        return Err(RelayMcpServerParseError::InvalidName { index });
    }
    Ok(RuntimeMcpServerDeclaration {
        name: server.name.clone(),
        transport: relay_transport_to_runtime(&server.transport),
        uses_relay: false,
    })
}

fn relay_transport_to_runtime(transport: &McpTransportConfigRelay) -> McpTransportConfig {
    match transport {
        McpTransportConfigRelay::Http { url, headers } => McpTransportConfig::Http {
            url: url.clone(),
            headers: headers
                .iter()
                .map(|header| McpHttpHeader {
                    name: header.name.clone(),
                    value: header.value.clone(),
                })
                .collect(),
        },
        McpTransportConfigRelay::Sse { url, headers } => McpTransportConfig::Sse {
            url: url.clone(),
            headers: headers
                .iter()
                .map(|header| McpHttpHeader {
                    name: header.name.clone(),
                    value: header.value.clone(),
                })
                .collect(),
        },
        McpTransportConfigRelay::Stdio {
            command,
            args,
            env,
            cwd,
        } => McpTransportConfig::Stdio {
            command: command.clone(),
            args: args.clone(),
            env: env
                .iter()
                .map(|item| McpEnvVar {
                    name: item.name.clone(),
                    value: item.value.clone(),
                })
                .collect(),
            cwd: cwd.clone(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::relay_mcp_servers_to_runtime;
    use agentdash_application::relay_connector::mcp_declaration_to_relay_prompt_server;
    use agentdash_relay::{McpServerDeclarationRelay, McpTransportConfigRelay};
    use agentdash_spi::{McpEnvVar, McpTransportConfig, RuntimeMcpServerDeclaration};

    #[test]
    fn relay_mcp_servers_convert_application_prompt_wire_shape() {
        let value = mcp_declaration_to_relay_prompt_server(&RuntimeMcpServerDeclaration {
            name: "application-stdio".to_string(),
            transport: McpTransportConfig::Stdio {
                command: "npx".to_string(),
                args: vec!["-y".to_string(), "server".to_string()],
                env: vec![McpEnvVar {
                    name: "TOKEN".to_string(),
                    value: "secret".to_string(),
                }],
                cwd: Some("/workspace/demo".to_string()),
            },
            uses_relay: false,
        });

        let servers = relay_mcp_servers_to_runtime(&[value])
            .expect("application relay prompt MCP wire shape 应被本机转换接受");

        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].name, "application-stdio");
        assert_eq!(
            servers[0].transport,
            McpTransportConfig::Stdio {
                command: "npx".to_string(),
                args: vec!["-y".to_string(), "server".to_string()],
                env: vec![McpEnvVar {
                    name: "TOKEN".to_string(),
                    value: "secret".to_string(),
                }],
                cwd: Some("/workspace/demo".to_string()),
            }
        );
    }

    #[test]
    fn relay_mcp_servers_reject_blank_name() {
        let error = relay_mcp_servers_to_runtime(&[McpServerDeclarationRelay {
            name: " ".to_string(),
            transport: McpTransportConfigRelay::Stdio {
                command: "npx".to_string(),
                args: Vec::new(),
                env: Vec::new(),
                cwd: None,
            },
        }])
        .expect_err("空 name 不应进入 runtime MCP 声明");

        assert!(error.to_string().contains("mcp_servers[0]"));
    }

    #[test]
    fn relay_mcp_servers_convert_valid_stdio() {
        let servers = relay_mcp_servers_to_runtime(&[McpServerDeclarationRelay {
            name: "stdio-server".to_string(),
            transport: McpTransportConfigRelay::Stdio {
                command: "npx".to_string(),
                args: vec!["-y".to_string(), "server".to_string()],
                env: vec![agentdash_relay::McpEnvVarRelay {
                    name: "TOKEN".to_string(),
                    value: String::new(),
                }],
                cwd: Some("/workspace/demo".to_string()),
            },
        }])
        .expect("合法 stdio MCP server 应被转换");

        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].name, "stdio-server");
        assert_eq!(
            match &servers[0].transport {
                McpTransportConfig::Stdio { cwd, .. } => cwd.as_deref(),
                _ => None,
            },
            Some("/workspace/demo")
        );
    }
}
