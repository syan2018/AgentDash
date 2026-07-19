use agentdash_agent_runtime_contract::managed_projection::{
    ManagedRuntimeChangeGap, ManagedRuntimeChangePage, ManagedRuntimeSnapshot,
};
use agentdash_agent_runtime_contract::{
    ManagedRuntimeChangeDelta, ManagedRuntimeEntityStatus, ManagedRuntimeItemTransition,
    ManagedRuntimeSourceProjectionDelta, ManagedRuntimeTerminalStatus, RuntimeChangeSequence,
    RuntimeProjectionRevision, RuntimeThreadId,
};
use thiserror::Error;

use super::AgentRunRuntimeProjectionPort;

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
    #[error("Runtime item `{item_id}` has an invalid canonical presentation")]
    InvalidItemPresentation { item_id: String },
    #[error(
        "Runtime interaction `{interaction_id}` has an invalid request/status/resolution tuple"
    )]
    InvalidInteraction { interaction_id: String },
    #[error("Runtime thread name and source evidence must be present or absent together")]
    ThreadNameEvidenceMismatch,
    #[error("change page contains a change for a different Runtime thread")]
    ChangeThreadMismatch,
    #[error("Runtime change page belongs to a different Runtime thread")]
    ChangePageThreadMismatch,
    #[error("change sequence {actual:?} does not follow retained sequence {expected_after:?}")]
    ChangeSequenceNotIncreasing {
        expected_after: RuntimeChangeSequence,
        actual: RuntimeChangeSequence,
    },
    #[error("Runtime change load failed: {0}")]
    ChangeLoad(String),
    #[error("Runtime snapshot reload failed: {0}")]
    SnapshotLoad(String),
    #[error("reloaded snapshot belongs to a different Runtime thread")]
    SnapshotThreadMismatch,
    #[error("reloaded snapshot revision predates the reported gap")]
    ReloadedSnapshotRevisionStale,
    #[error("reloaded snapshot sequence predates the retained Runtime tail")]
    ReloadedSnapshotSequenceStale,
    #[error("Runtime reported a second gap immediately after snapshot reload")]
    GapAfterSnapshotReload,
    #[error("change sequence {actual:?} is not contiguous after {expected_after:?}")]
    ChangeSequenceNotContiguous {
        expected_after: RuntimeChangeSequence,
        actual: RuntimeChangeSequence,
    },
    #[error("change page next cursor does not match its committed tail")]
    ChangePageNextMismatch,
}

fn validate_items_and_interactions(
    items: &[agentdash_agent_runtime_contract::ManagedRuntimeItem],
    interactions: &[agentdash_agent_runtime_contract::ManagedRuntimeInteraction],
) -> Result<(), ManagedRuntimeFeedContractError> {
    for item in items {
        if item.presentation.validate_for_status(item.status).is_err() {
            return Err(ManagedRuntimeFeedContractError::InvalidItemPresentation {
                item_id: item.id.as_str().to_owned(),
            });
        }
    }
    for interaction in interactions {
        if !interaction.validate() {
            return Err(ManagedRuntimeFeedContractError::InvalidInteraction {
                interaction_id: interaction.id.as_str().to_owned(),
            });
        }
    }
    Ok(())
}

fn validate_transition(
    item_id: &agentdash_agent_runtime_contract::RuntimeItemId,
    transition: &ManagedRuntimeItemTransition,
) -> Result<(), ManagedRuntimeFeedContractError> {
    let presentation = match transition {
        ManagedRuntimeItemTransition::Started { presentation }
        | ManagedRuntimeItemTransition::Updated { presentation, .. }
        | ManagedRuntimeItemTransition::Terminal { presentation } => presentation,
    };
    let status = match transition {
        ManagedRuntimeItemTransition::Started { .. }
        | ManagedRuntimeItemTransition::Updated { .. } => ManagedRuntimeEntityStatus::Running,
        ManagedRuntimeItemTransition::Terminal { .. } => match presentation
            .terminal
            .as_ref()
            .map(|terminal| terminal.outcome)
        {
            Some(ManagedRuntimeTerminalStatus::Completed) => ManagedRuntimeEntityStatus::Completed,
            Some(ManagedRuntimeTerminalStatus::Failed) => ManagedRuntimeEntityStatus::Failed,
            Some(ManagedRuntimeTerminalStatus::Interrupted) => {
                ManagedRuntimeEntityStatus::Interrupted
            }
            Some(ManagedRuntimeTerminalStatus::Lost) => ManagedRuntimeEntityStatus::Lost,
            None => {
                return Err(ManagedRuntimeFeedContractError::InvalidItemPresentation {
                    item_id: item_id.as_str().to_owned(),
                });
            }
        },
    };
    presentation.validate_for_status(status).map_err(|_| {
        ManagedRuntimeFeedContractError::InvalidItemPresentation {
            item_id: item_id.as_str().to_owned(),
        }
    })
}

