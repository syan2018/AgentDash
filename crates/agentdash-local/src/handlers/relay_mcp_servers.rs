//! relay MCP Server 配置解析——从 relay 协议 JSON 转换为 RuntimeMcpServerDeclaration

use agentdash_spi::{McpEnvVar, McpHttpHeader, McpTransportConfig, RuntimeMcpServerDeclaration};
use serde_json::{Map, Value};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RelayMcpServerParseError {
    #[error("mcp_servers[{index}] entry 必须是对象")]
    EntryNotObject { index: usize },
    #[error("mcp_servers[{index}] 缺少 name 字段")]
    MissingName { index: usize },
    #[error("mcp_servers[{index}] name 字段必须是非空字符串")]
    InvalidName { index: usize },
    #[error("mcp_servers[{index}] server `{server_name}` 缺少 type 字段")]
    MissingType { index: usize, server_name: String },
    #[error("mcp_servers[{index}] server `{server_name}` type 字段必须是非空字符串")]
    InvalidType { index: usize, server_name: String },
    #[error("mcp_servers[{index}] server `{server_name}` type 字段未知: {transport_type}")]
    UnknownType {
        index: usize,
        server_name: String,
        transport_type: String,
    },
    #[error("mcp_servers[{index}] server `{server_name}` 缺少 {field} 字段")]
    MissingField {
        index: usize,
        server_name: String,
        field: &'static str,
    },
    #[error("mcp_servers[{index}] server `{server_name}` {field} 字段必须是{expected}")]
    InvalidField {
        index: usize,
        server_name: String,
        field: &'static str,
        expected: &'static str,
    },
    #[error("mcp_servers[{index}] server `{server_name}` {field}[{item_index}] 必须是对象")]
    InvalidObjectItem {
        index: usize,
        server_name: String,
        field: &'static str,
        item_index: usize,
    },
    #[error(
        "mcp_servers[{index}] server `{server_name}` {field}[{item_index}].{item_field} 缺少字段"
    )]
    MissingItemField {
        index: usize,
        server_name: String,
        field: &'static str,
        item_index: usize,
        item_field: &'static str,
    },
    #[error(
        "mcp_servers[{index}] server `{server_name}` {field}[{item_index}].{item_field} 字段必须是{expected}"
    )]
    InvalidItemField {
        index: usize,
        server_name: String,
        field: &'static str,
        item_index: usize,
        item_field: &'static str,
        expected: &'static str,
    },
    #[error("mcp_servers[{index}] server `{server_name}` args[{item_index}] 必须是字符串")]
    InvalidArgItem {
        index: usize,
        server_name: String,
        item_index: usize,
    },
}

/// 从中继 `CommandPromptPayload.mcp_servers` JSON 列表解析出 `RuntimeMcpServerDeclaration` 列表。
///
/// 仅接受三种显式传输类型:
/// - `"type": "http"` → `McpTransportConfig::Http`
/// - `"type": "sse"`  → `McpTransportConfig::Sse`
/// - `"type": "stdio"` → `McpTransportConfig::Stdio`
pub fn parse_relay_mcp_servers(
    raw: &[Value],
) -> Result<Vec<RuntimeMcpServerDeclaration>, RelayMcpServerParseError> {
    raw.iter()
        .enumerate()
        .map(|(index, entry)| parse_relay_mcp_server(index, entry))
        .collect()
}

fn parse_relay_mcp_server(
    index: usize,
    entry: &Value,
) -> Result<RuntimeMcpServerDeclaration, RelayMcpServerParseError> {
    let obj = entry
        .as_object()
        .ok_or(RelayMcpServerParseError::EntryNotObject { index })?;
    let name = parse_server_name(index, obj)?;
    let transport_type = parse_transport_type(index, &name, obj)?;
    let transport = match transport_type {
        "http" => McpTransportConfig::Http {
            url: parse_required_string_field(index, &name, obj, "url")?,
            headers: parse_headers(index, &name, obj)?,
        },
        "sse" => McpTransportConfig::Sse {
            url: parse_required_string_field(index, &name, obj, "url")?,
            headers: parse_headers(index, &name, obj)?,
        },
        "stdio" => McpTransportConfig::Stdio {
            command: parse_required_string_field(index, &name, obj, "command")?,
            args: parse_args(index, &name, obj)?,
            env: parse_env(index, &name, obj)?,
            cwd: parse_optional_string_field(index, &name, obj, "cwd")?,
        },
        other => {
            return Err(RelayMcpServerParseError::UnknownType {
                index,
                server_name: name,
                transport_type: other.to_string(),
            });
        }
    };

    Ok(RuntimeMcpServerDeclaration {
        name,
        transport,
        uses_relay: false,
    })
}

