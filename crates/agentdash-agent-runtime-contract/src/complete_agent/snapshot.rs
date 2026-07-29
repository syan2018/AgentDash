use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use agentdash_agent_protocol::CanonicalConversationRecord;

use crate::{
    AgentInteractionId, AgentInteractionRequest, AgentInteractionResolution,
    AgentInteractionStatus, AgentItemId, AgentSnapshotRevision, AgentSourceCoordinate,
    AgentSourceCursor, AgentSourceRevision, AgentTurnId, SemanticFidelity,
};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "snake_case")]
pub enum AgentSnapshotAuthority {
    AgentAuthoritative,
    AgentObserved,
    Derived,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentSnapshotSource {
    pub authority: AgentSnapshotAuthority,
    pub source_revision: Option<AgentSourceRevision>,
    pub fidelity: SemanticFidelity,
    #[serde(with = "crate::wire_u64")]
    #[schemars(with = "crate::wire_u64::RuntimeU64")]
    #[ts(type = "RuntimeU64")]
    pub observed_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentThreadNameSnapshot {
    pub thread_name: Option<String>,
    pub source_info: AgentSnapshotSource,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "snake_case")]
pub enum AgentLifecycleStatus {
    Creating,
    Active,
    Suspended,
    Closed,
    Lost,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentInteractionSnapshot {
    pub id: AgentInteractionId,
    pub turn_id: AgentTurnId,
    pub item_id: Option<AgentItemId>,
    pub request: AgentInteractionRequest,
    pub status: AgentInteractionStatus,
    pub resolution: Option<AgentInteractionResolution>,
}

impl AgentInteractionSnapshot {
    pub fn validate(&self) -> bool {
        match (&self.status, &self.resolution) {
            (AgentInteractionStatus::Pending, None) => true,
            (AgentInteractionStatus::Resolved, Some(resolution)) => matches!(
                (&self.request, resolution),
                (
                    AgentInteractionRequest::Approval { .. },
                    AgentInteractionResolution::Approved
                        | AgentInteractionResolution::Denied { .. }
                ) | (
                    AgentInteractionRequest::UserInput { .. },
                    AgentInteractionResolution::UserInput { .. }
                ) | (
                    AgentInteractionRequest::McpElicitation { .. },
                    AgentInteractionResolution::McpElicitation { .. }
                ) | (
                    AgentInteractionRequest::DynamicTool { .. },
                    AgentInteractionResolution::DynamicToolResult { .. }
                )
            ),
            (
                AgentInteractionStatus::Cancelled,
                Some(AgentInteractionResolution::Cancelled { .. }),
            )
            | (AgentInteractionStatus::Expired, Some(AgentInteractionResolution::Expired))
            | (AgentInteractionStatus::Lost, Some(AgentInteractionResolution::Lost { .. })) => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum AgentActiveTurnKind {
    Conversation,
    ContextCompaction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum AgentActiveTurnPhase {
    Running,
    Applied,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentActiveTurnSnapshot {
    pub turn_id: AgentTurnId,
    pub kind: AgentActiveTurnKind,
    pub phase: AgentActiveTurnPhase,
    #[serde(with = "crate::wire_u64")]
    #[schemars(with = "crate::wire_u64::RuntimeU64")]
    #[ts(type = "RuntimeU64")]
    pub started_at_ms: u64,
    pub cancellable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum AgentCompactionOutcomeStatus {
    Succeeded,
    Failed,
    Lost,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentCompactionOutcomeSnapshot {
    pub turn_id: AgentTurnId,
    pub status: AgentCompactionOutcomeStatus,
    #[serde(with = "crate::wire_u64")]
    #[schemars(with = "crate::wire_u64::RuntimeU64")]
    #[ts(type = "RuntimeU64")]
    pub completed_at_ms: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentQueuedCompactionSnapshot {
    #[serde(with = "crate::wire_u64")]
    #[schemars(with = "crate::wire_u64::RuntimeU64")]
    #[ts(type = "RuntimeU64")]
    pub queued_at_ms: u64,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "snake_case")]
pub enum AgentControlKind {
    SubmitInput,
    Steer,
    Interrupt,
    RequestCompaction,
    ResolveInteraction,
    Close,
    Fork,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum AgentControlUnavailabilityReason {
    AgentNotActive,
    ActiveTurnRequired,
    NoActiveTurnRequired,
    ActiveTurnNotSteerable,
    CompactionInProgress,
    TurnNotCancellable,
    PendingInteractionRequired,
    SourceLost,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentControlAvailabilityEvidence {
    pub expected_snapshot_revision: AgentSnapshotRevision,
    pub expected_turn_id: Option<AgentTurnId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum AgentControlAvailability {
    Available {
        evidence: AgentControlAvailabilityEvidence,
    },
    Unavailable {
        reason: AgentControlUnavailabilityReason,
        evidence: AgentControlAvailabilityEvidence,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentExecutionSnapshot {
    pub active_turn: Option<AgentActiveTurnSnapshot>,
    pub queued_compaction: Option<AgentQueuedCompactionSnapshot>,
    pub last_compaction_outcome: Option<AgentCompactionOutcomeSnapshot>,
}

impl AgentExecutionSnapshot {
    /// Derives the control surface from the same owner execution fact published in the snapshot.
    ///
    /// Concrete adapters decide whether an active turn is cancellable. The shared policy then
    /// guarantees that every consumer sees the same Submit/Steer/Compaction command matrix.
    pub fn command_availability(
        &self,
        lifecycle: AgentLifecycleStatus,
        revision: AgentSnapshotRevision,
        has_pending_interaction: bool,
    ) -> BTreeMap<AgentControlKind, AgentControlAvailability> {
        let evidence = AgentControlAvailabilityEvidence {
            expected_snapshot_revision: revision,
            expected_turn_id: self.active_turn.as_ref().map(|turn| turn.turn_id.clone()),
        };
        let active = lifecycle == AgentLifecycleStatus::Active;
        let active_turn = self.active_turn.as_ref();
        let compaction_active =
            active_turn.is_some_and(|turn| turn.kind == AgentActiveTurnKind::ContextCompaction);
        let compaction_pending = compaction_active || self.queued_compaction.is_some();

        [
            AgentControlKind::SubmitInput,
            AgentControlKind::Steer,
            AgentControlKind::Interrupt,
            AgentControlKind::RequestCompaction,
            AgentControlKind::ResolveInteraction,
            AgentControlKind::Close,
            AgentControlKind::Fork,
        ]
        .into_iter()
        .map(|command| {
            let unavailable_reason = if !active {
                Some(if lifecycle == AgentLifecycleStatus::Lost {
                    AgentControlUnavailabilityReason::SourceLost
                } else {
                    AgentControlUnavailabilityReason::AgentNotActive
                })
            } else {
                match command {
                    AgentControlKind::SubmitInput if compaction_active => None,
                    AgentControlKind::SubmitInput if active_turn.is_some() => {
                        Some(AgentControlUnavailabilityReason::NoActiveTurnRequired)
                    }
                    AgentControlKind::SubmitInput => None,
                    AgentControlKind::Steer if compaction_active => {
                        Some(AgentControlUnavailabilityReason::ActiveTurnNotSteerable)
                    }
                    AgentControlKind::Steer if active_turn.is_none() => {
                        Some(AgentControlUnavailabilityReason::ActiveTurnRequired)
                    }
                    AgentControlKind::Steer => None,
                    AgentControlKind::Interrupt if active_turn.is_none() => {
                        Some(AgentControlUnavailabilityReason::ActiveTurnRequired)
                    }
                    AgentControlKind::Interrupt
                        if active_turn.is_some_and(|turn| !turn.cancellable) =>
                    {
                        Some(AgentControlUnavailabilityReason::TurnNotCancellable)
                    }
                    AgentControlKind::Interrupt => None,
                    AgentControlKind::RequestCompaction if compaction_pending => {
                        Some(AgentControlUnavailabilityReason::CompactionInProgress)
                    }
                    AgentControlKind::RequestCompaction if active_turn.is_some() => None,
                    AgentControlKind::RequestCompaction => None,
                    AgentControlKind::ResolveInteraction if !has_pending_interaction => {
                        Some(AgentControlUnavailabilityReason::PendingInteractionRequired)
                    }
                    AgentControlKind::ResolveInteraction => None,
                    AgentControlKind::Close if compaction_pending => {
                        Some(AgentControlUnavailabilityReason::CompactionInProgress)
                    }
                    AgentControlKind::Close => None,
                    AgentControlKind::Fork if active_turn.is_some() || compaction_pending => {
                        Some(if compaction_pending {
                            AgentControlUnavailabilityReason::CompactionInProgress
                        } else {
                            AgentControlUnavailabilityReason::NoActiveTurnRequired
                        })
                    }
                    AgentControlKind::Fork => None,
                }
            };
            (
                command,
                match unavailable_reason {
                    None => AgentControlAvailability::Available {
                        evidence: evidence.clone(),
                    },
                    Some(reason) => AgentControlAvailability::Unavailable {
                        reason,
                        evidence: evidence.clone(),
                    },
                },
            )
        })
        .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentObservation {
    pub revision: AgentSnapshotRevision,
    pub context: crate::AgentContextCoordinate,
    pub lifecycle: AgentLifecycleStatus,
    pub execution: AgentExecutionSnapshot,
    pub command_availability: BTreeMap<AgentControlKind, AgentControlAvailability>,
    pub interactions: Vec<AgentInteractionSnapshot>,
    pub thread_name: Option<AgentThreadNameSnapshot>,
    pub source_info: AgentSnapshotSource,
    #[ts(type = "Array<CanonicalConversationRecord>")]
    pub conversation: Vec<CanonicalConversationRecord>,
}

impl AgentObservation {
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
pub struct AgentSnapshot {
    pub source: AgentSourceCoordinate,
    pub observation: AgentObservation,
    pub applied_surface: Option<crate::AppliedAgentSurface>,
    pub initial_context: Option<crate::AppliedInitialContextEvidence>,
}

impl AgentSnapshot {
    pub fn conversation(&self) -> agentdash_agent_protocol::CanonicalConversationView<'_> {
        self.observation.conversation()
    }

    pub fn active_turn_id(&self) -> Option<&str> {
        self.observation.active_turn_id()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentReadQuery {
    pub source: AgentSourceCoordinate,
    pub at_revision: Option<AgentSnapshotRevision>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentObservationQuery {
    pub source: AgentSourceCoordinate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentTurnObservation {
    pub turn_id: AgentTurnId,
    pub status: agentdash_agent_protocol::codex_app_server_protocol::TurnStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentSourceState {
    pub source: AgentSourceCoordinate,
    pub revision: AgentSnapshotRevision,
    pub lifecycle: AgentLifecycleStatus,
    pub execution: AgentExecutionSnapshot,
    pub command_availability: BTreeMap<AgentControlKind, AgentControlAvailability>,
    pub latest_turn: Option<AgentTurnObservation>,
}

impl AgentSourceState {
    pub fn from_snapshot(snapshot: &AgentSnapshot) -> Result<Self, String> {
        let conversation = snapshot.conversation();
        let latest_turn = conversation
            .latest_turn()
            .map(|turn| -> Result<AgentTurnObservation, String> {
                Ok(AgentTurnObservation {
                    turn_id: AgentTurnId::new(turn.id.clone())
                        .map_err(|error| error.to_string())?,
                    status: turn.status,
                })
            })
            .transpose()?;
        Ok(Self {
            source: snapshot.source.clone(),
            revision: snapshot.observation.revision,
            lifecycle: snapshot.observation.lifecycle,
            execution: snapshot.observation.execution.clone(),
            command_availability: snapshot.observation.command_availability.clone(),
            latest_turn,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentChangesQuery {
    pub source: AgentSourceCoordinate,
    pub after: Option<AgentSourceCursor>,
    pub limit: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
pub enum AgentChangePayload {
    /// One source observation may update normalized service state and append zero or more
    /// immutable presentation records. Runtime must preserve both parts atomically.
    SourceObservation {
        state: Option<Box<AgentChangePayload>>,
        presentation: Vec<CanonicalConversationRecord>,
    },
    ThreadNameChanged {
        thread_name: Option<String>,
        source_info: AgentSnapshotSource,
    },
    LifecycleChanged {
        status: AgentLifecycleStatus,
    },
    ExecutionChanged {
        execution: AgentExecutionSnapshot,
        command_availability: BTreeMap<AgentControlKind, AgentControlAvailability>,
    },
    InteractionChanged {
        interaction: AgentInteractionSnapshot,
    },
    SurfaceApplied {
        applied: crate::AppliedAgentSurface,
    },
    SnapshotInvalidated {
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentChange {
    pub cursor: AgentSourceCursor,
    pub source_revision: Option<AgentSourceRevision>,
    #[serde(with = "crate::wire_u64")]
    #[schemars(with = "crate::wire_u64::RuntimeU64")]
    #[ts(type = "RuntimeU64")]
    pub occurred_at_ms: u64,
    pub payload: AgentChangePayload,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentChangePage {
    pub source: AgentSourceCoordinate,
    pub changes: Vec<AgentChange>,
    pub next: Option<AgentSourceCursor>,
    pub gap: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_interaction_lifecycle_has_explicit_resolution_evidence() {
        let request = AgentInteractionRequest::Approval {
            prompt: "approve?".to_owned(),
            reason: Some("required".to_owned()),
            proposed_action: Some(serde_json::json!({"action": "apply"})),
        };
        for (status, resolution) in [
            (AgentInteractionStatus::Pending, None),
            (
                AgentInteractionStatus::Resolved,
                Some(AgentInteractionResolution::Approved),
            ),
            (
                AgentInteractionStatus::Cancelled,
                Some(AgentInteractionResolution::Cancelled {
                    reason: Some("cancelled".to_owned()),
                }),
            ),
            (
                AgentInteractionStatus::Expired,
                Some(AgentInteractionResolution::Expired),
            ),
            (
                AgentInteractionStatus::Lost,
                Some(AgentInteractionResolution::Lost {
                    reason: "source lost".to_owned(),
                }),
            ),
        ] {
            let snapshot = AgentInteractionSnapshot {
                id: AgentInteractionId::new(format!("interaction-{status:?}"))
                    .expect("interaction id"),
                turn_id: AgentTurnId::new("turn-1").expect("turn id"),
                item_id: None,
                request: request.clone(),
                status,
                resolution,
            };
            assert!(snapshot.validate());
            let encoded = serde_json::to_value(&snapshot).expect("serialize");
            let decoded: AgentInteractionSnapshot =
                serde_json::from_value(encoded).expect("deserialize");
            assert_eq!(decoded, snapshot);
        }
    }

    #[test]
    fn resolved_interaction_requires_a_resolution_from_the_same_request_family() {
        let snapshot = AgentInteractionSnapshot {
            id: AgentInteractionId::new("interaction-mismatch").expect("interaction id"),
            turn_id: AgentTurnId::new("turn-1").expect("turn id"),
            item_id: None,
            request: AgentInteractionRequest::Approval {
                prompt: "approve?".to_owned(),
                reason: None,
                proposed_action: None,
            },
            status: AgentInteractionStatus::Resolved,
            resolution: Some(AgentInteractionResolution::UserInput {
                answers: serde_json::json!({"answer": "wrong family"}),
            }),
        };

        assert!(!snapshot.validate());
    }
}