#[derive(Debug, Clone, PartialEq)]
pub enum ManagedRuntimeReconnectOutcome {
    Continuous {
        change_page: ManagedRuntimeChangePage,
    },
    SnapshotReloaded {
        reported_gap: ManagedRuntimeChangeGap,
        snapshot: Box<ManagedRuntimeSnapshot>,
        change_page: ManagedRuntimeChangePage,
    },
}

pub struct AgentRunRuntimeFeedReconnect<'a> {
    snapshot_port: &'a dyn AgentRunRuntimeProjectionPort,
}

impl<'a> AgentRunRuntimeFeedReconnect<'a> {
    pub fn new(snapshot_port: &'a dyn AgentRunRuntimeProjectionPort) -> Self {
        Self { snapshot_port }
    }

    pub async fn reconnect(
        &self,
        thread_id: &RuntimeThreadId,
        after: Option<RuntimeChangeSequence>,
    ) -> Result<ManagedRuntimeReconnectOutcome, ManagedRuntimeFeedContractError> {
        let change_page = self
            .snapshot_port
            .load_changes(thread_id, after)
            .await
            .map_err(ManagedRuntimeFeedContractError::ChangeLoad)?;
        let change_page = consume_managed_runtime_change_page_for_thread(thread_id, change_page)?;
        if let Some(gap) = change_page.gap.clone() {
            let snapshot = self
                .snapshot_port
                .load_snapshot(thread_id)
                .await
                .map_err(ManagedRuntimeFeedContractError::SnapshotLoad)?;
            let snapshot = consume_managed_runtime_snapshot(snapshot)?;
            validate_reloaded_snapshot(thread_id, &gap, &snapshot)?;

            let change_page = self
                .snapshot_port
                .load_changes(thread_id, Some(snapshot.latest_change_sequence))
                .await
                .map_err(ManagedRuntimeFeedContractError::ChangeLoad)?;
            let change_page =
                consume_managed_runtime_change_page_for_thread(thread_id, change_page)?;
            if change_page.gap.is_some() {
                return Err(ManagedRuntimeFeedContractError::GapAfterSnapshotReload);
            }
            validate_contiguous_tail(&change_page, snapshot.latest_change_sequence)?;
            return Ok(ManagedRuntimeReconnectOutcome::SnapshotReloaded {
                reported_gap: gap,
                snapshot: Box::new(snapshot),
                change_page,
            });
        }
        if let Some(after) = after {
            validate_contiguous_tail(&change_page, after)?;
        }
        Ok(ManagedRuntimeReconnectOutcome::Continuous { change_page })
    }
}

fn validate_reloaded_snapshot(
    thread_id: &RuntimeThreadId,
    gap: &ManagedRuntimeChangeGap,
    snapshot: &ManagedRuntimeSnapshot,
) -> Result<(), ManagedRuntimeFeedContractError> {
    if &snapshot.thread_id != thread_id {
        return Err(ManagedRuntimeFeedContractError::SnapshotThreadMismatch);
    }
    if snapshot.revision < gap.snapshot_revision {
        return Err(ManagedRuntimeFeedContractError::ReloadedSnapshotRevisionStale);
    }
    if snapshot.latest_change_sequence < gap.latest_available {
        return Err(ManagedRuntimeFeedContractError::ReloadedSnapshotSequenceStale);
    }
    Ok(())
}

