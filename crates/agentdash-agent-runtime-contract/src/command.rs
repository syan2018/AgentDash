use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
    BindingEpoch, BoundRuntimeHookPlan, ContextCheckpointId, ContextCompactionId,
    ContextCompactionTrigger, DriverThreadId, IdempotencyKey, ProfileDigest, RuntimeBindingId,
    RuntimeDriverGeneration, RuntimeInteractionId, RuntimeOperationId, RuntimeProfile,
    RuntimeRecoveryIntentId, RuntimeRevision, RuntimeThreadId, RuntimeTurnId, SurfaceDigest,
    ToolSetRevision,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeActor {
    User { subject: String },
    Agent { name: String },
    System { component: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct OperationMeta {
    pub operation_id: RuntimeOperationId,
    pub idempotency_key: IdempotencyKey,
    pub expected_thread_revision: Option<RuntimeRevision>,
    pub actor: RuntimeActor,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuntimeInput {
    Text {
        text: String,
    },
    Image {
        mime_type: String,
        data_url: String,
    },
    FileReference {
        uri: String,
        media_type: Option<String>,
    },
    Structured {
        schema: String,
        value: serde_json::Value,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InteractionResponse {
    Approved,
    Denied { reason: Option<String> },
    UserInput { input: Vec<RuntimeInput> },
    DynamicToolResult { output: serde_json::Value },
    McpElicitation { value: serde_json::Value },
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeCommandKind {
    ThreadStart,
    ThreadResume,
    ThreadRebind,
    ThreadFork,
    ThreadSettingsUpdate,
    TurnStart,
    TurnSteer,
    TurnInterrupt,
    InteractionRespond,
    ContextCompact,
    ToolSetReplace,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuntimeCommand {
    ThreadStart {
        thread_id: RuntimeThreadId,
        binding_id: RuntimeBindingId,
        driver_generation: RuntimeDriverGeneration,
        source_thread_id: crate::DriverThreadId,
        profile_digest: ProfileDigest,
        bound_profile: Box<RuntimeProfile>,
        input: Vec<RuntimeInput>,
        surface_digest: SurfaceDigest,
        settings_revision: crate::ThreadSettingsRevision,
        tool_set_revision: crate::ToolSetRevision,
        hook_plan: BoundRuntimeHookPlan,
    },
    ThreadResume {
        thread_id: RuntimeThreadId,
    },
    ThreadRebind {
        thread_id: RuntimeThreadId,
        recovery_intent_id: RuntimeRecoveryIntentId,
        binding_epoch: BindingEpoch,
        expected_binding_id: RuntimeBindingId,
        expected_driver_generation: RuntimeDriverGeneration,
        new_binding_id: RuntimeBindingId,
        new_driver_generation: RuntimeDriverGeneration,
        source_thread_id: DriverThreadId,
        profile_digest: ProfileDigest,
        bound_profile: Box<RuntimeProfile>,
    },
    ThreadFork {
        thread_id: RuntimeThreadId,
        checkpoint_id: Option<ContextCheckpointId>,
    },
    ThreadSettingsUpdate {
        thread_id: RuntimeThreadId,
        instructions: Vec<String>,
    },
    TurnStart {
        thread_id: RuntimeThreadId,
        input: Vec<RuntimeInput>,
    },
    TurnSteer {
        thread_id: RuntimeThreadId,
        expected_turn_id: RuntimeTurnId,
        input: Vec<RuntimeInput>,
    },
    TurnInterrupt {
        thread_id: RuntimeThreadId,
        expected_turn_id: RuntimeTurnId,
    },
    InteractionRespond {
        thread_id: RuntimeThreadId,
        interaction_id: RuntimeInteractionId,
        response: InteractionResponse,
    },
    ContextCompact {
        thread_id: RuntimeThreadId,
        compaction_id: ContextCompactionId,
        trigger: ContextCompactionTrigger,
        base_checkpoint_id: Option<ContextCheckpointId>,
        expected_context_revision: crate::ContextRevision,
    },
    ToolSetReplace {
        thread_id: RuntimeThreadId,
        expected_tool_set_revision: ToolSetRevision,
        tool_set_digest: String,
    },
}

impl RuntimeCommand {
    pub fn kind(&self) -> RuntimeCommandKind {
        match self {
            Self::ThreadStart { .. } => RuntimeCommandKind::ThreadStart,
            Self::ThreadResume { .. } => RuntimeCommandKind::ThreadResume,
            Self::ThreadRebind { .. } => RuntimeCommandKind::ThreadRebind,
            Self::ThreadFork { .. } => RuntimeCommandKind::ThreadFork,
            Self::ThreadSettingsUpdate { .. } => RuntimeCommandKind::ThreadSettingsUpdate,
            Self::TurnStart { .. } => RuntimeCommandKind::TurnStart,
            Self::TurnSteer { .. } => RuntimeCommandKind::TurnSteer,
            Self::TurnInterrupt { .. } => RuntimeCommandKind::TurnInterrupt,
            Self::InteractionRespond { .. } => RuntimeCommandKind::InteractionRespond,
            Self::ContextCompact { .. } => RuntimeCommandKind::ContextCompact,
            Self::ToolSetReplace { .. } => RuntimeCommandKind::ToolSetReplace,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeCommandEnvelope {
    pub meta: OperationMeta,
    pub command: RuntimeCommand,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct OperationReceipt {
    pub operation_id: RuntimeOperationId,
    pub operation_sequence: crate::OperationSequence,
    pub thread_id: Option<RuntimeThreadId>,
    pub accepted_revision: RuntimeRevision,
    pub duplicate: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuntimeSnapshotQuery {
    Operation {
        operation_id: RuntimeOperationId,
    },
    Thread {
        thread_id: RuntimeThreadId,
        at_revision: Option<RuntimeRevision>,
    },
    Context {
        thread_id: RuntimeThreadId,
        at_context_revision: Option<crate::ContextRevision>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeEventSubscription {
    pub thread_id: RuntimeThreadId,
    pub after: Option<crate::EventSequence>,
    pub include_transient: bool,
}
