use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use agentdash_agent_protocol::CanonicalConversationRecord;

use crate::{
    AgentObservation, AgentObservationState, AgentSnapshotRevision, RuntimeSourceRef,
    RuntimeThreadId, RuntimeU64,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum AgentRuntimeOperationStatus {
    Accepted,
    Running,
    Succeeded,
    Failed,
    Interrupted,
    Lost,
}

impl AgentRuntimeOperationStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Interrupted => "interrupted",
            Self::Lost => "lost",
        }
    }
}

/// Product-owned evidence that a Runtime thread is bound to a source.
///
/// This is not part of the source observation. Binding revisions and launch evidence remain in
/// the Product aggregate; this value only fences consumers to the selected source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRuntimeSourceBindingEvidence {
    pub source_ref: RuntimeSourceRef,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "snake_case")]
pub enum AgentRuntimeCommandKind {
    Create,
    Resume,
    Rebind,
    Activate,
    SubmitInput,
    Steer,
    Interrupt,
    RequestCompaction,
    ResolveInteraction,
    Close,
    Fork,
}

impl AgentRuntimeCommandKind {
    pub const ALL: [Self; 11] = [
        Self::Create,
        Self::Resume,
        Self::Rebind,
        Self::Activate,
        Self::SubmitInput,
        Self::Steer,
        Self::Interrupt,
        Self::RequestCompaction,
        Self::ResolveInteraction,
        Self::Close,
        Self::Fork,
    ];
}

/// Browser-safe Product wrapper around one canonical source observation.
///
/// The wrapper adds only Product identity. Every execution, context, interaction, control and
/// presentation fact remains owned by `AgentObservation` and is shared with `AgentSnapshot`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRuntimeView {
    pub thread_id: RuntimeThreadId,
    pub observation: AgentObservation,
}

impl AgentRuntimeView {
    pub const fn view_revision(&self) -> AgentSnapshotRevision {
        self.observation.revision
    }

    pub fn conversation(&self) -> agentdash_agent_protocol::CanonicalConversationView<'_> {
        self.observation.conversation()
    }

    pub fn active_turn_id(&self) -> Option<&str> {
        self.observation.active_turn_id()
    }
}

