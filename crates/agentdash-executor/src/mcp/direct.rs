use std::{collections::HashMap, sync::Arc};

use agentdash_spi::{
    AgentTool, AgentToolError, AgentToolResult, CapabilityState, ContentPart, DynAgentTool,
    McpHttpHeader, McpTransportConfig, RuntimeMcpServer, ToolUpdateCallback,
};
use async_trait::async_trait;
use reqwest::header::{HeaderName, HeaderValue};
use rmcp::{
    RoleClient, ServiceExt,
    model::{CallToolRequestParams, CallToolResult, Tool},
    service::{RunningService, ServiceError},
    transport::streamable_http_client::{
        StreamableHttpClientTransportConfig, StreamableHttpClientWorker,
    },
};
use tokio::sync::{Mutex, RwLock};
use tokio_util::sync::CancellationToken;

use agentdash_mcp::render_content;
use agentdash_spi::ConnectorError;

use super::{
    DiscoveredMcpTool,
    common::{McpToolSurface, build_discovered_entry, normalize_args_object},
    naming::capability_key_for_mcp_server_name,
};

type McpHttpClient = RunningService<RoleClient, ()>;

#[derive(Debug, Clone)]
struct McpHttpServerSpec {
    name: String,
    url: String,
    headers: Vec<McpHttpHeader>,
}

#[derive(Default)]
struct DirectMcpClientPool {
    clients: RwLock<HashMap<String, Arc<Mutex<McpHttpClient>>>>,
}

impl DirectMcpClientPool {
    async fn list_tools(&self, server: &McpHttpServerSpec) -> Result<Vec<Tool>, ConnectorError> {
        let client = self.ensure_client(server).await?;
        let result = {
            let client = client.lock().await;
            client.list_all_tools().await
        };
        match result {
            Ok(tools) => Ok(tools),
            Err(error) => {
                self.invalidate(server).await;
                Err(ConnectorError::ConnectionFailed(format_service_error(
                    &error,
                )))
            }
        }
    }

    async fn call_tool(
        &self,
        server: &McpHttpServerSpec,
        request: CallToolRequestParams,
    ) -> Result<CallToolResult, String> {
        let client = self
            .ensure_client(server)
            .await
            .map_err(|e| e.to_string())?;
        let result = {
            let client = client.lock().await;
            client.call_tool(request).await
        };
        match result {
            Ok(result) => Ok(result),
            Err(error) => {
                self.invalidate(server).await;
                Err(format_service_error(&error))
            }
        }
    }

    async fn ensure_client(
        &self,
        server: &McpHttpServerSpec,
    ) -> Result<Arc<Mutex<McpHttpClient>>, ConnectorError> {
        let key = self.key(server);
        if let Some(client) = self.open_client(&key).await {
            return Ok(client);
        }

        let new_client = Arc::new(Mutex::new(
            connect_http_server(&server.url, &server.headers).await?,
        ));
        let mut clients = self.clients.write().await;
        if let Some(existing) = clients.get(&key).cloned() {
            drop(clients);
            let is_closed = existing.lock().await.is_closed();
            if !is_closed {
                return Ok(existing);
            }
            self.invalidate_key(&key).await;
            clients = self.clients.write().await;
        }
        clients.insert(key, new_client.clone());
        Ok(new_client)
    }

    async fn open_client(&self, key: &str) -> Option<Arc<Mutex<McpHttpClient>>> {
        let client = self.clients.read().await.get(key).cloned()?;
        let is_closed = client.lock().await.is_closed();
        if is_closed {
            self.invalidate_key(key).await;
            None
        } else {
            Some(client)
        }
    }

    async fn invalidate(&self, server: &McpHttpServerSpec) {
        let key = self.key(server);
        self.invalidate_key(&key).await;
    }

    async fn invalidate_key(&self, key: &str) {
        self.clients.write().await.remove(key);
    }

    fn key(&self, server: &McpHttpServerSpec) -> String {
        format!(
            "{}\n{}",
            server.url,
            serde_json::to_string(&server.headers).unwrap_or_default()
        )
    }
}

#[derive(Clone)]
pub struct McpToolAdapter {
    surface: McpToolSurface,
    server: McpHttpServerSpec,
    pool: Arc<DirectMcpClientPool>,
}

impl McpToolAdapter {
    fn from_tool(server: McpHttpServerSpec, pool: Arc<DirectMcpClientPool>, tool: Tool) -> Self {
        let surface = McpToolSurface::new(
            server.name.clone(),
            tool.name.to_string(),
            tool.description.as_deref(),
            serde_json::Value::Object((*tool.input_schema).clone()),
        );

        Self {
            surface,
            server,
            pool,
        }
    }
}

