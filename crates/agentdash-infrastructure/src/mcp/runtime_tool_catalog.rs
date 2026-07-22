use std::{collections::HashMap, sync::Arc};

use agentdash_agent_runtime::{
    RuntimeToolDefinition, RuntimeToolEffect, RuntimeToolExecutor, RuntimeToolInvocation,
    RuntimeToolPermission, ToolProtocolProjector,
};
use agentdash_agent_service_api::{AgentToolName, AgentToolResult};
use agentdash_platform_spi::{
    CapabilityState, McpHttpHeader, McpRelayProvider, McpTransportConfig, RelayMcpCallContext,
    RuntimeMcpServer, sanitize_tool_schema,
};
use async_trait::async_trait;
use reqwest::header::{HeaderName, HeaderValue};
use rmcp::{
    RoleClient, ServiceExt,
    model::{CallToolRequestParams, Tool},
    service::RunningService,
    transport::streamable_http_client::{
        StreamableHttpClientTransportConfig, StreamableHttpClientWorker,
    },
};
use tokio::sync::Mutex;

type DirectMcpClient = RunningService<RoleClient, ()>;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RuntimeMcpToolCatalogError {
    #[error("MCP server `{server}` has no supported production placement")]
    UnsupportedPlacement { server: String },
    #[error("MCP server `{server}` discovery failed: {reason}")]
    Discovery { server: String, reason: String },
    #[error("MCP runtime tool identity is invalid: {reason}")]
    InvalidTool { reason: String },
}

pub struct RuntimeMcpToolCatalogRequest {
    pub servers: Vec<RuntimeMcpServer>,
    pub capability_state: CapabilityState,
    pub relay_context: Option<RelayMcpCallContext>,
}

#[async_trait]
pub trait RuntimeDynamicToolCatalog: Send + Sync {
    async fn resolve(
        &self,
        request: RuntimeMcpToolCatalogRequest,
    ) -> Result<Vec<Arc<dyn RuntimeToolExecutor>>, RuntimeMcpToolCatalogError>;
}

/// Resolves the exact MCP definitions and execution handles bound to one Runtime target.
///
/// Direct HTTP and relay placement share the same namespacing and capability filtering. The
/// returned handles are installed into `PlatformToolBroker` before Host target provisioning, so
/// desired-surface declarations and callback execution cannot drift.
pub struct ProductionRuntimeMcpToolCatalog {
    relay: Option<Arc<dyn McpRelayProvider>>,
}

impl ProductionRuntimeMcpToolCatalog {
    pub fn new(relay: Option<Arc<dyn McpRelayProvider>>) -> Self {
        Self { relay }
    }
}

#[async_trait]
impl RuntimeDynamicToolCatalog for ProductionRuntimeMcpToolCatalog {
    async fn resolve(
        &self,
        request: RuntimeMcpToolCatalogRequest,
    ) -> Result<Vec<Arc<dyn RuntimeToolExecutor>>, RuntimeMcpToolCatalogError> {
        let mut executors = Vec::<Arc<dyn RuntimeToolExecutor>>::new();
        let mut relay_servers = Vec::new();
        for server in request.servers {
            if server.uses_relay {
                relay_servers.push(server);
                continue;
            }
            executors.extend(
                discover_direct_tools(server, &request.capability_state)
                    .await?
                    .into_iter()
                    .map(|executor| Arc::new(executor) as Arc<dyn RuntimeToolExecutor>),
            );
        }
        if relay_servers.is_empty() {
            return Ok(executors);
        }
        let relay = self.relay.as_ref().ok_or_else(|| {
            RuntimeMcpToolCatalogError::UnsupportedPlacement {
                server: relay_servers
                    .iter()
                    .map(|server| server.name.as_str())
                    .collect::<Vec<_>>()
                    .join(","),
            }
        })?;
        let outcome = relay
            .list_relay_tools(&relay_servers, request.relay_context.clone())
            .await;
        let requested = relay_servers
            .into_iter()
            .map(|server| (server.name.clone(), server))
            .collect::<HashMap<_, _>>();
        for tool in outcome.tools {
            let Some(server) = requested.get(&tool.server_name) else {
                continue;
            };
            let capability_key = capability_key_for_mcp_server_name(&tool.server_name);
            if !request.capability_state.is_capability_tool_enabled(
                &capability_key,
                &tool.tool_name,
                None,
            ) {
                continue;
            }
            executors.push(Arc::new(RelayRuntimeMcpTool {
                definition: runtime_definition(
                    &tool.server_name,
                    &tool.tool_name,
                    &tool.description,
                    tool.parameters_schema,
                )?,
                server: server.clone(),
                source_tool_name: tool.tool_name,
                relay: relay.clone(),
                context: request.relay_context.clone(),
            }));
        }
        Ok(executors)
    }
}