/// Live Runtime lane update. State is present only when the source owner publishes a state
/// transition at the same boundary as these presentations. Ephemeral updates never trigger an
/// authoritative read.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRuntimeUpdate {
    #[serde(with = "crate::wire_u64")]
    #[schemars(with = "crate::wire_u64::RuntimeU64")]
    #[ts(type = "RuntimeU64")]
    pub lane_sequence: u64,
    pub state: Option<AgentObservationState>,
    #[ts(type = "Array<CanonicalConversationRecord>")]
    pub presentations: Vec<CanonicalConversationRecord>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum AgentRuntimeResetReason {
    Lagged,
    SequenceGap,
    SourceMismatch,
    ProtocolError,
    BindingReplaced,
    TransportDisconnected,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
pub enum AgentRuntimeStreamFrame {
    Baseline {
        #[serde(with = "crate::wire_u64")]
        #[schemars(with = "crate::wire_u64::RuntimeU64")]
        #[ts(type = "RuntimeU64")]
        connection_epoch: u64,
        view: AgentRuntimeView,
    },
    Update {
        #[serde(with = "crate::wire_u64")]
        #[schemars(with = "crate::wire_u64::RuntimeU64")]
        #[ts(type = "RuntimeU64")]
        connection_epoch: u64,
        #[serde(with = "crate::wire_u64")]
        #[schemars(with = "crate::wire_u64::RuntimeU64")]
        #[ts(type = "RuntimeU64")]
        lane_sequence: u64,
        state: Option<AgentObservationState>,
        #[ts(type = "Array<CanonicalConversationRecord>")]
        presentations: Vec<CanonicalConversationRecord>,
    },
    ResetRequired {
        #[serde(with = "crate::wire_u64")]
        #[schemars(with = "crate::wire_u64::RuntimeU64")]
        #[ts(type = "RuntimeU64")]
        connection_epoch: u64,
        reason: AgentRuntimeResetReason,
        #[serde(default)]
        #[ts(type = "RuntimeU64 | null")]
        last_sequence: Option<RuntimeU64>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRuntimeProjectionSchema {
    pub view: AgentRuntimeView,
    pub update: AgentRuntimeUpdate,
    pub stream_frame: AgentRuntimeStreamFrame,
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::{
        AgentContextAuthority, AgentContextCoordinate, AgentContextFidelity,
        AgentExecutionSnapshot, AgentLifecycleStatus, AgentPayloadDigest, AgentSnapshotAuthority,
        AgentSnapshotSource, SemanticFidelity,
    };

    fn observation(revision: u64) -> AgentObservation {
        AgentObservation {
            revision: AgentSnapshotRevision(revision),
            context: AgentContextCoordinate {
                snapshot_revision: AgentSnapshotRevision(revision),
                context_revision: Some(format!("context-{revision}")),
                recipe_digest: AgentPayloadDigest::new(format!("sha256:context-{revision}"))
                    .expect("digest"),
                authority: AgentContextAuthority::AgentOwned,
                fidelity: AgentContextFidelity::Exact,
            },
            lifecycle: AgentLifecycleStatus::Active,
            execution: AgentExecutionSnapshot {
                active_turn: None,
                queued_compaction: None,
                last_compaction_outcome: None,
            },
            command_availability: BTreeMap::new(),
            interactions: Vec::new(),
            thread_name: None,
            source_info: AgentSnapshotSource {
                authority: AgentSnapshotAuthority::AgentAuthoritative,
                source_revision: None,
                fidelity: SemanticFidelity::Exact,
                observed_at_ms: 42,
            },
            conversation: Vec::new(),
        }
    }

    #[test]
    fn product_wrapper_preserves_the_canonical_observation_by_value() {
        let source = observation(5);
        let view = AgentRuntimeView {
            thread_id: RuntimeThreadId::new("runtime-thread-1").expect("thread"),
            observation: source.clone(),
        };

        assert_eq!(view.observation, source);
        assert_eq!(view.view_revision(), AgentSnapshotRevision(5));
    }

    #[test]
    fn update_round_trips_without_an_authoritative_conversation() {
        let update = AgentRuntimeUpdate {
            lane_sequence: u64::MAX,
            state: Some(observation(6).state()),
            presentations: Vec::new(),
        };
        let encoded = serde_json::to_value(&update).expect("serialize");
        assert!(encoded.get("observation").is_none());
        assert!(
            encoded
                .get("state")
                .and_then(serde_json::Value::as_object)
                .is_some_and(|state| !state.contains_key("conversation"))
        );
        let decoded: AgentRuntimeUpdate = serde_json::from_value(encoded).expect("deserialize");
        assert_eq!(decoded, update);
    }

    #[test]
    fn stream_frames_round_trip_full_u64_coordinates() {
        let frames = [
            AgentRuntimeStreamFrame::Baseline {
                connection_epoch: u64::MAX,
                view: AgentRuntimeView {
                    thread_id: RuntimeThreadId::new("runtime-thread").expect("thread"),
                    observation: observation(1),
                },
            },
            AgentRuntimeStreamFrame::Update {
                connection_epoch: u64::MAX,
                lane_sequence: u64::MAX,
                state: None,
                presentations: Vec::new(),
            },
            AgentRuntimeStreamFrame::ResetRequired {
                connection_epoch: u64::MAX,
                reason: AgentRuntimeResetReason::Lagged,
                last_sequence: Some(RuntimeU64(u64::MAX)),
            },
        ];

        for frame in frames {
            let encoded = serde_json::to_value(&frame).expect("serialize frame");
            let decoded: AgentRuntimeStreamFrame =
                serde_json::from_value(encoded).expect("deserialize frame");
            assert_eq!(decoded, frame);
        }
    }
}
