use agentdash_spi::SessionMcpServer;
use async_trait::async_trait;

use super::hub::SessionHub;
use super::hub::{
    LiveRuntimeContextTransitionInput, PendingRuntimeContextApplication,
    PendingRuntimeContextTransitionInput, RuntimeContextTransitionOutcome,
};
use super::types::{CapabilityState, PendingCapabilityStateTransition};
use crate::runtime_gateway::{
    McpCallToolInput, RuntimeMcpToolDescriptor, RuntimeSessionMcpAccess, RuntimeSessionMcpError,
};

#[derive(Clone)]
pub struct SessionCapabilityService {
    hub: SessionHub,
}

impl SessionCapabilityService {
    pub(super) fn new(hub: SessionHub) -> Self {
        Self { hub }
    }

    pub async fn get_runtime_mcp_servers(&self, session_id: &str) -> Vec<SessionMcpServer> {
        self.hub.get_runtime_mcp_servers(session_id).await
    }

    pub async fn get_current_capability_state(&self, session_id: &str) -> Option<CapabilityState> {
        self.hub.get_current_capability_state(session_id).await
    }

    pub async fn get_latest_capability_state(&self, session_id: &str) -> Option<CapabilityState> {
        self.hub.get_latest_capability_state(session_id).await
    }

    pub async fn enqueue_pending_capability_state_transition(
        &self,
        session_id: &str,
        transition: PendingCapabilityStateTransition,
    ) -> std::io::Result<()> {
        self.hub
            .enqueue_pending_capability_state_transition(session_id, transition)
            .await
    }

    pub(crate) async fn enqueue_pending_runtime_context_transition(
        &self,
        input: PendingRuntimeContextTransitionInput,
    ) -> Result<(), String> {
        self.hub
            .enqueue_pending_runtime_context_transition(input)
            .await
    }

    pub(crate) async fn apply_live_runtime_context_transition(
        &self,
        hook_session: &agentdash_spi::hooks::SharedHookSessionRuntime,
        input: LiveRuntimeContextTransitionInput,
    ) -> Result<RuntimeContextTransitionOutcome, String> {
        self.hub
            .apply_live_runtime_context_transition(hook_session, input)
            .await
    }

    pub(crate) async fn apply_pending_runtime_context_transitions_on_turn(
        &self,
        session_id: &str,
        turn_id: &str,
        hook_session: Option<&agentdash_spi::hooks::SharedHookSessionRuntime>,
        before_state: CapabilityState,
        transitions: &[PendingCapabilityStateTransition],
        tools: &[agentdash_agent_types::DynAgentTool],
    ) -> PendingRuntimeContextApplication {
        self.hub
            .apply_pending_runtime_context_transitions_on_turn(
                session_id,
                turn_id,
                hook_session,
                before_state,
                transitions,
                tools,
            )
            .await
    }
}

#[async_trait]
impl RuntimeSessionMcpAccess for SessionCapabilityService {
    async fn list_mcp_tools(
        &self,
        session_id: &str,
    ) -> Result<Vec<RuntimeMcpToolDescriptor>, RuntimeSessionMcpError> {
        let entries = self
            .hub
            .discover_runtime_mcp_tool_entries(session_id)
            .await
            .map_err(runtime_mcp_error_from_connector)?;
        Ok(entries
            .into_iter()
            .map(|entry| RuntimeMcpToolDescriptor {
                runtime_name: entry.runtime_name,
                server_name: entry.server_name,
                tool_name: entry.tool_name,
                uses_relay: entry.uses_relay,
                description: entry.description,
                parameters_schema: entry.parameters_schema,
            })
            .collect())
    }

    async fn call_mcp_tool(
        &self,
        session_id: &str,
        input: McpCallToolInput,
    ) -> Result<agentdash_agent_types::AgentToolResult, RuntimeSessionMcpError> {
        let entries = self
            .hub
            .discover_runtime_mcp_tool_entries(session_id)
            .await
            .map_err(runtime_mcp_error_from_connector)?;
        let entry = entries
            .into_iter()
            .find(|entry| runtime_mcp_entry_matches(entry, &input))
            .ok_or_else(|| {
                RuntimeSessionMcpError::ToolUnavailable(
                    "目标 MCP 工具不在当前 Session Runtime Surface 中".to_string(),
                )
            })?;
        let arguments = input.arguments.unwrap_or(serde_json::Value::Null);
        crate::runtime_gateway::execute_runtime_mcp_tool(entry.tool, &entry.runtime_name, arguments)
            .await
    }
}

fn runtime_mcp_entry_matches(
    entry: &agentdash_executor::mcp::DiscoveredMcpTool,
    input: &McpCallToolInput,
) -> bool {
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

fn runtime_mcp_error_from_connector(
    error: agentdash_spi::ConnectorError,
) -> RuntimeSessionMcpError {
    match error {
        agentdash_spi::ConnectorError::Runtime(message)
        | agentdash_spi::ConnectorError::InvalidConfig(message) => {
            RuntimeSessionMcpError::SessionUnavailable(message)
        }
        agentdash_spi::ConnectorError::ConnectionFailed(message) => {
            RuntimeSessionMcpError::DiscoveryFailed(message)
        }
        agentdash_spi::ConnectorError::SpawnFailed(message) => {
            RuntimeSessionMcpError::DiscoveryFailed(message)
        }
        agentdash_spi::ConnectorError::Io(error) => {
            RuntimeSessionMcpError::DiscoveryFailed(error.to_string())
        }
        agentdash_spi::ConnectorError::Json(error) => {
            RuntimeSessionMcpError::DiscoveryFailed(error.to_string())
        }
    }
}
