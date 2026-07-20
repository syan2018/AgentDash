use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use agentdash_agent_protocol::CanonicalConversationRecord;

use crate::{
    AgentInteractionId, AgentInteractionRequest, AgentInteractionResolution,
    AgentInteractionStatus, AgentItemId, AgentItemPresentation, AgentItemTransition,
    AgentSnapshotRevision, AgentSourceCoordinate, AgentSourceCursor, AgentSourceRevision,
    AgentTurnId, SemanticFidelity,
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

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "snake_case")]
pub enum AgentEntityStatus {
    Accepted,
    Running,
    Completed,
    Failed,
    Interrupted,
    Lost,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentItemSnapshot {
    pub id: AgentItemId,
    pub status: AgentEntityStatus,
    pub presentation: AgentItemPresentation,
}

impl AgentItemSnapshot {
    pub fn validate(&self) -> Result<(), crate::AgentPresentationViolation> {
        self.presentation.validate_for_status(self.status)
    }

    pub fn from_transition(
        id: AgentItemId,
        previous: Option<&Self>,
        transition: AgentItemTransition,
    ) -> Result<Self, AgentItemFoldError> {
        let (status, presentation) = match transition {
            AgentItemTransition::Started { presentation } => {
                if previous.is_some() {
                    return Err(AgentItemFoldError::AlreadyStarted);
                }
                (AgentEntityStatus::Running, presentation)
            }
            AgentItemTransition::Updated { presentation, .. } => {
                let previous = previous.ok_or(AgentItemFoldError::NotStarted)?;
                if is_terminal(previous.status) {
                    return Err(AgentItemFoldError::AlreadyTerminal);
                }
                (AgentEntityStatus::Running, presentation)
            }
            AgentItemTransition::Terminal { presentation } => {
                let previous = previous.ok_or(AgentItemFoldError::NotStarted)?;
                if is_terminal(previous.status) {
                    return Err(AgentItemFoldError::AlreadyTerminal);
                }
                let status = match presentation.terminal.as_ref().map(|value| value.outcome) {
                    Some(crate::AgentTerminalStatus::Completed) => AgentEntityStatus::Completed,
                    Some(crate::AgentTerminalStatus::Failed) => AgentEntityStatus::Failed,
                    Some(crate::AgentTerminalStatus::Interrupted) => AgentEntityStatus::Interrupted,
                    Some(crate::AgentTerminalStatus::Lost) => AgentEntityStatus::Lost,
                    None => return Err(AgentItemFoldError::MissingTerminalEvidence),
                };
                (status, presentation)
            }
        };
        let item = Self {
            id,
            status,
            presentation,
        };
        item.validate().map_err(AgentItemFoldError::Presentation)?;
        Ok(item)
    }
}

fn is_terminal(status: AgentEntityStatus) -> bool {
    matches!(
        status,
        AgentEntityStatus::Completed
            | AgentEntityStatus::Failed
            | AgentEntityStatus::Interrupted
            | AgentEntityStatus::Lost
    )
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AgentItemFoldError {
    #[error("item has already started")]
    AlreadyStarted,
    #[error("item has not started")]
    NotStarted,
    #[error("item is already terminal")]
    AlreadyTerminal,
    #[error("terminal transition has no terminal evidence")]
    MissingTerminalEvidence,
    #[error(transparent)]
    Presentation(#[from] crate::AgentPresentationViolation),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentExecutionFailure {
    pub code: String,
    pub message: String,
    pub retryable: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentTurnSnapshot {
    pub id: AgentTurnId,
    pub status: AgentEntityStatus,
    pub items: Vec<AgentItemSnapshot>,
    pub error: Option<AgentExecutionFailure>,
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
    pub active_turn_id: Option<AgentTurnId>,
    pub turns: Vec<AgentTurnSnapshot>,
    pub interactions: Vec<AgentInteractionSnapshot>,
    pub thread_name: Option<AgentThreadNameSnapshot>,
    pub source_info: AgentSnapshotSource,
    pub applied_surface: Option<crate::AppliedAgentSurface>,
    pub initial_context: Option<crate::AppliedInitialContextEvidence>,
    pub conversation_history: Vec<CanonicalConversationRecord>,
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
        state: Box<AgentChangePayload>,
        presentation: Vec<CanonicalConversationRecord>,
    },
    ThreadNameChanged {
        thread_name: Option<String>,
        source_info: AgentSnapshotSource,
    },
    LifecycleChanged {
        status: AgentLifecycleStatus,
    },
    TurnChanged {
        turn: AgentTurnSnapshot,
    },
    ActiveTurnChanged {
        active_turn_id: Option<AgentTurnId>,
    },
    ItemChanged {
        turn_id: AgentTurnId,
        item: AgentItemSnapshot,
    },
    ItemTransitioned {
        turn_id: AgentTurnId,
        item_id: AgentItemId,
        transition: AgentItemTransition,
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
    use crate::{
        AgentContentBlock, AgentItemBody, AgentItemTerminalEvidence, AgentItemUpdate,
        AgentPresentationError, AgentTerminalStatus,
    };

    fn running(text: &str) -> AgentItemPresentation {
        AgentItemPresentation::new(
            AgentItemBody::AgentMessage {
                content: vec![AgentContentBlock::Text {
                    text: text.to_owned(),
                }],
                phase: None,
            },
            Some(1),
            Some(2),
            None,
        )
        .expect("running presentation")
    }

    fn terminal(outcome: AgentTerminalStatus) -> AgentItemPresentation {
        AgentItemPresentation::new(
            AgentItemBody::AgentMessage {
                content: vec![AgentContentBlock::Text {
                    text: format!("{outcome:?}"),
                }],
                phase: None,
            },
            Some(1),
            Some(3),
            Some(AgentItemTerminalEvidence {
                outcome,
                completed_at_ms: None,
                duration_ms: None,
                process_exit: None,
                error: (outcome != AgentTerminalStatus::Completed).then(|| {
                    AgentPresentationError {
                        code: "terminal".to_owned(),
                        message: "terminal outcome".to_owned(),
                        retryable: false,
                    }
                }),
            }),
        )
        .expect("terminal presentation")
    }

    #[test]
    fn typed_transition_fold_covers_every_terminal_outcome() {
        for (outcome, expected) in [
            (AgentTerminalStatus::Completed, AgentEntityStatus::Completed),
            (AgentTerminalStatus::Failed, AgentEntityStatus::Failed),
            (
                AgentTerminalStatus::Interrupted,
                AgentEntityStatus::Interrupted,
            ),
            (AgentTerminalStatus::Lost, AgentEntityStatus::Lost),
        ] {
            let item_id = AgentItemId::new(format!("item-{outcome:?}")).expect("item id");
            let started = AgentItemSnapshot::from_transition(
                item_id.clone(),
                None,
                AgentItemTransition::Started {
                    presentation: running("started"),
                },
            )
            .expect("started");
            let updated = AgentItemSnapshot::from_transition(
                item_id.clone(),
                Some(&started),
                AgentItemTransition::Updated {
                    update: AgentItemUpdate::TextAppended {
                        text: "updated".to_owned(),
                    },
                    presentation: running("started updated"),
                },
            )
            .expect("updated");
            let settled = AgentItemSnapshot::from_transition(
                item_id,
                Some(&updated),
                AgentItemTransition::Terminal {
                    presentation: terminal(outcome),
                },
            )
            .expect("terminal");
            assert_eq!(settled.status, expected);
            settled.validate().expect("settled presentation");
        }
    }

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
