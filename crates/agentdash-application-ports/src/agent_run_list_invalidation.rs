use agentdash_agent_protocol::ControlPlaneProjectionChangeReason;
use async_trait::async_trait;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct AgentRunListInvalidation {
    pub project_id: Uuid,
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub frame_id: Option<Uuid>,
    pub reason: ControlPlaneProjectionChangeReason,
    pub delivery_runtime_session_id: Option<String>,
}

#[async_trait]
pub trait AgentRunListInvalidationPort: Send + Sync {
    async fn publish_agent_run_list_invalidated(
        &self,
        invalidation: AgentRunListInvalidation,
    ) -> Result<(), String>;
}
