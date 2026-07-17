use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
    ActiveContextHeadView, BindingEpoch, CommandAvailability, ContextBlock, ContextCheckpointId,
    ContextCheckpointView, ContextFidelity, ContextRevision, EventSequence, IdempotencyKey,
    OperationReceipt, ProfileDigest, RuntimeActor, RuntimeBindingId, RuntimeCommand,
    RuntimeCommandKind, RuntimeInteractionId, RuntimeInteractionRequest, RuntimeItemId,
    RuntimeOperationId, RuntimeOperationTerminal, RuntimeProfile, RuntimeRevision, RuntimeThreadId,
    RuntimeThreadStatus, RuntimeTurnId, ThreadSettingsRevision, ToolSetRevision,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeSnapshot {
    pub thread_id: RuntimeThreadId,
    pub revision: RuntimeRevision,
    /// Durable journal cursor atomically represented by this snapshot.
    pub latest_event_sequence: EventSequence,
    /// Authoritative time at which this presentation baseline was materialized.
    pub captured_at_ms: u64,
    pub status: RuntimeThreadStatus,
    pub thread_name: Option<String>,
    pub active_turn_id: Option<RuntimeTurnId>,
    /// Main-compatible source-session turn identity paired with the canonical active turn.
    pub active_presentation_turn_id: Option<crate::PresentationTurnId>,
    pub binding_id: RuntimeBindingId,
    pub binding_epoch: BindingEpoch,
    pub profile_digest: ProfileDigest,
    pub bound_profile: RuntimeProfile,
    pub surface: crate::RuntimeSurfaceDescriptor,
    pub active_checkpoint_id: Option<ContextCheckpointId>,
    pub context_revision: ContextRevision,
    pub settings_revision: ThreadSettingsRevision,
    pub tool_set_revision: ToolSetRevision,
    pub pending_interactions: Vec<RuntimeInteractionId>,
    #[serde(default)]
    pub pending_interaction_details: Vec<PendingRuntimeInteractionView>,
    pub command_availability: BTreeMap<RuntimeCommandKind, CommandAvailability>,
    pub transcript: Vec<RuntimeTranscriptItem>,
    pub transcript_fidelity: ContextFidelity,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct PendingRuntimeInteractionView {
    pub interaction_id: RuntimeInteractionId,
    pub turn_id: RuntimeTurnId,
    pub item_id: Option<RuntimeItemId>,
    pub request: RuntimeInteractionRequest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeTranscriptItem {
    pub source_thread_id: String,
    pub source_turn_id: String,
    pub source_item_id: String,
    /// The complete terminal presentation fact is retained verbatim. Consumers
    /// must not reconstruct a session event from a narrower transcript shape.
    pub terminal_event: crate::ImmutablePresentationEvent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeContextView {
    pub thread_id: RuntimeThreadId,
    pub head: Option<ActiveContextHeadView>,
    pub checkpoint: Option<ContextCheckpointView>,
    pub blocks: Vec<ContextBlock>,
    pub fidelity: ContextFidelity,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeOperationView {
    pub operation_id: RuntimeOperationId,
    pub idempotency_key: IdempotencyKey,
    pub actor: RuntimeActor,
    pub presentation: Vec<crate::RuntimePresentationInput>,
    pub command: RuntimeCommand,
    pub receipt: OperationReceipt,
    pub terminal: Option<RuntimeOperationTerminal>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuntimeSnapshotResult {
    Operation {
        operation: Box<RuntimeOperationView>,
    },
    Thread {
        snapshot: Box<RuntimeSnapshot>,
    },
    Context {
        context: Box<RuntimeContextView>,
    },
}
