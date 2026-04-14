//! 本机 MCP Client 管理器 — 管理 stdio 进程和 localhost HTTP 连接

use std::collections::HashMap;
use std::path::PathBuf;

use agentdash_relay::{McpServerInfoRelay, McpToolInfoRelay, ResponseMcpCallToolPayload};
use rmcp::model::{CallToolRequestParams, Content};
use rmcp::service::RunningService;
use rmcp::transport::child_process::TokioChildProcess;
use rmcp::transport::streamable_http_client::{
    StreamableHttpClientTransportConfig, StreamableHttpClientWorker,
};
use rmcp::{RoleClient, ServiceExt};
use serde::Deserialize;
use tokio::sync::RwLock;

// ─── 配置文件结构 ────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct McpLocalConfig {
    #[serde(default)]
    pub servers: Vec<McpLocalServerEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct McpLocalServerEntry {
    pub name: String,
    /// "stdio" | "http" | "sse"
    pub transport: String,
    // stdio 字段
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Option<Vec<String>>,
    #[serde(default)]
    pub env: Option<Vec<McpEnvEntry>>,
    // http/sse 字段
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct McpEnvEntry {
    pub name: String,
    pub value: String,
}

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

    /// 从 `.agentdash/mcp-servers.json` 加载配置
    pub fn load_config(accessible_roots: &[PathBuf]) -> Vec<McpLocalServerEntry> {
        let Some(root) = accessible_roots.first() else {
            return Vec::new();
        };
        let config_path = root.join(".agentdash").join("mcp-servers.json");
        if !config_path.exists() {
            tracing::debug!(path = %config_path.display(), "MCP 配置文件不存在，跳过");
            return Vec::new();
        }
        match std::fs::read_to_string(&config_path) {
            Ok(content) => match serde_json::from_str::<McpLocalConfig>(&content) {
                Ok(cfg) => {
                    tracing::info!(
                        count = cfg.servers.len(),
                        path = %config_path.display(),
                        "已加载 MCP server 配置"
                    );
                    cfg.servers
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        path = %config_path.display(),
                        "MCP 配置解析失败"
                    );
                    Vec::new()
                }
            },
            Err(e) => {
                tracing::warn!(error = %e, path = %config_path.display(), "读取 MCP 配置失败");
                Vec::new()
            }
        }
    }

    /// 生成 capabilities 上报信息
    pub fn capability_entries(&self) -> Vec<McpServerInfoRelay> {
        self.config
            .iter()
            .map(|entry| McpServerInfoRelay {
                name: entry.name.clone(),
                transport: entry.transport.clone(),
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
                description: tool
                    .description
                    .as_deref()
                    .unwrap_or("")
                    .to_string(),
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

        let content = result
            .content
            .iter()
            .map(render_content)
            .collect::<Vec<_>>()
            .join("\n");

        Ok(ResponseMcpCallToolPayload {
            server_name: server_name.to_string(),
            tool_name: tool_name.to_string(),
            content,
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

    /// 关闭所有 client（relay 断连时调用）
    pub async fn close_all(&self) {
        let mut clients = self.clients.write().await;
        for (name, client) in clients.drain() {
            let _ = client.cancel().await;
            tracing::info!(server = %name, "MCP client 已关闭");
        }
    }

    /// 惰性连接——如果 client 不存在则创建
    async fn ensure_connected(&self, server_name: &str) -> Result<(), anyhow::Error> {
        // 快路径：已连接
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

        let client = match entry.transport.as_str() {
            "stdio" => self.connect_stdio(&entry).await?,
            "http" | "sse" => self.connect_http(&entry).await?,
            other => anyhow::bail!("不支持的 MCP transport: {other}"),
        };

        let mut clients = self.clients.write().await;
        clients.insert(server_name.to_string(), client);
        tracing::info!(server = %server_name, transport = %entry.transport, "MCP client 已连接");
        Ok(())
    }

    async fn connect_stdio(
        &self,
        entry: &McpLocalServerEntry,
    ) -> Result<RunningService<RoleClient, ()>, anyhow::Error> {
        let command = entry
            .command
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("stdio MCP server '{}' 缺少 command", entry.name))?;

        let mut cmd = tokio::process::Command::new(command);
        if let Some(args) = &entry.args {
            cmd.args(args);
        }
        if let Some(env_vars) = &entry.env {
            for var in env_vars {
                cmd.env(&var.name, &var.value);
            }
        }

        let transport = TokioChildProcess::new(cmd)
            .map_err(|e| anyhow::anyhow!("spawn stdio MCP 进程失败: {e}"))?;

        let client = ().serve(transport)
            .await
            .map_err(|e| anyhow::anyhow!("stdio MCP 握手失败: {e}"))?;

        Ok(client)
    }

    async fn connect_http(
        &self,
        entry: &McpLocalServerEntry,
    ) -> Result<RunningService<RoleClient, ()>, anyhow::Error> {
        let url = entry
            .url
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("HTTP MCP server '{}' 缺少 url", entry.name))?;

        let worker = StreamableHttpClientWorker::new(
            reqwest::Client::new(),
            StreamableHttpClientTransportConfig::with_uri(url.to_string()),
        );

        let client = ().serve(worker)
            .await
            .map_err(|e| anyhow::anyhow!("HTTP MCP 连接失败: {e}"))?;

        Ok(client)
    }
}

fn render_content(content: &Content) -> String {
    if let Some(text) = content.raw.as_text() {
        return text.text.clone();
    }
    serde_json::to_string_pretty(content).unwrap_or_else(|_| "<无法解析 MCP 内容>".to_string())
}
