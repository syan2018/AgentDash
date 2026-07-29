use std::collections::{BTreeMap, BTreeSet};

use agentdash_agent_protocol::{BackboneEvent, CanonicalConversationView};

use agentdash_agent_runtime_contract::{
    AgentRuntimeActiveTurn, AgentRuntimeAvailabilityEvidence, AgentRuntimeCommandAvailability,
    AgentRuntimeCommandKind, AgentRuntimeCompactionOutcome, AgentRuntimeContextCoordinate,
    AgentRuntimeExecutionStatus, AgentRuntimeExecutionView, AgentRuntimeInteraction,
    AgentRuntimeInteractionRequest, AgentRuntimeInteractionResolution,
    AgentRuntimeInteractionStatus, AgentRuntimeLifecycleStatus, AgentRuntimeProjectionAuthority,
    AgentRuntimeProjectionFidelity, AgentRuntimeQueuedCompaction, AgentRuntimeThreadNameSource,
    AgentRuntimeUnavailabilityReason, AgentRuntimeView, RuntimeInteractionId, RuntimeItemId,
    RuntimeOperationId, RuntimePayloadDigest, RuntimeProjectionRevision, RuntimeThreadId,
    RuntimeTurnId, SurfaceRevision,
};
use agentdash_agent_service_api::{
    AgentContextAuthority, AgentContextFidelity, AgentControlAvailability, AgentControlKind,
    AgentControlUnavailabilityReason, AgentInteractionStatus, AgentLifecycleStatus, AgentSnapshot,
    AgentSnapshotAuthority, AgentSnapshotSource, SemanticFidelity,
};
use serde::{Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AgentSnapshotProjectionError {
    #[error("Complete Agent snapshot is invalid: {reason}")]
    InvalidSnapshot { reason: String },
    #[error("Complete Agent presentation cannot be mapped to the Product Runtime view: {reason}")]
    Presentation { reason: String },
}

/// Builds a request-scoped Product presentation from one authoritative Complete Agent snapshot.
///
/// The mapping keeps no Runtime journal, cursor, operation ledger, or source identity registry.
/// Runtime-facing ids are deterministic aliases of concrete-Agent coordinates, so reconnecting
/// and reading the same Agent state reconstructs the same presentation.
pub fn project_authoritative_agent_view(
    thread_id: RuntimeThreadId,
    snapshot: AgentSnapshot,
) -> Result<AgentRuntimeView, AgentSnapshotProjectionError> {
    validate_conversation_history(&snapshot)?;
    let revision = RuntimeProjectionRevision(snapshot.revision.0);
    if snapshot.context.snapshot_revision != snapshot.revision {
        return invalid("snapshot context coordinate does not match snapshot revision");
    }
    let context = AgentRuntimeContextCoordinate {
        snapshot_revision: RuntimeProjectionRevision(snapshot.context.snapshot_revision.0),
        context_revision: snapshot.context.context_revision,
        recipe_digest: RuntimePayloadDigest::new(
            snapshot.context.recipe_digest.as_str().to_owned(),
        )
        .map_err(|error| presentation(error.to_string()))?,
        authority: match snapshot.context.authority {
            AgentContextAuthority::AgentOwned => {
                AgentRuntimeProjectionAuthority::SourceAuthoritative
            }
            AgentContextAuthority::AgentObserved => AgentRuntimeProjectionAuthority::SourceObserved,
        },
        fidelity: match snapshot.context.fidelity {
            AgentContextFidelity::Exact => AgentRuntimeProjectionFidelity::Exact,
            AgentContextFidelity::Observed => AgentRuntimeProjectionFidelity::Observed,
        },
    };
    let captured_at_ms = snapshot.source_info.observed_at_ms;
    let applied_surface_revision = snapshot
        .applied_surface
        .as_ref()
        .map(|surface| SurfaceRevision(surface.revision.0));

    let mut known_turns = BTreeSet::new();
    let mut known_items = BTreeMap::new();
    for record in &snapshot.conversation_history {
        match &record.presentation.envelope.event {
            BackboneEvent::TurnStarted(notification) => {
                known_turns.insert(notification.turn.id.clone());
            }
            BackboneEvent::TurnCompleted(notification) => {
                known_turns.insert(notification.turn.id.clone());
            }
            BackboneEvent::ItemStarted(notification) => {
                known_turns.insert(notification.turn_id.clone());
                known_items.insert(
                    notification.item.id().to_owned(),
                    notification.turn_id.clone(),
                );
            }
            BackboneEvent::ItemUpdated(notification) => {
                known_turns.insert(notification.turn_id.clone());
                known_items.insert(
                    notification.item.id().to_owned(),
                    notification.turn_id.clone(),
                );
            }
            BackboneEvent::ItemCompleted(notification) => {
                known_turns.insert(notification.turn_id.clone());
                known_items.insert(
                    notification.item.id().to_owned(),
                    notification.turn_id.clone(),
                );
            }
            _ => {}
        }
    }

    let mut known_interactions = BTreeSet::new();
    let mut interactions = Vec::with_capacity(snapshot.interactions.len());
    for interaction in &snapshot.interactions {
        if !known_interactions.insert(interaction.id.clone()) {
            return invalid("snapshot contains a duplicate interaction id");
        }
        if !interaction.validate() || !known_turns.contains(interaction.turn_id.as_str()) {
            return invalid("interaction status or turn coordinate is invalid");
        }
        if interaction.item_id.as_ref().is_some_and(|item_id| {
            known_items.get(item_id.as_str()).map(String::as_str)
                != Some(interaction.turn_id.as_str())
        }) {
            return invalid("interaction item does not belong to its turn");
        }
        let projected = AgentRuntimeInteraction {
            id: runtime_interaction_id(&interaction.id)?,
            turn_id: runtime_turn_id(&interaction.turn_id)?,
            item_id: interaction
                .item_id
                .as_ref()
                .map(runtime_item_id)
                .transpose()?,
            request: transcode::<_, AgentRuntimeInteractionRequest>(&interaction.request)?,
            status: project_interaction_status(interaction.status),
            resolution: interaction
                .resolution
                .as_ref()
                .map(transcode::<_, AgentRuntimeInteractionResolution>)
                .transpose()?,
        };
        if !projected.validate() {
            return invalid("projected interaction is invalid");
        }
        interactions.push(projected);
    }

    let (thread_name, thread_name_source) =
        project_thread_name(snapshot.thread_name, &snapshot.source)?;
    let conversation = CanonicalConversationView::new(&snapshot.conversation_history);
    let active_turn = snapshot
        .execution
        .active_turn
        .as_ref()
        .map(transcode::<_, AgentRuntimeActiveTurn>)
        .transpose()?;
    let last_compaction_outcome = snapshot
        .execution
        .last_compaction_outcome
        .as_ref()
        .map(transcode::<_, AgentRuntimeCompactionOutcome>)
        .transpose()?;
    let queued_compaction = snapshot
        .execution
        .queued_compaction
        .as_ref()
        .map(|queued| {
            Ok(AgentRuntimeQueuedCompaction {
                operation_id: RuntimeOperationId::new(queued.operation_id.as_str().to_owned())
                    .map_err(|error| AgentSnapshotProjectionError::InvalidSnapshot {
                        reason: error.to_string(),
                    })?,
                queued_at_ms: queued.queued_at_ms,
            })
        })
        .transpose()?;
    let latest_turn_id = conversation
        .latest_turn()
        .map(|turn| RuntimeTurnId::new(turn.id.clone()))
        .transpose()
        .map_err(|error| presentation(error.to_string()))?;
    let execution = AgentRuntimeExecutionView {
        status: if active_turn.is_some() {
            AgentRuntimeExecutionStatus::Active
        } else {
            AgentRuntimeExecutionStatus::Idle
        },
        active_turn,
        queued_compaction,
        last_compaction_outcome,
        latest_turn_id,
    };
    let command_availability = project_command_availability(
        &snapshot.command_availability,
        snapshot.lifecycle,
        applied_surface_revision,
    )?;

    Ok(AgentRuntimeView {
        thread_id,
        view_revision: revision,
        captured_at_ms,
        lifecycle: project_lifecycle(snapshot.lifecycle),
        execution,
        context,
        interactions,
        thread_name,
        thread_name_source,
        operations: Vec::new(),
        source_binding: None,
        authority: project_authority(snapshot.source_info.authority),
        fidelity: project_fidelity(snapshot.source_info.fidelity),
        command_availability,
        conversation: snapshot.conversation_history,
    })
}

fn validate_conversation_history(
    snapshot: &AgentSnapshot,
) -> Result<(), AgentSnapshotProjectionError> {
    let mut presentation_ids = BTreeSet::new();
    if snapshot.conversation_history.iter().any(|record| {
        record.presentation_id.trim().is_empty()
            || !presentation_ids.insert(record.presentation_id.clone())
    }) {
        return invalid("conversation history contains a blank or duplicate presentation id");
    }
    Ok(())
}

fn project_thread_name(
    thread_name: Option<agentdash_agent_service_api::AgentThreadNameSnapshot>,
    source: &agentdash_agent_service_api::AgentSourceCoordinate,
) -> Result<(Option<String>, Option<AgentRuntimeThreadNameSource>), AgentSnapshotProjectionError> {
    let Some(thread_name) = thread_name else {
        return Ok((None, None));
    };
    if thread_name.source_info.authority != AgentSnapshotAuthority::AgentAuthoritative
        || thread_name.source_info.fidelity != SemanticFidelity::Exact
        || thread_name
            .thread_name
            .as_ref()
            .is_some_and(|value| value.trim().is_empty())
    {
        return invalid("thread name must be source-authoritative, exact, and non-blank");
    }
    let evidence = project_thread_name_source(&thread_name.source_info, source)?;
    Ok((thread_name.thread_name, Some(evidence)))
}

fn project_thread_name_source(
    source_info: &AgentSnapshotSource,
    source: &agentdash_agent_service_api::AgentSourceCoordinate,
) -> Result<AgentRuntimeThreadNameSource, AgentSnapshotProjectionError> {
    Ok(AgentRuntimeThreadNameSource {
        authority: project_authority(source_info.authority),
        fidelity: project_fidelity(source_info.fidelity),
        source_identity_digest: opaque_digest(source.as_str())?,
        source_revision_digest: source_info
            .source_revision
            .as_ref()
            .map(|revision| opaque_digest(revision.as_str()))
            .transpose()?,
        observed_at_ms: source_info.observed_at_ms,
    })
}

fn project_command_availability(
    source: &BTreeMap<AgentControlKind, AgentControlAvailability>,
    lifecycle: AgentLifecycleStatus,
    applied_surface_revision: Option<SurfaceRevision>,
) -> Result<
    BTreeMap<AgentRuntimeCommandKind, AgentRuntimeCommandAvailability>,
    AgentSnapshotProjectionError,
> {
    let mut projected = [
        AgentRuntimeCommandKind::Create,
        AgentRuntimeCommandKind::Resume,
        AgentRuntimeCommandKind::Rebind,
        AgentRuntimeCommandKind::Activate,
    ]
    .into_iter()
    .map(|command| {
        let available = match command {
            AgentRuntimeCommandKind::Resume => lifecycle == AgentLifecycleStatus::Suspended,
            _ => false,
        };
        let evidence = AgentRuntimeAvailabilityEvidence {
            blocking_operation_id: None,
            expected_view_revision: None,
            expected_turn_id: None,
            bound_surface_revision: applied_surface_revision,
            applied_surface_revision,
        };
        let availability = if available {
            AgentRuntimeCommandAvailability::Available { evidence }
        } else {
            AgentRuntimeCommandAvailability::Unavailable {
                reason: AgentRuntimeUnavailabilityReason::AdmissionDenied,
                evidence,
            }
        };
        (command, availability)
    })
    .collect::<BTreeMap<_, _>>();
    for (command, availability) in source {
        let runtime_command = match command {
            AgentControlKind::SubmitInput => AgentRuntimeCommandKind::SubmitInput,
            AgentControlKind::Steer => AgentRuntimeCommandKind::Steer,
            AgentControlKind::Interrupt => AgentRuntimeCommandKind::Interrupt,
            AgentControlKind::RequestCompaction => AgentRuntimeCommandKind::RequestCompaction,
            AgentControlKind::ResolveInteraction => AgentRuntimeCommandKind::ResolveInteraction,
            AgentControlKind::Close => AgentRuntimeCommandKind::Close,
            AgentControlKind::Fork => AgentRuntimeCommandKind::Fork,
        };
        let (available, reason, evidence) = match availability {
            AgentControlAvailability::Available { evidence } => (true, None, evidence),
            AgentControlAvailability::Unavailable { reason, evidence } => {
                (false, Some(*reason), evidence)
            }
        };
        let evidence = AgentRuntimeAvailabilityEvidence {
            blocking_operation_id: evidence
                .blocking_operation_id
                .as_ref()
                .map(|id| {
                    agentdash_agent_runtime_contract::RuntimeOperationId::new(
                        id.as_str().to_owned(),
                    )
                    .map_err(|error| presentation(error.to_string()))
                })
                .transpose()?,
            expected_view_revision: Some(RuntimeProjectionRevision(
                evidence.expected_snapshot_revision.0,
            )),
            expected_turn_id: evidence
                .expected_turn_id
                .as_ref()
                .map(runtime_turn_id)
                .transpose()?,
            bound_surface_revision: applied_surface_revision,
            applied_surface_revision,
        };
        projected.insert(
            runtime_command,
            if available {
                AgentRuntimeCommandAvailability::Available { evidence }
            } else {
                AgentRuntimeCommandAvailability::Unavailable {
                    reason: project_unavailability_reason(
                        reason.expect("unavailable owner command has reason"),
                    ),
                    evidence,
                }
            },
        );
    }
    if projected.len() != AgentRuntimeCommandKind::ALL.len() {
        return invalid("owner command availability is incomplete");
    }
    Ok(projected)
}

fn project_unavailability_reason(
    reason: AgentControlUnavailabilityReason,
) -> AgentRuntimeUnavailabilityReason {
    match reason {
        AgentControlUnavailabilityReason::AgentNotActive => {
            AgentRuntimeUnavailabilityReason::RuntimeNotActive
        }
        AgentControlUnavailabilityReason::ActiveTurnRequired => {
            AgentRuntimeUnavailabilityReason::ActiveTurnRequired
        }
        AgentControlUnavailabilityReason::NoActiveTurnRequired => {
            AgentRuntimeUnavailabilityReason::NoActiveTurnRequired
        }
        AgentControlUnavailabilityReason::ActiveTurnNotSteerable => {
            AgentRuntimeUnavailabilityReason::ActiveTurnNotSteerable
        }
        AgentControlUnavailabilityReason::CompactionInProgress => {
            AgentRuntimeUnavailabilityReason::CompactionInProgress
        }
        AgentControlUnavailabilityReason::TurnNotCancellable => {
            AgentRuntimeUnavailabilityReason::TurnNotCancellable
        }
        AgentControlUnavailabilityReason::PendingInteractionRequired => {
            AgentRuntimeUnavailabilityReason::PendingInteractionRequired
        }
        AgentControlUnavailabilityReason::SourceLost => {
            AgentRuntimeUnavailabilityReason::SourceUnavailable
        }
    }
}

fn runtime_turn_id(
    source: &agentdash_agent_service_api::AgentTurnId,
) -> Result<RuntimeTurnId, AgentSnapshotProjectionError> {
    RuntimeTurnId::new(source.as_str().to_owned()).map_err(|error| presentation(error.to_string()))
}

fn runtime_item_id(
    source: &agentdash_agent_service_api::AgentItemId,
) -> Result<RuntimeItemId, AgentSnapshotProjectionError> {
    RuntimeItemId::new(source.as_str().to_owned()).map_err(|error| presentation(error.to_string()))
}

fn runtime_interaction_id(
    source: &agentdash_agent_service_api::AgentInteractionId,
) -> Result<RuntimeInteractionId, AgentSnapshotProjectionError> {
    RuntimeInteractionId::new(source.as_str().to_owned())
        .map_err(|error| presentation(error.to_string()))
}

fn transcode<T: Serialize + ?Sized, U: DeserializeOwned>(
    value: &T,
) -> Result<U, AgentSnapshotProjectionError> {
    serde_json::to_value(value)
        .and_then(serde_json::from_value)
        .map_err(|error| presentation(error.to_string()))
}

fn opaque_digest(value: &str) -> Result<RuntimePayloadDigest, AgentSnapshotProjectionError> {
    RuntimePayloadDigest::new(format!("sha256:{:x}", Sha256::digest(value.as_bytes())))
        .map_err(|error| presentation(error.to_string()))
}

fn project_lifecycle(status: AgentLifecycleStatus) -> AgentRuntimeLifecycleStatus {
    match status {
        AgentLifecycleStatus::Creating => AgentRuntimeLifecycleStatus::Provisioning,
        AgentLifecycleStatus::Active => AgentRuntimeLifecycleStatus::Active,
        AgentLifecycleStatus::Suspended => AgentRuntimeLifecycleStatus::Suspended,
        AgentLifecycleStatus::Closed => AgentRuntimeLifecycleStatus::Closed,
        AgentLifecycleStatus::Lost => AgentRuntimeLifecycleStatus::Lost,
    }
}

fn project_interaction_status(status: AgentInteractionStatus) -> AgentRuntimeInteractionStatus {
    match status {
        AgentInteractionStatus::Pending => AgentRuntimeInteractionStatus::Pending,
        AgentInteractionStatus::Resolved => AgentRuntimeInteractionStatus::Resolved,
        AgentInteractionStatus::Cancelled => AgentRuntimeInteractionStatus::Cancelled,
        AgentInteractionStatus::Expired => AgentRuntimeInteractionStatus::Expired,
        AgentInteractionStatus::Lost => AgentRuntimeInteractionStatus::Lost,
    }
}

fn project_authority(authority: AgentSnapshotAuthority) -> AgentRuntimeProjectionAuthority {
    match authority {
        AgentSnapshotAuthority::AgentAuthoritative => {
            AgentRuntimeProjectionAuthority::SourceAuthoritative
        }
        AgentSnapshotAuthority::AgentObserved => AgentRuntimeProjectionAuthority::SourceObserved,
        AgentSnapshotAuthority::Derived => AgentRuntimeProjectionAuthority::RuntimeDerived,
    }
}

fn project_fidelity(fidelity: SemanticFidelity) -> AgentRuntimeProjectionFidelity {
    match fidelity {
        SemanticFidelity::Unsupported => AgentRuntimeProjectionFidelity::Unsupported,
        SemanticFidelity::Observed => AgentRuntimeProjectionFidelity::Observed,
        SemanticFidelity::Approximation => AgentRuntimeProjectionFidelity::Approximation,
        SemanticFidelity::Exact => AgentRuntimeProjectionFidelity::Exact,
    }
}

fn invalid<T>(reason: impl Into<String>) -> Result<T, AgentSnapshotProjectionError> {
    Err(AgentSnapshotProjectionError::InvalidSnapshot {
        reason: reason.into(),
    })
}

fn presentation(reason: impl Into<String>) -> AgentSnapshotProjectionError {
    AgentSnapshotProjectionError::Presentation {
        reason: reason.into(),
    }
}

#[cfg(test)]
mod tests {
    use agentdash_agent_service_api::{
        AgentActiveTurnKind, AgentActiveTurnPhase, AgentActiveTurnSnapshot, AgentContextAuthority,
        AgentContextCoordinate, AgentContextFidelity, AgentControlAvailability,
        AgentControlAvailabilityEvidence, AgentControlKind, AgentControlUnavailabilityReason,
        AgentExecutionSnapshot, AgentPayloadDigest, AgentSnapshot, AgentSnapshotRevision,
        AgentSnapshotSource, AgentSourceCoordinate,
    };

    use super::*;

    fn snapshot() -> AgentSnapshot {
        AgentSnapshot {
            source: AgentSourceCoordinate::new("source-1").expect("source"),
            revision: AgentSnapshotRevision(7),
            context: AgentContextCoordinate {
                snapshot_revision: AgentSnapshotRevision(7),
                context_revision: Some("context-7".to_owned()),
                recipe_digest: AgentPayloadDigest::new("sha256:context-7").unwrap(),
                authority: AgentContextAuthority::AgentOwned,
                fidelity: AgentContextFidelity::Exact,
            },
            lifecycle: AgentLifecycleStatus::Active,
            execution: AgentExecutionSnapshot {
                active_turn: None,
                queued_compaction: None,
                last_compaction_outcome: None,
            },
            command_availability: [
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
                (
                    command,
                    AgentControlAvailability::Unavailable {
                        reason: AgentControlUnavailabilityReason::ActiveTurnRequired,
                        evidence: AgentControlAvailabilityEvidence {
                            expected_snapshot_revision: AgentSnapshotRevision(7),
                            expected_turn_id: None,
                            blocking_operation_id: None,
                        },
                    },
                )
            })
            .collect(),
            interactions: Vec::new(),
            thread_name: None,
            source_info: AgentSnapshotSource {
                authority: AgentSnapshotAuthority::AgentAuthoritative,
                source_revision: None,
                fidelity: SemanticFidelity::Exact,
                observed_at_ms: 42,
            },
            applied_surface: None,
            initial_context: None,
            conversation_history: Vec::new(),
        }
    }

    #[test]
    fn authoritative_snapshot_projects_without_runtime_state() {
        let projected = project_authoritative_agent_view(
            RuntimeThreadId::new("thread-1").expect("thread"),
            snapshot(),
        )
        .expect("projection");

        assert_eq!(projected.view_revision, RuntimeProjectionRevision(7));
        assert_eq!(
            projected.execution.status,
            AgentRuntimeExecutionStatus::Idle
        );
        assert!(projected.operations.is_empty());
        assert!(projected.source_binding.is_none());
        assert_eq!(
            projected.authority,
            AgentRuntimeProjectionAuthority::SourceAuthoritative
        );
        assert_eq!(
            projected.context.snapshot_revision,
            RuntimeProjectionRevision(7)
        );
        assert_eq!(
            projected.context.context_revision.as_deref(),
            Some("context-7")
        );
        assert_eq!(projected.context.recipe_digest.as_str(), "sha256:context-7");
    }

    #[test]
    fn execution_uses_complete_agent_fact_even_when_presentation_has_not_arrived() {
        let mut snapshot = snapshot();
        snapshot.execution.active_turn = Some(AgentActiveTurnSnapshot {
            turn_id: agentdash_agent_service_api::AgentTurnId::new("turn-1").expect("turn"),
            kind: AgentActiveTurnKind::Conversation,
            phase: AgentActiveTurnPhase::Running,
            operation_id: None,
            started_at_ms: 42,
            cancellable: true,
        });

        let projected = project_authoritative_agent_view(
            RuntimeThreadId::new("thread-1").expect("thread"),
            snapshot,
        )
        .expect("projection");

        assert_eq!(
            projected.execution.status,
            AgentRuntimeExecutionStatus::Active
        );
        assert_eq!(projected.active_turn_id(), Some("turn-1"));
        assert!(projected.conversation.is_empty());
    }

    #[test]
    fn compaction_turn_and_owner_policy_are_projected_without_reinterpretation() {
        let mut snapshot = snapshot();
        let operation_id =
            agentdash_agent_service_api::AgentEffectIdentity::new("effect-compact").unwrap();
        snapshot.execution.active_turn = Some(AgentActiveTurnSnapshot {
            turn_id: agentdash_agent_service_api::AgentTurnId::new("turn-compact").unwrap(),
            kind: AgentActiveTurnKind::ContextCompaction,
            phase: AgentActiveTurnPhase::Applied,
            operation_id: Some(operation_id.clone()),
            started_at_ms: 42,
            cancellable: false,
        });
        snapshot.command_availability.insert(
            AgentControlKind::Interrupt,
            AgentControlAvailability::Unavailable {
                reason: AgentControlUnavailabilityReason::TurnNotCancellable,
                evidence: AgentControlAvailabilityEvidence {
                    expected_snapshot_revision: AgentSnapshotRevision(7),
                    expected_turn_id: Some(
                        agentdash_agent_service_api::AgentTurnId::new("turn-compact").unwrap(),
                    ),
                    blocking_operation_id: Some(operation_id),
                },
            },
        );

        let projected =
            project_authoritative_agent_view(RuntimeThreadId::new("thread-1").unwrap(), snapshot)
                .unwrap();
        let active = projected.execution.active_turn.expect("active compaction");
        assert_eq!(
            active.kind,
            agentdash_agent_runtime_contract::AgentRuntimeActiveTurnKind::ContextCompaction
        );
        assert_eq!(
            active.phase,
            agentdash_agent_runtime_contract::AgentRuntimeActiveTurnPhase::Applied
        );
        assert_eq!(
            active.operation_id.as_ref().map(|id| id.as_str()),
            Some("effect-compact")
        );
        assert!(matches!(
            projected
                .command_availability
                .get(&AgentRuntimeCommandKind::Interrupt),
            Some(AgentRuntimeCommandAvailability::Unavailable {
                reason: AgentRuntimeUnavailabilityReason::TurnNotCancellable,
                evidence,
            }) if evidence.expected_turn_id.as_ref().map(|id| id.as_str())
                == Some("turn-compact")
        ));
    }
}
