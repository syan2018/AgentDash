use agentdash_agent_runtime_contract::{
    ManagedRuntimeSourceBindingEvidence, RuntimePayloadDigest, RuntimeThreadId,
};
use agentdash_domain::agent_run_target::AgentRunTarget;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use agentdash_contracts::agent_run_product_projection as wire;

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
pub enum AgentRunTerminalProductChangeKind {
    BackendAvailability,
    ControlCorrelation,
    ReconcileLost,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentRunTerminalChangeOrigin {
    SourceFact {
        terminal_owner_epoch_id: AgentRunTerminalOwnerEpochId,
        source_sequence: AgentRunTerminalSourceSequence,
    },
    ProductFact {
        change_kind: AgentRunTerminalProductChangeKind,
    },
}

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
    pub target: AgentRunTarget,
    pub runtime_thread_id: RuntimeThreadId,
    pub source_binding: ManagedRuntimeSourceBindingEvidence,
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
    pub target: AgentRunTarget,
    pub sequence: AgentRunTerminalChangeSequence,
    pub revision: AgentRunTerminalProjectionRevision,
    pub origin: AgentRunTerminalChangeOrigin,
    pub payload_digest: RuntimePayloadDigest,
    pub delta: AgentRunTerminalProjectionDelta,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AgentRunTerminalOutboxEntry {
    pub change_id: AgentRunTerminalChangeId,
    pub target: AgentRunTarget,
    pub sequence: AgentRunTerminalChangeSequence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AgentRunTerminalProjectionCommit {
    pub expected_revision: AgentRunTerminalProjectionRevision,
    pub expected_source_sequence: Option<AgentRunTerminalSourceSequence>,
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
        let expects_source_fact = matches!(
            self.change.delta,
            AgentRunTerminalProjectionDelta::Registered { .. }
                | AgentRunTerminalProjectionDelta::OutputAppended { .. }
                | AgentRunTerminalProjectionDelta::OutputOmitted { .. }
                | AgentRunTerminalProjectionDelta::StateChanged { .. }
                | AgentRunTerminalProjectionDelta::Removed { .. }
        );
        match (&self.change.origin, self.expected_source_sequence) {
            (
                AgentRunTerminalChangeOrigin::SourceFact {
                    terminal_owner_epoch_id,
                    source_sequence,
                },
                Some(expected_source),
            ) if expects_source_fact
                && terminal_owner_epoch_id
                    == &self.change.delta.owner().terminal_owner_epoch_id =>
            {
                if *source_sequence
                    != AgentRunTerminalSourceSequence(expected_source.0.saturating_add(1))
                {
                    return Err(AgentRunTerminalProtocolError::SourceSequenceNotContiguous);
                }
            }
            (
                AgentRunTerminalChangeOrigin::ProductFact {
                    change_kind: AgentRunTerminalProductChangeKind::BackendAvailability,
                },
                None,
            ) if matches!(
                self.change.delta,
                AgentRunTerminalProjectionDelta::AvailabilityChanged { .. }
            ) => {}
            (
                AgentRunTerminalChangeOrigin::ProductFact {
                    change_kind: AgentRunTerminalProductChangeKind::ControlCorrelation,
                },
                None,
            ) if matches!(
                self.change.delta,
                AgentRunTerminalProjectionDelta::ControlCorrelated { .. }
            ) => {}
            (
                AgentRunTerminalChangeOrigin::ProductFact {
                    change_kind: AgentRunTerminalProductChangeKind::ReconcileLost,
                },
                None,
            ) if matches!(
                self.change.delta,
                AgentRunTerminalProjectionDelta::StateChanged {
                    state: AgentRunTerminalLifecycleState::Lost,
                    ..
                }
            ) => {}
            _ => return Err(AgentRunTerminalProtocolError::ChangeOriginMismatch),
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
    pub target: AgentRunTarget,
    pub revision: AgentRunTerminalProjectionRevision,
    pub latest_change_sequence: AgentRunTerminalChangeSequence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AgentRunTerminalSnapshot {
    pub target: AgentRunTarget,
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
    pub target: AgentRunTarget,
    pub changes: Vec<AgentRunTerminalChange>,
    pub next: AgentRunTerminalChangeSequence,
    pub gap: Option<AgentRunTerminalChangeGap>,
}

fn wire_target(target: AgentRunTarget) -> wire::AgentRunProjectionTarget {
    wire::AgentRunProjectionTarget {
        run_id: target.run_id.to_string(),
        agent_id: target.agent_id.to_string(),
    }
}

impl From<AgentRunTerminalOwnerFence> for wire::AgentRunTerminalOwnerFence {
    fn from(value: AgentRunTerminalOwnerFence) -> Self {
        Self {
            terminal_owner_epoch_id: value.terminal_owner_epoch_id.0,
            target: wire_target(value.target),
            runtime_thread_id: value.runtime_thread_id.to_string(),
            source_binding: value.source_binding,
            backend_id: value.backend_id,
        }
    }
}

fn wire_lifecycle(value: AgentRunTerminalLifecycleState) -> wire::AgentRunTerminalLifecycleState {
    match value {
        AgentRunTerminalLifecycleState::Starting => wire::AgentRunTerminalLifecycleState::Starting,
        AgentRunTerminalLifecycleState::Running => wire::AgentRunTerminalLifecycleState::Running,
        AgentRunTerminalLifecycleState::Exited => wire::AgentRunTerminalLifecycleState::Exited,
        AgentRunTerminalLifecycleState::Killed => wire::AgentRunTerminalLifecycleState::Killed,
        AgentRunTerminalLifecycleState::Lost => wire::AgentRunTerminalLifecycleState::Lost,
    }
}

fn wire_availability(value: AgentRunTerminalAvailability) -> wire::AgentRunTerminalAvailability {
    match value {
        AgentRunTerminalAvailability::Online => wire::AgentRunTerminalAvailability::Online,
        AgentRunTerminalAvailability::Offline => wire::AgentRunTerminalAvailability::Offline,
        AgentRunTerminalAvailability::Reconciling => {
            wire::AgentRunTerminalAvailability::Reconciling
        }
    }
}

impl From<AgentRunTerminalProjection> for wire::AgentRunTerminalProjection {
    fn from(value: AgentRunTerminalProjection) -> Self {
        Self {
            terminal_id: value.terminal_id.0,
            owner: value.owner.into(),
            mount_id: value.mount_id,
            cwd: value.cwd,
            capability: match value.capability {
                AgentRunTerminalCapability::Interactive => {
                    wire::AgentRunTerminalCapability::Interactive
                }
                AgentRunTerminalCapability::ReadOnlyOutput => {
                    wire::AgentRunTerminalCapability::ReadOnlyOutput
                }
            },
            max_output_bytes: value.max_output_bytes,
            state: wire_lifecycle(value.state),
            availability: wire_availability(value.availability),
            latest_source_sequence: value.latest_source_sequence.0,
            exit_code: value.exit_code,
            process_id: value.process_id,
            created_at_ms: value.created_at_ms,
            exited_at_ms: value.exited_at_ms,
            output: wire::AgentRunTerminalOutputProjection {
                next_sequence: value.output.next_sequence.0,
                retained_output: value.output.retained_output,
                truncated: value.output.truncated,
                omitted_bytes: value.output.omitted_bytes,
            },
        }
    }
}

impl From<AgentRunTerminalProjectionDelta> for wire::AgentRunTerminalProjectionDelta {
    fn from(value: AgentRunTerminalProjectionDelta) -> Self {
        match value {
            AgentRunTerminalProjectionDelta::Registered { terminal } => Self::Registered {
                terminal: terminal.into(),
            },
            AgentRunTerminalProjectionDelta::OutputAppended {
                terminal_id,
                owner,
                output_sequence,
                stream,
                data,
            } => Self::OutputAppended {
                terminal_id: terminal_id.0,
                owner: owner.into(),
                output_sequence: output_sequence.0,
                stream: match stream {
                    AgentRunTerminalOutputStream::Stdout => {
                        wire::AgentRunTerminalOutputStream::Stdout
                    }
                    AgentRunTerminalOutputStream::Stderr => {
                        wire::AgentRunTerminalOutputStream::Stderr
                    }
                    AgentRunTerminalOutputStream::Pty => wire::AgentRunTerminalOutputStream::Pty,
                },
                data,
            },
            AgentRunTerminalProjectionDelta::OutputOmitted {
                terminal_id,
                owner,
                output_sequence,
                omitted_bytes,
                retained_output,
            } => Self::OutputOmitted {
                terminal_id: terminal_id.0,
                owner: owner.into(),
                output_sequence: output_sequence.0,
                omitted_bytes,
                retained_output,
            },
            AgentRunTerminalProjectionDelta::StateChanged {
                terminal_id,
                owner,
                state,
                exit_code,
                changed_at_ms,
            } => Self::StateChanged {
                terminal_id: terminal_id.0,
                owner: owner.into(),
                state: wire_lifecycle(state),
                exit_code,
                changed_at_ms,
            },
            AgentRunTerminalProjectionDelta::AvailabilityChanged {
                terminal_id,
                owner,
                availability,
                changed_at_ms,
            } => Self::AvailabilityChanged {
                terminal_id: terminal_id.0,
                owner: owner.into(),
                availability: wire_availability(availability),
                changed_at_ms,
            },
            AgentRunTerminalProjectionDelta::ControlCorrelated {
                terminal_id,
                owner,
                correlation_id,
                control,
                status,
                diagnostic,
            } => Self::ControlCorrelated {
                terminal_id: terminal_id.0,
                owner: owner.into(),
                correlation_id: correlation_id.0,
                control: match control {
                    AgentRunTerminalControlKind::Input => wire::AgentRunTerminalControlKind::Input,
                    AgentRunTerminalControlKind::Resize => {
                        wire::AgentRunTerminalControlKind::Resize
                    }
                    AgentRunTerminalControlKind::Terminate => {
                        wire::AgentRunTerminalControlKind::Terminate
                    }
                    AgentRunTerminalControlKind::Read => wire::AgentRunTerminalControlKind::Read,
                    AgentRunTerminalControlKind::Status => {
                        wire::AgentRunTerminalControlKind::Status
                    }
                },
                status: match status {
                    AgentRunTerminalControlStatus::Accepted => {
                        wire::AgentRunTerminalControlStatus::Accepted
                    }
                    AgentRunTerminalControlStatus::Completed => {
                        wire::AgentRunTerminalControlStatus::Completed
                    }
                    AgentRunTerminalControlStatus::Failed => {
                        wire::AgentRunTerminalControlStatus::Failed
                    }
                },
                diagnostic,
            },
            AgentRunTerminalProjectionDelta::Removed { terminal_id, owner } => Self::Removed {
                terminal_id: terminal_id.0,
                owner: owner.into(),
            },
        }
    }
}

impl From<AgentRunTerminalChange> for wire::AgentRunTerminalChange {
    fn from(value: AgentRunTerminalChange) -> Self {
        Self {
            change_id: value.change_id.0,
            target: wire_target(value.target),
            sequence: value.sequence.0,
            revision: value.revision.0,
            origin: match value.origin {
                AgentRunTerminalChangeOrigin::SourceFact {
                    terminal_owner_epoch_id,
                    source_sequence,
                } => wire::AgentRunTerminalChangeOrigin::SourceFact {
                    terminal_owner_epoch_id: terminal_owner_epoch_id.0,
                    source_sequence: source_sequence.0,
                },
                AgentRunTerminalChangeOrigin::ProductFact { change_kind } => {
                    wire::AgentRunTerminalChangeOrigin::ProductFact {
                        change_kind: match change_kind {
                            AgentRunTerminalProductChangeKind::BackendAvailability => {
                                wire::AgentRunTerminalProductChangeKind::BackendAvailability
                            }
                            AgentRunTerminalProductChangeKind::ControlCorrelation => {
                                wire::AgentRunTerminalProductChangeKind::ControlCorrelation
                            }
                            AgentRunTerminalProductChangeKind::ReconcileLost => {
                                wire::AgentRunTerminalProductChangeKind::ReconcileLost
                            }
                        },
                    }
                }
            },
            payload_digest: value.payload_digest.to_string(),
            delta: value.delta.into(),
        }
    }
}

impl From<AgentRunTerminalSnapshot> for wire::AgentRunTerminalSnapshot {
    fn from(value: AgentRunTerminalSnapshot) -> Self {
        Self {
            target: wire_target(value.target),
            revision: value.revision.0,
            latest_change_sequence: value.latest_change_sequence.0,
            captured_at_ms: value.captured_at_ms,
            terminals: value.terminals.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<AgentRunTerminalChangeGap> for wire::AgentRunTerminalChangeGap {
    fn from(value: AgentRunTerminalChangeGap) -> Self {
        Self {
            requested_after: value.requested_after.map(|sequence| sequence.0),
            earliest_available: value.earliest_available.0,
            latest_available: value.latest_available.0,
            snapshot_revision: value.snapshot_revision.0,
        }
    }
}

impl From<AgentRunTerminalChangePage> for wire::AgentRunTerminalChangePage {
    fn from(value: AgentRunTerminalChangePage) -> Self {
        Self {
            target: wire_target(value.target),
            changes: value.changes.into_iter().map(Into::into).collect(),
            next: value.next.0,
            gap: value.gap.map(Into::into),
        }
    }
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
    pub target: AgentRunTarget,
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
            || self
                .inventory
                .terminals
                .iter()
                .any(|snapshot| snapshot.terminal.owner != self.inventory.owner)
        {
            return Err(AgentRunTerminalProtocolError::ReconcileOwnerMismatch);
        }
        let same_owner_epoch =
            self.inventory.owner.terminal_owner_epoch_id == self.request.terminal_owner_epoch_id;
        match self.resolution {
            AgentRunTerminalSourceResolution::Exact => {
                if !same_owner_epoch {
                    return Err(AgentRunTerminalProtocolError::ReconcileOwnerMismatch);
                }
                let snapshot = self
                    .snapshot
                    .as_ref()
                    .ok_or(AgentRunTerminalProtocolError::MissingReconcileSnapshot)?;
                if snapshot.terminal.terminal_id != self.request.terminal_id
                    || snapshot.terminal.owner != self.inventory.owner
                {
                    return Err(AgentRunTerminalProtocolError::ReconcileOwnerMismatch);
                }
            }
            AgentRunTerminalSourceResolution::Unknown => {
                let old_owner_terminal_is_absent = !self
                    .inventory
                    .terminals
                    .iter()
                    .any(|snapshot| snapshot.terminal.terminal_id == self.request.terminal_id);
                if !same_owner_epoch || self.snapshot.is_some() || !old_owner_terminal_is_absent {
                    return Err(AgentRunTerminalProtocolError::ReconcileResolutionEvidenceMismatch);
                }
            }
            AgentRunTerminalSourceResolution::OwnerFenceUnprovable => {
                if same_owner_epoch || self.snapshot.is_some() || !self.deltas.is_empty() {
                    return Err(AgentRunTerminalProtocolError::ReconcileResolutionEvidenceMismatch);
                }
            }
        }
        if !matches!(
            self.resolution,
            AgentRunTerminalSourceResolution::OwnerFenceUnprovable
        ) {
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
        target: &AgentRunTarget,
    ) -> Result<AgentRunTerminalProjectionHead, AgentRunTerminalProjectionStoreError>;

    async fn load_snapshot(
        &self,
        target: &AgentRunTarget,
    ) -> Result<AgentRunTerminalSnapshot, AgentRunTerminalProjectionStoreError>;

    async fn load_changes(
        &self,
        target: &AgentRunTarget,
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
        target: &AgentRunTarget,
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
    #[error("terminal change origin does not match its source or Product delta")]
    ChangeOriginMismatch,
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
    #[error("terminal reconcile resolution lacks its required owner evidence")]
    ReconcileResolutionEvidenceMismatch,
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
    "owner epoch, target, Runtime thread, source binding evidence, and backend are immutable",
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
            target: AgentRunTarget {
                run_id: Uuid::nil(),
                agent_id: Uuid::max(),
            },
            runtime_thread_id: RuntimeThreadId::new("thread-1").expect("thread"),
            source_binding: ManagedRuntimeSourceBindingEvidence {
                source_ref: agentdash_agent_runtime_contract::RuntimeSourceRef::new("source-1")
                    .expect("source"),
                committed_at_revision: agentdash_agent_runtime_contract::RuntimeProjectionRevision(
                    1,
                ),
                applied_surface_revision: agentdash_agent_runtime_contract::SurfaceRevision(2),
                activated_at_revision: Some(
                    agentdash_agent_runtime_contract::RuntimeProjectionRevision(2),
                ),
            },
            backend_id: "backend-1".to_string(),
        }
    }

    fn terminal(owner: AgentRunTerminalOwnerFence) -> AgentRunTerminalProjection {
        AgentRunTerminalProjection {
            terminal_id: AgentRunTerminalId::new("terminal-1").expect("terminal"),
            owner,
            mount_id: None,
            cwd: Some("F:/workspace".to_string()),
            capability: AgentRunTerminalCapability::Interactive,
            max_output_bytes: 16_384,
            state: AgentRunTerminalLifecycleState::Running,
            availability: AgentRunTerminalAvailability::Online,
            latest_source_sequence: AgentRunTerminalSourceSequence(6),
            exit_code: None,
            process_id: Some(42),
            created_at_ms: 1,
            exited_at_ms: None,
            output: AgentRunTerminalOutputProjection {
                next_sequence: AgentRunTerminalOutputSequence(8),
                retained_output: "hello".to_string(),
                truncated: false,
                omitted_bytes: 0,
            },
        }
    }

    fn source_snapshot(owner: AgentRunTerminalOwnerFence) -> AgentRunTerminalSourceSnapshot {
        AgentRunTerminalSourceSnapshot {
            terminal: terminal(owner),
            payload_digest: RuntimePayloadDigest::new("sha256:terminal-source-snapshot")
                .expect("digest"),
        }
    }

    fn reconcile_result(
        resolution: AgentRunTerminalSourceResolution,
        inventory_owner: AgentRunTerminalOwnerFence,
        snapshot: Option<AgentRunTerminalSourceSnapshot>,
    ) -> AgentRunTerminalReconcileResult {
        let requested_owner = owner();
        AgentRunTerminalReconcileResult {
            request: AgentRunTerminalReconcileRequest {
                target: requested_owner.target,
                terminal_id: AgentRunTerminalId::new("terminal-1").expect("terminal"),
                terminal_owner_epoch_id: requested_owner.terminal_owner_epoch_id,
                after_source_sequence: AgentRunTerminalSourceSequence(6),
            },
            resolution,
            inventory: AgentRunTerminalSourceInventory {
                owner: inventory_owner,
                captured_at_source_sequence: AgentRunTerminalSourceSequence(6),
                terminals: Vec::new(),
            },
            snapshot,
            deltas: Vec::new(),
        }
    }

    fn output_commit() -> AgentRunTerminalProjectionCommit {
        let owner = owner();
        let change_id = AgentRunTerminalChangeId::new("terminal-change-3").expect("change");
        AgentRunTerminalProjectionCommit {
            expected_revision: AgentRunTerminalProjectionRevision(2),
            expected_source_sequence: Some(AgentRunTerminalSourceSequence(6)),
            expected_output_sequence: Some(AgentRunTerminalOutputSequence(7)),
            expected_terminal_state: None,
            change: AgentRunTerminalChange {
                change_id: change_id.clone(),
                target: owner.target.clone(),
                sequence: AgentRunTerminalChangeSequence(3),
                revision: AgentRunTerminalProjectionRevision(3),
                origin: AgentRunTerminalChangeOrigin::SourceFact {
                    terminal_owner_epoch_id: owner.terminal_owner_epoch_id.clone(),
                    source_sequence: AgentRunTerminalSourceSequence(7),
                },
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
        commit.expected_source_sequence = None;
        commit.change.origin = AgentRunTerminalChangeOrigin::ProductFact {
            change_kind: AgentRunTerminalProductChangeKind::BackendAvailability,
        };
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
        commit.change.origin = AgentRunTerminalChangeOrigin::SourceFact {
            terminal_owner_epoch_id: owner().terminal_owner_epoch_id,
            source_sequence: AgentRunTerminalSourceSequence(9),
        };
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

    #[test]
    fn product_availability_change_does_not_consume_agent_source_sequence() {
        let mut commit = output_commit();
        commit.expected_output_sequence = None;
        commit.expected_source_sequence = None;
        commit.change.origin = AgentRunTerminalChangeOrigin::ProductFact {
            change_kind: AgentRunTerminalProductChangeKind::BackendAvailability,
        };
        commit.change.delta = AgentRunTerminalProjectionDelta::AvailabilityChanged {
            terminal_id: AgentRunTerminalId::new("terminal-1").expect("terminal"),
            owner: owner(),
            availability: AgentRunTerminalAvailability::Offline,
            changed_at_ms: 11,
        };
        commit.validate().expect("Product-only change");
    }

    #[test]
    fn product_reconcile_lost_does_not_consume_or_forge_agent_source_sequence() {
        let mut commit = output_commit();
        commit.expected_output_sequence = None;
        commit.expected_source_sequence = None;
        commit.expected_terminal_state = Some(AgentRunTerminalLifecycleState::Running);
        commit.change.origin = AgentRunTerminalChangeOrigin::ProductFact {
            change_kind: AgentRunTerminalProductChangeKind::ReconcileLost,
        };
        commit.change.delta = AgentRunTerminalProjectionDelta::StateChanged {
            terminal_id: AgentRunTerminalId::new("terminal-1").expect("terminal"),
            owner: owner(),
            state: AgentRunTerminalLifecycleState::Lost,
            exit_code: None,
            changed_at_ms: 12,
        };

        commit
            .validate()
            .expect("reconcile Lost is a Product-only change");
    }

    #[test]
    fn exact_reconcile_requires_the_requested_owner_snapshot() {
        let old_owner = owner();
        reconcile_result(
            AgentRunTerminalSourceResolution::Exact,
            old_owner.clone(),
            Some(source_snapshot(old_owner.clone())),
        )
        .validate()
        .expect("exact source snapshot");

        assert_eq!(
            reconcile_result(AgentRunTerminalSourceResolution::Exact, old_owner, None).validate(),
            Err(AgentRunTerminalProtocolError::MissingReconcileSnapshot)
        );
    }

    #[test]
    fn unknown_reconcile_requires_old_owner_absence_and_no_snapshot() {
        let old_owner = owner();
        reconcile_result(
            AgentRunTerminalSourceResolution::Unknown,
            old_owner.clone(),
            None,
        )
        .validate()
        .expect("old owner inventory proves terminal absence");

        let mut present_in_inventory = reconcile_result(
            AgentRunTerminalSourceResolution::Unknown,
            old_owner.clone(),
            None,
        );
        present_in_inventory
            .inventory
            .terminals
            .push(source_snapshot(old_owner.clone()));
        assert_eq!(
            present_in_inventory.validate(),
            Err(AgentRunTerminalProtocolError::ReconcileResolutionEvidenceMismatch)
        );

        let stale_snapshot = source_snapshot(old_owner.clone());
        assert_eq!(
            reconcile_result(
                AgentRunTerminalSourceResolution::Unknown,
                old_owner,
                Some(stale_snapshot),
            )
            .validate(),
            Err(AgentRunTerminalProtocolError::ReconcileResolutionEvidenceMismatch)
        );
    }

    #[test]
    fn owner_fence_unprovable_requires_changed_owner_evidence_without_old_snapshot() {
        let mut changed_owner = owner();
        changed_owner.terminal_owner_epoch_id =
            AgentRunTerminalOwnerEpochId::new("owner-epoch-2").expect("owner epoch");
        changed_owner.source_binding.source_ref =
            agentdash_agent_runtime_contract::RuntimeSourceRef::new("source-2").expect("source");

        reconcile_result(
            AgentRunTerminalSourceResolution::OwnerFenceUnprovable,
            changed_owner.clone(),
            None,
        )
        .validate()
        .expect("changed owner fence is explicit evidence");

        assert_eq!(
            reconcile_result(
                AgentRunTerminalSourceResolution::OwnerFenceUnprovable,
                changed_owner.clone(),
                Some(source_snapshot(changed_owner)),
            )
            .validate(),
            Err(AgentRunTerminalProtocolError::ReconcileResolutionEvidenceMismatch)
        );

        assert_eq!(
            reconcile_result(
                AgentRunTerminalSourceResolution::OwnerFenceUnprovable,
                owner(),
                None,
            )
            .validate(),
            Err(AgentRunTerminalProtocolError::ReconcileResolutionEvidenceMismatch)
        );
    }
}