fn parse_server_name(
    index: usize,
    obj: &Map<String, Value>,
) -> Result<String, RelayMcpServerParseError> {
    let value = obj
        .get("name")
        .ok_or(RelayMcpServerParseError::MissingName { index })?;
    let name = value
        .as_str()
        .ok_or(RelayMcpServerParseError::InvalidName { index })?;
    if name.trim().is_empty() {
        return Err(RelayMcpServerParseError::InvalidName { index });
    }
    Ok(name.to_string())
}

fn parse_transport_type<'a>(
    index: usize,
    server_name: &str,
    obj: &'a Map<String, Value>,
) -> Result<&'a str, RelayMcpServerParseError> {
    let value = obj
        .get("type")
        .ok_or_else(|| RelayMcpServerParseError::MissingType {
            index,
            server_name: server_name.to_string(),
        })?;
    let transport_type = value
        .as_str()
        .ok_or_else(|| RelayMcpServerParseError::InvalidType {
            index,
            server_name: server_name.to_string(),
        })?;
    if transport_type.trim().is_empty() {
        return Err(RelayMcpServerParseError::InvalidType {
            index,
            server_name: server_name.to_string(),
        });
    }
    Ok(transport_type)
}

fn parse_required_string_field(
    index: usize,
    server_name: &str,
    obj: &Map<String, Value>,
    field: &'static str,
) -> Result<String, RelayMcpServerParseError> {
    let value = obj
        .get(field)
        .ok_or_else(|| RelayMcpServerParseError::MissingField {
            index,
            server_name: server_name.to_string(),
            field,
        })?;
    let text = value
        .as_str()
        .ok_or_else(|| RelayMcpServerParseError::InvalidField {
            index,
            server_name: server_name.to_string(),
            field,
            expected: "非空字符串",
        })?;
    if text.trim().is_empty() {
        return Err(RelayMcpServerParseError::InvalidField {
            index,
            server_name: server_name.to_string(),
            field,
            expected: "非空字符串",
        });
    }
    Ok(text.to_string())
}

fn parse_optional_string_field(
    index: usize,
    server_name: &str,
    obj: &Map<String, Value>,
    field: &'static str,
) -> Result<Option<String>, RelayMcpServerParseError> {
    let Some(value) = obj.get(field) else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let text = value
        .as_str()
        .ok_or_else(|| RelayMcpServerParseError::InvalidField {
            index,
            server_name: server_name.to_string(),
            field,
            expected: "字符串",
        })?;
    if text.trim().is_empty() {
        return Ok(None);
    }
    Ok(Some(text.to_string()))
}

fn parse_optional_array_field<'a>(
    index: usize,
    server_name: &str,
    obj: &'a Map<String, Value>,
    field: &'static str,
) -> Result<Option<&'a Vec<Value>>, RelayMcpServerParseError> {
    match obj.get(field) {
        None => Ok(None),
        Some(Value::Array(items)) => Ok(Some(items)),
        Some(_) => Err(RelayMcpServerParseError::InvalidField {
            index,
            server_name: server_name.to_string(),
            field,
            expected: "数组",
        }),
    }
}