struct DirectRuntimeMcpTool {
    definition: RuntimeToolDefinition,
    source_tool_name: String,
    client: Arc<Mutex<DirectMcpClient>>,
}

#[async_trait]
impl RuntimeToolExecutor for DirectRuntimeMcpTool {
    fn definition(&self) -> RuntimeToolDefinition {
        self.definition.clone()
    }

    async fn execute(&self, invocation: RuntimeToolInvocation) -> AgentToolResult {
        let arguments = match arguments_object(invocation.arguments) {
            Ok(arguments) => arguments,
            Err(result) => return result,
        };
        let request = match arguments {
            Some(arguments) => {
                CallToolRequestParams::new(self.source_tool_name.clone()).with_arguments(arguments)
            }
            None => CallToolRequestParams::new(self.source_tool_name.clone()),
        };
        match self.client.lock().await.call_tool(request).await {
            Ok(result) => match serde_json::to_value(result) {
                Ok(output) => AgentToolResult::Completed { output },
                Err(error) => AgentToolResult::Failed {
                    code: "mcp_result_encoding_failed".to_owned(),
                    message: error.to_string(),
                },
            },
            Err(error) => AgentToolResult::Failed {
                code: "mcp_call_failed".to_owned(),
                message: error.to_string(),
            },
        }
    }
}

struct RelayRuntimeMcpTool {
    definition: RuntimeToolDefinition,
    server: RuntimeMcpServer,
    source_tool_name: String,
    relay: Arc<dyn McpRelayProvider>,
    context: Option<RelayMcpCallContext>,
}

#[async_trait]
impl RuntimeToolExecutor for RelayRuntimeMcpTool {
    fn definition(&self) -> RuntimeToolDefinition {
        self.definition.clone()
    }

    async fn execute(&self, invocation: RuntimeToolInvocation) -> AgentToolResult {
        let arguments = match arguments_object(invocation.arguments) {
            Ok(arguments) => arguments,
            Err(result) => return result,
        };
        let mut context = self.context.clone();
        if let Some(context) = context.as_mut() {
            context.turn_id = Some(invocation.context.turn_id.to_string());
            context.tool_call_id = Some(invocation.context.effect_id.to_string());
        }
        match self
            .relay
            .call_relay_tool(&self.server, &self.source_tool_name, arguments, context)
            .await
        {
            Ok(result) if !result.is_error => AgentToolResult::Completed {
                output: serde_json::json!({ "content": result.content }),
            },
            Ok(result) => AgentToolResult::Failed {
                code: "mcp_tool_error".to_owned(),
                message: result.content,
            },
            Err(error) => AgentToolResult::Failed {
                code: "mcp_call_failed".to_owned(),
                message: error.to_string(),
            },
        }
    }
}

async fn discover_direct_tools(
    server: RuntimeMcpServer,
    capability_state: &CapabilityState,
) -> Result<Vec<DirectRuntimeMcpTool>, RuntimeMcpToolCatalogError> {
    let McpTransportConfig::Http { url, headers } = &server.transport else {
        return Err(RuntimeMcpToolCatalogError::UnsupportedPlacement {
            server: server.name,
        });
    };
    let config = StreamableHttpClientTransportConfig::with_uri(url.clone()).custom_headers(
        build_header_map(headers).map_err(|reason| RuntimeMcpToolCatalogError::Discovery {
            server: server.name.clone(),
            reason,
        })?,
    );
    let worker = StreamableHttpClientWorker::new(reqwest::Client::new(), config);
    let client =
        ().serve(worker)
            .await
            .map_err(|error| RuntimeMcpToolCatalogError::Discovery {
                server: server.name.clone(),
                reason: error.to_string(),
            })?;
    let listed =
        client
            .list_all_tools()
            .await
            .map_err(|error| RuntimeMcpToolCatalogError::Discovery {
                server: server.name.clone(),
                reason: error.to_string(),
            })?;
    let client = Arc::new(Mutex::new(client));
    let capability_key = capability_key_for_mcp_server_name(&server.name);
    listed
        .into_iter()
        .filter(|tool| {
            capability_state.is_capability_tool_enabled(&capability_key, tool.name.as_ref(), None)
        })
        .map(|tool| direct_executor(&server.name, tool, client.clone()))
        .collect()
}

