use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
    ActiveContextHeadView, CommandAvailability, ContextBlock, ContextCheckpointId,
    ContextCheckpointView, ContextFidelity, ContextRevision, ProfileDigest, RuntimeBindingId,
    RuntimeCommandKind, RuntimeInteractionId, RuntimeProfile, RuntimeRevision, RuntimeThreadId,
    RuntimeThreadStatus, RuntimeTurnId, ThreadSettingsRevision, ToolSetRevision,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeSnapshot {
    pub thread_id: RuntimeThreadId,
    pub revision: RuntimeRevision,
    pub status: RuntimeThreadStatus,
    pub active_turn_id: Option<RuntimeTurnId>,
    pub binding_id: RuntimeBindingId,
    pub profile_digest: ProfileDigest,
    pub bound_profile: RuntimeProfile,
    pub active_checkpoint_id: Option<ContextCheckpointId>,
    pub context_revision: ContextRevision,
    pub settings_revision: ThreadSettingsRevision,
    pub tool_set_revision: ToolSetRevision,
    pub pending_interactions: Vec<RuntimeInteractionId>,
    pub command_availability: BTreeMap<RuntimeCommandKind, CommandAvailability>,
    pub transcript: Vec<RuntimeTranscriptItem>,
    pub transcript_fidelity: ContextFidelity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeTranscriptItem {
    pub turn_id: RuntimeTurnId,
    pub item_id: crate::RuntimeItemId,
    pub final_content: crate::RuntimeItemContent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeContextView {
    pub thread_id: RuntimeThreadId,
    pub head: Option<ActiveContextHeadView>,
    pub checkpoint: Option<ContextCheckpointView>,
    pub blocks: Vec<ContextBlock>,
    pub fidelity: ContextFidelity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuntimeSnapshotResult {
    Thread { snapshot: Box<RuntimeSnapshot> },
    Context { context: Box<RuntimeContextView> },
}
