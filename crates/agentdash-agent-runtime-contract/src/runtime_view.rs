use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use agentdash_agent_protocol::CanonicalConversationRecord;

use crate::{
    RuntimeContextContributionId, RuntimeContextPackageId, RuntimeContextSourceRef,
    RuntimeContextSourceRevision, RuntimeInteractionId, RuntimeItemId, RuntimePayloadDigest,
    RuntimeProjectionRevision, RuntimeSourceRef, RuntimeThreadId, RuntimeTurnId, SurfaceRevision,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum AgentRuntimeProjectionAuthority {
    SourceAuthoritative,
    SourceObserved,
    RuntimeDerived,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "snake_case")]
pub enum AgentRuntimeProjectionFidelity {
    Unsupported,
    Observed,
    Approximation,
    Exact,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRuntimeThreadNameSource {
    pub authority: AgentRuntimeProjectionAuthority,
    pub fidelity: AgentRuntimeProjectionFidelity,
    pub source_identity_digest: RuntimePayloadDigest,
    pub source_revision_digest: Option<RuntimePayloadDigest>,
    #[serde(with = "crate::wire_u64")]
    #[schemars(with = "crate::wire_u64::RuntimeU64")]
    #[ts(type = "RuntimeU64")]
    pub observed_at_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum AgentRuntimeLifecycleStatus {
    Provisioning,
    Active,
    Suspended,
    Closed,
    Lost,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentRuntimeInteractionRequest {
    Approval {
        prompt: String,
        reason: Option<String>,
        proposed_action: Option<Value>,
    },
    UserInput {
        prompt: String,
        questions: Vec<AgentRuntimeInteractionQuestion>,
    },
    McpElicitation {
        server: String,
        prompt: String,
        schema: Value,
    },
    DynamicTool {
        namespace: Option<String>,
        tool: String,
        prompt: String,
        arguments: Value,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRuntimeInteractionQuestion {
    pub id: String,
    pub prompt: String,
    pub options: Vec<String>,
    pub allows_free_form: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum AgentRuntimeInteractionStatus {
    Pending,
    Resolved,
    Cancelled,
    Expired,
    Lost,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentRuntimeInteractionResolution {
    Approved,
    Denied { reason: Option<String> },
    UserInput { answers: Value },
    McpElicitation { response: Value },
    DynamicToolResult { result: Value },
    Cancelled { reason: Option<String> },
    Expired,
    Lost { reason: String },
}

/// Application command input blocks are intentionally narrower than presentation blocks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentRuntimeContentBlock {
    Text {
        text: String,
    },
    Image {
        media_type: String,
        source: String,
        digest: RuntimePayloadDigest,
    },
    Resource {
        uri: String,
        media_type: Option<String>,
        digest: Option<RuntimePayloadDigest>,
    },
    Structured {
        schema: String,
        value: Value,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRuntimeInteraction {
    pub id: RuntimeInteractionId,
    pub turn_id: RuntimeTurnId,
    pub item_id: Option<RuntimeItemId>,
    pub request: AgentRuntimeInteractionRequest,
    pub status: AgentRuntimeInteractionStatus,
    pub resolution: Option<AgentRuntimeInteractionResolution>,
}

impl AgentRuntimeInteraction {
    pub fn validate(&self) -> bool {
        match (&self.status, &self.resolution) {
            (AgentRuntimeInteractionStatus::Pending, None) => true,
            (AgentRuntimeInteractionStatus::Resolved, Some(resolution)) => matches!(
                (&self.request, resolution),
                (
                    AgentRuntimeInteractionRequest::Approval { .. },
                    AgentRuntimeInteractionResolution::Approved
                        | AgentRuntimeInteractionResolution::Denied { .. }
                ) | (
                    AgentRuntimeInteractionRequest::UserInput { .. },
                    AgentRuntimeInteractionResolution::UserInput { .. }
                ) | (
                    AgentRuntimeInteractionRequest::McpElicitation { .. },
                    AgentRuntimeInteractionResolution::McpElicitation { .. }
                ) | (
                    AgentRuntimeInteractionRequest::DynamicTool { .. },
                    AgentRuntimeInteractionResolution::DynamicToolResult { .. }
                )
            ),
            (
                AgentRuntimeInteractionStatus::Cancelled,
                Some(AgentRuntimeInteractionResolution::Cancelled { .. }),
            )
            | (
                AgentRuntimeInteractionStatus::Expired,
                Some(AgentRuntimeInteractionResolution::Expired),
            )
            | (
                AgentRuntimeInteractionStatus::Lost,
                Some(AgentRuntimeInteractionResolution::Lost { .. }),
            ) => true,
            _ => false,
        }
    }
}

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum AgentRuntimeInitialContextAppliedFidelity {
    TypedNative {
        applied_digest: RuntimePayloadDigest,
    },
    CanonicalRendered {
        renderer_version: String,
        rendered_digest: RuntimePayloadDigest,
    },
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "snake_case")]
pub enum AgentRuntimeInitialContextContributionKind {
    CompactSummary,
    WorkflowContext,
    ConstraintSet,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRuntimeInitialContextContributionEvidence {
    pub contribution_id: RuntimeContextContributionId,
    pub kind: AgentRuntimeInitialContextContributionKind,
    pub contribution_digest: RuntimePayloadDigest,
    pub provenance: AgentRuntimeAppliedContextProvenance,
    pub fidelity: AgentRuntimeInitialContextAppliedFidelity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRuntimeAppliedContextProvenance {
    pub authority: crate::AgentRuntimeContextAuthority,
    pub source: RuntimeContextSourceRef,
    pub revision: RuntimeContextSourceRevision,
    pub digest: RuntimePayloadDigest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRuntimeAppliedInitialContextEvidence {
    pub package_id: RuntimeContextPackageId,
    pub package_digest: RuntimePayloadDigest,
    pub contributions: Vec<AgentRuntimeInitialContextContributionEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentRuntimeForkCutoff {
    Head,
    CompletedTurn { turn_id: RuntimeTurnId },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum AgentRuntimeForkProgressEvidence {
    ChildKnown {
        child_thread_id: RuntimeThreadId,
        child_source_ref: RuntimeSourceRef,
        cutoff: AgentRuntimeForkCutoff,
        child_history_digest: Option<RuntimePayloadDigest>,
    },
    Provisioned {
        child_thread_id: RuntimeThreadId,
        child_binding: AgentRuntimeSourceBindingEvidence,
        cutoff: AgentRuntimeForkCutoff,
        child_history_digest: RuntimePayloadDigest,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRuntimeSourceBindingEvidence {
    pub source_ref: RuntimeSourceRef,
    pub committed_at_revision: RuntimeProjectionRevision,
    pub applied_surface_revision: SurfaceRevision,
    pub activated_at_revision: Option<RuntimeProjectionRevision>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentRuntimeOperationEvidence {
    Create {
        binding: AgentRuntimeSourceBindingEvidence,
        initial_context: Option<AgentRuntimeAppliedInitialContextEvidence>,
    },
    Resume {
        binding: AgentRuntimeSourceBindingEvidence,
    },
    Rebind {
        previous_binding: AgentRuntimeSourceBindingEvidence,
        binding: AgentRuntimeSourceBindingEvidence,
    },
    Fork {
        parent_binding: AgentRuntimeSourceBindingEvidence,
        progress: AgentRuntimeForkProgressEvidence,
    },
    Activate {
        binding: AgentRuntimeSourceBindingEvidence,
    },
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum AgentRuntimeUnavailabilityReason {
    RuntimeNotActive,
    AdmissionDenied,
    BoundSurfaceUnavailable,
    AppliedSurfaceMismatch,
    ActiveTurnRequired,
    NoActiveTurnRequired,
    PendingInteractionRequired,
    OperationInFlight,
    SourceUnavailable,
    ActiveTurnNotSteerable,
    CompactionInProgress,
    TurnNotCancellable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRuntimeAvailabilityEvidence {
    pub expected_view_revision: Option<RuntimeProjectionRevision>,
    pub expected_turn_id: Option<RuntimeTurnId>,
    pub bound_surface_revision: Option<SurfaceRevision>,
    pub applied_surface_revision: Option<SurfaceRevision>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum AgentRuntimeCommandAvailability {
    Available {
        evidence: AgentRuntimeAvailabilityEvidence,
    },
    Unavailable {
        reason: AgentRuntimeUnavailabilityReason,
        evidence: AgentRuntimeAvailabilityEvidence,
    },
}

impl AgentRuntimeCommandAvailability {
    pub fn evidence(&self) -> &AgentRuntimeAvailabilityEvidence {
        match self {
            Self::Available { evidence } | Self::Unavailable { evidence, .. } => evidence,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum AgentRuntimeExecutionStatus {
    Idle,
    Active,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum AgentRuntimeActiveTurnKind {
    Conversation,
    ContextCompaction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum AgentRuntimeActiveTurnPhase {
    Running,
    Applied,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRuntimeActiveTurn {
    pub turn_id: RuntimeTurnId,
    pub kind: AgentRuntimeActiveTurnKind,
    pub phase: AgentRuntimeActiveTurnPhase,
    #[serde(with = "crate::wire_u64")]
    #[schemars(with = "crate::wire_u64::RuntimeU64")]
    #[ts(type = "RuntimeU64")]
    pub started_at_ms: u64,
    pub cancellable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum AgentRuntimeCompactionOutcomeStatus {
    Succeeded,
    Failed,
    Lost,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRuntimeCompactionOutcome {
    pub turn_id: RuntimeTurnId,
    pub status: AgentRuntimeCompactionOutcomeStatus,
    #[serde(with = "crate::wire_u64")]
    #[schemars(with = "crate::wire_u64::RuntimeU64")]
    #[ts(type = "RuntimeU64")]
    pub completed_at_ms: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRuntimeQueuedCompaction {
    #[serde(with = "crate::wire_u64")]
    #[schemars(with = "crate::wire_u64::RuntimeU64")]
    #[ts(type = "RuntimeU64")]
    pub queued_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRuntimeExecutionView {
    pub status: AgentRuntimeExecutionStatus,
    pub active_turn: Option<AgentRuntimeActiveTurn>,
    pub queued_compaction: Option<AgentRuntimeQueuedCompaction>,
    pub last_compaction_outcome: Option<AgentRuntimeCompactionOutcome>,
    pub latest_turn_id: Option<RuntimeTurnId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRuntimeContextCoordinate {
    pub snapshot_revision: RuntimeProjectionRevision,
    pub context_revision: Option<String>,
    pub recipe_digest: RuntimePayloadDigest,
    pub authority: AgentRuntimeProjectionAuthority,
    pub fidelity: AgentRuntimeProjectionFidelity,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRuntimeView {
    pub thread_id: RuntimeThreadId,
    pub view_revision: RuntimeProjectionRevision,
    #[serde(with = "crate::wire_u64")]
    #[schemars(with = "crate::wire_u64::RuntimeU64")]
    #[ts(type = "RuntimeU64")]
    pub captured_at_ms: u64,
    pub lifecycle: AgentRuntimeLifecycleStatus,
    pub execution: AgentRuntimeExecutionView,
    pub context: AgentRuntimeContextCoordinate,
    pub interactions: Vec<AgentRuntimeInteraction>,
    pub thread_name: Option<String>,
    pub thread_name_source: Option<AgentRuntimeThreadNameSource>,
    pub source_binding: Option<AgentRuntimeSourceBindingEvidence>,
    pub authority: AgentRuntimeProjectionAuthority,
    pub fidelity: AgentRuntimeProjectionFidelity,
    pub command_availability: BTreeMap<AgentRuntimeCommandKind, AgentRuntimeCommandAvailability>,
    #[ts(type = "Array<CanonicalConversationRecord>")]
    pub conversation: Vec<CanonicalConversationRecord>,
}

impl AgentRuntimeView {
    pub fn conversation(&self) -> agentdash_agent_protocol::CanonicalConversationView<'_> {
        agentdash_agent_protocol::CanonicalConversationView::new(&self.conversation)
    }

    pub fn active_turn_id(&self) -> Option<&str> {
        self.execution
            .active_turn
            .as_ref()
            .map(|turn| turn.turn_id.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRuntimeUpdate {
    #[serde(with = "crate::wire_u64")]
    #[schemars(with = "crate::wire_u64::RuntimeU64")]
    #[ts(type = "RuntimeU64")]
    pub lane_sequence: u64,
    pub view_revision: RuntimeProjectionRevision,
    pub execution: AgentRuntimeExecutionView,
    pub context: AgentRuntimeContextCoordinate,
    pub command_availability: BTreeMap<AgentRuntimeCommandKind, AgentRuntimeCommandAvailability>,
    pub interactions: Vec<AgentRuntimeInteraction>,
    #[ts(type = "Array<CanonicalConversationRecord>")]
    pub presentations: Vec<CanonicalConversationRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRuntimeProjectionSchema {
    pub view: AgentRuntimeView,
    pub update: AgentRuntimeUpdate,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id<T>(
        value: &str,
        constructor: impl FnOnce(String) -> Result<T, crate::InvalidRuntimeId>,
    ) -> T {
        constructor(value.to_owned()).expect("valid Runtime identity")
    }

    fn evidence() -> AgentRuntimeAvailabilityEvidence {
        AgentRuntimeAvailabilityEvidence {
            expected_view_revision: Some(RuntimeProjectionRevision(5)),
            expected_turn_id: None,
            bound_surface_revision: Some(SurfaceRevision(3)),
            applied_surface_revision: Some(SurfaceRevision(3)),
        }
    }

    #[test]
    fn application_contract_round_trips_authoritative_view_and_update() {
        let thread_id = id("runtime-thread-1", RuntimeThreadId::new);
        let active_turn_id = id("turn-1", RuntimeTurnId::new);
        let mut command_availability = BTreeMap::new();
        for command in AgentRuntimeCommandKind::ALL {
            command_availability.insert(
                command,
                AgentRuntimeCommandAvailability::Available {
                    evidence: evidence(),
                },
            );
        }
        let execution = AgentRuntimeExecutionView {
            status: AgentRuntimeExecutionStatus::Active,
            active_turn: Some(AgentRuntimeActiveTurn {
                turn_id: active_turn_id.clone(),
                kind: AgentRuntimeActiveTurnKind::Conversation,
                phase: AgentRuntimeActiveTurnPhase::Running,
                started_at_ms: 42,
                cancellable: true,
            }),
            queued_compaction: None,
            last_compaction_outcome: None,
            latest_turn_id: Some(active_turn_id),
        };
        let context = AgentRuntimeContextCoordinate {
            snapshot_revision: RuntimeProjectionRevision(5),
            context_revision: Some("context-5".to_owned()),
            recipe_digest: id("sha256:context-5", RuntimePayloadDigest::new),
            authority: AgentRuntimeProjectionAuthority::SourceAuthoritative,
            fidelity: AgentRuntimeProjectionFidelity::Exact,
        };
        let contract = AgentRuntimeProjectionSchema {
            view: AgentRuntimeView {
                thread_id,
                view_revision: RuntimeProjectionRevision(5),
                captured_at_ms: 42,
                lifecycle: AgentRuntimeLifecycleStatus::Active,
                execution: execution.clone(),
                context: context.clone(),
                interactions: Vec::new(),
                conversation: Vec::new(),
                thread_name: None,
                thread_name_source: None,
                source_binding: None,
                authority: AgentRuntimeProjectionAuthority::SourceAuthoritative,
                fidelity: AgentRuntimeProjectionFidelity::Exact,
                command_availability: command_availability.clone(),
            },
            update: AgentRuntimeUpdate {
                lane_sequence: 7,
                view_revision: RuntimeProjectionRevision(6),
                execution,
                context,
                command_availability,
                interactions: Vec::new(),
                presentations: Vec::new(),
            },
        };

        let json = serde_json::to_value(&contract).expect("serialize contract fixture");
        assert_eq!(json["view"]["view_revision"], "5");
        assert_eq!(
            json["view"]["command_availability"]["submit_input"]["status"],
            "available"
        );
        assert_eq!(json["update"]["lane_sequence"], "7");
        let decoded: AgentRuntimeProjectionSchema =
            serde_json::from_value(json).expect("deserialize contract fixture");
        assert_eq!(decoded, contract);
    }

    #[test]
    fn schema_closure_contains_runtime_ids_and_availability() {
        let schema = schemars::schema_for!(AgentRuntimeProjectionSchema);
        let schema = serde_json::to_string(&schema).expect("serialize schema");
        for required in [
            "thread_id",
            "thread_name",
            "thread_name_source",
            "turn_id",
            "item_id",
            "command_availability",
        ] {
            assert!(schema.contains(required), "missing {required}");
        }
        assert!(!schema.contains("AgentSourceCoordinate"));
        assert!(!schema.contains("AgentTurnId"));
        assert!(!schema.contains("AgentItemId"));
    }

    #[test]
    fn operation_evidence_round_trips_without_host_identity_leakage() {
        let source_ref = id("source-ref-1", RuntimeSourceRef::new);
        let child_source_ref = id("source-ref-2", RuntimeSourceRef::new);
        let child_thread_id = id("runtime-thread-2", RuntimeThreadId::new);
        let evidence = AgentRuntimeOperationEvidence::Fork {
            parent_binding: AgentRuntimeSourceBindingEvidence {
                source_ref,
                committed_at_revision: RuntimeProjectionRevision(2),
                applied_surface_revision: SurfaceRevision(4),
                activated_at_revision: Some(RuntimeProjectionRevision(3)),
            },
            progress: AgentRuntimeForkProgressEvidence::Provisioned {
                child_thread_id,
                child_binding: AgentRuntimeSourceBindingEvidence {
                    source_ref: child_source_ref,
                    committed_at_revision: RuntimeProjectionRevision(8),
                    applied_surface_revision: SurfaceRevision(9),
                    activated_at_revision: None,
                },
                cutoff: AgentRuntimeForkCutoff::CompletedTurn {
                    turn_id: id("runtime-turn-4", RuntimeTurnId::new),
                },
                child_history_digest: id("sha256:history", RuntimePayloadDigest::new),
            },
        };

        let json = serde_json::to_value(&evidence).expect("serialize operation evidence");
        assert_eq!(json["kind"], "fork");
        assert_eq!(json["progress"]["cutoff"]["kind"], "completed_turn");
        assert_eq!(
            serde_json::from_value::<AgentRuntimeOperationEvidence>(json)
                .expect("deserialize evidence"),
            evidence
        );

        let schema = serde_json::to_string(&schemars::schema_for!(AgentRuntimeOperationEvidence))
            .expect("serialize schema");
        assert!(schema.contains("source_ref"));
        assert!(!schema.contains("AgentBindingGeneration"));
        assert!(!schema.contains("AgentSourceCoordinate"));
        assert!(!schema.contains("CompleteAgent"));
    }

    #[test]
    fn operation_status_exposes_the_serialized_receipt_value() {
        for (status, expected) in [
            (AgentRuntimeOperationStatus::Accepted, "accepted"),
            (AgentRuntimeOperationStatus::Running, "running"),
            (AgentRuntimeOperationStatus::Succeeded, "succeeded"),
            (AgentRuntimeOperationStatus::Failed, "failed"),
            (AgentRuntimeOperationStatus::Interrupted, "interrupted"),
            (AgentRuntimeOperationStatus::Lost, "lost"),
        ] {
            assert_eq!(status.as_str(), expected);
            assert_eq!(
                serde_json::to_value(status).expect("serialize operation status"),
                expected
            );
        }
    }
}