fn direct_executor(
    server_name: &str,
    tool: Tool,
    client: Arc<Mutex<DirectMcpClient>>,
) -> Result<DirectRuntimeMcpTool, RuntimeMcpToolCatalogError> {
    let source_tool_name = tool.name.to_string();
    Ok(DirectRuntimeMcpTool {
        definition: runtime_definition(
            server_name,
            &source_tool_name,
            tool.description.as_deref().unwrap_or("MCP tool"),
            serde_json::Value::Object((*tool.input_schema).clone()),
        )?,
        source_tool_name,
        client,
    })
}

fn runtime_definition(
    server_name: &str,
    tool_name: &str,
    description: &str,
    parameters_schema: serde_json::Value,
) -> Result<RuntimeToolDefinition, RuntimeMcpToolCatalogError> {
    Ok(RuntimeToolDefinition {
        name: AgentToolName::new(namespaced_tool_name(server_name, tool_name)).map_err(
            |error| RuntimeMcpToolCatalogError::InvalidTool {
                reason: error.to_string(),
            },
        )?,
        description: description.trim().to_owned(),
        parameters_schema: sanitize_tool_schema(parameters_schema),
        protocol_projector: ToolProtocolProjector::Mcp {
            server_key: server_name.to_owned(),
        },
        permission: RuntimeToolPermission::ProductWrite,
        effect: RuntimeToolEffect::ProductMutation,
    })
}

fn arguments_object(
    arguments: serde_json::Value,
) -> Result<Option<serde_json::Map<String, serde_json::Value>>, AgentToolResult> {
    match arguments {
        serde_json::Value::Null => Ok(None),
        serde_json::Value::Object(arguments) => Ok(Some(arguments)),
        _ => Err(AgentToolResult::Rejected {
            code: "invalid_mcp_arguments".to_owned(),
            message: "MCP tool arguments must be a JSON object or null".to_owned(),
        }),
    }
}

fn build_header_map(headers: &[McpHttpHeader]) -> Result<HashMap<HeaderName, HeaderValue>, String> {
    let mut map = HashMap::new();
    for header in headers {
        let name = HeaderName::from_bytes(header.name.as_bytes())
            .map_err(|error| format!("invalid MCP HTTP header name: {error}"))?;
        let value = HeaderValue::from_str(&header.value)
            .map_err(|error| format!("invalid MCP HTTP header value: {error}"))?;
        map.insert(name, value);
    }
    Ok(map)
}

fn capability_key_for_mcp_server_name(server_name: &str) -> String {
    let stable_name = stable_server_name(server_name);
    match stable_name.as_str() {
        "agentdash-relay-tools" => "relay_management".to_owned(),
        "agentdash-story-tools" => "story_management".to_owned(),
        "agentdash-workflow-tools" => "workflow_management".to_owned(),
        other => format!("mcp:{other}"),
    }
}

fn namespaced_tool_name(server_name: &str, tool_name: &str) -> String {
    format!(
        "mcp_{}_{}",
        sanitize_identifier(&stable_server_name(server_name)),
        sanitize_identifier(tool_name)
    )
}

fn stable_server_name(server_name: &str) -> String {
    for (prefix, stable) in [
        ("agentdash-story-tools-", "agentdash-story-tools"),
        ("agentdash-workflow-tools-", "agentdash-workflow-tools"),
    ] {
        if server_name.starts_with(prefix) {
            return stable.to_owned();
        }
    }
    server_name.to_owned()
}

fn sanitize_identifier(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn namespaced_identity_is_stable_across_platform_scope_ids() {
        assert_eq!(
            namespaced_tool_name("agentdash-workflow-tools-8de613e7", "get_lifecycle"),
            "mcp_agentdash_workflow_tools_get_lifecycle"
        );
        assert_eq!(
            capability_key_for_mcp_server_name("agentdash-workflow-tools-8de613e7"),
            "workflow_management"
        );
    }
}
