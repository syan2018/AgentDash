use std::collections::{BTreeMap, BTreeSet};

use agentdash_agent_runtime_contract::{
    ManagedRuntimeAvailabilityEvidence, ManagedRuntimeCommandAvailability,
    ManagedRuntimeCommandKind, ManagedRuntimeEntityStatus, ManagedRuntimeInteraction,
    ManagedRuntimeInteractionRequest, ManagedRuntimeInteractionResolution,
    ManagedRuntimeInteractionStatus, ManagedRuntimeItem, ManagedRuntimeItemPresentation,
    ManagedRuntimeLifecycleStatus, ManagedRuntimeProjectionAuthority,
    ManagedRuntimeProjectionFidelity, ManagedRuntimeSnapshot, ManagedRuntimeThreadNameSource,
    ManagedRuntimeTurn, ManagedRuntimeUnavailabilityReason, RuntimeInteractionId, RuntimeItemId,
    RuntimePayloadDigest, RuntimeProjectionRevision, RuntimeThreadId, RuntimeTurnId,
    SurfaceRevision,
};
use agentdash_agent_service_api::{
    AgentEntityStatus, AgentInteractionStatus, AgentLifecycleStatus, AgentSnapshot,
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
pub fn project_authoritative_agent_snapshot(
    thread_id: RuntimeThreadId,
    snapshot: AgentSnapshot,
) -> Result<ManagedRuntimeSnapshot, AgentSnapshotProjectionError> {
    validate_conversation_history(&snapshot)?;
    let revision = RuntimeProjectionRevision(snapshot.revision.0);
    let captured_at_ms = snapshot.source_info.observed_at_ms;
    let applied_surface_revision = snapshot
        .applied_surface
        .as_ref()
        .map(|surface| SurfaceRevision(surface.revision.0));

    let mut known_turns = BTreeSet::new();
    let mut known_items = BTreeMap::new();
    let mut turns = Vec::with_capacity(snapshot.turns.len());
    let mut items = Vec::new();
    for turn in &snapshot.turns {
        if !known_turns.insert(turn.id.clone()) {
            return invalid("snapshot contains a duplicate turn id");
        }
        let runtime_turn_id = runtime_turn_id(&turn.id)?;
        let mut item_ids = Vec::with_capacity(turn.items.len());
        for item in &turn.items {
            item.validate()
                .map_err(|error| AgentSnapshotProjectionError::InvalidSnapshot {
                    reason: error.to_string(),
                })?;
            if known_items
                .insert(item.id.clone(), turn.id.clone())
                .is_some()
            {
                return invalid("snapshot contains a duplicate item id");
            }
            let runtime_item_id = runtime_item_id(&item.id)?;
            let status = project_entity_status(item.status);
            let presentation: ManagedRuntimeItemPresentation = transcode(&item.presentation)?;
            presentation.validate_for_status(status).map_err(|error| {
                AgentSnapshotProjectionError::InvalidSnapshot {
                    reason: error.to_string(),
                }
            })?;
            item_ids.push(runtime_item_id.clone());
            items.push(ManagedRuntimeItem {
                id: runtime_item_id,
                turn_id: runtime_turn_id.clone(),
                status,
                presentation,
            });
        }
        turns.push(ManagedRuntimeTurn {
            id: runtime_turn_id,
            source_turn_id: turn.id.to_string(),
            status: project_entity_status(turn.status),
            item_ids,
        });
    }

    if snapshot
        .active_turn_id
        .as_ref()
        .is_some_and(|turn_id| !known_turns.contains(turn_id))
    {
        return invalid("active turn does not exist in the snapshot");
    }

    let mut known_interactions = BTreeSet::new();
    let mut interactions = Vec::with_capacity(snapshot.interactions.len());
    for interaction in &snapshot.interactions {
        if !known_interactions.insert(interaction.id.clone()) {
            return invalid("snapshot contains a duplicate interaction id");
        }
        if !interaction.validate() || !known_turns.contains(&interaction.turn_id) {
            return invalid("interaction status or turn coordinate is invalid");
        }
        if interaction
            .item_id
            .as_ref()
            .is_some_and(|item_id| known_items.get(item_id) != Some(&interaction.turn_id))
        {
            return invalid("interaction item does not belong to its turn");
        }
        let projected = ManagedRuntimeInteraction {
            id: runtime_interaction_id(&interaction.id)?,
            turn_id: runtime_turn_id(&interaction.turn_id)?,
            item_id: interaction
                .item_id
                .as_ref()
                .map(runtime_item_id)
                .transpose()?,
            request: transcode::<_, ManagedRuntimeInteractionRequest>(&interaction.request)?,
            status: project_interaction_status(interaction.status),
            resolution: interaction
                .resolution
                .as_ref()
                .map(transcode::<_, ManagedRuntimeInteractionResolution>)
                .transpose()?,
        };
        if !projected.validate() {
            return invalid("projected interaction is invalid");
        }
        interactions.push(projected);
    }

    let (thread_name, thread_name_source) =
        project_thread_name(snapshot.thread_name, &snapshot.source)?;
    let active_turn_id = snapshot
        .active_turn_id
        .as_ref()
        .map(runtime_turn_id)
        .transpose()?;
    let command_availability = presentation_command_availability(
        snapshot.lifecycle,
        snapshot.active_turn_id.is_some(),
        snapshot
            .interactions
            .iter()
            .any(|interaction| interaction.status == AgentInteractionStatus::Pending),
        applied_surface_revision,
        revision,
    );

    Ok(ManagedRuntimeSnapshot {
        thread_id,
        revision,
        captured_at_ms,
        lifecycle: project_lifecycle(snapshot.lifecycle),
        active_turn_id,
        turns,
        items,
        interactions,
        thread_name,
        thread_name_source,
        operations: Vec::new(),
        source_binding: None,
        authority: project_authority(snapshot.source_info.authority),
        fidelity: project_fidelity(snapshot.source_info.fidelity),
        command_availability,
        conversation_history: snapshot.conversation_history,
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
) -> Result<(Option<String>, Option<ManagedRuntimeThreadNameSource>), AgentSnapshotProjectionError>
{
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
) -> Result<ManagedRuntimeThreadNameSource, AgentSnapshotProjectionError> {
    Ok(ManagedRuntimeThreadNameSource {
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

fn presentation_command_availability(
    lifecycle: AgentLifecycleStatus,
    has_active_turn: bool,
    has_pending_interaction: bool,
    applied_surface_revision: Option<SurfaceRevision>,
    revision: RuntimeProjectionRevision,
) -> BTreeMap<ManagedRuntimeCommandKind, ManagedRuntimeCommandAvailability> {
    let active = lifecycle == AgentLifecycleStatus::Active;
    ManagedRuntimeCommandKind::ALL
        .into_iter()
        .map(|command| {
            let available = match command {
                ManagedRuntimeCommandKind::Create
                | ManagedRuntimeCommandKind::Activate
                | ManagedRuntimeCommandKind::Rebind => false,
                ManagedRuntimeCommandKind::Resume => lifecycle == AgentLifecycleStatus::Suspended,
                ManagedRuntimeCommandKind::SubmitInput
                | ManagedRuntimeCommandKind::RequestCompaction
                | ManagedRuntimeCommandKind::Fork => active && !has_active_turn,
                ManagedRuntimeCommandKind::Steer | ManagedRuntimeCommandKind::Interrupt => {
                    active && has_active_turn
                }
                ManagedRuntimeCommandKind::ResolveInteraction => active && has_pending_interaction,
                ManagedRuntimeCommandKind::Close => !matches!(
                    lifecycle,
                    AgentLifecycleStatus::Closed | AgentLifecycleStatus::Lost
                ),
            };
            let evidence = ManagedRuntimeAvailabilityEvidence {
                decided_at_revision: revision,
                blocking_operation_id: None,
                bound_surface_revision: applied_surface_revision,
                applied_surface_revision,
            };
            let availability = if available {
                ManagedRuntimeCommandAvailability::Available { evidence }
            } else {
                ManagedRuntimeCommandAvailability::Unavailable {
                    reason: if !active {
                        ManagedRuntimeUnavailabilityReason::RuntimeNotActive
                    } else if has_active_turn {
                        ManagedRuntimeUnavailabilityReason::NoActiveTurnRequired
                    } else {
                        ManagedRuntimeUnavailabilityReason::ActiveTurnRequired
                    },
                    evidence,
                }
            };
            (command, availability)
        })
        .collect()
}

fn runtime_turn_id(
    source: &agentdash_agent_service_api::AgentTurnId,
) -> Result<RuntimeTurnId, AgentSnapshotProjectionError> {
    RuntimeTurnId::new(format!("agent-turn:{source}"))
        .map_err(|error| presentation(error.to_string()))
}

fn runtime_item_id(
    source: &agentdash_agent_service_api::AgentItemId,
) -> Result<RuntimeItemId, AgentSnapshotProjectionError> {
    RuntimeItemId::new(format!("agent-item:{source}"))
        .map_err(|error| presentation(error.to_string()))
}

fn runtime_interaction_id(
    source: &agentdash_agent_service_api::AgentInteractionId,
) -> Result<RuntimeInteractionId, AgentSnapshotProjectionError> {
    RuntimeInteractionId::new(format!("agent-interaction:{source}"))
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

fn project_lifecycle(status: AgentLifecycleStatus) -> ManagedRuntimeLifecycleStatus {
    match status {
        AgentLifecycleStatus::Creating => ManagedRuntimeLifecycleStatus::Provisioning,
        AgentLifecycleStatus::Active => ManagedRuntimeLifecycleStatus::Active,
        AgentLifecycleStatus::Suspended => ManagedRuntimeLifecycleStatus::Suspended,
        AgentLifecycleStatus::Closed => ManagedRuntimeLifecycleStatus::Closed,
        AgentLifecycleStatus::Lost => ManagedRuntimeLifecycleStatus::Lost,
    }
}

fn project_entity_status(status: AgentEntityStatus) -> ManagedRuntimeEntityStatus {
    match status {
        AgentEntityStatus::Accepted => ManagedRuntimeEntityStatus::Accepted,
        AgentEntityStatus::Running => ManagedRuntimeEntityStatus::Running,
        AgentEntityStatus::Completed => ManagedRuntimeEntityStatus::Completed,
        AgentEntityStatus::Failed => ManagedRuntimeEntityStatus::Failed,
        AgentEntityStatus::Interrupted => ManagedRuntimeEntityStatus::Interrupted,
        AgentEntityStatus::Lost => ManagedRuntimeEntityStatus::Lost,
    }
}

fn project_interaction_status(status: AgentInteractionStatus) -> ManagedRuntimeInteractionStatus {
    match status {
        AgentInteractionStatus::Pending => ManagedRuntimeInteractionStatus::Pending,
        AgentInteractionStatus::Resolved => ManagedRuntimeInteractionStatus::Resolved,
        AgentInteractionStatus::Cancelled => ManagedRuntimeInteractionStatus::Cancelled,
        AgentInteractionStatus::Expired => ManagedRuntimeInteractionStatus::Expired,
        AgentInteractionStatus::Lost => ManagedRuntimeInteractionStatus::Lost,
    }
}

fn project_authority(authority: AgentSnapshotAuthority) -> ManagedRuntimeProjectionAuthority {
    match authority {
        AgentSnapshotAuthority::AgentAuthoritative => {
            ManagedRuntimeProjectionAuthority::SourceAuthoritative
        }
        AgentSnapshotAuthority::AgentObserved => ManagedRuntimeProjectionAuthority::SourceObserved,
        AgentSnapshotAuthority::Derived => ManagedRuntimeProjectionAuthority::RuntimeDerived,
    }
}

fn project_fidelity(fidelity: SemanticFidelity) -> ManagedRuntimeProjectionFidelity {
    match fidelity {
        SemanticFidelity::Unsupported => ManagedRuntimeProjectionFidelity::Unsupported,
        SemanticFidelity::Observed => ManagedRuntimeProjectionFidelity::Observed,
        SemanticFidelity::Approximation => ManagedRuntimeProjectionFidelity::Approximation,
        SemanticFidelity::Exact => ManagedRuntimeProjectionFidelity::Exact,
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
        AgentSnapshot, AgentSnapshotRevision, AgentSnapshotSource, AgentSourceCoordinate,
        AgentTurnId, AgentTurnSnapshot,
    };

    use super::*;

    fn snapshot() -> AgentSnapshot {
        AgentSnapshot {
            source: AgentSourceCoordinate::new("source-1").expect("source"),
            revision: AgentSnapshotRevision(7),
            lifecycle: AgentLifecycleStatus::Active,
            active_turn_id: None,
            turns: Vec::new(),
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
        let projected = project_authoritative_agent_snapshot(
            RuntimeThreadId::new("thread-1").expect("thread"),
            snapshot(),
        )
        .expect("projection");

        assert_eq!(projected.revision, RuntimeProjectionRevision(7));
        assert!(projected.operations.is_empty());
        assert!(projected.source_binding.is_none());
        assert_eq!(
            projected.authority,
            ManagedRuntimeProjectionAuthority::SourceAuthoritative
        );
    }

    #[test]
    fn duplicate_agent_coordinates_are_rejected() {
        let mut snapshot = snapshot();
        let turn = AgentTurnSnapshot {
            id: AgentTurnId::new("turn-1").expect("turn"),
            status: AgentEntityStatus::Completed,
            items: Vec::new(),
            error: None,
        };
        snapshot.turns = vec![turn.clone(), turn];

        assert!(matches!(
            project_authoritative_agent_snapshot(
                RuntimeThreadId::new("thread-1").expect("thread"),
                snapshot,
            ),
            Err(AgentSnapshotProjectionError::InvalidSnapshot { .. })
        ));
    }
}