fn validate_contiguous_tail(
    page: &ManagedRuntimeChangePage,
    after: RuntimeChangeSequence,
) -> Result<(), ManagedRuntimeFeedContractError> {
    let mut previous = after;
    for change in &page.changes {
        let expected = RuntimeChangeSequence(previous.0 + 1);
        if change.sequence != expected {
            return Err(
                ManagedRuntimeFeedContractError::ChangeSequenceNotContiguous {
                    expected_after: previous,
                    actual: change.sequence,
                },
            );
        }
        previous = change.sequence;
    }
    if page.next != previous {
        return Err(ManagedRuntimeFeedContractError::ChangePageNextMismatch);
    }
    Ok(())
}

/// Validate and return the canonical Runtime snapshot without translating it
/// into an AgentRun- or UI-owned state shape.
pub fn consume_managed_runtime_snapshot(
    snapshot: ManagedRuntimeSnapshot,
) -> Result<ManagedRuntimeSnapshot, ManagedRuntimeFeedContractError> {
    validate_items_and_interactions(&snapshot.items, &snapshot.interactions)?;
    if snapshot.thread_name.is_some() != snapshot.thread_name_source.is_some() {
        return Err(ManagedRuntimeFeedContractError::ThreadNameEvidenceMismatch);
    }
    for (command, availability) in &snapshot.command_availability {
        let decided_at = availability.evidence().decided_at_revision;
        if decided_at != snapshot.revision {
            return Err(
                ManagedRuntimeFeedContractError::AvailabilityRevisionMismatch {
                    command: match command {
                        agentdash_agent_runtime_contract::managed_projection::ManagedRuntimeCommandKind::Create => "create",
                        agentdash_agent_runtime_contract::managed_projection::ManagedRuntimeCommandKind::Resume => "resume",
                        agentdash_agent_runtime_contract::managed_projection::ManagedRuntimeCommandKind::Activate => "activate",
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
        match &change.delta {
            ManagedRuntimeChangeDelta::ThreadNameChanged {
                thread_name,
                source: _,
                ..
            } if thread_name.as_deref().is_some_and(str::is_empty) => {
                return Err(ManagedRuntimeFeedContractError::ThreadNameEvidenceMismatch);
            }
            ManagedRuntimeChangeDelta::SourceProjectionChanged { delta, .. } => match delta {
                ManagedRuntimeSourceProjectionDelta::SnapshotReplaced {
                    items,
                    interactions,
                    ..
                } => validate_items_and_interactions(items, interactions)?,
                ManagedRuntimeSourceProjectionDelta::ItemsChanged { items } => {
                    validate_items_and_interactions(items, &[])?
                }
                ManagedRuntimeSourceProjectionDelta::ItemTransitioned {
                    item_id,
                    transition,
                } => validate_transition(item_id, transition)?,
                ManagedRuntimeSourceProjectionDelta::InteractionsChanged { interactions } => {
                    validate_items_and_interactions(&[], interactions)?
                }
                ManagedRuntimeSourceProjectionDelta::LifecycleChanged { .. }
                | ManagedRuntimeSourceProjectionDelta::ActiveTurnChanged { .. }
                | ManagedRuntimeSourceProjectionDelta::TurnsChanged { .. }
                | ManagedRuntimeSourceProjectionDelta::SurfaceChanged { .. } => {}
            },
            _ => {}
        }
        previous = change.sequence;
    }
    if page.gap.is_none() && page.next != previous {
        return Err(ManagedRuntimeFeedContractError::ChangePageNextMismatch);
    }
    Ok(page)
}

fn consume_managed_runtime_change_page_for_thread(
    thread_id: &RuntimeThreadId,
    page: ManagedRuntimeChangePage,
) -> Result<ManagedRuntimeChangePage, ManagedRuntimeFeedContractError> {
    let page = consume_managed_runtime_change_page(page)?;
    if &page.thread_id != thread_id {
        return Err(ManagedRuntimeFeedContractError::ChangePageThreadMismatch);
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
    use std::collections::{BTreeMap, VecDeque};
    use std::sync::Mutex;

    use agentdash_agent_runtime_contract::managed_projection::{
        ManagedRuntimeAvailabilityEvidence, ManagedRuntimeChangeDelta, ManagedRuntimeChangeGap,
        ManagedRuntimeCommandAvailability, ManagedRuntimeCommandKind,
        ManagedRuntimeLifecycleStatus, ManagedRuntimePlatformChange,
        ManagedRuntimeProjectionAuthority, ManagedRuntimeProjectionFidelity,
    };
    use async_trait::async_trait;

    use super::*;

    fn snapshot_for(thread_id: &str, revision: u64, sequence: u64) -> ManagedRuntimeSnapshot {
        let revision = RuntimeProjectionRevision(revision);
        ManagedRuntimeSnapshot {
            thread_id: RuntimeThreadId::new(thread_id).expect("thread"),
            revision,
            latest_change_sequence: RuntimeChangeSequence(sequence),
            captured_at_ms: 10,
            lifecycle: ManagedRuntimeLifecycleStatus::Active,
            active_turn_id: None,
            turns: Vec::new(),
            items: Vec::new(),
            interactions: Vec::new(),
            conversation_history: Vec::new(),
            thread_name: None,
            thread_name_source: None,
            operations: Vec::new(),
            source_binding: None,
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

    fn snapshot(revision: u64) -> ManagedRuntimeSnapshot {
        snapshot_for("runtime-thread-feed", revision, 3)
    }

    fn change_page(
        thread_id: &RuntimeThreadId,
        sequence: u64,
        revision: u64,
    ) -> ManagedRuntimeChangePage {
        ManagedRuntimeChangePage {
            thread_id: thread_id.clone(),
            changes: vec![ManagedRuntimePlatformChange {
                thread_id: thread_id.clone(),
                sequence: RuntimeChangeSequence(sequence),
                revision: RuntimeProjectionRevision(revision),
                delta: ManagedRuntimeChangeDelta::RuntimeLifecycleChanged {
                    lifecycle: ManagedRuntimeLifecycleStatus::Active,
                },
            }],
            next: RuntimeChangeSequence(sequence),
            gap: None,
        }
    }

    #[derive(Default)]
    struct RecordingSnapshotPort {
        snapshots: Mutex<VecDeque<ManagedRuntimeSnapshot>>,
        change_pages: Mutex<VecDeque<ManagedRuntimeChangePage>>,
        calls: Mutex<Vec<String>>,
    }

    impl RecordingSnapshotPort {
        fn with_results(
            snapshots: Vec<ManagedRuntimeSnapshot>,
            change_pages: Vec<ManagedRuntimeChangePage>,
        ) -> Self {
            Self {
                snapshots: Mutex::new(snapshots.into()),
                change_pages: Mutex::new(change_pages.into()),
                calls: Mutex::new(Vec::new()),
            }
        }

        fn calls(&self) -> Vec<String> {
            self.calls.lock().expect("calls").clone()
        }
    }

    #[async_trait]
    impl AgentRunRuntimeProjectionPort for RecordingSnapshotPort {
        async fn load_snapshot(
            &self,
            thread_id: &RuntimeThreadId,
        ) -> Result<ManagedRuntimeSnapshot, String> {
            self.calls
                .lock()
                .expect("calls")
                .push(format!("snapshot:{}", thread_id.as_str()));
            self.snapshots
                .lock()
                .expect("snapshots")
                .pop_front()
                .ok_or_else(|| "missing recorded snapshot".to_owned())
        }

        async fn load_changes(
            &self,
            thread_id: &RuntimeThreadId,
            after: Option<RuntimeChangeSequence>,
        ) -> Result<ManagedRuntimeChangePage, String> {
            self.calls.lock().expect("calls").push(format!(
                "changes:{}:{}",
                thread_id.as_str(),
                after.map_or_else(|| "none".to_owned(), |sequence| sequence.0.to_string())
            ));
            self.change_pages
                .lock()
                .expect("change pages")
                .pop_front()
                .ok_or_else(|| "missing recorded change page".to_owned())
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

    #[test]
    fn every_managed_runtime_command_kind_has_an_explicit_availability_contract() {
        let mut snapshot = snapshot(4);
        snapshot.command_availability = ManagedRuntimeCommandKind::ALL
            .into_iter()
            .map(|command| {
                (
                    command,
                    ManagedRuntimeCommandAvailability::Available {
                        evidence: ManagedRuntimeAvailabilityEvidence {
                            decided_at_revision: snapshot.revision,
                            blocking_operation_id: None,
                            bound_surface_revision: None,
                            applied_surface_revision: None,
                        },
                    },
                )
            })
            .collect();

        assert_eq!(
            consume_managed_runtime_snapshot(snapshot.clone()).expect("all command kinds"),
            snapshot
        );
    }

    #[tokio::test]
    async fn continuous_reconnect_returns_the_canonical_change_page_losslessly() {
        let thread_id = RuntimeThreadId::new("runtime-thread-feed").expect("thread");
        let page = change_page(&thread_id, 9, 6);
        let port = RecordingSnapshotPort::with_results(Vec::new(), vec![page.clone()]);

        let outcome = AgentRunRuntimeFeedReconnect::new(&port)
            .reconnect(&thread_id, Some(RuntimeChangeSequence(8)))
            .await
            .expect("continuous reconnect");

        assert_eq!(
            outcome,
            ManagedRuntimeReconnectOutcome::Continuous { change_page: page }
        );
        assert_eq!(port.calls(), vec!["changes:runtime-thread-feed:8"]);
    }

    #[tokio::test]
    async fn continuous_reconnect_rejects_an_empty_page_for_another_thread() {
        let thread_id = RuntimeThreadId::new("runtime-thread-feed").expect("thread");
        let other_thread_id = RuntimeThreadId::new("runtime-thread-other").expect("thread");
        let page = ManagedRuntimeChangePage {
            thread_id: other_thread_id,
            changes: Vec::new(),
            next: RuntimeChangeSequence(8),
            gap: None,
        };
        let port = RecordingSnapshotPort::with_results(Vec::new(), vec![page]);

        assert_eq!(
            AgentRunRuntimeFeedReconnect::new(&port)
                .reconnect(&thread_id, Some(RuntimeChangeSequence(8)))
                .await,
            Err(ManagedRuntimeFeedContractError::ChangePageThreadMismatch)
        );
        assert_eq!(port.calls(), vec!["changes:runtime-thread-feed:8"]);
    }

    #[tokio::test]
    async fn gap_reconnect_rejects_the_initial_page_for_another_thread() {
        let thread_id = RuntimeThreadId::new("runtime-thread-feed").expect("thread");
        let other_thread_id = RuntimeThreadId::new("runtime-thread-other").expect("thread");
        let gap_page = ManagedRuntimeChangePage {
            thread_id: other_thread_id,
            changes: Vec::new(),
            next: RuntimeChangeSequence(12),
            gap: Some(ManagedRuntimeChangeGap {
                requested_after: Some(RuntimeChangeSequence(4)),
                earliest_available: RuntimeChangeSequence(9),
                latest_available: RuntimeChangeSequence(12),
                snapshot_revision: RuntimeProjectionRevision(8),
            }),
        };
        let port = RecordingSnapshotPort::with_results(Vec::new(), vec![gap_page]);

        assert_eq!(
            AgentRunRuntimeFeedReconnect::new(&port)
                .reconnect(&thread_id, Some(RuntimeChangeSequence(4)))
                .await,
            Err(ManagedRuntimeFeedContractError::ChangePageThreadMismatch)
        );
        assert_eq!(port.calls(), vec!["changes:runtime-thread-feed:4"]);
    }

    #[tokio::test]
    async fn typed_gap_reloads_and_validates_snapshot_before_reading_its_tail() {
        let thread_id = RuntimeThreadId::new("runtime-thread-feed").expect("thread");
        let gap = ManagedRuntimeChangeGap {
            requested_after: Some(RuntimeChangeSequence(4)),
            earliest_available: RuntimeChangeSequence(9),
            latest_available: RuntimeChangeSequence(12),
            snapshot_revision: RuntimeProjectionRevision(8),
        };
        let gap_page = ManagedRuntimeChangePage {
            thread_id: thread_id.clone(),
            changes: Vec::new(),
            next: RuntimeChangeSequence(12),
            gap: Some(gap.clone()),
        };
        let snapshot = snapshot_for("runtime-thread-feed", 8, 12);
        let tail = ManagedRuntimeChangePage {
            thread_id: thread_id.clone(),
            changes: Vec::new(),
            next: RuntimeChangeSequence(12),
            gap: None,
        };
        let port = RecordingSnapshotPort::with_results(
            vec![snapshot.clone()],
            vec![gap_page, tail.clone()],
        );

        let outcome = AgentRunRuntimeFeedReconnect::new(&port)
            .reconnect(&thread_id, Some(RuntimeChangeSequence(4)))
            .await
            .expect("gap reconnect");

        assert_eq!(
            outcome,
            ManagedRuntimeReconnectOutcome::SnapshotReloaded {
                reported_gap: gap,
                snapshot: Box::new(snapshot),
                change_page: tail,
            }
        );
        assert_eq!(
            port.calls(),
            vec![
                "changes:runtime-thread-feed:4",
                "snapshot:runtime-thread-feed",
                "changes:runtime-thread-feed:12",
            ]
        );
    }

    #[tokio::test]
    async fn gap_reconnect_rejects_the_reloaded_tail_for_another_thread() {
        let thread_id = RuntimeThreadId::new("runtime-thread-feed").expect("thread");
        let gap_page = ManagedRuntimeChangePage {
            thread_id: thread_id.clone(),
            changes: Vec::new(),
            next: RuntimeChangeSequence(12),
            gap: Some(ManagedRuntimeChangeGap {
                requested_after: Some(RuntimeChangeSequence(4)),
                earliest_available: RuntimeChangeSequence(9),
                latest_available: RuntimeChangeSequence(12),
                snapshot_revision: RuntimeProjectionRevision(8),
            }),
        };
        let snapshot = snapshot_for("runtime-thread-feed", 8, 12);
        let tail = ManagedRuntimeChangePage {
            thread_id: RuntimeThreadId::new("runtime-thread-other").expect("thread"),
            changes: Vec::new(),
            next: RuntimeChangeSequence(12),
            gap: None,
        };
        let port = RecordingSnapshotPort::with_results(vec![snapshot], vec![gap_page, tail]);

        assert_eq!(
            AgentRunRuntimeFeedReconnect::new(&port)
                .reconnect(&thread_id, Some(RuntimeChangeSequence(4)))
                .await,
            Err(ManagedRuntimeFeedContractError::ChangePageThreadMismatch)
        );
        assert_eq!(
            port.calls(),
            vec![
                "changes:runtime-thread-feed:4",
                "snapshot:runtime-thread-feed",
                "changes:runtime-thread-feed:12",
            ]
        );
    }

    #[tokio::test]
    async fn stale_snapshot_cannot_complete_gap_reconnect() {
        let thread_id = RuntimeThreadId::new("runtime-thread-feed").expect("thread");
        let gap_page = ManagedRuntimeChangePage {
            thread_id: thread_id.clone(),
            changes: Vec::new(),
            next: RuntimeChangeSequence(12),
            gap: Some(ManagedRuntimeChangeGap {
                requested_after: Some(RuntimeChangeSequence(4)),
                earliest_available: RuntimeChangeSequence(9),
                latest_available: RuntimeChangeSequence(12),
                snapshot_revision: RuntimeProjectionRevision(8),
            }),
        };
        let port = RecordingSnapshotPort::with_results(
            vec![snapshot_for("runtime-thread-feed", 7, 11)],
            vec![gap_page],
        );

        assert_eq!(
            AgentRunRuntimeFeedReconnect::new(&port)
                .reconnect(&thread_id, Some(RuntimeChangeSequence(4)))
                .await,
            Err(ManagedRuntimeFeedContractError::ReloadedSnapshotRevisionStale)
        );
    }
}
