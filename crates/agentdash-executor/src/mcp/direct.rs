use std::sync::Arc;

use agentdash_agent::{
    AgentTool, AgentToolError, AgentToolResult, ContentPart, DynAgentTool, ToolUpdateCallback,
    tools::sanitize_tool_schema,
};
use agentdash_spi::platform::tool_capability::{
    CAP_RELAY_MANAGEMENT, CAP_STORY_MANAGEMENT, CAP_TASK_MANAGEMENT, CAP_WORKFLOW_MANAGEMENT,
};
use agentdash_spi::{CapabilityState, McpTransportConfig, SessionMcpServer};
use async_trait::async_trait;
use rmcp::{
    ServiceExt,
    model::{CallToolRequestParams, CallToolResult, Tool},
    service::ServiceError,
    transport::streamable_http_client::{
        StreamableHttpClientTransportConfig, StreamableHttpClientWorker,
    },
};
use tokio_util::sync::CancellationToken;

use agentdash_mcp::render_content;
use agentdash_spi::ConnectorError;

use super::DiscoveredMcpTool;

#[derive(Debug, Clone)]
struct McpHttpServerSpec {
    name: String,
    url: String,
}

#[derive(Clone)]
pub struct McpToolAdapter {
    runtime_name: String,
    original_name: String,
    description: String,
    parameters_schema: serde_json::Value,
    server: McpHttpServerSpec,
}

impl McpToolAdapter {
    fn from_tool(server: McpHttpServerSpec, tool: Tool) -> Self {
        let original_name = tool.name.to_string();
        let runtime_name = namespaced_tool_name(&server.name, &original_name);
        let description = tool
            .description
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("MCP 工具")
            .to_string();
        let parameters_schema =
            sanitize_tool_schema(serde_json::Value::Object((*tool.input_schema).clone()));

        Self {
            runtime_name,
            original_name,
            description,
            parameters_schema,
            server,
        }
    }
}

