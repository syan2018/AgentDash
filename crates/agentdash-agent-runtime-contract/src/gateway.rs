use async_trait::async_trait;

use crate::{
    OperationReceipt, RuntimeCommandEnvelope, RuntimeEventEnvelope, RuntimeEventSubscription,
    RuntimeExecuteError, RuntimePresentationAppendError, RuntimePresentationAppendReceipt,
    RuntimePresentationAppendRequest, RuntimeSnapshotError, RuntimeSnapshotQuery,
    RuntimeSnapshotResult, RuntimeSubscribeError,
};

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeTerminalPresentationContext {
    pub presentation_thread_id: crate::PresentationThreadId,
    pub runtime_turn_id: crate::RuntimeTurnId,
    pub presentation_turn_id: crate::PresentationTurnId,
    pub terminal: crate::RuntimeTurnTerminal,
    pub message: Option<String>,
    pub diagnostic: Option<agentdash_agent_protocol::RuntimeTerminalDiagnostic>,
    pub started_at_ms: Option<u64>,
    pub completed_at_ms: u64,
    pub prior_records: Vec<crate::RuntimeJournalRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RuntimeApplicationPresentationProjectionError {
    #[error("application presentation projection is invalid: {0}")]
    Invalid(String),
}

pub trait RuntimeApplicationPresentationProjector: Send + Sync {
    fn project_terminal(
        &self,
        context: RuntimeTerminalPresentationContext,
    ) -> Result<Vec<crate::RuntimePresentationInput>, RuntimeApplicationPresentationProjectionError>;
}

#[async_trait]
pub trait RuntimeEventStream: Send {
    async fn next(&mut self) -> Option<Result<RuntimeEventEnvelope, RuntimeSubscribeError>>;
}

#[async_trait]
pub trait AgentRuntimeGateway: Send + Sync {
    async fn append_presentation(
        &self,
        request: RuntimePresentationAppendRequest,
    ) -> Result<RuntimePresentationAppendReceipt, RuntimePresentationAppendError>;

    async fn execute(
        &self,
        command: RuntimeCommandEnvelope,
    ) -> Result<OperationReceipt, RuntimeExecuteError>;

    async fn snapshot(
        &self,
        query: RuntimeSnapshotQuery,
    ) -> Result<RuntimeSnapshotResult, RuntimeSnapshotError>;

    async fn events(
        &self,
        subscription: RuntimeEventSubscription,
    ) -> Result<Box<dyn RuntimeEventStream>, RuntimeSubscribeError>;
}
