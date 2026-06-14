//! 本机 MCP Client 管理器 — 管理 stdio 进程和 localhost HTTP 连接

use std::collections::HashMap;

use agentdash_application::mcp_relay_adapter::{
    mcp_server_parts_to_relay, relay_transport_to_mcp_transport,
};
use agentdash_domain::mcp_preset::McpTransportConfig;
use agentdash_mcp::render_content;
use agentdash_relay::{
    McpServerInfoRelay, McpServerRelay, McpToolInfoRelay, ResponseMcpCallToolPayload,
};
use rmcp::model::CallToolRequestParams;
use rmcp::service::RunningService;
use rmcp::transport::child_process::TokioChildProcess;
use rmcp::{RoleClient, ServiceExt};
use sha2::{Digest, Sha256};
use tokio::sync::RwLock;

use crate::local_backend_config::McpLocalServerEntry;

// ─── Client Manager ──────────────────────────────────────

pub struct McpClientManager {
    config: Vec<McpLocalServerEntry>,
    protect_mode: bool,
    clients: RwLock<HashMap<String, RunningService<RoleClient, ()>>>,
}

#[derive(Clone)]
struct ResolvedMcpServerEntry {
    name: String,
    transport: McpTransportConfig,
}

impl McpClientManager {
    pub fn new(config: Vec<McpLocalServerEntry>, protect_mode: bool) -> Self {
        Self {
            config,
            protect_mode,
            clients: RwLock::new(HashMap::new()),
        }
    }

    /// 生成 capabilities 上报信息
    pub fn capability_entries(&self) -> Vec<McpServerInfoRelay> {
        self.config
            .iter()
            .map(|entry| McpServerInfoRelay {
                name: entry.name.clone(),
                transport: entry.transport.transport_kind().to_string(),
            })
            .collect()
    }

    /// 列举指定 server 的工具
    pub async fn list_tools(
        &self,
        server: &McpServerRelay,
    ) -> Result<Vec<McpToolInfoRelay>, anyhow::Error> {
        let entry = resolved_server_entry(server);
        let key = self.ensure_connected(&entry).await?;

        let clients = self.clients.read().await;
        let client = clients
            .get(&key)
            .ok_or_else(|| anyhow::anyhow!("MCP client 未找到: {}", entry.name))?;

        let tools = client
            .list_all_tools()
            .await
            .map_err(|e| anyhow::anyhow!("list_tools 失败: {e}"))?;

        Ok(tools
            .into_iter()
            .map(|tool| McpToolInfoRelay {
                name: tool.name.to_string(),
                description: tool.description.as_deref().unwrap_or("").to_string(),
                parameters_schema: serde_json::Value::Object((*tool.input_schema).clone()),
            })
            .collect())
    }

    /// 调用指定 server 上的工具
    pub async fn call_tool(
        &self,
        server: &McpServerRelay,
        tool_name: &str,
        arguments: Option<serde_json::Map<String, serde_json::Value>>,
    ) -> Result<ResponseMcpCallToolPayload, anyhow::Error> {
        let entry = resolved_server_entry(server);
        let key = self.ensure_connected(&entry).await?;

        let clients = self.clients.read().await;
        let client = clients
            .get(&key)
            .ok_or_else(|| anyhow::anyhow!("MCP client 未找到: {}", entry.name))?;

        let request = if let Some(args) = arguments {
            CallToolRequestParams::new(tool_name.to_string()).with_arguments(args)
        } else {
            CallToolRequestParams::new(tool_name.to_string())
        };

        let result = client
            .call_tool(request)
            .await
            .map_err(|e| anyhow::anyhow!("call_tool 失败: {e}"))?;

        Ok(ResponseMcpCallToolPayload {
            server_name: entry.name,
            tool_name: tool_name.to_string(),
            content: result
                .content
                .iter()
                .map(render_content)
                .collect::<Vec<_>>()
                .join("\n"),
            is_error: result.is_error.unwrap_or(false),
        })
    }

    /// 关闭指定 server 的 client
    pub async fn close(&self, server_name: &str) -> Result<(), anyhow::Error> {
        let mut clients = self.clients.write().await;
        let prefix = connection_key_prefix(server_name)?;
        let keys = clients
            .keys()
            .filter(|key| key.starts_with(&prefix))
            .cloned()
            .collect::<Vec<_>>();
        for key in keys {
            let Some(client) = clients.remove(&key) else {
                continue;
            };
            let _ = client.cancel().await;
            tracing::info!(server = %server_name, "MCP client 已关闭");
        }
        Ok(())
    }