fn parse_headers(
    index: usize,
    server_name: &str,
    obj: &Map<String, Value>,
) -> Result<Vec<McpHttpHeader>, RelayMcpServerParseError> {
    let Some(items) = parse_optional_array_field(index, server_name, obj, "headers")? else {
        return Ok(Vec::new());
    };

    items
        .iter()
        .enumerate()
        .map(|(item_index, item)| {
            let item_obj = parse_object_item(index, server_name, "headers", item_index, item)?;
            Ok(McpHttpHeader {
                name: parse_item_string_field(
                    index,
                    server_name,
                    "headers",
                    item_index,
                    item_obj,
                    "name",
                    true,
                )?,
                value: parse_item_string_field(
                    index,
                    server_name,
                    "headers",
                    item_index,
                    item_obj,
                    "value",
                    false,
                )?,
            })
        })
        .collect()
}

fn parse_env(
    index: usize,
    server_name: &str,
    obj: &Map<String, Value>,
) -> Result<Vec<McpEnvVar>, RelayMcpServerParseError> {
    let Some(items) = parse_optional_array_field(index, server_name, obj, "env")? else {
        return Ok(Vec::new());
    };

    items
        .iter()
        .enumerate()
        .map(|(item_index, item)| {
            let item_obj = parse_object_item(index, server_name, "env", item_index, item)?;
            Ok(McpEnvVar {
                name: parse_item_string_field(
                    index,
                    server_name,
                    "env",
                    item_index,
                    item_obj,
                    "name",
                    true,
                )?,
                value: parse_item_string_field(
                    index,
                    server_name,
                    "env",
                    item_index,
                    item_obj,
                    "value",
                    false,
                )?,
            })
        })
        .collect()
}

fn parse_args(
    index: usize,
    server_name: &str,
    obj: &Map<String, Value>,
) -> Result<Vec<String>, RelayMcpServerParseError> {
    let Some(items) = parse_optional_array_field(index, server_name, obj, "args")? else {
        return Ok(Vec::new());
    };

    items
        .iter()
        .enumerate()
        .map(|(item_index, item)| {
            item.as_str().map(String::from).ok_or_else(|| {
                RelayMcpServerParseError::InvalidArgItem {
                    index,
                    server_name: server_name.to_string(),
                    item_index,
                }
            })
        })
        .collect()
}

fn parse_object_item<'a>(
    index: usize,
    server_name: &str,
    field: &'static str,
    item_index: usize,
    item: &'a Value,
) -> Result<&'a Map<String, Value>, RelayMcpServerParseError> {
    item.as_object()
        .ok_or_else(|| RelayMcpServerParseError::InvalidObjectItem {
            index,
            server_name: server_name.to_string(),
            field,
            item_index,
        })
}

fn parse_item_string_field(
    index: usize,
    server_name: &str,
    field: &'static str,
    item_index: usize,
    item: &Map<String, Value>,
    item_field: &'static str,
    reject_blank: bool,
) -> Result<String, RelayMcpServerParseError> {
    let value = item
        .get(item_field)
        .ok_or_else(|| RelayMcpServerParseError::MissingItemField {
            index,
            server_name: server_name.to_string(),
            field,
            item_index,
            item_field,
        })?;
    let text = value
        .as_str()
        .ok_or_else(|| RelayMcpServerParseError::InvalidItemField {
            index,
            server_name: server_name.to_string(),
            field,
            item_index,
            item_field,
            expected: "字符串",
        })?;
    if reject_blank && text.trim().is_empty() {
        return Err(RelayMcpServerParseError::InvalidItemField {
            index,
            server_name: server_name.to_string(),
            field,
            item_index,
            item_field,
            expected: "非空字符串",
        });
    }
    Ok(text.to_string())
}

#[cfg(test)]
mod tests {
    use super::parse_relay_mcp_servers;
    use agentdash_application::relay_connector::mcp_declaration_to_relay_prompt_value;
    use agentdash_spi::{McpEnvVar, McpTransportConfig, RuntimeMcpServerDeclaration};

    #[test]
    fn relay_mcp_servers_require_explicit_type() {
        let error = parse_relay_mcp_servers(&[serde_json::json!({
            "name": "missing-type",
            "url": "http://127.0.0.1:8080/mcp"
        })])
        .expect_err("缺少显式 type 的 MCP server 不应被接受");

        assert!(error.to_string().contains("type"));
    }

