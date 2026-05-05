//! relay MCP Server 配置解析——从 relay 协议 JSON 转换为 SessionMcpServer

use agentdash_spi::{McpEnvVar, McpHeader, McpTransportConfig, SessionMcpServer};

/// 从中继 `CommandPromptPayload.mcp_servers` JSON 列表解析出 `SessionMcpServer` 列表。
///
/// 仅接受三种显式传输类型:
/// - `"type": "http"` → `McpTransportConfig::Http`
/// - `"type": "sse"`  → `McpTransportConfig::Sse`
/// - `"type": "stdio"` → `McpTransportConfig::Stdio`
pub fn parse_relay_mcp_servers(raw: &[serde_json::Value]) -> Vec<SessionMcpServer> {
    let mut servers = Vec::new();

    for entry in raw {
        let obj = match entry.as_object() {
            Some(o) => o,
            None => continue,
        };

        let name = obj
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let transport_type = match obj.get("type").and_then(|v| v.as_str()) {
            Some("http") => "http",
            Some("sse") => "sse",
            Some("stdio") => "stdio",
            Some(other) => {
                tracing::warn!(name = %name, transport = %other, "relay MCP server type 非法，跳过");
                continue;
            }
            None => {
                tracing::warn!(name = %name, "relay MCP server 缺少显式 type，跳过");
                continue;
            }
        };

        match transport_type {
            "http" | "sse" => {
                let url = match obj.get("url").and_then(|v| v.as_str()) {
                    Some(u) => u.to_string(),
                    None => {
                        tracing::warn!(name = %name, "relay MCP Http/SSE server 缺少 url，跳过");
                        continue;
                    }
                };
                let headers: Vec<McpHeader> = obj
                    .get("headers")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|h| {
                                let ho = h.as_object()?;
                                let hname = ho.get("name")?.as_str()?.to_string();
                                let hvalue = ho.get("value")?.as_str()?.to_string();
                                Some(McpHeader {
                                    name: hname,
                                    value: hvalue,
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();

                let transport = if transport_type == "http" {
                    McpTransportConfig::Http { url, headers }
                } else {
                    McpTransportConfig::Sse { url, headers }
                };
                servers.push(SessionMcpServer {
                    name,
                    transport,
                    uses_relay: false,
                });
            }
            "stdio" => {
                let command = match obj.get("command").and_then(|v| v.as_str()) {
                    Some(c) => c.to_string(),
                    None => {
                        tracing::warn!(name = %name, "relay MCP Stdio server 缺少 command，跳过");
                        continue;
                    }
                };
                let args: Vec<String> = obj
                    .get("args")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|a| a.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                let env: Vec<McpEnvVar> = obj
                    .get("env")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|e| {
                                let eo = e.as_object()?;
                                let ename = eo.get("name")?.as_str()?.to_string();
                                let evalue = eo.get("value")?.as_str()?.to_string();
                                Some(McpEnvVar {
                                    name: ename,
                                    value: evalue,
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();

                servers.push(SessionMcpServer {
                    name,
                    transport: McpTransportConfig::Stdio { command, args, env },
                    uses_relay: false,
                });
            }
            _ => {}
        }
    }

    servers
}

#[cfg(test)]
mod tests {
    use super::parse_relay_mcp_servers;

    #[test]
    fn relay_mcp_servers_require_explicit_type() {
        let servers = parse_relay_mcp_servers(&[serde_json::json!({
            "name": "missing-type",
            "url": "http://127.0.0.1:8080/mcp"
        })]);

        assert!(servers.is_empty(), "缺少显式 type 的 MCP server 不应被接受");
    }

    #[test]
    fn relay_mcp_servers_reject_unknown_type() {
        let servers = parse_relay_mcp_servers(&[serde_json::json!({
            "name": "bad-type",
            "type": "ws",
            "url": "ws://127.0.0.1:8080/mcp"
        })]);

        assert!(servers.is_empty(), "未知 type 的 MCP server 不应被接受");
    }
}