    /// 惰性连接——如果 client 不存在则创建
    async fn ensure_connected(
        &self,
        entry: &ResolvedMcpServerEntry,
    ) -> Result<String, anyhow::Error> {
        if self.protect_mode && !self.is_preconfigured_server(entry) {
            anyhow::bail!(
                "local_policy_denied: MCP protect mode 已开启，server `{}` 未在本机预配置中声明相同 transport",
                entry.name
            );
        }
        let key = connection_key(entry)?;
        {
            let clients = self.clients.read().await;
            if clients.contains_key(&key) {
                return Ok(key);
            }
        }

        let transport_kind = entry.transport.transport_kind();
        let client = match &entry.transport {
            McpTransportConfig::Stdio {
                command,
                args,
                env,
                cwd,
            } => {
                let mut cmd = tokio::process::Command::new(command);
                cmd.args(args);
                for var in env {
                    cmd.env(&var.name, &var.value);
                }
                if let Some(cwd) = cwd {
                    cmd.current_dir(cwd);
                }
                let transport = TokioChildProcess::new(cmd)
                    .map_err(|e| anyhow::anyhow!("spawn stdio MCP 进程失败: {e}"))?;
                ().serve(transport)
                    .await
                    .map_err(|e| anyhow::anyhow!("stdio MCP 握手失败: {e}"))?
            }
            McpTransportConfig::Http { url, headers }
            | McpTransportConfig::Sse { url, headers } => ()
                .serve(crate::mcp_connect::mcp_http_worker(url, headers)?)
                .await
                .map_err(|e| anyhow::anyhow!("HTTP MCP 连接失败: {e}"))?,
        };

        let mut clients = self.clients.write().await;
        clients.insert(key.clone(), client);
        tracing::info!(server = %entry.name, transport = %transport_kind, "MCP client 已连接");
        Ok(key)
    }

    fn is_preconfigured_server(&self, entry: &ResolvedMcpServerEntry) -> bool {
        self.config
            .iter()
            .any(|server| server.name == entry.name && server.transport == entry.transport)
    }
}

pub(crate) fn local_server_to_relay_mcp_server(server: &McpLocalServerEntry) -> McpServerRelay {
    mcp_server_parts_to_relay(&server.name, &server.transport)
}

fn resolved_server_entry(server: &McpServerRelay) -> ResolvedMcpServerEntry {
    ResolvedMcpServerEntry {
        name: server.name.clone(),
        transport: relay_transport_to_mcp_transport(&server.transport),
    }
}

fn connection_key(entry: &ResolvedMcpServerEntry) -> Result<String, anyhow::Error> {
    let raw = serde_json::to_vec(&entry.transport)?;
    let digest = Sha256::digest(raw);
    Ok(format!("{}{digest:x}", connection_key_prefix(&entry.name)?))
}

