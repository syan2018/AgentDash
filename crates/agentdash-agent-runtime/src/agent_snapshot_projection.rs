use std::collections::{BTreeMap, BTreeSet};

use agentdash_agent_protocol::BackboneEvent;
use agentdash_agent_runtime_contract::{
    AgentRuntimeView, AgentSnapshot, AgentSnapshotAuthority, RuntimeThreadId, SemanticFidelity,
};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AgentSnapshotProjectionError {
    #[error("Complete Agent snapshot is invalid: {reason}")]
    InvalidSnapshot { reason: String },
}

/// Wrap one canonical Complete Agent observation with its Product Runtime identity.
///
/// This seam validates owner invariants but does not translate, copy or reinterpret source facts.
pub fn project_authoritative_agent_view(
    thread_id: RuntimeThreadId,
    snapshot: AgentSnapshot,
) -> Result<AgentRuntimeView, AgentSnapshotProjectionError> {
    validate_snapshot(&snapshot)?;
    Ok(AgentRuntimeView {
        thread_id,
        observation: snapshot.observation,
    })
}

fn validate_snapshot(snapshot: &AgentSnapshot) -> Result<(), AgentSnapshotProjectionError> {
    let observation = &snapshot.observation;
    if observation.context.snapshot_revision != observation.revision {
        return invalid("snapshot context coordinate does not match snapshot revision");
    }
    if observation.thread_name.as_ref().is_some_and(|thread_name| {
        thread_name.source_info.authority != AgentSnapshotAuthority::AgentAuthoritative
            || thread_name.source_info.fidelity != SemanticFidelity::Exact
            || thread_name
                .thread_name
                .as_ref()
                .is_some_and(|value| value.trim().is_empty())
    }) {
        return invalid("thread name must be source-authoritative, exact, and non-blank");
    }

    let mut presentation_ids = BTreeSet::new();
    let mut known_turns = BTreeSet::new();
    let mut known_items = BTreeMap::new();
    for record in &observation.conversation {
        if record.presentation_id.trim().is_empty()
            || !presentation_ids.insert(record.presentation_id.clone())
        {
            return invalid("conversation history contains a blank or duplicate presentation id");
        }
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
    for interaction in &observation.interactions {
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
    }
    Ok(())
}

fn invalid<T>(reason: impl Into<String>) -> Result<T, AgentSnapshotProjectionError> {
    Err(AgentSnapshotProjectionError::InvalidSnapshot {
        reason: reason.into(),
    })
}

#[cfg(test)]
mod tests {
    use agentdash_agent_runtime_contract::{
        AgentActiveTurnKind, AgentActiveTurnPhase, AgentActiveTurnSnapshot, AgentSnapshotRevision,
    };
    use agentdash_agent_runtime_test_support::coherent_runtime::CoherentAgentObservationBuilder;

    use super::*;

    fn snapshot() -> AgentSnapshot {
        CoherentAgentObservationBuilder::new(7)
            .observed_at_ms(42)
            .snapshot("source-1")
    }

    #[test]
    fn product_wrapper_hides_source_identity_without_mutating_observation() {
        let source = snapshot();
        let expected = source.observation.clone();
        let projected = project_authoritative_agent_view(
            RuntimeThreadId::new("thread-1").expect("thread"),
            source,
        )
        .expect("projection");

        assert_eq!(projected.thread_id.as_str(), "thread-1");
        assert_eq!(projected.observation, expected);
        let wire = serde_json::to_string(&projected).expect("serialize Product Runtime view");
        assert!(!wire.contains("source-1"));
    }

    #[test]
    fn execution_fact_is_shared_even_without_a_presentation() {
        let mut source = snapshot();
        source.observation.execution.active_turn = Some(AgentActiveTurnSnapshot {
            turn_id: agentdash_agent_runtime_contract::AgentTurnId::new("turn-1").expect("turn"),
            kind: AgentActiveTurnKind::Conversation,
            phase: AgentActiveTurnPhase::Running,
            started_at_ms: 42,
            cancellable: true,
        });

        let projected = project_authoritative_agent_view(
            RuntimeThreadId::new("thread-1").expect("thread"),
            source,
        )
        .expect("projection");

        assert_eq!(projected.active_turn_id(), Some("turn-1"));
        assert!(projected.observation.conversation.is_empty());
    }

    #[test]
    fn mismatched_context_coordinate_is_rejected() {
        let mut source = snapshot();
        source.observation.context.snapshot_revision = AgentSnapshotRevision(6);

        assert!(matches!(
            project_authoritative_agent_view(
                RuntimeThreadId::new("thread-1").expect("thread"),
                source,
            ),
            Err(AgentSnapshotProjectionError::InvalidSnapshot { reason })
                if reason.contains("context coordinate")
        ));
    }
}
