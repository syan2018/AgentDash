use agentdash_agent_protocol::UserInputBlock;
use agentdash_domain::workflow::ExecutionSource;
use async_trait::async_trait;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeSessionCreationRequest {
    pub project_id: Uuid,
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub source: ExecutionSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeSessionCreationResult {
    pub runtime_session_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeSessionDeliveryRef {
    pub runtime_session_id: String,
    pub turn_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeSessionTurnDeliveryCommand {
    pub runtime_session_id: String,
    pub input: Vec<UserInputBlock>,
    pub expected_turn_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeSessionTurnDeliveryResult {
    pub runtime_session_id: String,
    pub accepted_turn_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RuntimeSessionDeliveryError {
    #[error("runtime session was not found: {runtime_session_id}")]
    NotFound { runtime_session_id: String },
    #[error("runtime session delivery rejected: {message}")]
    Rejected { message: String },
    #[error("runtime session delivery unavailable: {message}")]
    Unavailable { message: String },
    #[error("runtime session delivery failed: {message}")]
    Internal { message: String },
}

#[async_trait]
pub trait RuntimeSessionCreationPort: Send + Sync {
    async fn create_runtime_session(
        &self,
        request: RuntimeSessionCreationRequest,
    ) -> Result<RuntimeSessionCreationResult, RuntimeSessionDeliveryError>;
}

#[async_trait]
pub trait RuntimeSessionTurnDeliveryPort: Send + Sync {
    async fn start_turn(
        &self,
        command: RuntimeSessionTurnDeliveryCommand,
    ) -> Result<RuntimeSessionTurnDeliveryResult, RuntimeSessionDeliveryError>;

    async fn steer_turn(
        &self,
        command: RuntimeSessionTurnDeliveryCommand,
    ) -> Result<RuntimeSessionTurnDeliveryResult, RuntimeSessionDeliveryError>;

    async fn cancel_turn(
        &self,
        runtime_session_id: &str,
        expected_turn_id: Option<&str>,
    ) -> Result<(), RuntimeSessionDeliveryError>;
}