#[async_trait]
impl AgentTool for McpToolAdapter {
    fn name(&self) -> &str {
        &self.surface.runtime_name
    }

    fn description(&self) -> &str {
        &self.surface.description
    }

    fn parameters_schema(&self) -> serde_json::Value {
        self.surface.parameters_schema.clone()
    }

    async fn execute(
        &self,
        _tool_call_id: &str,
        args: serde_json::Value,
        _cancel: CancellationToken,
        _on_update: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let arguments = normalize_args_object(args)?;

        let request = if let Some(arguments) = arguments {
            CallToolRequestParams::new(self.surface.tool_name.clone()).with_arguments(arguments)
        } else {
            CallToolRequestParams::new(self.surface.tool_name.clone())
        };

        let call_result = self.pool.call_tool(&self.server, request).await;

        match call_result {
            Ok(result) => Ok(convert_call_result(
                &self.server,
                &self.surface.tool_name,
                result,
            )),
            Err(error) => Err(AgentToolError::ExecutionFailed(format!(
                "调用 MCP 工具失败（tool={}）: {}",
                self.surface.tool_name, error
            ))),
        }
    }
}

pub async fn discover_mcp_tools(
    servers: &[RuntimeMcpServer],
    capability_state: &CapabilityState,
) -> Result<Vec<DynAgentTool>, ConnectorError> {
    Ok(discover_mcp_tool_entries(servers, capability_state)
        .await?
        .into_iter()
        .map(|entry| entry.tool)
        .collect())
}

pub async fn discover_mcp_tool_entries(
    servers: &[RuntimeMcpServer],
    capability_state: &CapabilityState,
) -> Result<Vec<DiscoveredMcpTool>, ConnectorError> {
    let mut entries = Vec::new();
    let pool = Arc::new(DirectMcpClientPool::default());

    for server in servers {
        let Some(server_spec) = parse_http_mcp_server(server) else {
            tracing::debug!("跳过非 HTTP MCP Server");
            continue;
        };

        let listed = pool.list_tools(&server_spec).await?;

        entries.extend(build_direct_discovered_entries_from_listed_tools(
            server_spec,
            pool.clone(),
            listed,
            capability_state,
        ));
    }

    Ok(entries)
}

fn build_direct_discovered_entries_from_listed_tools(
    server_spec: McpHttpServerSpec,
    pool: Arc<DirectMcpClientPool>,
    listed: Vec<Tool>,
    capability_state: &CapabilityState,
) -> Vec<DiscoveredMcpTool> {
    let capability_key = capability_key_for_mcp_server_name(&server_spec.name);
    listed
        .into_iter()
        .filter(|tool| {
            capability_state.is_capability_tool_enabled(&capability_key, tool.name.as_ref(), None)
        })
        .map(|tool| {
            let adapter = Arc::new(McpToolAdapter::from_tool(
                server_spec.clone(),
                pool.clone(),
                tool,
            ));
            let tool = adapter.clone() as DynAgentTool;
            build_discovered_entry(&adapter.surface, false, tool)
        })
        .collect()
}

async fn connect_http_server(
    url: &str,
    headers: &[McpHttpHeader],
) -> Result<rmcp::service::RunningService<rmcp::RoleClient, ()>, ConnectorError> {
    let config = StreamableHttpClientTransportConfig::with_uri(url.to_string())
        .custom_headers(build_header_map(headers)?);
    let worker = StreamableHttpClientWorker::new(reqwest::Client::new(), config);
    ().serve(worker)
        .await
        .map_err(|e| ConnectorError::ConnectionFailed(e.to_string()))
}

fn build_header_map(
    headers: &[McpHttpHeader],
) -> Result<HashMap<HeaderName, HeaderValue>, ConnectorError> {
    let mut map = HashMap::new();
    for header in headers {
        let name = HeaderName::from_bytes(header.name.as_bytes()).map_err(|error| {
            ConnectorError::InvalidConfig(format!("MCP HTTP header name 无效: {error}"))
        })?;
        let value = HeaderValue::from_str(&header.value).map_err(|error| {
            ConnectorError::InvalidConfig(format!("MCP HTTP header value 无效: {error}"))
        })?;
        map.insert(name, value);
    }
    Ok(map)
}