#[async_trait]
impl AgentTool for McpToolAdapter {
    fn name(&self) -> &str {
        &self.runtime_name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters_schema(&self) -> serde_json::Value {
        self.parameters_schema.clone()
    }

    async fn execute(
        &self,
        _tool_call_id: &str,
        args: serde_json::Value,
        _cancel: CancellationToken,
        _on_update: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let arguments = match args {
            serde_json::Value::Null => None,
            serde_json::Value::Object(map) => Some(map),
            other => {
                return Err(AgentToolError::InvalidArguments(format!(
                    "MCP 工具参数必须是 JSON object，实际为: {}",
                    other
                )));
            }
        };

        let client = connect_http_server(&self.server.url)
            .await
            .map_err(|e| AgentToolError::ExecutionFailed(e.to_string()))?;

        let request = if let Some(arguments) = arguments {
            CallToolRequestParams::new(self.original_name.clone()).with_arguments(arguments)
        } else {
            CallToolRequestParams::new(self.original_name.clone())
        };

        let call_result = client.call_tool(request).await;
        let _ = client.cancel().await;

        match call_result {
            Ok(result) => Ok(convert_call_result(
                &self.server,
                &self.original_name,
                result,
            )),
            Err(error) => Err(AgentToolError::ExecutionFailed(format!(
                "调用 MCP 工具失败（tool={}）: {}",
                self.original_name,
                format_service_error(&error)
            ))),
        }
    }
}

pub async fn discover_mcp_tools(
    servers: &[SessionMcpServer],
    capability_state: &CapabilityState,
) -> Result<Vec<DynAgentTool>, ConnectorError> {
    Ok(discover_mcp_tool_entries(servers, capability_state)
        .await?
        .into_iter()
        .map(|entry| entry.tool)
        .collect())
}

pub async fn discover_mcp_tool_entries(
    servers: &[SessionMcpServer],
    capability_state: &CapabilityState,
) -> Result<Vec<DiscoveredMcpTool>, ConnectorError> {
    let mut entries = Vec::new();

    for server in servers {
        let Some(server_spec) = parse_http_session_server(server) else {
            tracing::debug!("跳过非 HTTP MCP Server");
            continue;
        };

        let client = connect_http_server(&server_spec.url)
            .await
            .map_err(|e| ConnectorError::ConnectionFailed(e.to_string()))?;
        let listed = client
            .list_all_tools()
            .await
            .map_err(|e| ConnectorError::ConnectionFailed(format_service_error(&e)))?;
        let _ = client.cancel().await;

        let capability_key = capability_key_for_mcp_server_name(&server_spec.name);
        for tool in listed {
            if !capability_state.is_capability_tool_enabled(
                &capability_key,
                tool.name.as_ref(),
                None,
            ) {
                continue;
            }
            let adapter = Arc::new(McpToolAdapter::from_tool(server_spec.clone(), tool));
            let tool = adapter.clone() as DynAgentTool;
            entries.push(DiscoveredMcpTool {
                runtime_name: adapter.runtime_name.clone(),
                server_name: server_spec.name.clone(),
                tool_name: adapter.original_name.clone(),
                uses_relay: false,
                description: adapter.description.clone(),
                parameters_schema: adapter.parameters_schema.clone(),
                tool,
            });
        }
    }

    Ok(entries)
}

pub fn capability_key_for_mcp_server_name(server_name: &str) -> String {
    match agent_facing_mcp_server_name(server_name).as_str() {
        "agentdash-relay-tools" => CAP_RELAY_MANAGEMENT.to_string(),
        "agentdash-story-tools" => CAP_STORY_MANAGEMENT.to_string(),
        "agentdash-task-tools" => CAP_TASK_MANAGEMENT.to_string(),
        "agentdash-workflow-tools" => CAP_WORKFLOW_MANAGEMENT.to_string(),
        other => format!("mcp:{other}"),
    }
}

async fn connect_http_server(
    url: &str,
) -> Result<rmcp::service::RunningService<rmcp::RoleClient, ()>, ConnectorError> {
    let worker = StreamableHttpClientWorker::new(
        reqwest::Client::new(),
        StreamableHttpClientTransportConfig::with_uri(url.to_string()),
    );
    ().serve(worker)
        .await
        .map_err(|e| ConnectorError::ConnectionFailed(e.to_string()))
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

fn parse_http_session_server(server: &SessionMcpServer) -> Option<McpHttpServerSpec> {
    match &server.transport {
        McpTransportConfig::Http { url, .. } => Some(McpHttpServerSpec {
            name: server.name.clone(),
            url: url.clone(),
        }),
        _ => None,
    }
}

pub fn namespaced_tool_name(server_name: &str, tool_name: &str) -> String {
    let agent_facing_server = agent_facing_mcp_server_name(server_name);
    format!(
        "mcp_{}_{}",
        sanitize_identifier(&agent_facing_server),
        sanitize_identifier(tool_name)
    )
}

pub fn agent_facing_mcp_server_name(server_name: &str) -> String {
    const PLATFORM_SCOPED_PREFIXES: &[(&str, &str)] = &[
        ("agentdash-story-tools-", "agentdash-story-tools"),
        ("agentdash-task-tools-", "agentdash-task-tools"),
        ("agentdash-workflow-tools-", "agentdash-workflow-tools"),
    ];

    for (prefix, stable_name) in PLATFORM_SCOPED_PREFIXES {
        if server_name.starts_with(prefix) {
            return (*stable_name).to_string();
        }
    }

    server_name.to_string()
}

pub(crate) fn sanitize_identifier(input: &str) -> String {
    let sanitized = input
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    sanitized.trim_matches('_').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn namespaced_name_hides_platform_scope_ids() {
        assert_eq!(
            namespaced_tool_name("agentdash-task-tools-1234", "update_status"),
            "mcp_agentdash_task_tools_update_status"
        );
        assert_eq!(
            namespaced_tool_name("agentdash-workflow-tools-8de613e7", "get_lifecycle"),
            "mcp_agentdash_workflow_tools_get_lifecycle"
        );
    }

    #[test]
    fn namespaced_name_keeps_custom_server_namespace() {
        assert_eq!(
            namespaced_tool_name("code-analyzer", "scan_repo"),
            "mcp_code_analyzer_scan_repo"
        );
    }

    #[test]
    fn platform_mcp_server_names_map_to_capability_keys() {
        assert_eq!(
            capability_key_for_mcp_server_name("agentdash-workflow-tools-8de613e7"),
            "workflow_management"
        );
        assert_eq!(
            capability_key_for_mcp_server_name("agentdash-task-tools-1234"),
            "task_management"
        );
        assert_eq!(
            capability_key_for_mcp_server_name("code-analyzer"),
            "mcp:code-analyzer"
        );
    }
}
