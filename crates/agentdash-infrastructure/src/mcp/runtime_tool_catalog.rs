use std::{collections::HashMap, sync::Arc};

use agentdash_agent_runtime::{
    RuntimeToolAuthorizationPolicy, RuntimeToolDefinition, RuntimeToolEffect, RuntimeToolExecutor,
    RuntimeToolInvocation, RuntimeToolPermission, RuntimeToolProvenance, ToolProtocolProjector,
};
use agentdash_agent_runtime_contract::{AgentToolName, AgentToolResult};
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
    #[error("MCP tool `{server}.{tool}` is not present on the resolved surface")]
    ToolMissing { server: String, tool: String },
    #[error("MCP tool `{server}.{tool}` invocation failed: {reason}")]
    Invocation {
        server: String,
        tool: String,
        reason: String,
    },
}

#[derive(Clone)]
pub struct RuntimeMcpToolCatalogRequest {
    pub servers: Vec<RuntimeMcpServer>,
    pub capability_state: CapabilityState,
    pub relay_context: Option<RelayMcpCallContext>,
}

pub struct RuntimeMcpOperationInvocation {
    pub server_name: String,
    pub tool_name: String,
    pub arguments: serde_json::Value,
}

#[async_trait]
pub trait RuntimeDynamicToolCatalog: Send + Sync {
    async fn resolve(
        &self,
        request: RuntimeMcpToolCatalogRequest,
    ) -> Result<Vec<Arc<dyn RuntimeToolExecutor>>, RuntimeMcpToolCatalogError>;

    /// Invoke MCP as a neutral Operation caller.
    ///
    /// Operation Gateway owns principal admission and audit. This path intentionally does not
    /// fabricate an Agent Runtime authorization grant for user/workshop callers.
    async fn invoke_operation(
        &self,
        request: RuntimeMcpToolCatalogRequest,
        invocation: RuntimeMcpOperationInvocation,
    ) -> Result<serde_json::Value, RuntimeMcpToolCatalogError>;
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
            let capability_key = runtime_mcp_capability_key(&tool.server_name);
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

    async fn invoke_operation(
        &self,
        request: RuntimeMcpToolCatalogRequest,
        invocation: RuntimeMcpOperationInvocation,
    ) -> Result<serde_json::Value, RuntimeMcpToolCatalogError> {
        let server = request
            .servers
            .into_iter()
            .find(|server| server.name == invocation.server_name)
            .ok_or_else(|| RuntimeMcpToolCatalogError::ToolMissing {
                server: invocation.server_name.clone(),
                tool: invocation.tool_name.clone(),
            })?;
        if server.uses_relay {
            let relay = self.relay.as_ref().ok_or_else(|| {
                RuntimeMcpToolCatalogError::UnsupportedPlacement {
                    server: server.name.clone(),
                }
            })?;
            let arguments = arguments_object(invocation.arguments).map_err(|result| {
                RuntimeMcpToolCatalogError::Invocation {
                    server: server.name.clone(),
                    tool: invocation.tool_name.clone(),
                    reason: agent_tool_result_message(result),
                }
            })?;
            let result = relay
                .call_relay_tool(
                    &server,
                    &invocation.tool_name,
                    arguments,
                    request.relay_context,
                )
                .await
                .map_err(|error| RuntimeMcpToolCatalogError::Invocation {
                    server: server.name.clone(),
                    tool: invocation.tool_name.clone(),
                    reason: error.to_string(),
                })?;
            if result.is_error {
                return Err(RuntimeMcpToolCatalogError::Invocation {
                    server: server.name,
                    tool: invocation.tool_name,
                    reason: result.content,
                });
            }
            return Ok(serde_json::json!({ "content": result.content }));
        }

        let tools = discover_direct_tools(server, &request.capability_state).await?;
        let tool = tools
            .into_iter()
            .find(|tool| tool.source_tool_name == invocation.tool_name)
            .ok_or_else(|| RuntimeMcpToolCatalogError::ToolMissing {
                server: invocation.server_name.clone(),
                tool: invocation.tool_name.clone(),
            })?;
        tool.invoke_arguments(invocation.arguments).await
    }
}