fn convert_call_result(
    _server: &McpHttpServerSpec,
    tool_name: &str,
    result: CallToolResult,
) -> AgentToolResult {
    let _ = tool_name;
    let mut sections: Vec<String> = Vec::new();

    if let Some(structured) = &result.structured_content {
        sections.push(format!(
            "structured_content:\n{}",
            serde_json::to_string_pretty(structured).unwrap_or_else(|_| structured.to_string())
        ));
    }

    if !result.content.is_empty() {
        sections.push(format!(
            "content:\n{}",
            result
                .content
                .iter()
                .map(render_content)
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }

    AgentToolResult {
        content: vec![ContentPart::text(sections.join("\n\n"))],
        is_error: result.is_error.unwrap_or(false),
        details: None,
    }
}

fn format_service_error(error: &ServiceError) -> String {
    match error {
        ServiceError::McpError(data) => format!("{} ({:?})", data.message, data.code),
        other => other.to_string(),
    }
}

fn parse_http_mcp_server(server: &RuntimeMcpServer) -> Option<McpHttpServerSpec> {
    match &server.transport {
        McpTransportConfig::Http { url, headers } => Some(McpHttpServerSpec {
            name: server.name.clone(),
            url: url.clone(),
            headers: headers.clone(),
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{borrow::Cow, sync::Arc};

    use agentdash_spi::{ToolCapability, ToolCapabilityFilter};

    fn header(value: &str) -> McpHttpHeader {
        McpHttpHeader {
            name: "x-session".to_string(),
            value: value.to_string(),
        }
    }

    fn listed_tool(name: &str) -> Tool {
        let mut tool = Tool::default();
        tool.name = Cow::Owned(name.to_string());
        tool.description = Some(Cow::Owned(format!("{name} description")));
        let mut input_schema = serde_json::Map::new();
        input_schema.insert("type".to_string(), serde_json::json!("object"));
        tool.input_schema = Arc::new(input_schema);
        tool
    }

    #[test]
    fn parse_http_mcp_server_preserves_resolved_headers() {
        let server = RuntimeMcpServer {
            name: "p4-tools".to_string(),
            transport: McpTransportConfig::Http {
                url: "http://127.0.0.1:8999/mcp?p4_client=demo".to_string(),
                headers: vec![header("demo")],
            },
            uses_relay: false,
        };

        let parsed = parse_http_mcp_server(&server).expect("http server should parse");
        assert_eq!(parsed.name, "p4-tools");
        assert_eq!(parsed.url, "http://127.0.0.1:8999/mcp?p4_client=demo");
        assert_eq!(parsed.headers, vec![header("demo")]);
    }

    #[test]
    fn direct_pool_key_includes_url_and_headers() {
        let pool = DirectMcpClientPool::default();
        let base = McpHttpServerSpec {
            name: "p4-tools".to_string(),
            url: "http://127.0.0.1:8999/mcp".to_string(),
            headers: vec![header("session-a")],
        };
        let same = McpHttpServerSpec {
            headers: vec![header("session-a")],
            ..base.clone()
        };
        let different_headers = McpHttpServerSpec {
            headers: vec![header("session-b")],
            ..base.clone()
        };
        let different_url = McpHttpServerSpec {
            url: "http://127.0.0.1:8999/other".to_string(),
            ..base.clone()
        };

        let base_key = pool.key(&base);
        assert_eq!(base_key, pool.key(&same));
        assert_ne!(base_key, pool.key(&different_headers));
        assert_ne!(base_key, pool.key(&different_url));
    }

    #[test]
    fn direct_discovery_filters_custom_mcp_raw_tool_policy_from_entries_and_callables() {
        let server_spec = McpHttpServerSpec {
            name: "code-analyzer".to_string(),
            url: "http://127.0.0.1:8999/mcp".to_string(),
            headers: vec![],
        };
        let mut capability_state = CapabilityState::default();
        capability_state
            .tool
            .capabilities
            .insert(ToolCapability::custom_mcp("code-analyzer"));
        capability_state.tool.tool_policy.insert(
            "mcp:code-analyzer".to_string(),
            ToolCapabilityFilter {
                include_only: Default::default(),
                exclude: ["blocked_tool".to_string()].into_iter().collect(),
            },
        );

        let entries = build_direct_discovered_entries_from_listed_tools(
            server_spec,
            Arc::new(DirectMcpClientPool::default()),
            vec![listed_tool("allowed_tool"), listed_tool("blocked_tool")],
            &capability_state,
        );

        let raw_tool_names = entries
            .iter()
            .map(|entry| entry.tool_name.as_str())
            .collect::<Vec<_>>();
        let callable_names = entries
            .iter()
            .map(|entry| entry.tool.name())
            .collect::<Vec<_>>();

        assert_eq!(raw_tool_names, vec!["allowed_tool"]);
        assert_eq!(callable_names, vec!["mcp_code_analyzer_allowed_tool"]);
    }
}