    #[test]
    fn relay_mcp_servers_reject_unknown_type() {
        let error = parse_relay_mcp_servers(&[serde_json::json!({
            "name": "bad-type",
            "type": "ws",
            "url": "ws://127.0.0.1:8080/mcp"
        })])
        .expect_err("未知 type 的 MCP server 不应被接受");

        assert!(error.to_string().contains("bad-type"));
        assert!(error.to_string().contains("ws"));
    }

    #[test]
    fn relay_mcp_servers_reject_non_object_entry() {
        let error = parse_relay_mcp_servers(&[serde_json::json!("not-object")])
            .expect_err("非对象 MCP server entry 不应被接受");

        assert!(error.to_string().contains("mcp_servers[0]"));
    }

    #[test]
    fn relay_mcp_servers_reject_missing_name() {
        let error = parse_relay_mcp_servers(&[serde_json::json!({
            "type": "stdio",
            "command": "npx"
        })])
        .expect_err("缺少 name 的 MCP server 不应被接受");

        assert!(error.to_string().contains("name"));
    }

    #[test]
    fn relay_mcp_servers_reject_nested_runtime_mcp_declaration_shape() {
        let error = parse_relay_mcp_servers(&[serde_json::json!({
            "name": "nested-server",
            "transport": {
                "type": "stdio",
                "command": "npx"
            },
            "uses_relay": false
        })])
        .expect_err("嵌套 transport 的内部 RuntimeMcpServerDeclaration 形态不应被接受");

        assert!(error.to_string().contains("nested-server"));
        assert!(error.to_string().contains("type"));
    }

    #[test]
    fn relay_mcp_servers_parse_application_prompt_wire_shape() {
        let value = mcp_declaration_to_relay_prompt_value(&RuntimeMcpServerDeclaration {
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

        assert_eq!(
            value.get("type").and_then(serde_json::Value::as_str),
            Some("stdio")
        );
        assert_eq!(
            value.get("command").and_then(serde_json::Value::as_str),
            Some("npx")
        );
        assert_eq!(
            value.get("cwd").and_then(serde_json::Value::as_str),
            Some("/workspace/demo")
        );
        assert!(value.get("transport").is_none());

        let servers = parse_relay_mcp_servers(&[value])
            .expect("application relay prompt MCP wire shape 应被本机 parser 接受");

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
    fn relay_mcp_servers_reject_invalid_header_item() {
        let error = parse_relay_mcp_servers(&[serde_json::json!({
            "name": "http-server",
            "type": "http",
            "url": "http://127.0.0.1:8080/mcp",
            "headers": [{ "name": "Authorization" }]
        })])
        .expect_err("非法 header item 不应被接受");

        assert!(error.to_string().contains("http-server"));
        assert!(error.to_string().contains("headers[0].value"));
    }

    #[test]
    fn relay_mcp_servers_reject_invalid_env_item() {
        let error = parse_relay_mcp_servers(&[serde_json::json!({
            "name": "stdio-server",
            "type": "stdio",
            "command": "npx",
            "env": [{ "name": 42, "value": "1" }]
        })])
        .expect_err("非法 env item 不应被接受");

        assert!(error.to_string().contains("stdio-server"));
        assert!(error.to_string().contains("env[0].name"));
    }

    #[test]
    fn relay_mcp_servers_reject_invalid_arg_item() {
        let error = parse_relay_mcp_servers(&[serde_json::json!({
            "name": "stdio-server",
            "type": "stdio",
            "command": "npx",
            "args": ["-y", 42]
        })])
        .expect_err("非法 arg item 不应被接受");

        assert!(error.to_string().contains("stdio-server"));
        assert!(error.to_string().contains("args[1]"));
    }

    #[test]
    fn relay_mcp_servers_parse_valid_stdio() {
        let servers = parse_relay_mcp_servers(&[serde_json::json!({
            "name": "stdio-server",
            "type": "stdio",
            "command": "npx",
            "args": ["-y", "server"],
            "env": [{ "name": "TOKEN", "value": "" }],
            "cwd": "/workspace/demo"
        })])
        .expect("合法 stdio MCP server 应被解析");

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
