use agentdash_agent_runtime_contract::{
    RuntimeBindingId, RuntimeDriverGeneration, RuntimeItemId, RuntimeOperationId,
    RuntimePayloadDigest, RuntimeThreadId, RuntimeTurnId, SurfaceRevision,
};
use agentdash_application_ports::agent_run_runtime::AgentRunRuntimeTarget;
use agentdash_contracts::workspace_module::WorkspaceModulePresentation;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

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
    pub binding_id: RuntimeBindingId,
    pub binding_generation: RuntimeDriverGeneration,
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
    pub target: AgentRunRuntimeTarget,
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
    pub target: AgentRunRuntimeTarget,
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
        if !self.currentness_fence.matches(&self.presentation) {
            return Err(WorkspaceModulePresentationProtocolError::CurrentnessFenceMismatch);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WorkspaceModulePresentationHead {
    pub target: AgentRunRuntimeTarget,
    pub revision: WorkspaceModulePresentationRevision,
    pub latest_change_sequence: WorkspaceModulePresentationChangeSequence,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WorkspaceModulePresentationChange {
    pub change_id: WorkspaceModulePresentationChangeId,
    pub target: AgentRunRuntimeTarget,
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
    pub target: AgentRunRuntimeTarget,
    pub sequence: WorkspaceModulePresentationChangeSequence,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WorkspaceModulePresentationCommit {
    pub expected_revision: WorkspaceModulePresentationRevision,
    pub change: WorkspaceModulePresentationChange,
    pub outbox: WorkspaceModulePresentationOutboxEntry,
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
    pub target: AgentRunRuntimeTarget,
    pub revision: WorkspaceModulePresentationRevision,
    pub latest_change_sequence: WorkspaceModulePresentationChangeSequence,
    pub captured_at_ms: u64,
    /// 只返回仍需 UI 履行的 pending intent；fulfilled intent 不会再次触发打开。
    pub pending_intents: Vec<WorkspaceModulePresentationIntent>,
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
    pub target: AgentRunRuntimeTarget,
    pub changes: Vec<WorkspaceModulePresentationChange>,
    pub next: WorkspaceModulePresentationChangeSequence,
    pub gap: Option<WorkspaceModulePresentationChangeGap>,
}

#[async_trait]
pub trait WorkspaceModulePresentationRepository: Send + Sync {
    async fn load_head(
        &self,
        target: &AgentRunRuntimeTarget,
    ) -> Result<WorkspaceModulePresentationHead, WorkspaceModulePresentationStoreError>;

    async fn load_snapshot(
        &self,
        target: &AgentRunRuntimeTarget,
    ) -> Result<WorkspaceModulePresentationSnapshot, WorkspaceModulePresentationStoreError>;

    async fn load_changes(
        &self,
        target: &AgentRunRuntimeTarget,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WorkspaceModulePresentationAcknowledgeRequest {
    pub target: AgentRunRuntimeTarget,
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
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum WorkspaceModulePresentationStoreError {
    #[error("workspace presentation projection conflict")]
    Conflict,
    #[error("workspace presentation projection persistence failed: {0}")]
    Persistence(String),
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
        let target = AgentRunRuntimeTarget {
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
                binding_id: RuntimeBindingId::new("binding-1").expect("binding"),
                binding_generation: RuntimeDriverGeneration(2),
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
