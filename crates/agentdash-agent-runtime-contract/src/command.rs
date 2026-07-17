use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
    BindingEpoch, BoundRuntimeHookPlan, ContextCheckpointId, ContextCompactionId,
    ContextCompactionTrigger, ContextDigest, ContextRecipeRevision, DriverThreadId, IdempotencyKey,
    ProfileDigest, RuntimeBindingId, RuntimeDriverGeneration, RuntimeInteractionId,
    RuntimeOperationId, RuntimeProfile, RuntimeRecoveryIntentId, RuntimeRevision, RuntimeThreadId,
    RuntimeTurnId, SurfaceDigest, SurfaceRevision, ToolSetRevision,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeSurfaceDescriptor {
    pub source_frame_id: String,
    pub surface_revision: SurfaceRevision,
    pub surface_digest: SurfaceDigest,
    pub vfs_digest: String,
    pub context_recipe_revision: ContextRecipeRevision,
    pub context_digest: ContextDigest,
    pub settings_revision: crate::ThreadSettingsRevision,
    pub tool_set_revision: ToolSetRevision,
    pub tool_set_digest: String,
    pub hook_plan: BoundRuntimeHookPlan,
    pub terminal_hook_effect_binding: Option<crate::RuntimeTerminalHookEffectBinding>,
}

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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuntimeInput {
    UserInput {
        block: agentdash_agent_protocol::UserInputBlock,
    },
    Structured {
        schema: String,
        value: serde_json::Value,
    },
}

impl RuntimeInput {
    pub fn user_input(block: agentdash_agent_protocol::UserInputBlock) -> Self {
        Self::UserInput { block }
    }

    pub fn text(text: impl Into<String>) -> Self {
        Self::user_input(agentdash_agent_protocol::text_user_input_block(text))
    }

    pub fn modality(&self) -> crate::InputModality {
        match self {
            Self::UserInput { block } => match block {
                agentdash_agent_protocol::UserInputBlock::Text { .. } => crate::InputModality::Text,
                agentdash_agent_protocol::UserInputBlock::Image { .. } => {
                    crate::InputModality::Image
                }
                agentdash_agent_protocol::UserInputBlock::LocalImage { .. } => {
                    crate::InputModality::LocalImage
                }
                agentdash_agent_protocol::UserInputBlock::Skill { .. } => {
                    crate::InputModality::Skill
                }
                agentdash_agent_protocol::UserInputBlock::Mention { .. } => {
                    crate::InputModality::Mention
                }
            },
            Self::Structured { .. } => crate::InputModality::Structured,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
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
    SurfaceAdopt,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuntimeCommand {
    ThreadStart {
        thread_id: RuntimeThreadId,
        presentation_thread_id: crate::PresentationThreadId,
        presentation_turn_id: Option<crate::PresentationTurnId>,
        binding_id: RuntimeBindingId,
        driver_generation: RuntimeDriverGeneration,
        source_thread_id: crate::DriverThreadId,
        profile_digest: ProfileDigest,
        bound_profile: Box<RuntimeProfile>,
        input: Vec<RuntimeInput>,
        surface: Box<RuntimeSurfaceDescriptor>,
        settings_revision: crate::ThreadSettingsRevision,
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
        presentation_turn_id: crate::PresentationTurnId,
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
        expected_current_tool_set_revision: ToolSetRevision,
        target_tool_set_revision: ToolSetRevision,
        tool_set_digest: String,
    },
    SurfaceAdopt {
        thread_id: RuntimeThreadId,
        expected_surface_revision: SurfaceRevision,
        expected_surface_digest: SurfaceDigest,
        target: Box<RuntimeSurfaceDescriptor>,
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
            Self::SurfaceAdopt { .. } => RuntimeCommandKind::SurfaceAdopt,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeCommandEnvelope {
    pub meta: OperationMeta,
    pub presentation: Vec<crate::RuntimePresentationInput>,
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
    /// Optional cursor for bounded replay of the currently active live stream.
    pub transient_after: Option<crate::RuntimeTransientSequence>,
    /// Rejects replay from an obsolete binding generation after reconnect or target switch.
    pub stream_generation: Option<crate::RuntimeDriverGeneration>,
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::{RuntimeCommand, RuntimeInput};
    use crate::{InputModality, PresentationTurnId, RuntimeThreadId};

    #[test]
    fn runtime_user_input_round_trip_preserves_the_complete_codex_shape() {
        let fixtures = [
            (
                serde_json::json!({
                    "type": "text",
                    "text": "ask @agent",
                    "text_elements": [{
                        "byteRange": { "start": 4, "end": 10 },
                        "placeholder": null
                    }]
                }),
                InputModality::Text,
            ),
            (
                serde_json::json!({ "type": "image", "url": "https://example.test/absent.png" }),
                InputModality::Image,
            ),
            (
                serde_json::json!({ "type": "image", "detail": null, "url": "https://example.test/null.png" }),
                InputModality::Image,
            ),
            (
                serde_json::json!({ "type": "image", "detail": "low", "url": "https://example.test/low.png" }),
                InputModality::Image,
            ),
            (
                serde_json::json!({ "type": "localImage", "detail": "original", "path": "C:/workspace/image.png" }),
                InputModality::LocalImage,
            ),
            (
                serde_json::json!({ "type": "skill", "name": "review", "path": "C:/skills/review/SKILL.md" }),
                InputModality::Skill,
            ),
            (
                serde_json::json!({ "type": "mention", "name": "main.rs", "path": "C:/workspace/src/main.rs" }),
                InputModality::Mention,
            ),
        ];

        for (block_json, modality) in fixtures {
            let block = serde_json::from_value(block_json.clone()).expect("Codex UserInput");
            let runtime_input = RuntimeInput::user_input(block);
            assert_eq!(runtime_input.modality(), modality);

            let wire = serde_json::to_value(&runtime_input).expect("serialize RuntimeInput");
            assert_eq!(wire["kind"], "user_input");
            assert_eq!(wire["block"], block_json);

            let decoded: RuntimeInput =
                serde_json::from_value(wire).expect("deserialize RuntimeInput");
            assert_eq!(decoded, runtime_input);
        }
    }

    #[test]
    fn turn_start_preserves_non_empty_presentation_turn_identity() {
        let command = RuntimeCommand::TurnStart {
            thread_id: RuntimeThreadId::from_str("runtime-thread-roundtrip")
                .expect("runtime thread id"),
            presentation_turn_id: PresentationTurnId::from_str("presentation-turn-roundtrip")
                .expect("presentation turn id"),
            input: Vec::new(),
        };

        let encoded = serde_json::to_value(&command).expect("serialize turn start");
        assert_eq!(
            encoded
                .get("presentation_turn_id")
                .and_then(|value| value.as_str()),
            Some("presentation-turn-roundtrip")
        );
        let decoded: RuntimeCommand =
            serde_json::from_value(encoded).expect("deserialize turn start");
        assert_eq!(decoded, command);
    }
}
