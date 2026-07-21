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
    #[schemars(with = "crate::wire_u64::AgentServiceU64")]
    #[ts(type = "AgentServiceU64")]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentSnapshot {
    pub source: AgentSourceCoordinate,
    pub revision: AgentSnapshotRevision,
    pub lifecycle: AgentLifecycleStatus,
    pub interactions: Vec<AgentInteractionSnapshot>,
    pub thread_name: Option<AgentThreadNameSnapshot>,
    pub source_info: AgentSnapshotSource,
    pub applied_surface: Option<crate::AppliedAgentSurface>,
    pub initial_context: Option<crate::AppliedInitialContextEvidence>,
    pub conversation_history: Vec<CanonicalConversationRecord>,
}

impl AgentSnapshot {
    pub fn conversation(&self) -> agentdash_agent_protocol::CanonicalConversationView<'_> {
        agentdash_agent_protocol::CanonicalConversationView::new(&self.conversation_history)
    }

    pub fn active_turn_id(&self) -> Option<&str> {
        self.conversation()
            .active_turn()
            .map(|turn| turn.id.as_str())
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
    #[schemars(with = "crate::wire_u64::AgentServiceU64")]
    #[ts(type = "AgentServiceU64")]
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
