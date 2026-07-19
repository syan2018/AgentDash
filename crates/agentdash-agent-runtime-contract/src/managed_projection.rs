use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use crate::{
    ManagedRuntimeInteractionRequest, ManagedRuntimeInteractionResolution,
    ManagedRuntimeInteractionStatus, ManagedRuntimeItemPresentation, ManagedRuntimeItemTransition,
    RuntimeChangeSequence, RuntimeContextContributionId, RuntimeContextPackageId,
    RuntimeContextSourceRef, RuntimeContextSourceRevision, RuntimeInteractionId, RuntimeItemId,
    RuntimeOperationId, RuntimePayloadDigest, RuntimeProjectionRevision, RuntimeSourceRef,
    RuntimeThreadId, RuntimeTurnId, SurfaceRevision,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum ManagedRuntimeProjectionAuthority {
    SourceAuthoritative,
    SourceObserved,
    RuntimeDerived,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "snake_case")]
pub enum ManagedRuntimeProjectionFidelity {
    Unsupported,
    Observed,
    Approximation,
    Exact,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct ManagedRuntimeThreadNameSource {
    pub authority: ManagedRuntimeProjectionAuthority,
    pub fidelity: ManagedRuntimeProjectionFidelity,
    pub source_identity_digest: RuntimePayloadDigest,
    pub source_revision_digest: Option<RuntimePayloadDigest>,
    #[serde(with = "crate::wire_u64")]
    #[schemars(with = "crate::wire_u64::RuntimeU64")]
    #[ts(type = "RuntimeU64")]
    pub observed_at_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum ManagedRuntimeLifecycleStatus {
    Provisioning,
    Active,
    Suspended,
    Closed,
    Lost,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum ManagedRuntimeEntityStatus {
    Accepted,
    Running,
    Completed,
    Failed,
    Interrupted,
    Lost,
}

/// Application command input blocks are intentionally narrower than presentation blocks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ManagedRuntimeContentBlock {
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
pub struct ManagedRuntimeTurn {
    pub id: RuntimeTurnId,
    pub status: ManagedRuntimeEntityStatus,
    pub item_ids: Vec<RuntimeItemId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct ManagedRuntimeItem {
    pub id: RuntimeItemId,
    pub turn_id: RuntimeTurnId,
    pub status: ManagedRuntimeEntityStatus,
    pub presentation: ManagedRuntimeItemPresentation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct ManagedRuntimeInteraction {
    pub id: RuntimeInteractionId,
    pub turn_id: RuntimeTurnId,
    pub item_id: Option<RuntimeItemId>,
    pub request: ManagedRuntimeInteractionRequest,
    pub status: ManagedRuntimeInteractionStatus,
    pub resolution: Option<ManagedRuntimeInteractionResolution>,
}

impl ManagedRuntimeInteraction {
    pub fn validate(&self) -> bool {
        match (&self.status, &self.resolution) {
            (ManagedRuntimeInteractionStatus::Pending, None) => true,
            (ManagedRuntimeInteractionStatus::Resolved, Some(resolution)) => matches!(
                (&self.request, resolution),
                (
                    ManagedRuntimeInteractionRequest::Approval { .. },
                    ManagedRuntimeInteractionResolution::Approved
                        | ManagedRuntimeInteractionResolution::Denied { .. }
                ) | (
                    ManagedRuntimeInteractionRequest::UserInput { .. },
                    ManagedRuntimeInteractionResolution::UserInput { .. }
                ) | (
                    ManagedRuntimeInteractionRequest::McpElicitation { .. },
                    ManagedRuntimeInteractionResolution::McpElicitation { .. }
                ) | (
                    ManagedRuntimeInteractionRequest::DynamicTool { .. },
                    ManagedRuntimeInteractionResolution::DynamicToolResult { .. }
                )
            ),
            (
                ManagedRuntimeInteractionStatus::Cancelled,
                Some(ManagedRuntimeInteractionResolution::Cancelled { .. }),
            )
            | (
                ManagedRuntimeInteractionStatus::Expired,
                Some(ManagedRuntimeInteractionResolution::Expired),
            )
            | (
                ManagedRuntimeInteractionStatus::Lost,
                Some(ManagedRuntimeInteractionResolution::Lost { .. }),
            ) => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum ManagedRuntimeOperationStatus {
    Accepted,
    Running,
    Succeeded,
    Failed,
    Interrupted,
    Lost,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum ManagedRuntimeInitialContextAppliedFidelity {
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
pub enum ManagedRuntimeInitialContextContributionKind {
    CompactSummary,
    WorkflowContext,
    ConstraintSet,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct ManagedRuntimeInitialContextContributionEvidence {
    pub contribution_id: RuntimeContextContributionId,
    pub kind: ManagedRuntimeInitialContextContributionKind,
    pub contribution_digest: RuntimePayloadDigest,
    pub provenance: ManagedRuntimeAppliedContextProvenance,
    pub fidelity: ManagedRuntimeInitialContextAppliedFidelity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct ManagedRuntimeAppliedContextProvenance {
    pub authority: crate::ManagedRuntimeContextAuthority,
    pub source: RuntimeContextSourceRef,
    pub revision: RuntimeContextSourceRevision,
    pub digest: RuntimePayloadDigest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct ManagedRuntimeAppliedInitialContextEvidence {
    pub package_id: RuntimeContextPackageId,
    pub package_digest: RuntimePayloadDigest,
    pub contributions: Vec<ManagedRuntimeInitialContextContributionEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ManagedRuntimeForkCutoff {
    Head,
    CompletedTurn { turn_id: RuntimeTurnId },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ManagedRuntimeForkProgressEvidence {
    ChildKnown {
        child_thread_id: RuntimeThreadId,
        child_source_ref: RuntimeSourceRef,
        cutoff: ManagedRuntimeForkCutoff,
        child_history_digest: Option<RuntimePayloadDigest>,
    },
    Provisioned {
        child_thread_id: RuntimeThreadId,
        child_binding: ManagedRuntimeSourceBindingEvidence,
        cutoff: ManagedRuntimeForkCutoff,
        child_history_digest: RuntimePayloadDigest,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct ManagedRuntimeSourceBindingEvidence {
    pub source_ref: RuntimeSourceRef,
    pub committed_at_revision: RuntimeProjectionRevision,
    pub applied_surface_revision: SurfaceRevision,
    pub activated_at_revision: Option<RuntimeProjectionRevision>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ManagedRuntimeOperationEvidence {
    Create {
        binding: ManagedRuntimeSourceBindingEvidence,
        initial_context: Option<ManagedRuntimeAppliedInitialContextEvidence>,
    },
    Resume {
        binding: ManagedRuntimeSourceBindingEvidence,
    },
    Fork {
        parent_binding: ManagedRuntimeSourceBindingEvidence,
        progress: ManagedRuntimeForkProgressEvidence,
    },
    Activate {
        binding: ManagedRuntimeSourceBindingEvidence,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct ManagedRuntimeOperation {
    pub id: RuntimeOperationId,
    pub turn_id: Option<RuntimeTurnId>,
    pub status: ManagedRuntimeOperationStatus,
    pub evidence: Option<ManagedRuntimeOperationEvidence>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "snake_case")]
pub enum ManagedRuntimeCommandKind {
    Create,
    Resume,
    Activate,
    SubmitInput,
    Steer,
    Interrupt,
    RequestCompaction,
    ResolveInteraction,
    Close,
    Fork,
}

impl ManagedRuntimeCommandKind {
    pub const ALL: [Self; 10] = [
        Self::Create,
        Self::Resume,
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
pub enum ManagedRuntimeUnavailabilityReason {
    RuntimeNotActive,
    AdmissionDenied,
    BoundSurfaceUnavailable,
    AppliedSurfaceMismatch,
    ActiveTurnRequired,
    NoActiveTurnRequired,
    PendingInteractionRequired,
    OperationInFlight,
    SourceUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct ManagedRuntimeAvailabilityEvidence {
    pub decided_at_revision: RuntimeProjectionRevision,
    pub blocking_operation_id: Option<RuntimeOperationId>,
    pub bound_surface_revision: Option<SurfaceRevision>,
    pub applied_surface_revision: Option<SurfaceRevision>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ManagedRuntimeCommandAvailability {
    Available {
        evidence: ManagedRuntimeAvailabilityEvidence,
    },
    Unavailable {
        reason: ManagedRuntimeUnavailabilityReason,
        evidence: ManagedRuntimeAvailabilityEvidence,
    },
}

impl ManagedRuntimeCommandAvailability {
    pub fn evidence(&self) -> &ManagedRuntimeAvailabilityEvidence {
        match self {
            Self::Available { evidence } | Self::Unavailable { evidence, .. } => evidence,
        }
    }

    pub fn evidence_mut(&mut self) -> &mut ManagedRuntimeAvailabilityEvidence {
        match self {
            Self::Available { evidence } | Self::Unavailable { evidence, .. } => evidence,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct ManagedRuntimeSnapshot {
    pub thread_id: RuntimeThreadId,
    pub revision: RuntimeProjectionRevision,
    pub latest_change_sequence: RuntimeChangeSequence,
    #[serde(with = "crate::wire_u64")]
    #[schemars(with = "crate::wire_u64::RuntimeU64")]
    #[ts(type = "RuntimeU64")]
    pub captured_at_ms: u64,
    pub lifecycle: ManagedRuntimeLifecycleStatus,
    pub active_turn_id: Option<RuntimeTurnId>,
    pub turns: Vec<ManagedRuntimeTurn>,
    pub items: Vec<ManagedRuntimeItem>,
    pub interactions: Vec<ManagedRuntimeInteraction>,
    pub thread_name: Option<String>,
    pub thread_name_source: Option<ManagedRuntimeThreadNameSource>,
    pub operations: Vec<ManagedRuntimeOperation>,
    pub source_binding: Option<ManagedRuntimeSourceBindingEvidence>,
    pub authority: ManagedRuntimeProjectionAuthority,
    pub fidelity: ManagedRuntimeProjectionFidelity,
    pub command_availability:
        BTreeMap<ManagedRuntimeCommandKind, ManagedRuntimeCommandAvailability>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "snake_case")]
pub enum ManagedRuntimeProjectionSection {
    Snapshot,
    ThreadName,
    Lifecycle,
    ActiveTurn,
    Turns,
    Items,
    Interactions,
    Surface,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
pub enum ManagedRuntimeSourceProjectionDelta {
    SnapshotReplaced {
        lifecycle: ManagedRuntimeLifecycleStatus,
        active_turn_id: Option<RuntimeTurnId>,
        turns: Vec<ManagedRuntimeTurn>,
        items: Vec<ManagedRuntimeItem>,
        interactions: Vec<ManagedRuntimeInteraction>,
        authority: ManagedRuntimeProjectionAuthority,
        fidelity: ManagedRuntimeProjectionFidelity,
        applied_surface_revision: Option<SurfaceRevision>,
    },
    LifecycleChanged {
        lifecycle: ManagedRuntimeLifecycleStatus,
    },
    ActiveTurnChanged {
        active_turn_id: Option<RuntimeTurnId>,
    },
    TurnsChanged {
        turns: Vec<ManagedRuntimeTurn>,
    },
    ItemsChanged {
        items: Vec<ManagedRuntimeItem>,
    },
    ItemTransitioned {
        item_id: RuntimeItemId,
        transition: ManagedRuntimeItemTransition,
    },
    InteractionsChanged {
        interactions: Vec<ManagedRuntimeInteraction>,
    },
    SurfaceChanged {
        applied_surface_revision: Option<SurfaceRevision>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
pub enum ManagedRuntimeChangeDelta {
    ThreadNameChanged {
        #[serde(with = "crate::wire_u64")]
        #[schemars(with = "crate::wire_u64::RuntimeU64")]
        #[ts(type = "RuntimeU64")]
        source_change_sequence: u64,
        source_projection_revision: RuntimeProjectionRevision,
        thread_name: Option<String>,
        source: ManagedRuntimeThreadNameSource,
    },
    SourceObservationApplied {
        #[serde(with = "crate::wire_u64")]
        #[schemars(with = "crate::wire_u64::RuntimeU64")]
        #[ts(type = "RuntimeU64")]
        source_change_sequence: u64,
        source_projection_revision: RuntimeProjectionRevision,
        source_identity_digest: RuntimePayloadDigest,
        observation_digest: RuntimePayloadDigest,
        source_revision_digest: Option<RuntimePayloadDigest>,
        source_cursor_digest: Option<RuntimePayloadDigest>,
        changed_sections: BTreeSet<ManagedRuntimeProjectionSection>,
    },
    SourceProjectionChanged {
        #[serde(with = "crate::wire_u64")]
        #[schemars(with = "crate::wire_u64::RuntimeU64")]
        #[ts(type = "RuntimeU64")]
        source_change_sequence: u64,
        source_projection_revision: RuntimeProjectionRevision,
        observation_digest: RuntimePayloadDigest,
        section: ManagedRuntimeProjectionSection,
        section_digest: RuntimePayloadDigest,
        delta: ManagedRuntimeSourceProjectionDelta,
    },
    OperationUpserted {
        operation: ManagedRuntimeOperation,
    },
    CommandAvailabilityChanged {
        command: ManagedRuntimeCommandKind,
        availability: ManagedRuntimeCommandAvailability,
    },
    SurfaceEvidenceChanged {
        bound_surface_revision: Option<SurfaceRevision>,
        applied_surface_revision: Option<SurfaceRevision>,
    },
    SourceBindingChanged {
        binding: Option<ManagedRuntimeSourceBindingEvidence>,
    },
    RuntimeLifecycleChanged {
        lifecycle: ManagedRuntimeLifecycleStatus,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct ManagedRuntimePlatformChange {
    pub thread_id: RuntimeThreadId,
    pub sequence: RuntimeChangeSequence,
    pub revision: RuntimeProjectionRevision,
    pub delta: ManagedRuntimeChangeDelta,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct ManagedRuntimeChangeGap {
    pub requested_after: Option<RuntimeChangeSequence>,
    pub earliest_available: RuntimeChangeSequence,
    pub latest_available: RuntimeChangeSequence,
    pub snapshot_revision: RuntimeProjectionRevision,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct ManagedRuntimeChangePage {
    pub thread_id: RuntimeThreadId,
    pub changes: Vec<ManagedRuntimePlatformChange>,
    pub next: RuntimeChangeSequence,
    pub gap: Option<ManagedRuntimeChangeGap>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct ManagedRuntimeProjectionSchema {
    pub snapshot: ManagedRuntimeSnapshot,
    pub change_page: ManagedRuntimeChangePage,
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

    fn evidence(revision: u64) -> ManagedRuntimeAvailabilityEvidence {
        ManagedRuntimeAvailabilityEvidence {
            decided_at_revision: RuntimeProjectionRevision(revision),
            blocking_operation_id: None,
            bound_surface_revision: Some(SurfaceRevision(3)),
            applied_surface_revision: Some(SurfaceRevision(3)),
        }
    }

    #[test]
    fn application_contract_round_trips_snapshot_availability_and_typed_gap() {
        let thread_id = id("runtime-thread-1", RuntimeThreadId::new);
        let turn_id = id("runtime-turn-1", RuntimeTurnId::new);
        let item_id = id("runtime-item-1", RuntimeItemId::new);
        let mut command_availability = BTreeMap::new();
        for command in ManagedRuntimeCommandKind::ALL {
            command_availability.insert(
                command,
                ManagedRuntimeCommandAvailability::Available {
                    evidence: evidence(5),
                },
            );
        }
        let contract = ManagedRuntimeProjectionSchema {
            snapshot: ManagedRuntimeSnapshot {
                thread_id: thread_id.clone(),
                revision: RuntimeProjectionRevision(5),
                latest_change_sequence: RuntimeChangeSequence(8),
                captured_at_ms: 42,
                lifecycle: ManagedRuntimeLifecycleStatus::Active,
                active_turn_id: Some(turn_id.clone()),
                turns: vec![ManagedRuntimeTurn {
                    id: turn_id.clone(),
                    status: ManagedRuntimeEntityStatus::Running,
                    item_ids: vec![item_id.clone()],
                }],
                items: vec![ManagedRuntimeItem {
                    id: item_id,
                    turn_id,
                    status: ManagedRuntimeEntityStatus::Running,
                    presentation: ManagedRuntimeItemPresentation::new(
                        crate::ManagedRuntimeItemBody::ContextCompaction {
                            summary: None,
                            source_digest: None,
                        },
                        Some(40),
                        Some(42),
                        None,
                    )
                    .expect("valid compaction presentation"),
                }],
                interactions: Vec::new(),
                thread_name: None,
                thread_name_source: None,
                operations: Vec::new(),
                source_binding: None,
                authority: ManagedRuntimeProjectionAuthority::SourceAuthoritative,
                fidelity: ManagedRuntimeProjectionFidelity::Exact,
                command_availability,
            },
            change_page: ManagedRuntimeChangePage {
                thread_id,
                changes: Vec::new(),
                next: RuntimeChangeSequence(8),
                gap: Some(ManagedRuntimeChangeGap {
                    requested_after: Some(RuntimeChangeSequence(2)),
                    earliest_available: RuntimeChangeSequence(5),
                    latest_available: RuntimeChangeSequence(8),
                    snapshot_revision: RuntimeProjectionRevision(5),
                }),
            },
        };

        let json = serde_json::to_value(&contract).expect("serialize contract fixture");
        assert_eq!(json["snapshot"]["revision"], "5");
        assert_eq!(
            json["snapshot"]["command_availability"]["submit_input"]["status"],
            "available"
        );
        assert_eq!(json["change_page"]["gap"]["earliest_available"], "5");
        let decoded: ManagedRuntimeProjectionSchema =
            serde_json::from_value(json).expect("deserialize contract fixture");
        assert_eq!(decoded, contract);
    }

    #[test]
    fn schema_closure_contains_runtime_ids_availability_and_gap() {
        let schema = schemars::schema_for!(ManagedRuntimeProjectionSchema);
        let schema = serde_json::to_string(&schema).expect("serialize schema");
        for required in [
            "thread_id",
            "thread_name",
            "thread_name_source",
            "turn_id",
            "item_id",
            "operations",
            "command_availability",
            "ManagedRuntimeChangeGap",
        ] {
            assert!(schema.contains(required), "missing {required}");
        }
        assert!(!schema.contains("AgentSourceCoordinate"));
        assert!(!schema.contains("AgentTurnId"));
        assert!(!schema.contains("AgentItemId"));
        assert!(schema.contains("thread_name_changed"));
    }

    #[test]
    fn operation_evidence_round_trips_without_host_identity_leakage() {
        let source_ref = id("source-ref-1", RuntimeSourceRef::new);
        let child_source_ref = id("source-ref-2", RuntimeSourceRef::new);
        let child_thread_id = id("runtime-thread-2", RuntimeThreadId::new);
        let operation = ManagedRuntimeOperation {
            id: id("fork-operation", RuntimeOperationId::new),
            turn_id: None,
            status: ManagedRuntimeOperationStatus::Succeeded,
            evidence: Some(ManagedRuntimeOperationEvidence::Fork {
                parent_binding: ManagedRuntimeSourceBindingEvidence {
                    source_ref,
                    committed_at_revision: RuntimeProjectionRevision(2),
                    applied_surface_revision: SurfaceRevision(4),
                    activated_at_revision: Some(RuntimeProjectionRevision(3)),
                },
                progress: ManagedRuntimeForkProgressEvidence::Provisioned {
                    child_thread_id,
                    child_binding: ManagedRuntimeSourceBindingEvidence {
                        source_ref: child_source_ref,
                        committed_at_revision: RuntimeProjectionRevision(8),
                        applied_surface_revision: SurfaceRevision(9),
                        activated_at_revision: None,
                    },
                    cutoff: ManagedRuntimeForkCutoff::CompletedTurn {
                        turn_id: id("runtime-turn-4", RuntimeTurnId::new),
                    },
                    child_history_digest: id("sha256:history", RuntimePayloadDigest::new),
                },
            }),
        };

        let json = serde_json::to_value(&operation).expect("serialize operation evidence");
        assert_eq!(json["evidence"]["kind"], "fork");
        assert_eq!(
            json["evidence"]["progress"]["cutoff"]["kind"],
            "completed_turn"
        );
        assert_eq!(
            serde_json::from_value::<ManagedRuntimeOperation>(json).expect("deserialize evidence"),
            operation
        );

        let schema = serde_json::to_string(&schemars::schema_for!(ManagedRuntimeProjectionSchema))
            .expect("serialize schema");
        assert!(schema.contains("source_ref"));
        assert!(!schema.contains("AgentBindingGeneration"));
        assert!(!schema.contains("AgentSourceCoordinate"));
        assert!(!schema.contains("CompleteAgent"));
    }
}
