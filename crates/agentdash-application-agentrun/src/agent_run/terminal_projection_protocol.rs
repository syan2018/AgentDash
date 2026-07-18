use agentdash_agent_runtime_contract::{
    RuntimeBindingId, RuntimeDriverGeneration, RuntimePayloadDigest, RuntimeThreadId,
};
use agentdash_application_ports::agent_run_runtime::AgentRunRuntimeTarget;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AgentRunTerminalId(String);

impl AgentRunTerminalId {
    pub fn new(value: impl Into<String>) -> Result<Self, AgentRunTerminalProtocolError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(AgentRunTerminalProtocolError::EmptyIdentity("terminal_id"));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AgentRunTerminalChangeId(String);

impl AgentRunTerminalChangeId {
    pub fn new(value: impl Into<String>) -> Result<Self, AgentRunTerminalProtocolError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(AgentRunTerminalProtocolError::EmptyIdentity("change_id"));
        }
        Ok(Self(value))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AgentRunTerminalOwnerEpochId(String);

impl AgentRunTerminalOwnerEpochId {
    pub fn new(value: impl Into<String>) -> Result<Self, AgentRunTerminalProtocolError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(AgentRunTerminalProtocolError::EmptyIdentity(
                "terminal_owner_epoch_id",
            ));
        }
        Ok(Self(value))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AgentRunTerminalControlCorrelationId(String);

impl AgentRunTerminalControlCorrelationId {
    pub fn new(value: impl Into<String>) -> Result<Self, AgentRunTerminalProtocolError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(AgentRunTerminalProtocolError::EmptyIdentity(
                "control_correlation_id",
            ));
        }
        Ok(Self(value))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AgentRunTerminalProjectionRevision(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AgentRunTerminalChangeSequence(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AgentRunTerminalOutputSequence(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AgentRunTerminalSourceSequence(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRunTerminalOutputStream {
    Stdout,
    Stderr,
    Pty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRunTerminalLifecycleState {
    Starting,
    Running,
    Exited,
    Killed,
    Lost,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRunTerminalAvailability {
    Online,
    Offline,
    Reconciling,
}

impl AgentRunTerminalLifecycleState {
    fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (
                Self::Starting,
                Self::Running | Self::Exited | Self::Killed | Self::Lost
            ) | (Self::Running, Self::Exited | Self::Killed | Self::Lost)
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRunTerminalControlKind {
    Input,
    Resize,
    Terminate,
    Read,
    Status,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRunTerminalControlStatus {
    Accepted,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRunTerminalCapability {
    Interactive,
    ReadOnlyOutput,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AgentRunTerminalOwnerFence {
    pub terminal_owner_epoch_id: AgentRunTerminalOwnerEpochId,
    pub target: AgentRunRuntimeTarget,
    pub runtime_thread_id: RuntimeThreadId,
    pub binding_id: RuntimeBindingId,
    pub binding_generation: RuntimeDriverGeneration,
    pub backend_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AgentRunTerminalOutputProjection {
    pub next_sequence: AgentRunTerminalOutputSequence,
    /// 按生产者提交顺序保留的终端字节流文本。快照必须能无歧义重建 UI，
    /// stream 分类仅保留在增量中用于诊断，不能拆分后再猜测交错顺序。
    pub retained_output: String,
    pub truncated: bool,
    pub omitted_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AgentRunTerminalProjection {
    pub terminal_id: AgentRunTerminalId,
    pub owner: AgentRunTerminalOwnerFence,
    pub mount_id: Option<String>,
    pub cwd: Option<String>,
    pub capability: AgentRunTerminalCapability,
    /// 复用 spawn 时确定的 max_output_bytes；中央持久投影不得另设隐藏 TTL/cap。
    pub max_output_bytes: u64,
    pub state: AgentRunTerminalLifecycleState,
    pub availability: AgentRunTerminalAvailability,
    pub latest_source_sequence: AgentRunTerminalSourceSequence,
    pub exit_code: Option<i32>,
    pub process_id: Option<u32>,
    pub created_at_ms: u64,
    pub exited_at_ms: Option<u64>,
    pub output: AgentRunTerminalOutputProjection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentRunTerminalProjectionDelta {
    Registered {
        terminal: AgentRunTerminalProjection,
    },
    OutputAppended {
        terminal_id: AgentRunTerminalId,
        owner: AgentRunTerminalOwnerFence,
        output_sequence: AgentRunTerminalOutputSequence,
        stream: AgentRunTerminalOutputStream,
        data: String,
    },
    OutputOmitted {
        terminal_id: AgentRunTerminalId,
        owner: AgentRunTerminalOwnerFence,
        output_sequence: AgentRunTerminalOutputSequence,
        omitted_bytes: u64,
        retained_output: String,
    },
    StateChanged {
        terminal_id: AgentRunTerminalId,
        owner: AgentRunTerminalOwnerFence,
        state: AgentRunTerminalLifecycleState,
        exit_code: Option<i32>,
        changed_at_ms: u64,
    },
    AvailabilityChanged {
        terminal_id: AgentRunTerminalId,
        owner: AgentRunTerminalOwnerFence,
        availability: AgentRunTerminalAvailability,
        changed_at_ms: u64,
    },
    ControlCorrelated {
        terminal_id: AgentRunTerminalId,
        owner: AgentRunTerminalOwnerFence,
        correlation_id: AgentRunTerminalControlCorrelationId,
        control: AgentRunTerminalControlKind,
        status: AgentRunTerminalControlStatus,
        diagnostic: Option<String>,
    },
    Removed {
        terminal_id: AgentRunTerminalId,
        owner: AgentRunTerminalOwnerFence,
    },
}

impl AgentRunTerminalProjectionDelta {
    fn terminal_id(&self) -> &AgentRunTerminalId {
        match self {
            Self::Registered { terminal } => &terminal.terminal_id,
            Self::OutputAppended { terminal_id, .. }
            | Self::OutputOmitted { terminal_id, .. }
            | Self::StateChanged { terminal_id, .. }
            | Self::AvailabilityChanged { terminal_id, .. }
            | Self::ControlCorrelated { terminal_id, .. }
            | Self::Removed { terminal_id, .. } => terminal_id,
        }
    }

    pub(crate) fn owner(&self) -> &AgentRunTerminalOwnerFence {
        match self {
            Self::Registered { terminal } => &terminal.owner,
            Self::OutputAppended { owner, .. }
            | Self::OutputOmitted { owner, .. }
            | Self::StateChanged { owner, .. }
            | Self::AvailabilityChanged { owner, .. }
            | Self::ControlCorrelated { owner, .. }
            | Self::Removed { owner, .. } => owner,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AgentRunTerminalChange {
    pub change_id: AgentRunTerminalChangeId,
    pub target: AgentRunRuntimeTarget,
    pub sequence: AgentRunTerminalChangeSequence,
    pub revision: AgentRunTerminalProjectionRevision,
    pub source_sequence: AgentRunTerminalSourceSequence,
    pub payload_digest: RuntimePayloadDigest,
    pub delta: AgentRunTerminalProjectionDelta,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AgentRunTerminalOutboxEntry {
    pub change_id: AgentRunTerminalChangeId,
    pub target: AgentRunRuntimeTarget,
    pub sequence: AgentRunTerminalChangeSequence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AgentRunTerminalProjectionCommit {
    pub expected_revision: AgentRunTerminalProjectionRevision,
    pub expected_source_sequence: AgentRunTerminalSourceSequence,
    pub expected_output_sequence: Option<AgentRunTerminalOutputSequence>,
    pub expected_terminal_state: Option<AgentRunTerminalLifecycleState>,
    pub change: AgentRunTerminalChange,
    pub outbox: AgentRunTerminalOutboxEntry,
}

impl AgentRunTerminalProjectionCommit {
    pub fn validate(&self) -> Result<(), AgentRunTerminalProtocolError> {
        let expected =
            AgentRunTerminalProjectionRevision(self.expected_revision.0.saturating_add(1));
        if self.change.revision != expected {
            return Err(AgentRunTerminalProtocolError::RevisionNotContiguous);
        }
        let expected_sequence =
            AgentRunTerminalChangeSequence(self.expected_revision.0.saturating_add(1));
        if self.change.sequence != expected_sequence {
            return Err(AgentRunTerminalProtocolError::ChangeSequenceNotContiguous);
        }
        let expected_source =
            AgentRunTerminalSourceSequence(self.expected_source_sequence.0.saturating_add(1));
        if self.change.source_sequence != expected_source {
            return Err(AgentRunTerminalProtocolError::SourceSequenceNotContiguous);
        }
        if self.change.target != self.change.delta.owner().target
            || self.outbox.target != self.change.target
        {
            return Err(AgentRunTerminalProtocolError::OwnerFenceMismatch);
        }
        if self.outbox.change_id != self.change.change_id
            || self.outbox.sequence != self.change.sequence
        {
            return Err(AgentRunTerminalProtocolError::OutboxMismatch);
        }
        match &self.change.delta {
            AgentRunTerminalProjectionDelta::OutputAppended {
                output_sequence, ..
            }
            | AgentRunTerminalProjectionDelta::OutputOmitted {
                output_sequence, ..
            } => {
                let expected_output = self
                    .expected_output_sequence
                    .ok_or(AgentRunTerminalProtocolError::MissingOutputFence)?;
                if *output_sequence != expected_output {
                    return Err(AgentRunTerminalProtocolError::OutputSequenceMismatch);
                }
            }
            AgentRunTerminalProjectionDelta::StateChanged { state, .. } => {
                let current = self
                    .expected_terminal_state
                    .ok_or(AgentRunTerminalProtocolError::MissingStateFence)?;
                if !current.can_transition_to(*state) {
                    return Err(AgentRunTerminalProtocolError::InvalidStateTransition {
                        current,
                        next: *state,
                    });
                }
            }
            AgentRunTerminalProjectionDelta::Registered { terminal } => {
                if terminal.terminal_id != *self.change.delta.terminal_id() {
                    return Err(AgentRunTerminalProtocolError::TerminalIdentityMismatch);
                }
            }
            AgentRunTerminalProjectionDelta::ControlCorrelated { .. }
            | AgentRunTerminalProjectionDelta::AvailabilityChanged { .. }
            | AgentRunTerminalProjectionDelta::Removed { .. } => {}
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AgentRunTerminalProjectionHead {
    pub target: AgentRunRuntimeTarget,
    pub revision: AgentRunTerminalProjectionRevision,
    pub latest_change_sequence: AgentRunTerminalChangeSequence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AgentRunTerminalSnapshot {
    pub target: AgentRunRuntimeTarget,
    pub revision: AgentRunTerminalProjectionRevision,
    pub latest_change_sequence: AgentRunTerminalChangeSequence,
    pub captured_at_ms: u64,
    pub terminals: Vec<AgentRunTerminalProjection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AgentRunTerminalChangeGap {
    pub requested_after: Option<AgentRunTerminalChangeSequence>,
    pub earliest_available: AgentRunTerminalChangeSequence,
    pub latest_available: AgentRunTerminalChangeSequence,
    pub snapshot_revision: AgentRunTerminalProjectionRevision,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AgentRunTerminalChangePage {
    pub target: AgentRunRuntimeTarget,
    pub changes: Vec<AgentRunTerminalChange>,
    pub next: AgentRunTerminalChangeSequence,
    pub gap: Option<AgentRunTerminalChangeGap>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AgentRunTerminalSourceSnapshot {
    pub terminal: AgentRunTerminalProjection,
    pub payload_digest: RuntimePayloadDigest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AgentRunTerminalSourceInventory {
    pub owner: AgentRunTerminalOwnerFence,
    pub captured_at_source_sequence: AgentRunTerminalSourceSequence,
    pub terminals: Vec<AgentRunTerminalSourceSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AgentRunTerminalSourceDelta {
    pub terminal_id: AgentRunTerminalId,
    pub terminal_owner_epoch_id: AgentRunTerminalOwnerEpochId,
    pub source_sequence: AgentRunTerminalSourceSequence,
    pub payload_digest: RuntimePayloadDigest,
    pub delta: AgentRunTerminalProjectionDelta,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AgentRunTerminalReconcileRequest {
    pub target: AgentRunRuntimeTarget,
    pub terminal_id: AgentRunTerminalId,
    pub terminal_owner_epoch_id: AgentRunTerminalOwnerEpochId,
    pub after_source_sequence: AgentRunTerminalSourceSequence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRunTerminalSourceResolution {
    Exact,
    /// Local 明确确认该 owner epoch 下不存在 terminal，Product 才能收敛为 Lost。
    Unknown,
    /// Owner fence 已变化且 Local 无法证明仍是同一进程，Product 才能收敛为 Lost。
    OwnerFenceUnprovable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AgentRunTerminalReconcileResult {
    pub request: AgentRunTerminalReconcileRequest,
    pub resolution: AgentRunTerminalSourceResolution,
    pub inventory: AgentRunTerminalSourceInventory,
    pub snapshot: Option<AgentRunTerminalSourceSnapshot>,
    pub deltas: Vec<AgentRunTerminalSourceDelta>,
}

impl AgentRunTerminalReconcileResult {
    pub fn validate(&self) -> Result<(), AgentRunTerminalProtocolError> {
        if self.inventory.owner.target != self.request.target
            || self.inventory.owner.terminal_owner_epoch_id != self.request.terminal_owner_epoch_id
        {
            return Err(AgentRunTerminalProtocolError::ReconcileOwnerMismatch);
        }
        if let Some(snapshot) = &self.snapshot
            && (snapshot.terminal.terminal_id != self.request.terminal_id
                || snapshot.terminal.owner != self.inventory.owner)
        {
            return Err(AgentRunTerminalProtocolError::ReconcileOwnerMismatch);
        }
        let mut expected = self.request.after_source_sequence.0.saturating_add(1);
        for delta in &self.deltas {
            if delta.terminal_id != self.request.terminal_id
                || delta.terminal_owner_epoch_id != self.request.terminal_owner_epoch_id
                || delta.delta.owner() != &self.inventory.owner
                || delta.source_sequence != AgentRunTerminalSourceSequence(expected)
            {
                return Err(AgentRunTerminalProtocolError::ReconcileSequenceMismatch);
            }
            expected = expected.saturating_add(1);
        }
        if matches!(self.resolution, AgentRunTerminalSourceResolution::Exact)
            && self.snapshot.is_none()
        {
            return Err(AgentRunTerminalProtocolError::MissingReconcileSnapshot);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AgentRunTerminalControlRoute {
    pub terminal_id: AgentRunTerminalId,
    pub owner: AgentRunTerminalOwnerFence,
    pub availability: AgentRunTerminalAvailability,
}

#[async_trait]
pub trait AgentRunTerminalProjectionRepository: Send + Sync {
    async fn load_head(
        &self,
        target: &AgentRunRuntimeTarget,
    ) -> Result<AgentRunTerminalProjectionHead, AgentRunTerminalProjectionStoreError>;

    async fn load_snapshot(
        &self,
        target: &AgentRunRuntimeTarget,
    ) -> Result<AgentRunTerminalSnapshot, AgentRunTerminalProjectionStoreError>;

    async fn load_changes(
        &self,
        target: &AgentRunRuntimeTarget,
        after: Option<AgentRunTerminalChangeSequence>,
        limit: usize,
    ) -> Result<AgentRunTerminalChangePage, AgentRunTerminalProjectionStoreError>;
}

#[async_trait]
pub trait AgentRunTerminalSourceReconcilePort: Send + Sync {
    async fn reconcile(
        &self,
        request: AgentRunTerminalReconcileRequest,
    ) -> Result<AgentRunTerminalReconcileResult, AgentRunTerminalProjectionStoreError>;
}

#[async_trait]
pub trait AgentRunTerminalControlRoutingRepository: Send + Sync {
    async fn resolve_control_route(
        &self,
        target: &AgentRunRuntimeTarget,
        terminal_id: &AgentRunTerminalId,
    ) -> Result<Option<AgentRunTerminalControlRoute>, AgentRunTerminalProjectionStoreError>;
}

#[async_trait]
pub trait AgentRunTerminalProjectionUnitOfWork: Send + Sync {
    async fn commit(
        &self,
        commit: AgentRunTerminalProjectionCommit,
    ) -> Result<(), AgentRunTerminalProjectionStoreError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AgentRunTerminalProtocolError {
    #[error("{0} must be non-empty")]
    EmptyIdentity(&'static str),
    #[error("terminal projection revision is not contiguous")]
    RevisionNotContiguous,
    #[error("terminal change sequence is not contiguous")]
    ChangeSequenceNotContiguous,
    #[error("terminal source sequence is not contiguous for its owner epoch")]
    SourceSequenceNotContiguous,
    #[error("terminal owner fence differs across the atomic write set")]
    OwnerFenceMismatch,
    #[error("terminal outbox identity differs from the committed change")]
    OutboxMismatch,
    #[error("terminal output mutation is missing its expected sequence")]
    MissingOutputFence,
    #[error("terminal output sequence is not monotonic")]
    OutputSequenceMismatch,
    #[error("terminal state mutation is missing its expected state")]
    MissingStateFence,
    #[error("terminal state transition {current:?} -> {next:?} is invalid")]
    InvalidStateTransition {
        current: AgentRunTerminalLifecycleState,
        next: AgentRunTerminalLifecycleState,
    },
    #[error("terminal projection identity differs from its change")]
    TerminalIdentityMismatch,
    #[error("terminal reconcile result differs from the requested owner epoch")]
    ReconcileOwnerMismatch,
    #[error(
        "terminal reconcile delta sequence is not contiguous after the requested source cursor"
    )]
    ReconcileSequenceMismatch,
    #[error("exact terminal reconcile result is missing its source snapshot")]
    MissingReconcileSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AgentRunTerminalProjectionStoreError {
    #[error("AgentRun terminal projection conflict")]
    Conflict,
    #[error("AgentRun terminal projection persistence failed: {0}")]
    Persistence(String),
}

pub const AGENT_RUN_TERMINAL_PERSISTENCE_CONSTRAINTS: &[&str] = &[
    "one head row per (target_run_id, target_agent_id), advanced by exact revision CAS",
    "unique(target_run_id, target_agent_id, revision)",
    "unique(target_run_id, target_agent_id, change_sequence)",
    "unique(terminal_owner_epoch_id, source_sequence)",
    "unique(terminal_id, output_sequence)",
    "unique(change_id)",
    "owner epoch, target, Runtime thread, binding, generation, and backend are immutable",
    "process state and backend availability are independent projection dimensions",
    "retained output uses the spawn max_output_bytes and over-cap writes emit typed OutputOmitted",
    "terminal and AgentRun retention policies own cleanup; the projection adds no hidden TTL",
    "terminal projection, change, control correlation, and outbox commit in one transaction",
];

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn owner() -> AgentRunTerminalOwnerFence {
        AgentRunTerminalOwnerFence {
            terminal_owner_epoch_id: AgentRunTerminalOwnerEpochId::new("owner-epoch-1")
                .expect("owner epoch"),
            target: AgentRunRuntimeTarget {
                run_id: Uuid::nil(),
                agent_id: Uuid::max(),
            },
            runtime_thread_id: RuntimeThreadId::new("thread-1").expect("thread"),
            binding_id: RuntimeBindingId::new("binding-1").expect("binding"),
            binding_generation: RuntimeDriverGeneration(2),
            backend_id: "backend-1".to_string(),
        }
    }

    fn output_commit() -> AgentRunTerminalProjectionCommit {
        let owner = owner();
        let change_id = AgentRunTerminalChangeId::new("terminal-change-3").expect("change");
        AgentRunTerminalProjectionCommit {
            expected_revision: AgentRunTerminalProjectionRevision(2),
            expected_source_sequence: AgentRunTerminalSourceSequence(6),
            expected_output_sequence: Some(AgentRunTerminalOutputSequence(7)),
            expected_terminal_state: None,
            change: AgentRunTerminalChange {
                change_id: change_id.clone(),
                target: owner.target.clone(),
                sequence: AgentRunTerminalChangeSequence(3),
                revision: AgentRunTerminalProjectionRevision(3),
                source_sequence: AgentRunTerminalSourceSequence(7),
                payload_digest: RuntimePayloadDigest::new("sha256:terminal-output-7")
                    .expect("digest"),
                delta: AgentRunTerminalProjectionDelta::OutputAppended {
                    terminal_id: AgentRunTerminalId::new("terminal-1").expect("terminal"),
                    owner: owner.clone(),
                    output_sequence: AgentRunTerminalOutputSequence(7),
                    stream: AgentRunTerminalOutputStream::Pty,
                    data: "hello".to_string(),
                },
            },
            outbox: AgentRunTerminalOutboxEntry {
                change_id,
                target: owner.target,
                sequence: AgentRunTerminalChangeSequence(3),
            },
        }
    }

    #[test]
    fn accepts_monotonic_output_under_exact_owner_and_revision_fences() {
        output_commit().validate().expect("valid output commit");
    }

    #[test]
    fn rejects_duplicate_or_skipped_output_sequence() {
        let mut commit = output_commit();
        commit.expected_output_sequence = Some(AgentRunTerminalOutputSequence(8));
        assert_eq!(
            commit.validate(),
            Err(AgentRunTerminalProtocolError::OutputSequenceMismatch)
        );
    }

    #[test]
    fn terminal_state_cannot_revive_after_exit() {
        let mut commit = output_commit();
        let owner = owner();
        commit.expected_output_sequence = None;
        commit.expected_terminal_state = Some(AgentRunTerminalLifecycleState::Exited);
        commit.change.delta = AgentRunTerminalProjectionDelta::StateChanged {
            terminal_id: AgentRunTerminalId::new("terminal-1").expect("terminal"),
            owner,
            state: AgentRunTerminalLifecycleState::Running,
            exit_code: None,
            changed_at_ms: 10,
        };
        assert_eq!(
            commit.validate(),
            Err(AgentRunTerminalProtocolError::InvalidStateTransition {
                current: AgentRunTerminalLifecycleState::Exited,
                next: AgentRunTerminalLifecycleState::Running,
            })
        );
    }

    #[test]
    fn backend_disconnect_changes_availability_without_marking_process_lost() {
        let mut commit = output_commit();
        let owner = owner();
        commit.expected_output_sequence = None;
        commit.change.delta = AgentRunTerminalProjectionDelta::AvailabilityChanged {
            terminal_id: AgentRunTerminalId::new("terminal-1").expect("terminal"),
            owner,
            availability: AgentRunTerminalAvailability::Offline,
            changed_at_ms: 10,
        };
        commit.validate().expect("availability is orthogonal");
    }

    #[test]
    fn source_sequence_cannot_skip_within_owner_epoch() {
        let mut commit = output_commit();
        commit.change.source_sequence = AgentRunTerminalSourceSequence(9);
        assert_eq!(
            commit.validate(),
            Err(AgentRunTerminalProtocolError::SourceSequenceNotContiguous)
        );
    }

    #[test]
    fn over_cap_output_is_a_typed_recoverable_projection_change() {
        let mut commit = output_commit();
        commit.change.delta = AgentRunTerminalProjectionDelta::OutputOmitted {
            terminal_id: AgentRunTerminalId::new("terminal-1").expect("terminal"),
            owner: owner(),
            output_sequence: AgentRunTerminalOutputSequence(7),
            omitted_bytes: 4_096,
            retained_output: "retained tail".to_string(),
        };
        commit.validate().expect("typed omission preserves fences");
    }
}
