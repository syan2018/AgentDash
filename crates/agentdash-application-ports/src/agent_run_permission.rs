use async_trait::async_trait;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunPermissionRequest {
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub runtime_session_id: String,
    pub turn_id: String,
    pub item_id: String,
    pub capability_key: String,
    pub tool_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentRunPermissionDecision {
    Allowed,
    Denied {
        reason: String,
    },
    PendingApproval {
        interaction_id: String,
        reason: String,
    },
}

#[derive(Debug, Error)]
pub enum AgentRunPermissionError {
    #[error("AgentRun permission validation failed: {message}")]
    Validation { message: String },
}

#[async_trait]
pub trait AgentRunPermissionFacade: Send + Sync {
    async fn authorize(
        &self,
        request: AgentRunPermissionRequest,
    ) -> Result<AgentRunPermissionDecision, AgentRunPermissionError>;
}
