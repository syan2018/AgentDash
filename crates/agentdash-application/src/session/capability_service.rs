use agentdash_spi::SessionMcpServer;
use async_trait::async_trait;

use super::hub::SessionHub;
use super::hub::{
    LiveRuntimeContextTransitionInput, PendingRuntimeContextTransitionInput,
    RuntimeContextTransitionOutcome,
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
}

#[async_trait]
impl RuntimeSessionMcpAccess for SessionCapabilityService {
    async fn list_mcp_tools(
        &self,
        session_id: &str,
    ) -> Result<Vec<RuntimeMcpToolDescriptor>, RuntimeSessionMcpError> {
        self.hub.list_mcp_tools(session_id).await
    }

    async fn call_mcp_tool(
        &self,
        session_id: &str,
        input: McpCallToolInput,
    ) -> Result<agentdash_agent_types::AgentToolResult, RuntimeSessionMcpError> {
        self.hub.call_mcp_tool(session_id, input).await
    }
}
