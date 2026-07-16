use agentdash_agent_runtime_contract::{
    EventSequence, IdempotencyKey, ImmutablePresentationEvent, PresentationThreadId,
    RuntimeBindingId, RuntimeCommand, RuntimeDriverGeneration, RuntimeEvent, RuntimeEventEnvelope,
    RuntimeJournalRecord, RuntimeOperationId, RuntimePresentationCoordinate, RuntimeRevision,
    RuntimeThreadId, RuntimeTurnId,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    ActiveContextHead, ContextActivation, ContextCandidate, ContextCheckpoint, ContextHeadWrite,
    ContextPreparationWorkItem, RuntimeOperationRecord, RuntimeThreadState,
};

#[async_trait]
pub trait RuntimeSurfaceReferenceValidator: Send + Sync {
    async fn validate_surface_reference(
        &self,
        binding_id: &RuntimeBindingId,
        runtime_thread_id: &RuntimeThreadId,
        target: &agentdash_agent_runtime_contract::RuntimeSurfaceDescriptor,
    ) -> Result<(), String>;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeOutboxEntry {
    pub operation_id: RuntimeOperationId,
    pub thread_id: RuntimeThreadId,
    pub presentation_thread_id: PresentationThreadId,
    pub binding_id: RuntimeBindingId,
    pub binding_epoch: agentdash_agent_runtime_contract::BindingEpoch,
    pub generation: RuntimeDriverGeneration,
    pub command: RuntimeCommand,
}

impl RuntimeOutboxEntry {
    pub fn matches_thread_binding(&self, thread: &RuntimeThreadState) -> bool {
        self.thread_id == thread.thread_id
            && self.presentation_thread_id == thread.presentation_thread_id
            && self.binding_id == thread.binding_id
            && self.binding_epoch == thread.binding_epoch
            && self.generation == thread.driver_generation
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RuntimeTerminalApplicationEffectId(String);

impl RuntimeTerminalApplicationEffectId {
    pub fn new(value: impl Into<String>) -> Result<Self, RuntimeStoreError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(RuntimeStoreError::Unavailable(
                "terminal application effect id must not be empty".to_string(),
            ));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeTerminalApplicationEffectOutboxEntry {
    pub effect_id: RuntimeTerminalApplicationEffectId,
    pub runtime_thread_id: RuntimeThreadId,
    pub presentation_thread_id: PresentationThreadId,
    pub runtime_turn_id: RuntimeTurnId,
    pub presentation_turn_id: agentdash_agent_runtime_contract::PresentationTurnId,
    pub terminal_event_sequence: EventSequence,
    pub terminal: agentdash_agent_runtime_contract::RuntimeTurnTerminal,
    pub message: Option<String>,
    pub diagnostic: Option<agentdash_agent_protocol::RuntimeTerminalDiagnostic>,
    pub started_at_ms: Option<u64>,
    pub completed_at_ms: u64,
    pub binding_id: RuntimeBindingId,
    pub driver_generation: RuntimeDriverGeneration,
    pub surface_revision: agentdash_agent_runtime_contract::SurfaceRevision,
    pub surface_digest: agentdash_agent_runtime_contract::SurfaceDigest,
    pub source_thread_id: String,
    pub source_turn_id: Option<String>,
    pub terminal_hook_effect_binding:
        Option<agentdash_agent_runtime_contract::RuntimeTerminalHookEffectBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeTerminalApplicationEffectClaimRequest {
    pub owner: RuntimeWorkerId,
    pub lease_duration_ms: u64,
    pub limit: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeTerminalApplicationEffectClaim {
    pub entry: RuntimeTerminalApplicationEffectOutboxEntry,
    pub token: RuntimeWorkClaimToken,
    pub owner: RuntimeWorkerId,
    pub lease_expires_at_ms: i64,
    pub attempt: u32,
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

#[derive(Debug, Clone, PartialEq)]
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

#[derive(Debug, Clone, PartialEq)]
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
    EmptyFactBatch,
    TransientInternalFact,
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
    InvalidDriverFact {
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuarantinedDriverEvent {
    pub event: agentdash_agent_runtime_contract::DriverEventEnvelope,
    pub reason: DriverEventQuarantineReason,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeEventBatch {
    /// Earliest retained event sequence. When the retained journal is empty this is one past the
    /// latest durable sequence, which still lets callers detect a retention gap.
    pub earliest_available: EventSequence,
    pub latest_available: EventSequence,
    pub events: Vec<RuntimeEventEnvelope>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeJournalBatch {
    /// Earliest retained durable journal sequence. An empty retained journal is
    /// represented by one past the latest durable sequence.
    pub earliest_available: EventSequence,
    pub latest_available: EventSequence,
    pub records: Vec<RuntimeJournalRecord>,
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
    pub records: Vec<RuntimeJournalRecord>,
    pub outbox: Vec<RuntimeOutboxEntry>,
    pub terminal_application_effects: Vec<RuntimeTerminalApplicationEffectOutboxEntry>,
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

pub fn internal_journal_records(
    events: Vec<RuntimeEventEnvelope>,
) -> Result<Vec<RuntimeJournalRecord>, RuntimeStoreError> {
    events
        .into_iter()
        .map(|event| {
            RuntimeJournalRecord::from_internal_envelope(event).map_err(|error| {
                RuntimeStoreError::Unavailable(format!(
                    "invalid internal runtime journal record: {error}"
                ))
            })
        })
        .collect()
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

    async fn journal_records_after(
        &self,
        thread_id: &RuntimeThreadId,
        after: Option<EventSequence>,
    ) -> Result<RuntimeJournalBatch, RuntimeStoreError>;

    /// Internal Runtime state-machine view over the single journal source.
    /// Session presentation must consume `journal_records_after` instead.
    async fn internal_events_after(
        &self,
        thread_id: &RuntimeThreadId,
        after: Option<EventSequence>,
    ) -> Result<RuntimeEventBatch, RuntimeStoreError> {
        let batch = self.journal_records_after(thread_id, after).await?;
        Ok(RuntimeEventBatch {
            earliest_available: batch.earliest_available,
            latest_available: batch.latest_available,
            events: batch
                .records
                .iter()
                .filter_map(RuntimeJournalRecord::to_internal_envelope)
                .collect(),
        })
    }

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
    async fn commit(&self, commit: RuntimeCommit) -> Result<(), RuntimeStoreError> {
        self.commit_with_live_presentation_publication(commit, true)
            .await
    }

    /// Commits the authoritative write-set while allowing the caller to defer only the
    /// presentation live broadcast. Durable storage and the internal Runtime live stream still
    /// complete before this method returns. Callers that defer must publish the committed
    /// presentation records explicitly after success.
    async fn commit_with_live_presentation_publication(
        &self,
        commit: RuntimeCommit,
        publish_live_presentations: bool,
    ) -> Result<(), RuntimeStoreError>;

    async fn quarantine(&self, event: QuarantinedDriverEvent) -> Result<(), RuntimeStoreError>;
}

/// Ephemeral deltas deliberately live outside the authoritative unit of work.
#[async_trait]
pub trait RuntimeTransientEvents: Send + Sync {
    async fn publish_transient(
        &self,
        thread_id: RuntimeThreadId,
        binding_id: RuntimeBindingId,
        stream_generation: RuntimeDriverGeneration,
        turn_id: Option<RuntimeTurnId>,
        revision: RuntimeRevision,
        event: RuntimeEvent,
    );
    async fn publish_transient_presentation(
        &self,
        thread_id: RuntimeThreadId,
        binding_id: RuntimeBindingId,
        stream_generation: RuntimeDriverGeneration,
        turn_id: Option<RuntimeTurnId>,
        revision: RuntimeRevision,
        coordinate: RuntimePresentationCoordinate,
        event: ImmutablePresentationEvent,
    );
    async fn publish_durable_presentation(&self, record: RuntimeJournalRecord);
    async fn publish_durable(&self, event: RuntimeEventEnvelope);
    async fn subscribe(
        &self,
        thread_id: &RuntimeThreadId,
    ) -> tokio::sync::broadcast::Receiver<RuntimeEventEnvelope>;
    async fn read(
        &self,
        thread_id: &RuntimeThreadId,
        stream_generation: Option<RuntimeDriverGeneration>,
        after: Option<agentdash_agent_runtime_contract::RuntimeTransientSequence>,
    ) -> Vec<RuntimeEventEnvelope>;
    async fn subscribe_presentation(
        &self,
        thread_id: &RuntimeThreadId,
    ) -> tokio::sync::broadcast::Receiver<RuntimeJournalRecord>;
    async fn read_presentation(
        &self,
        thread_id: &RuntimeThreadId,
        stream_generation: Option<RuntimeDriverGeneration>,
        after: Option<agentdash_agent_runtime_contract::RuntimeTransientSequence>,
    ) -> Vec<RuntimeJournalRecord>;
    async fn clear(&self, thread_id: &RuntimeThreadId);
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

#[async_trait]
pub trait RuntimeTerminalApplicationEffectOutbox: Send + Sync {
    async fn claim_terminal_application_effects(
        &self,
        request: RuntimeTerminalApplicationEffectClaimRequest,
    ) -> Result<Vec<RuntimeTerminalApplicationEffectClaim>, RuntimeStoreError>;

    async fn ack_terminal_application_effect(
        &self,
        claim: &RuntimeTerminalApplicationEffectClaim,
    ) -> Result<(), RuntimeStoreError>;

    async fn release_terminal_application_effect(
        &self,
        claim: &RuntimeTerminalApplicationEffectClaim,
        error: String,
    ) -> Result<(), RuntimeStoreError>;
}
