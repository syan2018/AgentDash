use agentdash_agent_runtime_contract::managed_projection::{
    ManagedRuntimeChangePage, ManagedRuntimeSnapshot,
};
use agentdash_agent_runtime_contract::{RuntimeChangeSequence, RuntimeProjectionRevision};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ManagedRuntimeFeedContractError {
    #[error(
        "command {command} availability was decided at revision {decided_at:?}, expected {snapshot_revision:?}"
    )]
    AvailabilityRevisionMismatch {
        command: &'static str,
        decided_at: RuntimeProjectionRevision,
        snapshot_revision: RuntimeProjectionRevision,
    },
    #[error("change page contains a change for a different Runtime thread")]
    ChangeThreadMismatch,
    #[error("change sequence {actual:?} does not follow retained sequence {expected_after:?}")]
    ChangeSequenceNotIncreasing {
        expected_after: RuntimeChangeSequence,
        actual: RuntimeChangeSequence,
    },
}

/// Validate and return the canonical Runtime snapshot without translating it
/// into an AgentRun- or UI-owned state shape.
pub fn consume_managed_runtime_snapshot(
    snapshot: ManagedRuntimeSnapshot,
) -> Result<ManagedRuntimeSnapshot, ManagedRuntimeFeedContractError> {
    for (command, availability) in &snapshot.command_availability {
        let decided_at = availability.evidence().decided_at_revision;
        if decided_at != snapshot.revision {
            return Err(
                ManagedRuntimeFeedContractError::AvailabilityRevisionMismatch {
                    command: match command {
                        agentdash_agent_runtime_contract::managed_projection::ManagedRuntimeCommandKind::SubmitInput => "submit_input",
                        agentdash_agent_runtime_contract::managed_projection::ManagedRuntimeCommandKind::Steer => "steer",
                        agentdash_agent_runtime_contract::managed_projection::ManagedRuntimeCommandKind::Interrupt => "interrupt",
                        agentdash_agent_runtime_contract::managed_projection::ManagedRuntimeCommandKind::RequestCompaction => "request_compaction",
                        agentdash_agent_runtime_contract::managed_projection::ManagedRuntimeCommandKind::ResolveInteraction => "resolve_interaction",
                        agentdash_agent_runtime_contract::managed_projection::ManagedRuntimeCommandKind::Close => "close",
                        agentdash_agent_runtime_contract::managed_projection::ManagedRuntimeCommandKind::Fork => "fork",
                    },
                    decided_at,
                    snapshot_revision: snapshot.revision,
                },
            );
        }
    }
    Ok(snapshot)
}

/// Validate and return the committed Runtime tail without projecting a second
/// change vocabulary. A typed gap is handled by reloading the canonical
/// snapshot, so the retained tail is never replayed as full history.
pub fn consume_managed_runtime_change_page(
    page: ManagedRuntimeChangePage,
) -> Result<ManagedRuntimeChangePage, ManagedRuntimeFeedContractError> {
    let mut previous = page
        .gap
        .as_ref()
        .and_then(|gap| gap.requested_after)
        .unwrap_or_default();
    for change in &page.changes {
        if change.thread_id != page.thread_id {
            return Err(ManagedRuntimeFeedContractError::ChangeThreadMismatch);
        }
        if change.sequence <= previous {
            return Err(
                ManagedRuntimeFeedContractError::ChangeSequenceNotIncreasing {
                    expected_after: previous,
                    actual: change.sequence,
                },
            );
        }
        previous = change.sequence;
    }
    Ok(page)
}

pub fn managed_runtime_change_page_requires_snapshot_reload(
    page: &ManagedRuntimeChangePage,
) -> bool {
    page.gap.is_some()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use agentdash_agent_runtime_contract::RuntimeThreadId;
    use agentdash_agent_runtime_contract::managed_projection::{
        ManagedRuntimeAvailabilityEvidence, ManagedRuntimeCommandAvailability,
        ManagedRuntimeCommandKind, ManagedRuntimeLifecycleStatus,
        ManagedRuntimeProjectionAuthority, ManagedRuntimeProjectionFidelity,
    };

    use super::*;

    fn snapshot(revision: u64) -> ManagedRuntimeSnapshot {
        let revision = RuntimeProjectionRevision(revision);
        ManagedRuntimeSnapshot {
            thread_id: RuntimeThreadId::new("runtime-thread-feed").expect("thread"),
            revision,
            latest_change_sequence: RuntimeChangeSequence(3),
            captured_at_ms: 10,
            lifecycle: ManagedRuntimeLifecycleStatus::Active,
            active_turn_id: None,
            turns: Vec::new(),
            items: Vec::new(),
            interactions: Vec::new(),
            operations: Vec::new(),
            authority: ManagedRuntimeProjectionAuthority::SourceAuthoritative,
            fidelity: ManagedRuntimeProjectionFidelity::Exact,
            command_availability: BTreeMap::from([(
                ManagedRuntimeCommandKind::SubmitInput,
                ManagedRuntimeCommandAvailability::Available {
                    evidence: ManagedRuntimeAvailabilityEvidence {
                        decided_at_revision: revision,
                        blocking_operation_id: None,
                        bound_surface_revision: None,
                        applied_surface_revision: None,
                    },
                },
            )]),
        }
    }

    #[test]
    fn canonical_snapshot_is_returned_without_availability_derivation() {
        let snapshot = snapshot(4);
        assert_eq!(
            consume_managed_runtime_snapshot(snapshot.clone()).expect("valid snapshot"),
            snapshot
        );
    }

    #[test]
    fn availability_evidence_must_match_the_committed_snapshot_revision() {
        let mut snapshot = snapshot(4);
        let availability = snapshot
            .command_availability
            .get_mut(&ManagedRuntimeCommandKind::SubmitInput)
            .expect("availability");
        *availability = ManagedRuntimeCommandAvailability::Available {
            evidence: ManagedRuntimeAvailabilityEvidence {
                decided_at_revision: RuntimeProjectionRevision(3),
                blocking_operation_id: None,
                bound_surface_revision: None,
                applied_surface_revision: None,
            },
        };

        assert!(matches!(
            consume_managed_runtime_snapshot(snapshot),
            Err(ManagedRuntimeFeedContractError::AvailabilityRevisionMismatch { .. })
        ));
    }
}
