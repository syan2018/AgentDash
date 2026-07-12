use agentdash_agent_runtime_contract::{
    EventSequence, IdempotencyKey, RuntimeBindingId, RuntimeCommand, RuntimeDriverGeneration,
    RuntimeEventEnvelope, RuntimeOperationId, RuntimeRevision, RuntimeThreadId,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    ActiveContextHead, ContextActivation, ContextCandidate, ContextCheckpoint, ContextHeadWrite,
    ContextPreparationWorkItem, RuntimeOperationRecord, RuntimeThreadState,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeOutboxEntry {
    pub operation_id: RuntimeOperationId,
    pub thread_id: RuntimeThreadId,
    pub binding_id: RuntimeBindingId,
    pub binding_epoch: agentdash_agent_runtime_contract::BindingEpoch,
    pub generation: RuntimeDriverGeneration,
    pub command: RuntimeCommand,
}

impl RuntimeOutboxEntry {
    pub fn matches_thread_binding(&self, thread: &RuntimeThreadState) -> bool {
        self.thread_id == thread.thread_id
            && self.binding_id == thread.binding_id
            && self.binding_epoch == thread.binding_epoch
            && self.generation == thread.driver_generation
    }
}

/// A durable work category. Each category retains its own business state; the queue only owns
/// leases and delivery acknowledgement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeWorkKind {
    RuntimeOutbox,
    ContextPreparation,
    ContextActivationDispatch,
    ContextActivationRecovery,
    HookRunRecovery,
    HookEffect,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeWorkIdentity {
    Operation(RuntimeOperationId),
    Compaction(agentdash_agent_runtime_contract::ContextCompactionId),
    Activation(agentdash_agent_runtime_contract::ContextActivationId),
    HookEffect(agentdash_agent_runtime_contract::HookEffectId),
    HookRun(agentdash_agent_runtime_contract::HookRunId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeWorkPayload {
    RuntimeOutbox(RuntimeOutboxEntry),
    ContextPreparation(ContextPreparationWorkItem),
    ContextActivationDispatch(ContextActivationOutboxEntry),
    ContextActivationRecovery(ContextActivation),
    HookEffect(crate::HookEffect),
    HookRunRecovery(crate::HookRun),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RuntimeWorkerId(pub String);

impl RuntimeWorkerId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RuntimeWorkClaimToken(pub String);

impl RuntimeWorkClaimToken {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeWorkClaimRequest {
    pub kind: RuntimeWorkKind,
    pub owner: RuntimeWorkerId,
    pub lease_duration_ms: u64,
    pub limit: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeWorkClaim {
    pub kind: RuntimeWorkKind,
    pub identity: RuntimeWorkIdentity,
    pub token: RuntimeWorkClaimToken,
    pub owner: RuntimeWorkerId,
    pub lease_expires_at_ms: i64,
    pub attempt: u32,
    pub payload: RuntimeWorkPayload,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextActivationOutboxEntry {
    pub activation_id: agentdash_agent_runtime_contract::ContextActivationId,
    pub candidate_id: agentdash_agent_runtime_contract::ContextCandidateId,
    pub compaction_id: agentdash_agent_runtime_contract::ContextCompactionId,
    pub thread_id: RuntimeThreadId,
    pub binding_id: RuntimeBindingId,
    pub generation: RuntimeDriverGeneration,
    pub checkpoint_id: agentdash_agent_runtime_contract::ContextCheckpointId,
    pub digest: agentdash_agent_runtime_contract::ContextDigest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DriverEventQuarantineReason {
    CanonicalThreadNotFound,
    StaleBinding {
        expected_binding_id: RuntimeBindingId,
        expected_generation: RuntimeDriverGeneration,
    },
    DriverOperationAcceptance,
    DriverRuntimeOwnedContextEvent,
    DriverRuntimeOwnedHookEvent,
    DriverRuntimeOwnedBindingEvent,
    InvalidTransition {
        error: crate::TransitionError,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    pub context_activation_outbox: Vec<ContextActivationOutboxEntry>,
    pub context_preparation_work_items: Vec<ContextPreparationWorkItem>,
    pub context_checkpoints: Vec<ContextCheckpoint>,
    pub context_candidates: Vec<ContextCandidate>,
    pub context_activations: Vec<ContextActivation>,
    pub context_head: Option<ContextHeadWrite>,
    pub hook_plan_binding: Option<crate::RuntimeHookPlanBinding>,
    pub hook_runs: Vec<crate::HookRun>,
    pub hook_effects: Vec<crate::HookEffect>,
    pub quarantine: Vec<QuarantinedDriverEvent>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ContextStoreInvariant {
    #[error("immutable checkpoint identity changed")]
    CheckpointIdentity,
    #[error("checkpoint revision was reused within a thread")]
    CheckpointRevisionReused,
    #[error("immutable candidate identity changed")]
    CandidateIdentity,
    #[error("candidate id was reused by another compaction")]
    CandidateIdReused,
    #[error("candidate coordinates do not match checkpoint or preparation work")]
    CandidateCoordinates,
    #[error("immutable activation identity changed")]
    ActivationIdentity,
    #[error("activation coordinates do not match its candidate")]
    ActivationCoordinates,
    #[error("activation status transition is invalid")]
    ActivationTransition,
    #[error("immutable preparation work identity changed")]
    PreparationIdentity,
    #[error("preparation work status transition is invalid")]
    PreparationTransition,
    #[error("activation dispatch identity changed")]
    ActivationDispatchIdentity,
    #[error("context head does not match its immutable checkpoint")]
    HeadCheckpointMismatch,
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
    #[error("active context head changed during commit")]
    ContextHeadConflict {
        expected: Option<agentdash_agent_runtime_contract::ContextRevision>,
        actual: Option<agentdash_agent_runtime_contract::ContextRevision>,
    },
    #[error("context store invariant failed: {violation}")]
    ContextInvariant { violation: ContextStoreInvariant },
    #[error("context compaction operation {operation_id} is already active")]
    ContextCompactionConflict { operation_id: RuntimeOperationId },
    #[error("operation id {operation_id} already exists")]
    OperationConflict { operation_id: RuntimeOperationId },
    #[error("idempotency key is already accepted by operation {operation_id}")]
    IdempotencyConflict { operation_id: RuntimeOperationId },
    #[error("runtime work claim is not owned by the supplied owner and token")]
    WorkClaimConflict,
    #[error("runtime work claim request is invalid: {0}")]
    InvalidWorkClaim(String),
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

    async fn load_context_head(
        &self,
        thread_id: &RuntimeThreadId,
    ) -> Result<Option<ActiveContextHead>, RuntimeStoreError>;

    async fn load_context_checkpoint(
        &self,
        checkpoint_id: &agentdash_agent_runtime_contract::ContextCheckpointId,
    ) -> Result<Option<ContextCheckpoint>, RuntimeStoreError>;

    async fn load_context_candidate(
        &self,
        compaction_id: &agentdash_agent_runtime_contract::ContextCompactionId,
    ) -> Result<Option<ContextCandidate>, RuntimeStoreError>;

    async fn load_context_activation(
        &self,
        activation_id: &agentdash_agent_runtime_contract::ContextActivationId,
    ) -> Result<Option<ContextActivation>, RuntimeStoreError>;

    async fn load_context_preparation(
        &self,
        compaction_id: &agentdash_agent_runtime_contract::ContextCompactionId,
    ) -> Result<Option<ContextPreparationWorkItem>, RuntimeStoreError>;

    async fn pending_context_preparations(
        &self,
    ) -> Result<Vec<ContextPreparationWorkItem>, RuntimeStoreError>;

    async fn pending_context_activations(
        &self,
    ) -> Result<Vec<ContextActivationOutboxEntry>, RuntimeStoreError>;

    async fn recoverable_context_activations(
        &self,
    ) -> Result<Vec<ContextActivation>, RuntimeStoreError>;

    async fn load_hook_run(
        &self,
        hook_run_id: &agentdash_agent_runtime_contract::HookRunId,
    ) -> Result<Option<crate::HookRun>, RuntimeStoreError>;

    async fn recoverable_hook_runs(&self) -> Result<Vec<crate::HookRun>, RuntimeStoreError>;

    async fn load_hook_plan(
        &self,
        thread_id: &RuntimeThreadId,
    ) -> Result<Option<crate::RuntimeHookPlanBinding>, RuntimeStoreError>;

    async fn hook_effects(
        &self,
        hook_run_id: &agentdash_agent_runtime_contract::HookRunId,
    ) -> Result<Vec<crate::HookEffect>, RuntimeStoreError>;
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

/// Durable multi-worker lease queue for side effects and recovery work.
#[async_trait]
pub trait RuntimeWorkQueue: Send + Sync {
    async fn claim(
        &self,
        request: RuntimeWorkClaimRequest,
    ) -> Result<Vec<RuntimeWorkClaim>, RuntimeStoreError>;

    async fn ack(&self, claim: &RuntimeWorkClaim) -> Result<(), RuntimeStoreError>;

    async fn release(
        &self,
        claim: &RuntimeWorkClaim,
        error: String,
    ) -> Result<(), RuntimeStoreError>;
}
