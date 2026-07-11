use std::sync::Arc;

use agentdash_agent_types::{AgentToolError, AgentToolResult};
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

use agentdash_application_operation_gateway::{
    OperationAuthorizationScope, OperationExecutionError, OperationMcpAccess, OperationMcpTool,
    OperationPrincipal, OperationPrincipalRef,
};

const RUNTIME_MCP_TOOL_DISCOVERY_COMPONENT: &str = "runtime_mcp_tool_discovery";

pub struct CurrentSurfaceRuntimeMcpAccess {
    surface_query: Arc<dyn RuntimeGatewayMcpSurfaceQueryPort>,
    mcp_tool_discovery: Arc<dyn McpToolDiscovery>,
    binding_repo:
        Arc<dyn agentdash_application_ports::agent_run_runtime::AgentRunRuntimeBindingRepository>,
}

impl CurrentSurfaceRuntimeMcpAccess {
    pub fn new(
        surface_query: Arc<dyn RuntimeGatewayMcpSurfaceQueryPort>,
        mcp_tool_discovery: Arc<dyn McpToolDiscovery>,
        binding_repo: Arc<
            dyn agentdash_application_ports::agent_run_runtime::AgentRunRuntimeBindingRepository,
        >,
    ) -> Self {
        Self {
            surface_query,
            mcp_tool_discovery,
            binding_repo,
        }
    }

    async fn discover_entries_for_agent_run(
        &self,
        run_id: uuid::Uuid,
        agent_id: uuid::Uuid,
    ) -> Result<(Vec<DiscoveredMcpTool>, String), McpAccessError> {
        let binding = self
            .binding_repo
            .load(
                &agentdash_application_ports::agent_run_runtime::AgentRunRuntimeTarget {
                    run_id,
                    agent_id,
                },
            )
            .await
            .map_err(|error| McpAccessError::SurfaceUnavailable(error.to_string()))?
            .ok_or_else(|| {
                McpAccessError::SurfaceUnavailable("AgentRun runtime binding 不存在".to_string())
            })?;
        let surface = self
            .surface_query
            .current_runtime_mcp_surface_with_backend(
                &binding.thread_id.to_string(),
                RuntimeGatewayMcpSurfaceQueryPurpose::new(RUNTIME_MCP_TOOL_DISCOVERY_COMPONENT),
            )
            .await
            .map_err(runtime_surface_query_error_to_mcp)?;
        let backend_id = surface.runtime_backend_anchor.backend_id().to_string();
        let tools = self
            .mcp_tool_discovery
            .discover_tool_entries(discovery_request(surface))
            .await
            .map(|outcome| outcome.tools)
            .map_err(runtime_mcp_error_from_connector)?;
        Ok((tools, backend_id))
    }
}

#[async_trait]
impl OperationMcpAccess for CurrentSurfaceRuntimeMcpAccess {
    async fn discover_tools(
        &self,
        principal: &OperationPrincipal,
        _: &OperationAuthorizationScope,
        cancel: tokio_util::sync::CancellationToken,
    ) -> Result<Vec<OperationMcpTool>, OperationExecutionError> {
        if cancel.is_cancelled() {
            return Err(OperationExecutionError::Cancelled);
        }
        let OperationPrincipalRef::AgentRunAgent { run_id, agent_id } = principal.principal_ref()
        else {
            return Ok(Vec::new());
        };
        let (entries, backend_id) = self
            .discover_entries_for_agent_run(*run_id, *agent_id)
            .await
            .map_err(operation_error_from_mcp)?;
        Ok(entries
            .into_iter()
            .map(|entry| OperationMcpTool {
                server_name: entry.server_name,
                tool_name: entry.tool_name,
                description: entry.description,
                input_schema: entry.parameters_schema,
                backend_id: backend_id.clone(),
            })
            .collect())
    }

    async fn invoke_tool(
        &self,
        principal: &OperationPrincipal,
        _: &OperationAuthorizationScope,
        server_name: &str,
        tool_name: &str,
        arguments: Value,
        cancel: tokio_util::sync::CancellationToken,
    ) -> Result<Value, OperationExecutionError> {
        let OperationPrincipalRef::AgentRunAgent { run_id, agent_id } = principal.principal_ref()
        else {
            return Err(OperationExecutionError::CapabilitiesDenied {
                missing: vec!["agent_run.mcp".to_string()],
            });
        };
        let (entries, _) = self
            .discover_entries_for_agent_run(*run_id, *agent_id)
            .await
            .map_err(operation_error_from_mcp)?;
        let operation_ref = agentdash_domain::operation::OperationRef::new(
            agentdash_application_operation_gateway::MCP_OPERATION_NAMESPACE,
            server_name,
            tool_name,
            1,
        )
        .map_err(|error| OperationExecutionError::invalid_request(error.to_string()))?;
        let entry = entries
            .into_iter()
            .find(|entry| entry.server_name == server_name && entry.tool_name == tool_name)
            .ok_or(OperationExecutionError::OperationUnavailable { operation_ref })?;
        let result = execute_runtime_mcp_tool(entry.tool, &entry.runtime_name, arguments, cancel)
            .await
            .map_err(operation_error_from_mcp)?;
        serde_json::to_value(result)
            .map_err(|error| OperationExecutionError::provider_failed(error.to_string()))
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
            vfs_access_policy: Some(surface.vfs_access_policy),
            identity: surface.identity,
        }),
    }
}

fn runtime_surface_query_error_to_mcp(error: RuntimeGatewayMcpSurfaceQueryError) -> McpAccessError {
    if let Some(anchor_error) = error.runtime_backend_anchor_error {
        return McpAccessError::SurfaceUnavailable(anchor_error.to_string());
    }
    McpAccessError::SurfaceUnavailable(error.to_string())
}

