use std::sync::Arc;

use agentdash_agent_types::AgentToolResult;
use agentdash_application_ports::mcp_discovery::{
    DiscoveredMcpTool, McpToolDiscovery, McpToolDiscoveryRequest,
};
use agentdash_application_ports::runtime_gateway_mcp_surface::{
    RuntimeGatewayMcpSurfaceQueryError, RuntimeGatewayMcpSurfaceQueryPort,
    RuntimeGatewayMcpSurfaceQueryPurpose, RuntimeGatewayMcpSurfaceWithBackend,
};
use agentdash_spi::{ConnectorError, RelayMcpCallContext};
use async_trait::async_trait;
use serde_json::Value;

use super::{
    McpCallToolInput, RuntimeMcpToolDescriptor, RuntimeSessionMcpAccess, RuntimeSessionMcpError,
    execute_runtime_mcp_tool,
};

const RUNTIME_MCP_TOOL_DISCOVERY_COMPONENT: &str = "runtime_mcp_tool_discovery";

pub struct CurrentSurfaceRuntimeMcpAccess {
    surface_query: Arc<dyn RuntimeGatewayMcpSurfaceQueryPort>,
    mcp_tool_discovery: Arc<dyn McpToolDiscovery>,
}

impl CurrentSurfaceRuntimeMcpAccess {
    pub fn new(
        surface_query: Arc<dyn RuntimeGatewayMcpSurfaceQueryPort>,
        mcp_tool_discovery: Arc<dyn McpToolDiscovery>,
    ) -> Self {
        Self {
            surface_query,
            mcp_tool_discovery,
        }
    }

    async fn discover_entries(
        &self,
        session_id: &str,
    ) -> Result<Vec<DiscoveredMcpTool>, RuntimeSessionMcpError> {
        let surface = self
            .surface_query
            .current_runtime_mcp_surface_with_backend(
                session_id,
                RuntimeGatewayMcpSurfaceQueryPurpose::new(RUNTIME_MCP_TOOL_DISCOVERY_COMPONENT),
            )
            .await
            .map_err(runtime_surface_query_error_to_mcp)?;

        self.mcp_tool_discovery
            .discover_tool_entries(discovery_request(surface))
            .await
            .map_err(runtime_mcp_error_from_connector)
    }
}

#[async_trait]
impl RuntimeSessionMcpAccess for CurrentSurfaceRuntimeMcpAccess {
    async fn list_mcp_tools(
        &self,
        session_id: &str,
    ) -> Result<Vec<RuntimeMcpToolDescriptor>, RuntimeSessionMcpError> {
        let entries = self.discover_entries(session_id).await?;
        Ok(entries
            .into_iter()
            .map(runtime_mcp_tool_descriptor_from_entry)
            .collect())
    }

    async fn call_mcp_tool(
        &self,
        session_id: &str,
        input: McpCallToolInput,
    ) -> Result<AgentToolResult, RuntimeSessionMcpError> {
        let entry = self
            .discover_entries(session_id)
            .await?
            .into_iter()
            .find(|entry| runtime_mcp_entry_matches(entry, &input))
            .ok_or_else(|| {
                RuntimeSessionMcpError::ToolUnavailable(
                    "目标 MCP 工具不在当前 Session Runtime Surface 中".to_string(),
                )
            })?;
        let arguments = input.arguments.unwrap_or(Value::Null);
        execute_runtime_mcp_tool(entry.tool, &entry.runtime_name, arguments).await
    }
}

fn discovery_request(surface: RuntimeGatewayMcpSurfaceWithBackend) -> McpToolDiscoveryRequest {
    let RuntimeGatewayMcpSurfaceWithBackend {
        surface,
        runtime_backend_anchor,
    } = surface;
    McpToolDiscoveryRequest {
        servers: surface.mcp_servers,
        capability_state: surface.capability_state,
        call_context: Some(RelayMcpCallContext {
            session_id: surface.runtime_session_id,
            turn_id: surface.active_turn_id,
            tool_call_id: None,
            backend_anchor: Some(runtime_backend_anchor),
            vfs: Some(surface.vfs),
            identity: surface.identity,
        }),
    }
}

