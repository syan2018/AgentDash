use agentdash_agent_runtime_contract::{
    EventSequence, IdempotencyKey, RuntimeBindingId, RuntimeCommand, RuntimeDriverGeneration,
    RuntimeEventEnvelope, RuntimeOperationId, RuntimeRevision, RuntimeThreadId,
};
use async_trait::async_trait;
use thiserror::Error;

use crate::{RuntimeOperationRecord, RuntimeThreadState};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeOutboxEntry {
    pub operation_id: RuntimeOperationId,
    pub thread_id: RuntimeThreadId,
    pub generation: RuntimeDriverGeneration,
    pub command: RuntimeCommand,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DriverEventQuarantineReason {
    CanonicalThreadNotFound,
    StaleBinding {
        expected_binding_id: RuntimeBindingId,
        expected_generation: RuntimeDriverGeneration,
    },
    DriverOperationAcceptance,
    InvalidTransition {
        error: crate::TransitionError,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuarantinedDriverEvent {
    pub event: agentdash_agent_runtime_contract::DriverEventEnvelope,
    pub reason: DriverEventQuarantineReason,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeEventBatch {
    /// Earliest retained event sequence. When the retained journal is empty this is one past the
    /// latest durable sequence, which still lets callers detect a retention gap.
    pub earliest_available: EventSequence,
    pub latest_available: EventSequence,
    pub events: Vec<RuntimeEventEnvelope>,
}

/// Complete write-set for one optimistic runtime transaction.
#[derive(Debug, Clone)]
pub struct RuntimeCommit {
    /// `None` means create-if-absent; `Some` means update only when the durable projection has
    /// exactly this revision. Infrastructure must evaluate this CAS in the same database
    /// transaction that writes every field below.
    pub expected_projection_revision: Option<agentdash_agent_runtime_contract::RuntimeRevision>,
    pub projection: RuntimeThreadState,
    pub operation: Option<RuntimeOperationRecord>,
    pub operation_terminals: Vec<(
        RuntimeOperationId,
        agentdash_agent_runtime_contract::RuntimeOperationTerminal,
    )>,
    pub events: Vec<RuntimeEventEnvelope>,
    pub outbox: Vec<RuntimeOutboxEntry>,
    pub quarantine: Vec<QuarantinedDriverEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RuntimeStoreError {
    #[error("thread projection was not found")]
    NotFound,
    #[error("projection revision changed during commit")]
    ProjectionConflict {
        expected: Option<RuntimeRevision>,
        actual: Option<RuntimeRevision>,
    },
    #[error("operation id {operation_id} already exists")]
    OperationConflict { operation_id: RuntimeOperationId },
    #[error("idempotency key is already accepted by operation {operation_id}")]
    IdempotencyConflict { operation_id: RuntimeOperationId },
    #[error("runtime store is unavailable: {0}")]
    Unavailable(String),
}

/// Runtime-owned read repository. Infrastructure adapters implement storage, not transitions.
#[async_trait]
pub trait RuntimeRepository: Send + Sync {
    async fn load_thread(
        &self,
        thread_id: &RuntimeThreadId,
    ) -> Result<Option<RuntimeThreadState>, RuntimeStoreError>;

    async fn find_thread_by_source(
        &self,
        binding_id: &agentdash_agent_runtime_contract::RuntimeBindingId,
        source_thread_id: &agentdash_agent_runtime_contract::DriverThreadId,
    ) -> Result<Option<RuntimeThreadState>, RuntimeStoreError>;

    async fn find_operation(
        &self,
        operation_id: &RuntimeOperationId,
    ) -> Result<Option<RuntimeOperationRecord>, RuntimeStoreError>;

    async fn find_idempotency(
        &self,
        thread_id: &RuntimeThreadId,
        key: &IdempotencyKey,
    ) -> Result<Option<RuntimeOperationRecord>, RuntimeStoreError>;

    async fn events_after(
        &self,
        thread_id: &RuntimeThreadId,
        after: Option<EventSequence>,
    ) -> Result<RuntimeEventBatch, RuntimeStoreError>;
}

/// Atomic unit-of-work boundary for journal + projection + operation + outbox.
#[async_trait]
pub trait RuntimeUnitOfWork: Send + Sync {
    async fn commit(&self, commit: RuntimeCommit) -> Result<(), RuntimeStoreError>;

    async fn quarantine(&self, event: QuarantinedDriverEvent) -> Result<(), RuntimeStoreError>;
}

/// Ephemeral deltas deliberately live outside the authoritative unit of work.
#[async_trait]
pub trait RuntimeTransientEvents: Send + Sync {
    async fn publish(&self, event: RuntimeEventEnvelope);
    async fn read(&self, thread_id: &RuntimeThreadId) -> Vec<RuntimeEventEnvelope>;
}