fn connection_key_prefix(server_name: &str) -> Result<String, anyhow::Error> {
    Ok(format!("{}:", serde_json::to_string(server_name)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::mcp_preset::{McpEnvVar, McpHttpHeader};
    use agentdash_relay::{McpEnvVarRelay, McpHttpHeaderRelay, McpTransportConfigRelay};

    fn http_entry(name: &str, header_value: &str) -> ResolvedMcpServerEntry {
        ResolvedMcpServerEntry {
            name: name.to_string(),
            transport: McpTransportConfig::Http {
                url: "http://127.0.0.1:8999/mcp".to_string(),
                headers: vec![McpHttpHeader {
                    name: "x-session".to_string(),
                    value: header_value.to_string(),
                }],
            },
        }
    }

    #[test]
    fn connection_key_uses_server_name_and_stable_transport_hash() {
        let first = http_entry("p4-tools", "session-a");
        let same = http_entry("p4-tools", "session-a");
        let different_transport = http_entry("p4-tools", "session-b");
        let different_name = http_entry("other-tools", "session-a");

        let first_key = connection_key(&first).expect("first key");
        assert_eq!(first_key, connection_key(&same).expect("same key"));
        assert_ne!(
            first_key,
            connection_key(&different_transport).expect("different transport key")
        );
        assert_ne!(
            first_key,
            connection_key(&different_name).expect("different name key")
        );
    }

    #[test]
    fn connection_key_prefix_matches_exact_server_name_only() {
        let foo = http_entry("foo", "session-a");
        let foo_child = http_entry("foo:child", "session-a");

        let foo_prefix = connection_key_prefix("foo").expect("foo prefix");
        assert!(
            connection_key(&foo)
                .expect("foo key")
                .starts_with(&foo_prefix)
        );
        assert!(
            !connection_key(&foo_child)
                .expect("foo child key")
                .starts_with(&foo_prefix)
        );
    }

    #[test]
    fn local_server_to_relay_mcp_server_preserves_resolved_transport_fields() {
        let server = McpLocalServerEntry {
            name: "stdio-tools".to_string(),
            transport: McpTransportConfig::Stdio {
                command: "tool-server".to_string(),
                args: vec!["--mode".to_string(), "mcp".to_string()],
                env: vec![McpEnvVar {
                    name: "P4CLIENT".to_string(),
                    value: "demo-client".to_string(),
                }],
                cwd: Some("F:/work/demo".to_string()),
            },
        };

        let relay_server = local_server_to_relay_mcp_server(&server);
        assert_eq!(relay_server.name, "stdio-tools");
        match relay_server.transport {
            McpTransportConfigRelay::Stdio {
                command,
                args,
                env,
                cwd,
            } => {
                assert_eq!(command, "tool-server");
                assert_eq!(args, vec!["--mode".to_string(), "mcp".to_string()]);
                assert_eq!(env[0].name, "P4CLIENT");
                assert_eq!(env[0].value, "demo-client");
                assert_eq!(cwd.as_deref(), Some("F:/work/demo"));
            }
            other => panic!("expected stdio transport, got {other:?}"),
        }
    }

    #[test]
    fn resolved_server_entry_preserves_http_headers() {
        let relay_server = McpServerRelay {
            name: "http-tools".to_string(),
            transport: McpTransportConfigRelay::Http {
                url: "http://127.0.0.1:8999/mcp?p4_client=demo".to_string(),
                headers: vec![McpHttpHeaderRelay {
                    name: "x-p4-client".to_string(),
                    value: "demo".to_string(),
                }],
            },
        };

        let entry = resolved_server_entry(&relay_server);
        assert_eq!(entry.name, "http-tools");
        assert_eq!(
            entry.transport,
            McpTransportConfig::Http {
                url: "http://127.0.0.1:8999/mcp?p4_client=demo".to_string(),
                headers: vec![McpHttpHeader {
                    name: "x-p4-client".to_string(),
                    value: "demo".to_string(),
                }],
            }
        );
    }

    #[test]
    fn resolved_server_entry_preserves_stdio_env_and_cwd() {
        let relay_server = McpServerRelay {
            name: "stdio-tools".to_string(),
            transport: McpTransportConfigRelay::Stdio {
                command: "tool-server".to_string(),
                args: vec!["--mode".to_string(), "mcp".to_string()],
                env: vec![McpEnvVarRelay {
                    name: "P4CLIENT".to_string(),
                    value: "demo-client".to_string(),
                }],
                cwd: Some("F:/work/demo".to_string()),
            },
        };

        let entry = resolved_server_entry(&relay_server);
        assert_eq!(entry.name, "stdio-tools");
        assert_eq!(
            entry.transport,
            McpTransportConfig::Stdio {
                command: "tool-server".to_string(),
                args: vec!["--mode".to_string(), "mcp".to_string()],
                env: vec![McpEnvVar {
                    name: "P4CLIENT".to_string(),
                    value: "demo-client".to_string(),
                }],
                cwd: Some("F:/work/demo".to_string()),
            }
        );
    }

    #[tokio::test]
    async fn protect_mode_rejects_undeclared_server_name_before_connecting() {
        let manager = McpClientManager::new(
            vec![McpLocalServerEntry {
                name: "declared".to_string(),
                transport: McpTransportConfig::Http {
                    url: "http://127.0.0.1:8999/mcp".to_string(),
                    headers: vec![],
                },
            }],
            true,
        );
        let undeclared = McpServerRelay {
            name: "undeclared".to_string(),
            transport: McpTransportConfigRelay::Http {
                url: "http://127.0.0.1:8999/mcp".to_string(),
                headers: vec![],
            },
        };

        let error = manager
            .list_tools(&undeclared)
            .await
            .expect_err("undeclared server should fail in protect mode");
        assert!(error.to_string().contains("local_policy_denied"));
    }

    #[tokio::test]
    async fn protect_mode_rejects_changed_transport_before_connecting() {
        let manager = McpClientManager::new(
            vec![McpLocalServerEntry {
                name: "declared".to_string(),
                transport: McpTransportConfig::Http {
                    url: "http://127.0.0.1:8999/mcp".to_string(),
                    headers: vec![],
                },
            }],
            true,
        );
        let changed_transport = McpServerRelay {
            name: "declared".to_string(),
            transport: McpTransportConfigRelay::Http {
                url: "http://127.0.0.1:8998/mcp".to_string(),
                headers: vec![],
            },
        };

        let error = manager
            .list_tools(&changed_transport)
            .await
            .expect_err("changed transport should fail in protect mode");
        assert!(error.to_string().contains("local_policy_denied"));
    }

    #[test]
    fn unprotected_manager_accepts_project_scoped_server_policy() {
        let manager = McpClientManager::new(Vec::new(), false);
        let entry = resolved_server_entry(&McpServerRelay {
            name: "project-tools".to_string(),
            transport: McpTransportConfigRelay::Http {
                url: "http://127.0.0.1:8999/mcp".to_string(),
                headers: vec![],
            },
        });

        assert!(
            !manager.protect_mode && !manager.is_preconfigured_server(&entry),
            "默认关闭 protect mode 时，项目级 MCP server 不需要命中本机静态 catalog"
        );
    }
}