fn runtime_mcp_tool_descriptor_from_entry(entry: DiscoveredMcpTool) -> RuntimeMcpToolDescriptor {
    RuntimeMcpToolDescriptor {
        runtime_name: entry.runtime_name,
        server_name: entry.server_name,
        tool_name: entry.tool_name,
        uses_relay: entry.uses_relay,
        description: entry.description,
        parameters_schema: entry.parameters_schema,
    }
}

fn runtime_mcp_entry_matches(entry: &DiscoveredMcpTool, input: &McpCallToolInput) -> bool {
    if let Some(runtime_name) = input.runtime_name.as_deref()
        && runtime_name == entry.runtime_name
    {
        return true;
    }
    matches!(
        (input.server_name.as_deref(), input.tool_name.as_deref()),
        (Some(server_name), Some(tool_name))
            if server_name == entry.server_name && tool_name == entry.tool_name
    )
}

fn runtime_surface_query_error_to_mcp(
    error: RuntimeGatewayMcpSurfaceQueryError,
) -> RuntimeSessionMcpError {
    if let Some(anchor_error) = error.runtime_backend_anchor_error {
        return RuntimeSessionMcpError::SessionUnavailable(anchor_error.to_string());
    }
    RuntimeSessionMcpError::SessionUnavailable(error.to_string())
}