fn runtime_mcp_error_from_connector(error: ConnectorError) -> McpAccessError {
    match error {
        ConnectorError::Runtime(message) | ConnectorError::InvalidConfig(message) => {
            McpAccessError::SurfaceUnavailable(message)
        }
        ConnectorError::ConnectionFailed(message) => McpAccessError::DiscoveryFailed(message),
        ConnectorError::SpawnFailed(message) => McpAccessError::DiscoveryFailed(message),
        ConnectorError::Io(error) => McpAccessError::DiscoveryFailed(error.to_string()),
        ConnectorError::Json(error) => McpAccessError::DiscoveryFailed(error.to_string()),
    }
}

fn operation_error_from_mcp(error: McpAccessError) -> OperationExecutionError {
    match error {
        McpAccessError::InvalidArguments(message) => {
            OperationExecutionError::invalid_request(message)
        }
        McpAccessError::SurfaceUnavailable(message) => OperationExecutionError::NotReady {
            code: "mcp_unavailable".to_string(),
            message,
        },
        McpAccessError::DiscoveryFailed(message) | McpAccessError::ExecutionFailed(message) => {
            OperationExecutionError::provider_failed(message)
        }
    }
}

#[derive(Debug, thiserror::Error)]
enum McpAccessError {
    #[error("AgentRun MCP surface 不可用: {0}")]
    SurfaceUnavailable(String),
    #[error("MCP 工具参数非法: {0}")]
    InvalidArguments(String),
    #[error("MCP 工具发现失败: {0}")]
    DiscoveryFailed(String),
    #[error("MCP 工具执行失败: {0}")]
    ExecutionFailed(String),
}

async fn execute_runtime_mcp_tool(
    tool: agentdash_agent_types::DynAgentTool,
    runtime_name: &str,
    arguments: Value,
    cancel: tokio_util::sync::CancellationToken,
) -> Result<AgentToolResult, McpAccessError> {
    tool.execute(&format!("op-mcp-{runtime_name}"), arguments, cancel, None)
        .await
        .map_err(|error| match error {
            AgentToolError::InvalidArguments(message) => McpAccessError::InvalidArguments(message),
            AgentToolError::ExecutionFailed(message) => McpAccessError::ExecutionFailed(message),
            AgentToolError::Other(error) => McpAccessError::ExecutionFailed(error.to_string()),
        })
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use agentdash_agent_types::{AgentTool, AgentToolError, ContentPart, ToolUpdateCallback};
    use agentdash_application_ports::mcp_discovery::{
        McpToolDiscoveryOutcome, McpToolDiscoveryRequest,
    };
    use agentdash_application_ports::runtime_gateway_mcp_surface::RuntimeGatewayMcpSurface;
    use agentdash_domain::backend::{RuntimeBackendAnchor, RuntimeBackendAnchorSource};
    use agentdash_domain::common::{Mount, MountCapability};
    use agentdash_spi::{
        CapabilityState, McpTransportConfig, RuntimeMcpServer, RuntimeVfsAccessPolicy,
        ToolCapability, ToolCapabilityFilter, ToolCluster, Vfs,
    };
    use serde_json::json;
    use tokio_util::sync::CancellationToken;
    use uuid::Uuid;

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

        async fn current_runtime_mcp_surface_for_agent_run(
            &self,
            _run_id: Uuid,
            _agent_id: Uuid,
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
        ) -> Result<McpToolDiscoveryOutcome, ConnectorError> {
            *self
                .captured_backend
                .lock()
                .expect("captured backend mutex poisoned") = request
                .call_context
                .as_ref()
                .and_then(|context| context.backend_anchor.clone());
            Ok(McpToolDiscoveryOutcome {
                tools: entries_for_request(&request, false),
                sources: Vec::new(),
            })
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
        let vfs = vfs();
        let vfs_access_policy = RuntimeVfsAccessPolicy::whole_mounts_from_vfs(&vfs);
        RuntimeGatewayMcpSurfaceWithBackend {
            surface: RuntimeGatewayMcpSurface {
                runtime_session_id: "session-1".to_string(),
                capability_state: capability_state(),
                vfs,
                vfs_access_policy,
                mcp_servers: vec![RuntimeMcpServer {
                    name: "code-analyzer".to_string(),
                    transport: McpTransportConfig::Http {
                        url: "http://localhost/mcp".to_string(),
                        headers: Vec::new(),
                    },
                    uses_relay: true,
                    readiness: Default::default(),
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
    async fn operation_mcp_access_uses_agent_run_surface_and_capability_filter() {
        let discovery = Arc::new(CapturingMcpDiscovery::new());
        let access = access(discovery.clone());
        let principal = OperationPrincipal::server_resolved(OperationPrincipalRef::AgentRunAgent {
            run_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
        });
        let scope = OperationAuthorizationScope {
            scope_ref: agentdash_domain::operation::OperationScopeRef::Project {
                project_id: Uuid::new_v4(),
            },
            authority_revision: "authority-1".into(),
        };

        let tools = access
            .discover_tools(&principal, &scope, CancellationToken::new())
            .await
            .expect("discover");

        assert_eq!(tools.len(), 2);
        assert_eq!(
            discovery.captured_backend_id().as_deref(),
            Some("backend-1")
        );
        let result = access
            .invoke_tool(
                &principal,
                &scope,
                "code-analyzer",
                "allowed_tool",
                json!({}),
                CancellationToken::new(),
            )
            .await
            .expect("invoke");
        assert_eq!(result["is_error"], false);
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
