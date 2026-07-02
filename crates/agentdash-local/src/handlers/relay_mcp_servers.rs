//! relay MCP Server typed DTO 转换。

use agentdash_application::mcp_relay_adapter;
use agentdash_relay::McpServerRelay;
use agentdash_spi::RuntimeMcpServer;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RelayMcpServerParseError {
    #[error("mcp_servers[{index}] name 字段必须是非空字符串")]
    InvalidName { index: usize },
}

/// 从中继 `CommandPromptPayload.mcp_servers` typed wire DTO 转换出 runtime MCP 声明。
pub fn relay_mcp_servers_to_runtime(
    servers: &[McpServerRelay],
) -> Result<Vec<RuntimeMcpServer>, RelayMcpServerParseError> {
    servers
        .iter()
        .enumerate()
        .map(|(index, server)| relay_mcp_server_to_runtime(index, server))
        .collect()
}

fn relay_mcp_server_to_runtime(
    index: usize,
    server: &McpServerRelay,
) -> Result<RuntimeMcpServer, RelayMcpServerParseError> {
    if server.name.trim().is_empty() {
        return Err(RelayMcpServerParseError::InvalidName { index });
    }
    Ok(mcp_relay_adapter::relay_mcp_server_to_runtime(
        server, false,
    ))
}

#[cfg(test)]
mod tests {
    use super::relay_mcp_servers_to_runtime;
    use agentdash_application::mcp_relay_adapter::runtime_mcp_server_to_relay;
    use agentdash_relay::{McpServerRelay, McpTransportConfigRelay};
    use agentdash_spi::{McpEnvVar, McpTransportConfig, RuntimeMcpServer};

    #[test]
    fn relay_mcp_servers_convert_application_prompt_wire_shape() {
        let value = runtime_mcp_server_to_relay(&RuntimeMcpServer {
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
            readiness: Default::default(),
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
        let error = relay_mcp_servers_to_runtime(&[McpServerRelay {
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
        let servers = relay_mcp_servers_to_runtime(&[McpServerRelay {
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