fn runtime_mcp_error_from_connector(error: ConnectorError) -> RuntimeSessionMcpError {
    match error {
        ConnectorError::Runtime(message) | ConnectorError::InvalidConfig(message) => {
            RuntimeSessionMcpError::SessionUnavailable(message)
        }
        ConnectorError::ConnectionFailed(message) => {
            RuntimeSessionMcpError::DiscoveryFailed(message)
        }
        ConnectorError::SpawnFailed(message) => RuntimeSessionMcpError::DiscoveryFailed(message),
        ConnectorError::Io(error) => RuntimeSessionMcpError::DiscoveryFailed(error.to_string()),
        ConnectorError::Json(error) => RuntimeSessionMcpError::DiscoveryFailed(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use agentdash_agent_types::{AgentTool, AgentToolError, ContentPart, ToolUpdateCallback};
    use agentdash_application_ports::mcp_discovery::McpToolDiscoveryRequest;
    use agentdash_application_ports::runtime_gateway_mcp_surface::RuntimeGatewayMcpSurface;
    use agentdash_domain::backend::{RuntimeBackendAnchor, RuntimeBackendAnchorSource};
    use agentdash_domain::common::{Mount, MountCapability};
    use agentdash_spi::{
        CapabilityState, McpTransportConfig, RuntimeMcpServer, ToolCapability,
        ToolCapabilityFilter, ToolCluster, Vfs,
    };
    use serde_json::json;
    use tokio_util::sync::CancellationToken;
    use uuid::Uuid;

    use crate::runtime_gateway::{
        MCP_CALL_TOOL_ACTION, MCP_LIST_TOOLS_ACTION, RuntimeActionKey, RuntimeActor,
        RuntimeContext, RuntimeGateway, RuntimeInvocationRequest,
    };

    use super::*;

    #[derive(Default)]
    struct FakeSurfaceQuery {
        surface: Mutex<Option<RuntimeGatewayMcpSurfaceWithBackend>>,
    }

    impl FakeSurfaceQuery {
        fn new(surface: RuntimeGatewayMcpSurfaceWithBackend) -> Self {
            Self {
                surface: Mutex::new(Some(surface)),
            }
        }
    }

    #[async_trait]
    impl RuntimeGatewayMcpSurfaceQueryPort for FakeSurfaceQuery {
        async fn current_runtime_mcp_surface_with_backend(
            &self,
            _runtime_session_id: &str,
            _purpose: RuntimeGatewayMcpSurfaceQueryPurpose,
        ) -> Result<RuntimeGatewayMcpSurfaceWithBackend, RuntimeGatewayMcpSurfaceQueryError>
        {
            Ok(self
                .surface
                .lock()
                .expect("surface mutex poisoned")
                .as_ref()
                .expect("surface")
                .clone())
        }
    }

    struct CapturingMcpDiscovery {
        captured_backend: Mutex<Option<RuntimeBackendAnchor>>,
    }

    impl CapturingMcpDiscovery {
        fn new() -> Self {
            Self {
                captured_backend: Mutex::new(None),
            }
        }

        fn captured_backend_id(&self) -> Option<String> {
            self.captured_backend
                .lock()
                .expect("captured backend mutex poisoned")
                .as_ref()
                .map(|anchor| anchor.backend_id().to_string())
        }
    }

    #[async_trait]
    impl McpToolDiscovery for CapturingMcpDiscovery {
        async fn discover_tool_entries(
            &self,
            request: McpToolDiscoveryRequest,
        ) -> Result<Vec<DiscoveredMcpTool>, ConnectorError> {
            *self
                .captured_backend
                .lock()
                .expect("captured backend mutex poisoned") = request
                .call_context
                .as_ref()
                .and_then(|context| context.backend_anchor.clone());
            Ok(entries_for_request(&request, false))
        }
    }

    struct FilteringMcpDiscovery;

    #[async_trait]
    impl McpToolDiscovery for FilteringMcpDiscovery {
        async fn discover_tool_entries(
            &self,
            request: McpToolDiscoveryRequest,
        ) -> Result<Vec<DiscoveredMcpTool>, ConnectorError> {
            Ok(entries_for_request(&request, true))
        }
    }

    struct TestTool {
        name: String,
    }

    #[async_trait]
    impl AgentTool for TestTool {
        fn name(&self) -> &str {
            &self.name
        }

        fn description(&self) -> &str {
            "test MCP tool"
        }

        fn parameters_schema(&self) -> Value {
            json!({ "type": "object" })
        }

        async fn execute(
            &self,
            _tool_call_id: &str,
            _args: Value,
            _cancel: CancellationToken,
            _on_update: Option<ToolUpdateCallback>,
        ) -> Result<AgentToolResult, AgentToolError> {
            Ok(AgentToolResult {
                content: vec![ContentPart::text(self.name.clone())],
                is_error: false,
                details: None,
            })
        }
    }

    fn entries_for_request(
        request: &McpToolDiscoveryRequest,
        apply_capability_filter: bool,
    ) -> Vec<DiscoveredMcpTool> {
        ["allowed_tool", "blocked_tool"]
            .into_iter()
            .filter(|tool_name| {
                !apply_capability_filter
                    || request.capability_state.is_capability_tool_enabled(
                        "mcp:code-analyzer",
                        tool_name,
                        None,
                    )
            })
            .map(|tool_name| {
                let runtime_name = format!("mcp_code_analyzer_{tool_name}");
                DiscoveredMcpTool {
                    runtime_name: runtime_name.clone(),
                    server_name: "code-analyzer".to_string(),
                    tool_name: tool_name.to_string(),
                    uses_relay: true,
                    description: format!("{tool_name} description"),
                    parameters_schema: json!({ "type": "object" }),
                    tool: Arc::new(TestTool { name: runtime_name }),
                }
            })
            .collect()
    }

    fn gateway_request(action_key: &str, input: Value) -> RuntimeInvocationRequest {
        RuntimeInvocationRequest::new(
            RuntimeActionKey::parse(action_key).expect("valid action key"),
            RuntimeActor::UserCanvas {
                session_id: "session-1".to_string(),
                canvas_id: Some(Uuid::new_v4()),
            },
            RuntimeContext::Session {
                session_id: "session-1".to_string(),
                project_id: Some(Uuid::new_v4()),
                workspace_id: None,
            },
            input,
        )
    }

    fn access(discovery: Arc<dyn McpToolDiscovery>) -> Arc<CurrentSurfaceRuntimeMcpAccess> {
        Arc::new(CurrentSurfaceRuntimeMcpAccess::new(
            Arc::new(FakeSurfaceQuery::new(surface_with_backend())),
            discovery,
        ))
    }

    fn surface_with_backend() -> RuntimeGatewayMcpSurfaceWithBackend {
        let backend_anchor =
            RuntimeBackendAnchor::new("backend-1", RuntimeBackendAnchorSource::System)
                .expect("backend anchor");
        RuntimeGatewayMcpSurfaceWithBackend {
            surface: RuntimeGatewayMcpSurface {
                runtime_session_id: "session-1".to_string(),
                capability_state: capability_state(),
                vfs: vfs(),
                mcp_servers: vec![RuntimeMcpServer {
                    name: "code-analyzer".to_string(),
                    transport: McpTransportConfig::Http {
                        url: "http://localhost/mcp".to_string(),
                        headers: Vec::new(),
                    },
                    uses_relay: true,
                }],
                active_turn_id: None,
                identity: None,
            },
            runtime_backend_anchor: backend_anchor,
        }
    }

    fn capability_state() -> CapabilityState {
        let mut capability_state = CapabilityState::from_clusters([ToolCluster::Read]);
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
        capability_state
    }

    fn vfs() -> Vfs {
        Vfs {
            mounts: vec![Mount {
                id: "workspace".to_string(),
                provider: "relay_fs".to_string(),
                backend_id: "backend-1".to_string(),
                root_ref: "F:/Projects/AgentDash".to_string(),
                capabilities: vec![MountCapability::Read, MountCapability::Write],
                default_write: true,
                display_name: "Workspace".to_string(),
                metadata: Value::Null,
            }],
            default_mount_id: Some("workspace".to_string()),
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        }
    }

    #[tokio::test]
    async fn idle_mcp_list_tools_uses_runtime_surface_backend_anchor() {
        let discovery = Arc::new(CapturingMcpDiscovery::new());
        let gateway = RuntimeGateway::new().with_provider(Arc::new(
            super::super::McpListToolsProvider::new(access(discovery.clone())),
        ));

        let result = gateway
            .invoke(gateway_request(MCP_LIST_TOOLS_ACTION, json!({})))
            .await
            .expect("list tools");

        assert_eq!(result.output.output["tools"].as_array().unwrap().len(), 2);
        assert_eq!(
            discovery.captured_backend_id().as_deref(),
            Some("backend-1")
        );
    }

    #[tokio::test]
    async fn mcp_call_tool_matches_by_server_and_tool_name() {
        let gateway = RuntimeGateway::new().with_provider(Arc::new(
            super::super::McpCallToolProvider::new(access(Arc::new(FilteringMcpDiscovery))),
        ));

        let result = gateway
            .invoke(gateway_request(
                MCP_CALL_TOOL_ACTION,
                json!({
                    "server_name": "code-analyzer",
                    "tool_name": "allowed_tool",
                    "arguments": {}
                }),
            ))
            .await
            .expect("call tool");

        assert_eq!(result.output.output["is_error"], false);
        assert_eq!(
            result.output.output["content"][0]["text"],
            "mcp_code_analyzer_allowed_tool"
        );
    }

    #[tokio::test]
    async fn capability_disabled_mcp_tool_is_not_exposed() {
        let tools = access(Arc::new(FilteringMcpDiscovery))
            .list_mcp_tools("session-1")
            .await
            .expect("tools");

        let tool_names = tools
            .iter()
            .map(|tool| tool.tool_name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(tool_names, vec!["allowed_tool"]);
    }

    #[test]
    fn production_mcp_access_does_not_import_session_or_frame_boundaries() {
        let production_code = include_str!("mcp_access.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("production segment");
        let forbidden = [
            concat!("Session", "Hub"),
            concat!("Agent", "Frame"),
            concat!("Agent", "Frame", "Surface", "Ext"),
            concat!(
                "resolve",
                "_current",
                "_frame",
                "_from",
                "_delivery",
                "_trace",
                "_ref"
            ),
        ];
        for token in forbidden {
            assert!(
                !production_code.contains(token),
                "runtime_gateway::mcp_access production code must not import or reference {token}"
            );
        }
    }
}
