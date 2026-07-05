use agentdash_spi::ConnectorError;
use async_trait::async_trait;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptedTurnLifecycleAdvanceInput {
    pub runtime_session_id: String,
    pub turn_id: String,
}

#[async_trait]
pub trait AcceptedTurnLifecycleAdvancePort: Send + Sync {
    async fn advance_node_started_for_accepted_turn(
        &self,
        input: AcceptedTurnLifecycleAdvanceInput,
    ) -> Result<(), ConnectorError>;
}
