use agentdash_agent_runtime_contract::{
    RuntimeItemId, RuntimeOperationId, RuntimePayloadDigest, RuntimeSourceRef, RuntimeThreadId,
    RuntimeTurnId, SurfaceRevision,
};
use agentdash_contracts::workspace_module::WorkspaceModulePresentation;
use agentdash_domain::agent_run_target::AgentRunTarget;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use thiserror::Error;

use agentdash_contracts::agent_run_product_projection as wire;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WorkspaceModulePresentationEffectId(String);

impl WorkspaceModulePresentationEffectId {
    pub fn new(value: impl Into<String>) -> Result<Self, WorkspaceModulePresentationProtocolError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(WorkspaceModulePresentationProtocolError::EmptyIdentity(
                "effect_id",
            ));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WorkspaceModulePresentationIntentId(String);

impl WorkspaceModulePresentationIntentId {
    pub fn new(value: impl Into<String>) -> Result<Self, WorkspaceModulePresentationProtocolError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(WorkspaceModulePresentationProtocolError::EmptyIdentity(
                "intent_id",
            ));
        }
        Ok(Self(value))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WorkspaceModulePresentationChangeId(String);

impl WorkspaceModulePresentationChangeId {
    pub fn new(value: impl Into<String>) -> Result<Self, WorkspaceModulePresentationProtocolError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(WorkspaceModulePresentationProtocolError::EmptyIdentity(
                "change_id",
            ));
        }
        Ok(Self(value))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WorkspaceModulePresentationAckId(String);

impl WorkspaceModulePresentationAckId {
    pub fn new(value: impl Into<String>) -> Result<Self, WorkspaceModulePresentationProtocolError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(WorkspaceModulePresentationProtocolError::EmptyIdentity(
                "ack_id",
            ));
        }
        Ok(Self(value))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WorkspaceModulePresentationRevision(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WorkspaceModulePresentationChangeSequence(pub u64);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WorkspaceModulePresentationCause {
    pub runtime_thread_id: RuntimeThreadId,
    pub runtime_operation_id: Option<RuntimeOperationId>,
    pub runtime_turn_id: RuntimeTurnId,
    pub runtime_item_id: RuntimeItemId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceModulePresentationActorKind {
    AgentTool,
    User,
    System,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WorkspaceModulePresentationActor {
    pub kind: WorkspaceModulePresentationActorKind,
    pub actor_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WorkspaceModulePresentationCurrentnessFence {
    pub runtime_thread_id: RuntimeThreadId,
    pub source_ref: RuntimeSourceRef,
    pub surface_revision: SurfaceRevision,
    pub module_id: String,
    pub view_key: String,
    pub renderer_kind: String,
    pub presentation_uri: String,
}

impl WorkspaceModulePresentationCurrentnessFence {
    fn matches(&self, presentation: &WorkspaceModulePresentation) -> bool {
        self.module_id == presentation.module_id
            && self.view_key == presentation.view_key
            && self.renderer_kind == presentation.renderer_kind
            && self.presentation_uri == presentation.presentation_uri
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WorkspaceModulePresentationIntent {
    pub intent_id: WorkspaceModulePresentationIntentId,
    pub effect_id: WorkspaceModulePresentationEffectId,
    pub target: AgentRunTarget,
    pub actor: WorkspaceModulePresentationActor,
    pub cause: WorkspaceModulePresentationCause,
    pub currentness_fence: WorkspaceModulePresentationCurrentnessFence,
    pub presentation_digest: RuntimePayloadDigest,
    pub presentation: WorkspaceModulePresentation,
    pub committed_at_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceModulePresentationIntentStatus {
    Pending,
    Fulfilled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WorkspaceModulePresentationAcknowledgement {
    pub ack_id: WorkspaceModulePresentationAckId,
    pub target: AgentRunTarget,
    pub intent_id: WorkspaceModulePresentationIntentId,
    pub effect_id: WorkspaceModulePresentationEffectId,
    pub acknowledged_change_sequence: WorkspaceModulePresentationChangeSequence,
    pub fulfilled_at_ms: u64,
}

impl WorkspaceModulePresentationIntent {
    pub fn validate(&self) -> Result<(), WorkspaceModulePresentationProtocolError> {
        if self.actor.actor_id.trim().is_empty() {
            return Err(WorkspaceModulePresentationProtocolError::EmptyIdentity(
                "actor_id",
            ));
        }
        if self.cause.runtime_thread_id != self.currentness_fence.runtime_thread_id {
            return Err(WorkspaceModulePresentationProtocolError::BindingFenceMismatch);
        }
        if !self.currentness_fence.matches(&self.presentation) {
            return Err(WorkspaceModulePresentationProtocolError::CurrentnessFenceMismatch);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WorkspaceModulePresentationHead {
    pub target: AgentRunTarget,
    pub revision: WorkspaceModulePresentationRevision,
    pub latest_change_sequence: WorkspaceModulePresentationChangeSequence,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WorkspaceModulePresentationChange {
    pub change_id: WorkspaceModulePresentationChangeId,
    pub target: AgentRunTarget,
    pub sequence: WorkspaceModulePresentationChangeSequence,
    pub revision: WorkspaceModulePresentationRevision,
    pub status: WorkspaceModulePresentationIntentStatus,
    pub intent: WorkspaceModulePresentationIntent,
    pub acknowledgement: Option<WorkspaceModulePresentationAcknowledgement>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WorkspaceModulePresentationOutboxEntry {
    pub effect_id: WorkspaceModulePresentationEffectId,
    pub change_id: WorkspaceModulePresentationChangeId,
    pub target: AgentRunTarget,
    pub sequence: WorkspaceModulePresentationChangeSequence,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WorkspaceModulePresentationCommit {
    pub expected_revision: WorkspaceModulePresentationRevision,
    pub change: WorkspaceModulePresentationChange,
    pub outbox: WorkspaceModulePresentationOutboxEntry,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WorkspaceModulePresentationCommand {
    pub effect_id: WorkspaceModulePresentationEffectId,
    pub target: AgentRunTarget,
    pub actor: WorkspaceModulePresentationActor,
    pub cause: WorkspaceModulePresentationCause,
    pub source_ref: RuntimeSourceRef,
    pub surface_revision: SurfaceRevision,
    pub presentation: WorkspaceModulePresentation,
    pub committed_at_ms: u64,
}

pub fn build_pending_workspace_module_presentation_commit(
    head: &WorkspaceModulePresentationHead,
    command: WorkspaceModulePresentationCommand,
) -> Result<WorkspaceModulePresentationCommit, WorkspaceModulePresentationProtocolError> {
    if head.target != command.target {
        return Err(WorkspaceModulePresentationProtocolError::TargetMismatch);
    }
    let sequence = WorkspaceModulePresentationChangeSequence(
        head.revision
            .0
            .checked_add(1)
            .ok_or(WorkspaceModulePresentationProtocolError::RevisionExhausted)?,
    );
    let effect_suffix = command.effect_id.as_str();
    let change_id = WorkspaceModulePresentationChangeId::new(format!(
        "workspace-module-presentation-change:{effect_suffix}"
    ))?;
    let intent = WorkspaceModulePresentationIntent {
        intent_id: WorkspaceModulePresentationIntentId::new(format!(
            "workspace-module-presentation-intent:{effect_suffix}"
        ))?,
        effect_id: command.effect_id.clone(),
        target: command.target.clone(),
        actor: command.actor,
        cause: command.cause.clone(),
        currentness_fence: WorkspaceModulePresentationCurrentnessFence {
            runtime_thread_id: command.cause.runtime_thread_id,
            source_ref: command.source_ref,
            surface_revision: command.surface_revision,
            module_id: command.presentation.module_id.clone(),
            view_key: command.presentation.view_key.clone(),
            renderer_kind: command.presentation.renderer_kind.clone(),
            presentation_uri: command.presentation.presentation_uri.clone(),
        },
        presentation_digest: presentation_digest(&command.presentation)?,
        presentation: command.presentation,
        committed_at_ms: command.committed_at_ms,
    };
    let commit = WorkspaceModulePresentationCommit {
        expected_revision: head.revision,
        change: WorkspaceModulePresentationChange {
            change_id: change_id.clone(),
            target: command.target.clone(),
            sequence,
            revision: WorkspaceModulePresentationRevision(sequence.0),
            status: WorkspaceModulePresentationIntentStatus::Pending,
            intent,
            acknowledgement: None,
        },
        outbox: WorkspaceModulePresentationOutboxEntry {
            effect_id: command.effect_id,
            change_id,
            target: command.target,
            sequence,
        },
    };
    commit.validate()?;
    Ok(commit)
}

fn presentation_digest(
    presentation: &WorkspaceModulePresentation,
) -> Result<RuntimePayloadDigest, WorkspaceModulePresentationProtocolError> {
    let payload = serde_json::to_vec(presentation).map_err(|error| {
        WorkspaceModulePresentationProtocolError::Serialization(error.to_string())
    })?;
    RuntimePayloadDigest::new(format!("sha256:{:x}", Sha256::digest(payload)))
        .map_err(|error| WorkspaceModulePresentationProtocolError::Serialization(error.to_string()))
}

impl WorkspaceModulePresentationCommit {
    pub fn validate(&self) -> Result<(), WorkspaceModulePresentationProtocolError> {
        self.change.intent.validate()?;
        let expected_revision =
            WorkspaceModulePresentationRevision(self.expected_revision.0.saturating_add(1));
        if self.change.revision != expected_revision {
            return Err(WorkspaceModulePresentationProtocolError::RevisionNotContiguous);
        }
        let expected_sequence =
            WorkspaceModulePresentationChangeSequence(self.expected_revision.0.saturating_add(1));
        if self.change.sequence != expected_sequence {
            return Err(WorkspaceModulePresentationProtocolError::SequenceNotContiguous);
        }
        if self.change.target != self.change.intent.target
            || self.outbox.target != self.change.target
        {
            return Err(WorkspaceModulePresentationProtocolError::TargetMismatch);
        }
        if self.outbox.effect_id != self.change.intent.effect_id
            || self.outbox.change_id != self.change.change_id
            || self.outbox.sequence != self.change.sequence
        {
            return Err(WorkspaceModulePresentationProtocolError::OutboxMismatch);
        }
        match (self.change.status, self.change.acknowledgement.as_ref()) {
            (WorkspaceModulePresentationIntentStatus::Pending, None) => {}
            (WorkspaceModulePresentationIntentStatus::Fulfilled, Some(acknowledgement))
                if acknowledgement.target == self.change.target
                    && acknowledgement.intent_id == self.change.intent.intent_id
                    && acknowledgement.effect_id == self.change.intent.effect_id
                    && acknowledgement.acknowledged_change_sequence < self.change.sequence => {}
            _ => {
                return Err(WorkspaceModulePresentationProtocolError::AcknowledgementMismatch);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WorkspaceModulePresentationSnapshot {
    pub target: AgentRunTarget,
    pub revision: WorkspaceModulePresentationRevision,
    pub latest_change_sequence: WorkspaceModulePresentationChangeSequence,
    pub captured_at_ms: u64,
    /// 只返回仍需 UI 履行的 pending intent；fulfilled intent 不会再次触发打开。
    pub pending_intents: Vec<WorkspaceModulePresentationPendingIntent>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WorkspaceModulePresentationPendingIntent {
    pub change_sequence: WorkspaceModulePresentationChangeSequence,
    pub intent: WorkspaceModulePresentationIntent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WorkspaceModulePresentationChangeGap {
    pub requested_after: Option<WorkspaceModulePresentationChangeSequence>,
    pub earliest_available: WorkspaceModulePresentationChangeSequence,
    pub latest_available: WorkspaceModulePresentationChangeSequence,
    pub snapshot_revision: WorkspaceModulePresentationRevision,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WorkspaceModulePresentationChangePage {
    pub target: AgentRunTarget,
    pub changes: Vec<WorkspaceModulePresentationChange>,
    pub next: WorkspaceModulePresentationChangeSequence,
    pub gap: Option<WorkspaceModulePresentationChangeGap>,
}

fn wire_target(target: AgentRunTarget) -> wire::AgentRunProjectionTarget {
    wire::AgentRunProjectionTarget {
        run_id: target.run_id.to_string(),
        agent_id: target.agent_id.to_string(),
    }
}

impl From<WorkspaceModulePresentationIntent> for wire::WorkspaceModulePresentationIntent {
    fn from(value: WorkspaceModulePresentationIntent) -> Self {
        Self {
            intent_id: value.intent_id.0,
            effect_id: value.effect_id.0,
            target: wire_target(value.target),
            actor: wire::WorkspaceModulePresentationActor {
                kind: match value.actor.kind {
                    WorkspaceModulePresentationActorKind::AgentTool => {
                        wire::WorkspaceModulePresentationActorKind::AgentTool
                    }
                    WorkspaceModulePresentationActorKind::User => {
                        wire::WorkspaceModulePresentationActorKind::User
                    }
                    WorkspaceModulePresentationActorKind::System => {
                        wire::WorkspaceModulePresentationActorKind::System
                    }
                },
                actor_id: value.actor.actor_id,
            },
            cause: wire::WorkspaceModulePresentationCause {
                runtime_thread_id: value.cause.runtime_thread_id.to_string(),
                runtime_operation_id: value
                    .cause
                    .runtime_operation_id
                    .map(|identity| identity.to_string()),
                runtime_turn_id: value.cause.runtime_turn_id.to_string(),
                runtime_item_id: value.cause.runtime_item_id.to_string(),
            },
            currentness_fence: wire::WorkspaceModulePresentationCurrentnessFence {
                runtime_thread_id: value.currentness_fence.runtime_thread_id.to_string(),
                source_ref: value.currentness_fence.source_ref,
                surface_revision: value.currentness_fence.surface_revision.0,
                module_id: value.currentness_fence.module_id,
                view_key: value.currentness_fence.view_key,
                renderer_kind: value.currentness_fence.renderer_kind,
                presentation_uri: value.currentness_fence.presentation_uri,
            },
            presentation_digest: value.presentation_digest.to_string(),
            presentation: value.presentation,
            committed_at_ms: value.committed_at_ms,
        }
    }
}

impl From<WorkspaceModulePresentationAcknowledgement>
    for wire::WorkspaceModulePresentationAcknowledgement
{
    fn from(value: WorkspaceModulePresentationAcknowledgement) -> Self {
        Self {
            ack_id: value.ack_id.0,
            target: wire_target(value.target),
            intent_id: value.intent_id.0,
            effect_id: value.effect_id.0,
            acknowledged_change_sequence: value.acknowledged_change_sequence.0,
            fulfilled_at_ms: value.fulfilled_at_ms,
        }
    }
}

impl From<WorkspaceModulePresentationChange> for wire::WorkspaceModulePresentationChange {
    fn from(value: WorkspaceModulePresentationChange) -> Self {
        Self {
            change_id: value.change_id.0,
            target: wire_target(value.target),
            sequence: value.sequence.0,
            revision: value.revision.0,
            status: match value.status {
                WorkspaceModulePresentationIntentStatus::Pending => {
                    wire::WorkspaceModulePresentationIntentStatus::Pending
                }
                WorkspaceModulePresentationIntentStatus::Fulfilled => {
                    wire::WorkspaceModulePresentationIntentStatus::Fulfilled
                }
            },
            intent: value.intent.into(),
            acknowledgement: value.acknowledgement.map(Into::into),
        }
    }
}

impl From<WorkspaceModulePresentationSnapshot> for wire::WorkspaceModulePresentationSnapshot {
    fn from(value: WorkspaceModulePresentationSnapshot) -> Self {
        Self {
            target: wire_target(value.target),
            revision: value.revision.0,
            latest_change_sequence: value.latest_change_sequence.0,
            captured_at_ms: value.captured_at_ms,
            pending_intents: value
                .pending_intents
                .into_iter()
                .map(|pending| wire::WorkspaceModulePresentationPendingIntent {
                    change_sequence: pending.change_sequence.0,
                    intent: pending.intent.into(),
                })
                .collect(),
        }
    }
}

impl From<WorkspaceModulePresentationChangeGap> for wire::WorkspaceModulePresentationChangeGap {
    fn from(value: WorkspaceModulePresentationChangeGap) -> Self {
        Self {
            requested_after: value.requested_after.map(|sequence| sequence.0),
            earliest_available: value.earliest_available.0,
            latest_available: value.latest_available.0,
            snapshot_revision: value.snapshot_revision.0,
        }
    }
}

impl From<WorkspaceModulePresentationChangePage> for wire::WorkspaceModulePresentationChangePage {
    fn from(value: WorkspaceModulePresentationChangePage) -> Self {
        Self {
            target: wire_target(value.target),
            changes: value.changes.into_iter().map(Into::into).collect(),
            next: value.next.0,
            gap: value.gap.map(Into::into),
        }
    }
}

#[async_trait]
pub trait WorkspaceModulePresentationRepository: Send + Sync {
    async fn load_change_by_effect(
        &self,
        effect_id: &WorkspaceModulePresentationEffectId,
    ) -> Result<Option<WorkspaceModulePresentationChange>, WorkspaceModulePresentationStoreError>;

    async fn load_head(
        &self,
        target: &AgentRunTarget,
    ) -> Result<WorkspaceModulePresentationHead, WorkspaceModulePresentationStoreError>;

    async fn load_snapshot(
        &self,
        target: &AgentRunTarget,
    ) -> Result<WorkspaceModulePresentationSnapshot, WorkspaceModulePresentationStoreError>;

    async fn load_changes(
        &self,
        target: &AgentRunTarget,
        after: Option<WorkspaceModulePresentationChangeSequence>,
        limit: usize,
    ) -> Result<WorkspaceModulePresentationChangePage, WorkspaceModulePresentationStoreError>;
}

#[async_trait]
pub trait WorkspaceModulePresentationUnitOfWork: Send + Sync {
    async fn commit(
        &self,
        commit: WorkspaceModulePresentationCommit,
    ) -> Result<(), WorkspaceModulePresentationStoreError>;
}

#[async_trait]
pub trait WorkspaceModulePresentationCommandPort: Send + Sync {
    async fn present(
        &self,
        command: WorkspaceModulePresentationCommand,
    ) -> Result<WorkspaceModulePresentationChange, WorkspaceModulePresentationCommandError>;
}

pub struct WorkspaceModulePresentationCommandService {
    repository: Arc<dyn WorkspaceModulePresentationRepository>,
    unit_of_work: Arc<dyn WorkspaceModulePresentationUnitOfWork>,
}

impl WorkspaceModulePresentationCommandService {
    pub fn new(
        repository: Arc<dyn WorkspaceModulePresentationRepository>,
        unit_of_work: Arc<dyn WorkspaceModulePresentationUnitOfWork>,
    ) -> Self {
        Self {
            repository,
            unit_of_work,
        }
    }
}

#[async_trait]
impl WorkspaceModulePresentationCommandPort for WorkspaceModulePresentationCommandService {
    async fn present(
        &self,
        command: WorkspaceModulePresentationCommand,
    ) -> Result<WorkspaceModulePresentationChange, WorkspaceModulePresentationCommandError> {
        if let Some(change) = self
            .repository
            .load_change_by_effect(&command.effect_id)
            .await?
        {
            return replayed_change(change, &command);
        }
        let head = self.repository.load_head(&command.target).await?;
        let commit = build_pending_workspace_module_presentation_commit(&head, command.clone())?;
        let change = commit.change.clone();
        match self.unit_of_work.commit(commit).await {
            Ok(()) => Ok(change),
            Err(WorkspaceModulePresentationStoreError::Conflict) => {
                let replay = self
                    .repository
                    .load_change_by_effect(&command.effect_id)
                    .await?
                    .ok_or(WorkspaceModulePresentationStoreError::Conflict)?;
                replayed_change(replay, &command)
            }
            Err(error) => Err(error.into()),
        }
    }
}

fn replayed_change(
    change: WorkspaceModulePresentationChange,
    command: &WorkspaceModulePresentationCommand,
) -> Result<WorkspaceModulePresentationChange, WorkspaceModulePresentationCommandError> {
    let intent = &change.intent;
    let fence = &intent.currentness_fence;
    if intent.effect_id != command.effect_id
        || intent.target != command.target
        || intent.actor != command.actor
        || intent.cause != command.cause
        || fence.runtime_thread_id != command.cause.runtime_thread_id
        || fence.source_ref != command.source_ref
        || fence.surface_revision != command.surface_revision
        || intent.presentation != command.presentation
        || intent.presentation_digest != presentation_digest(&command.presentation)?
    {
        return Err(WorkspaceModulePresentationProtocolError::EffectConflict.into());
    }
    Ok(change)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WorkspaceModulePresentationAcknowledgeRequest {
    pub target: AgentRunTarget,
    pub intent_id: WorkspaceModulePresentationIntentId,
    pub observed_change_sequence: WorkspaceModulePresentationChangeSequence,
}

#[async_trait]
pub trait WorkspaceModulePresentationAcknowledgePort: Send + Sync {
    async fn acknowledge(
        &self,
        request: WorkspaceModulePresentationAcknowledgeRequest,
    ) -> Result<WorkspaceModulePresentationChange, WorkspaceModulePresentationStoreError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum WorkspaceModulePresentationProtocolError {
    #[error("{0} must be non-empty")]
    EmptyIdentity(&'static str),
    #[error("presentation currentness fence does not match its typed payload")]
    CurrentnessFenceMismatch,
    #[error("presentation Runtime cause differs from its committed source binding fence")]
    BindingFenceMismatch,
    #[error("presentation revision is not contiguous")]
    RevisionNotContiguous,
    #[error("presentation change sequence is not contiguous")]
    SequenceNotContiguous,
    #[error("presentation target differs across the atomic write set")]
    TargetMismatch,
    #[error("presentation outbox identity differs from the committed change")]
    OutboxMismatch,
    #[error("presentation acknowledgement does not fulfill the exact pending intent")]
    AcknowledgementMismatch,
    #[error("presentation revision is exhausted")]
    RevisionExhausted,
    #[error("presentation serialization failed: {0}")]
    Serialization(String),
    #[error("presentation effect identity was reused with different immutable facts")]
    EffectConflict,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum WorkspaceModulePresentationStoreError {
    #[error("workspace presentation projection conflict")]
    Conflict,
    #[error("workspace presentation projection persistence failed: {0}")]
    Persistence(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum WorkspaceModulePresentationCommandError {
    #[error(transparent)]
    Protocol(#[from] WorkspaceModulePresentationProtocolError),
    #[error(transparent)]
    Store(#[from] WorkspaceModulePresentationStoreError),
}

pub const WORKSPACE_MODULE_PRESENTATION_PERSISTENCE_CONSTRAINTS: &[&str] = &[
    "one head row per (target_run_id, target_agent_id), advanced by exact revision CAS",
    "unique(target_run_id, target_agent_id, revision)",
    "unique(target_run_id, target_agent_id, change_sequence)",
    "unique(intent_id)",
    "unique(effect_id)",
    "unique(change_id)",
    "unique(ack_id)",
    "intent digest, module/view coordinates, actor, Runtime cause, and binding fence are immutable",
    "pending intents are retained until an idempotent UI acknowledgement commits fulfilled status",
    "intent, change, and outbox commit in one transaction under exact revision CAS",
];

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn commit() -> WorkspaceModulePresentationCommit {
        let target = AgentRunTarget {
            run_id: Uuid::nil(),
            agent_id: Uuid::max(),
        };
        let effect_id =
            WorkspaceModulePresentationEffectId::new("workspace-present:item-1").expect("effect");
        let change_id = WorkspaceModulePresentationChangeId::new("workspace-present-change:item-1")
            .expect("change");
        let presentation = WorkspaceModulePresentation {
            module_id: "canvas:one".to_string(),
            view_key: "preview".to_string(),
            renderer_kind: "canvas".to_string(),
            presentation_uri: "canvas://one".to_string(),
            title: "Canvas".to_string(),
            payload: None,
            diagnostics: None,
        };
        let intent = WorkspaceModulePresentationIntent {
            intent_id: WorkspaceModulePresentationIntentId::new("workspace-present-intent:item-1")
                .expect("intent"),
            effect_id: effect_id.clone(),
            target: target.clone(),
            actor: WorkspaceModulePresentationActor {
                kind: WorkspaceModulePresentationActorKind::AgentTool,
                actor_id: "agent-1".to_string(),
            },
            cause: WorkspaceModulePresentationCause {
                runtime_thread_id: RuntimeThreadId::new("thread-1").expect("thread"),
                runtime_operation_id: None,
                runtime_turn_id: RuntimeTurnId::new("turn-1").expect("turn"),
                runtime_item_id: RuntimeItemId::new("item-1").expect("item"),
            },
            currentness_fence: WorkspaceModulePresentationCurrentnessFence {
                runtime_thread_id: RuntimeThreadId::new("thread-1").expect("thread"),
                source_ref: RuntimeSourceRef::new("source-1").expect("source"),
                surface_revision: SurfaceRevision(3),
                module_id: presentation.module_id.clone(),
                view_key: presentation.view_key.clone(),
                renderer_kind: presentation.renderer_kind.clone(),
                presentation_uri: presentation.presentation_uri.clone(),
            },
            presentation_digest: RuntimePayloadDigest::new("sha256:presentation-1")
                .expect("digest"),
            presentation,
            committed_at_ms: 1,
        };
        WorkspaceModulePresentationCommit {
            expected_revision: WorkspaceModulePresentationRevision(4),
            change: WorkspaceModulePresentationChange {
                change_id: change_id.clone(),
                target: target.clone(),
                sequence: WorkspaceModulePresentationChangeSequence(5),
                revision: WorkspaceModulePresentationRevision(5),
                status: WorkspaceModulePresentationIntentStatus::Pending,
                intent,
                acknowledgement: None,
            },
            outbox: WorkspaceModulePresentationOutboxEntry {
                effect_id,
                change_id,
                target,
                sequence: WorkspaceModulePresentationChangeSequence(5),
            },
        }
    }

    #[test]
    fn pending_command_builds_one_contiguous_typed_commit() {
        let fixture = commit();
        let target = fixture.change.target.clone();
        let presentation = fixture.change.intent.presentation.clone();
        let command = WorkspaceModulePresentationCommand {
            effect_id: WorkspaceModulePresentationEffectId::new("effect-command").expect("effect"),
            target: target.clone(),
            actor: WorkspaceModulePresentationActor {
                kind: WorkspaceModulePresentationActorKind::AgentTool,
                actor_id: "service-instance".to_owned(),
            },
            cause: WorkspaceModulePresentationCause {
                runtime_thread_id: RuntimeThreadId::new("thread-command").expect("thread"),
                runtime_operation_id: None,
                runtime_turn_id: RuntimeTurnId::new("turn-command").expect("turn"),
                runtime_item_id: RuntimeItemId::new("item-command").expect("item"),
            },
            source_ref: RuntimeSourceRef::new("source-command").expect("source"),
            surface_revision: SurfaceRevision(8),
            presentation: presentation.clone(),
            committed_at_ms: 10,
        };

        let commit = build_pending_workspace_module_presentation_commit(
            &WorkspaceModulePresentationHead {
                target,
                revision: WorkspaceModulePresentationRevision(4),
                latest_change_sequence: WorkspaceModulePresentationChangeSequence(4),
            },
            command,
        )
        .expect("canonical commit");

        assert_eq!(
            commit.change.revision,
            WorkspaceModulePresentationRevision(5)
        );
        assert_eq!(
            commit.change.sequence,
            WorkspaceModulePresentationChangeSequence(5)
        );
        assert_eq!(commit.change.intent.presentation, presentation);
        assert_eq!(
            commit.change.intent.currentness_fence.surface_revision,
            SurfaceRevision(8)
        );
        assert_eq!(
            commit.change.intent.presentation_digest.as_str().len(),
            "sha256:".len() + 64
        );
        commit.validate().expect("valid atomic write set");
    }

    #[test]
    fn validates_one_atomic_intent_change_outbox_write_set() {
        commit().validate().expect("valid commit");
    }

    #[test]
    fn rejects_payload_currentness_drift() {
        let mut commit = commit();
        commit.change.intent.presentation.presentation_uri = "canvas://other".to_string();
        assert_eq!(
            commit.validate(),
            Err(WorkspaceModulePresentationProtocolError::CurrentnessFenceMismatch)
        );
    }

    #[test]
    fn rejects_outbox_identity_drift() {
        let mut commit = commit();
        commit.outbox.effect_id =
            WorkspaceModulePresentationEffectId::new("other").expect("effect");
        assert_eq!(
            commit.validate(),
            Err(WorkspaceModulePresentationProtocolError::OutboxMismatch)
        );
    }

    #[test]
    fn fulfilled_change_requires_an_ack_for_the_exact_pending_intent() {
        let mut commit = commit();
        commit.change.status = WorkspaceModulePresentationIntentStatus::Fulfilled;
        commit.change.acknowledgement = Some(WorkspaceModulePresentationAcknowledgement {
            ack_id: WorkspaceModulePresentationAckId::new("ui-ack:intent-1").expect("ack"),
            target: commit.change.target.clone(),
            intent_id: commit.change.intent.intent_id.clone(),
            effect_id: commit.change.intent.effect_id.clone(),
            acknowledged_change_sequence: WorkspaceModulePresentationChangeSequence(4),
            fulfilled_at_ms: 2,
        });
        commit.validate().expect("exact acknowledgement");

        commit
            .change
            .acknowledgement
            .as_mut()
            .expect("ack")
            .intent_id = WorkspaceModulePresentationIntentId::new("other-intent").expect("intent");
        assert_eq!(
            commit.validate(),
            Err(WorkspaceModulePresentationProtocolError::AcknowledgementMismatch)
        );
    }
}
