//! 本机 MCP Client 管理器 — 管理 stdio 进程和 localhost HTTP 连接

use std::collections::HashMap;

use agentdash_mcp::render_content;
use agentdash_relay::{McpServerInfoRelay, McpToolInfoRelay, ResponseMcpCallToolPayload};
use rmcp::model::CallToolRequestParams;
use rmcp::service::RunningService;
use rmcp::transport::child_process::TokioChildProcess;
use rmcp::{RoleClient, ServiceExt};
use tokio::sync::RwLock;

use crate::local_backend_config::McpLocalServerEntry;

// ─── Client Manager ──────────────────────────────────────

pub struct McpClientManager {
    config: Vec<McpLocalServerEntry>,
    clients: RwLock<HashMap<String, RunningService<RoleClient, ()>>>,
}

impl McpClientManager {
    pub fn new(config: Vec<McpLocalServerEntry>) -> Self {
        Self {
            config,
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
        server_name: &str,
    ) -> Result<Vec<McpToolInfoRelay>, anyhow::Error> {
        self.ensure_connected(server_name).await?;

        let clients = self.clients.read().await;
        let client = clients
            .get(server_name)
            .ok_or_else(|| anyhow::anyhow!("MCP client 未找到: {server_name}"))?;

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
        server_name: &str,
        tool_name: &str,
        arguments: Option<serde_json::Map<String, serde_json::Value>>,
    ) -> Result<ResponseMcpCallToolPayload, anyhow::Error> {
        self.ensure_connected(server_name).await?;

        let clients = self.clients.read().await;
        let client = clients
            .get(server_name)
            .ok_or_else(|| anyhow::anyhow!("MCP client 未找到: {server_name}"))?;

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
            server_name: server_name.to_string(),
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
        if let Some(client) = clients.remove(server_name) {
            let _ = client.cancel().await;
            tracing::info!(server = %server_name, "MCP client 已关闭");
        }
        Ok(())
    }

    /// 惰性连接——如果 client 不存在则创建
    async fn ensure_connected(&self, server_name: &str) -> Result<(), anyhow::Error> {
        {
            let clients = self.clients.read().await;
            if clients.contains_key(server_name) {
                return Ok(());
            }
        }

        let entry = self
            .config
            .iter()
            .find(|e| e.name == server_name)
            .ok_or_else(|| anyhow::anyhow!("未知的 MCP server: {server_name}"))?
            .clone();

        let transport_kind = entry.transport.transport_kind();
        let client = match &entry.transport {
            agentdash_domain::mcp_preset::McpTransportConfig::Stdio { command, args, env } => {
                let mut cmd = tokio::process::Command::new(command);
                cmd.args(args);
                for var in env {
                    cmd.env(&var.name, &var.value);
                }
                let transport = TokioChildProcess::new(cmd)
                    .map_err(|e| anyhow::anyhow!("spawn stdio MCP 进程失败: {e}"))?;
                ().serve(transport)
                    .await
                    .map_err(|e| anyhow::anyhow!("stdio MCP 握手失败: {e}"))?
            }
            agentdash_domain::mcp_preset::McpTransportConfig::Http { url, .. }
            | agentdash_domain::mcp_preset::McpTransportConfig::Sse { url, .. } => {
                ().serve(crate::mcp_connect::mcp_http_worker(url))
                    .await
                    .map_err(|e| anyhow::anyhow!("HTTP MCP 连接失败: {e}"))?
            }
        };

        let mut clients = self.clients.write().await;
        clients.insert(server_name.to_string(), client);
        tracing::info!(server = %server_name, transport = %transport_kind, "MCP client 已连接");
        Ok(())
    }
}