struct DirectRuntimeMcpTool {
    definition: RuntimeToolDefinition,
    source_tool_name: String,
    client: Arc<Mutex<DirectMcpClient>>,
}

impl DirectRuntimeMcpTool {
    async fn invoke_arguments(
        &self,
        arguments: serde_json::Value,
    ) -> Result<serde_json::Value, RuntimeMcpToolCatalogError> {
        let arguments = arguments_object(arguments).map_err(|result| {
            RuntimeMcpToolCatalogError::Invocation {
                server: runtime_tool_server_name(&self.definition),
                tool: self.source_tool_name.clone(),
                reason: agent_tool_result_message(result),
            }
        })?;
        let request = match arguments {
            Some(arguments) => {
                CallToolRequestParams::new(self.source_tool_name.clone()).with_arguments(arguments)
            }
            None => CallToolRequestParams::new(self.source_tool_name.clone()),
        };
        let result = self
            .client
            .lock()
            .await
            .call_tool(request)
            .await
            .map_err(|error| RuntimeMcpToolCatalogError::Invocation {
                server: runtime_tool_server_name(&self.definition),
                tool: self.source_tool_name.clone(),
                reason: error.to_string(),
            })?;
        serde_json::to_value(result).map_err(|error| RuntimeMcpToolCatalogError::Invocation {
            server: runtime_tool_server_name(&self.definition),
            tool: self.source_tool_name.clone(),
            reason: error.to_string(),
        })
    }
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
    let capability_key = runtime_mcp_capability_key(&server.name);
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
    let stable_server_name = stable_server_name(server_name);
    let capability_key = runtime_mcp_capability_key(&stable_server_name);
    Ok(RuntimeToolDefinition {
        name: AgentToolName::new(namespaced_tool_name(server_name, tool_name)).map_err(
            |error| RuntimeMcpToolCatalogError::InvalidTool {
                reason: error.to_string(),
            },
        )?,
        description: description.trim().to_owned(),
        parameters_schema: sanitize_tool_schema(parameters_schema),
        provenance: RuntimeToolProvenance {
            source: format!("mcp:{stable_server_name}"),
            tool_path: format!("{capability_key}::{tool_name}"),
            capability_key,
            context_usage_kind: agentdash_platform_spi::context_usage_kind::MCP_TOOLS.to_owned(),
        },
        protocol_projector: ToolProtocolProjector::Mcp {
            server_key: server_name.to_owned(),
        },
        permission: RuntimeToolPermission::ProductWrite,
        effect: RuntimeToolEffect::ProductMutation,
        authorization_policy: RuntimeToolAuthorizationPolicy::Product,
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

fn runtime_tool_server_name(definition: &RuntimeToolDefinition) -> String {
    match &definition.protocol_projector {
        ToolProtocolProjector::Mcp { server_key } => server_key.clone(),
        _ => definition.provenance.source.clone(),
    }
}

fn agent_tool_result_message(result: AgentToolResult) -> String {
    match result {
        AgentToolResult::Rejected { code, message } | AgentToolResult::Failed { code, message } => {
            format!("{code}: {message}")
        }
        AgentToolResult::Completed { .. } => "unexpected completed result".to_owned(),
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

pub fn runtime_mcp_capability_key(server_name: &str) -> String {
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
            runtime_mcp_capability_key("agentdash-workflow-tools-8de613e7"),
            "workflow_management"
        );
        let definition = runtime_definition(
            "agentdash-workflow-tools-8de613e7",
            "get_lifecycle",
            "Get lifecycle",
            serde_json::json!({"type": "object"}),
        )
        .expect("runtime definition");
        assert_eq!(definition.provenance.capability_key, "workflow_management");
        assert_eq!(definition.provenance.source, "mcp:agentdash-workflow-tools");
        assert_eq!(
            definition.provenance.tool_path,
            "workflow_management::get_lifecycle"
        );
        assert_eq!(
            definition.provenance.context_usage_kind,
            agentdash_platform_spi::context_usage_kind::MCP_TOOLS
        );
    }
}
