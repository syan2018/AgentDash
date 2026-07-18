use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeFeedItem {
    pub item_id: String,
    pub turn_id: String,
    pub kind: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum RuntimeCompactionLifecycle {
    Idle,
    Started {
        operation_id: String,
    },
    Completed {
        operation_id: String,
    },
    Failed {
        operation_id: String,
        reason: String,
    },
    Lost {
        operation_id: String,
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeCommandAvailability {
    pub submit_input: bool,
    pub compact: bool,
    pub reason: Option<String>,
}

impl RuntimeCommandAvailability {
    pub fn from_compaction(compaction: &RuntimeCompactionLifecycle) -> Self {
        match compaction {
            RuntimeCompactionLifecycle::Started { .. } => Self {
                submit_input: false,
                compact: false,
                reason: Some("compaction_in_progress".to_owned()),
            },
            RuntimeCompactionLifecycle::Lost { .. } => Self {
                submit_input: false,
                compact: false,
                reason: Some("compaction_state_lost".to_owned()),
            },
            RuntimeCompactionLifecycle::Idle
            | RuntimeCompactionLifecycle::Completed { .. }
            | RuntimeCompactionLifecycle::Failed { .. } => Self {
                submit_input: true,
                compact: true,
                reason: None,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentRunRuntimeFeedSnapshot {
    pub source_coordinate: String,
    pub snapshot_revision: u64,
    pub committed_sequence: u64,
    pub cursor: Option<String>,
    pub items: Vec<RuntimeFeedItem>,
    pub compaction: RuntimeCompactionLifecycle,
    pub availability: RuntimeCommandAvailability,
}

impl AgentRunRuntimeFeedSnapshot {
    pub fn new(
        source_coordinate: String,
        snapshot_revision: u64,
        committed_sequence: u64,
        cursor: Option<String>,
        items: Vec<RuntimeFeedItem>,
        compaction: RuntimeCompactionLifecycle,
    ) -> Self {
        let availability = RuntimeCommandAvailability::from_compaction(&compaction);
        Self {
            source_coordinate,
            snapshot_revision,
            committed_sequence,
            cursor,
            items,
            compaction,
            availability,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentRunRuntimeCommittedChangePayload {
    ItemUpserted {
        item: RuntimeFeedItem,
    },
    ItemRemoved {
        item_id: String,
    },
    CompactionStarted {
        operation_id: String,
    },
    CompactionCompleted {
        operation_id: String,
    },
    CompactionFailed {
        operation_id: String,
        reason: String,
    },
    CompactionLost {
        operation_id: String,
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentRunRuntimeCommittedChange {
    pub sequence: u64,
    pub previous_snapshot_revision: u64,
    pub snapshot_revision: u64,
    pub payload: AgentRunRuntimeCommittedChangePayload,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentRunRuntimeChangePage {
    pub source_coordinate: String,
    pub after_cursor: Option<String>,
    pub next_cursor: Option<String>,
    pub gap: bool,
    pub changes: Vec<AgentRunRuntimeCommittedChange>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeFeedReduceOutcome {
    Applied(AgentRunRuntimeFeedSnapshot),
    SnapshotReloadRequired,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RuntimeFeedReduceError {
    #[error("change page source does not match the target Runtime snapshot")]
    SourceMismatch,
    #[error("change page cursor does not continue the target Runtime snapshot")]
    CursorMismatch,
    #[error("committed change sequence is not contiguous")]
    SequenceMismatch,
    #[error("committed change revision is not contiguous")]
    RevisionMismatch,
}

pub fn reduce_runtime_change_page(
    mut snapshot: AgentRunRuntimeFeedSnapshot,
    page: AgentRunRuntimeChangePage,
) -> Result<RuntimeFeedReduceOutcome, RuntimeFeedReduceError> {
    if snapshot.source_coordinate != page.source_coordinate {
        return Err(RuntimeFeedReduceError::SourceMismatch);
    }
    if page.gap {
        return Ok(RuntimeFeedReduceOutcome::SnapshotReloadRequired);
    }
    if snapshot.cursor != page.after_cursor {
        return Err(RuntimeFeedReduceError::CursorMismatch);
    }
    for change in page.changes {
        if change.sequence != snapshot.committed_sequence + 1 {
            return Err(RuntimeFeedReduceError::SequenceMismatch);
        }
        if change.previous_snapshot_revision != snapshot.snapshot_revision
            || change.snapshot_revision != snapshot.snapshot_revision + 1
        {
            return Err(RuntimeFeedReduceError::RevisionMismatch);
        }
        apply_change(&mut snapshot, change.payload);
        snapshot.committed_sequence = change.sequence;
        snapshot.snapshot_revision = change.snapshot_revision;
    }
    snapshot.cursor = page.next_cursor;
    snapshot.availability = RuntimeCommandAvailability::from_compaction(&snapshot.compaction);
    Ok(RuntimeFeedReduceOutcome::Applied(snapshot))
}

fn apply_change(
    snapshot: &mut AgentRunRuntimeFeedSnapshot,
    payload: AgentRunRuntimeCommittedChangePayload,
) {
    match payload {
        AgentRunRuntimeCommittedChangePayload::ItemUpserted { item } => {
            if let Some(existing) = snapshot
                .items
                .iter_mut()
                .find(|existing| existing.item_id == item.item_id)
            {
                *existing = item;
            } else {
                snapshot.items.push(item);
            }
        }
        AgentRunRuntimeCommittedChangePayload::ItemRemoved { item_id } => {
            snapshot.items.retain(|item| item.item_id != item_id);
        }
        AgentRunRuntimeCommittedChangePayload::CompactionStarted { operation_id } => {
            snapshot.compaction = RuntimeCompactionLifecycle::Started { operation_id };
        }
        AgentRunRuntimeCommittedChangePayload::CompactionCompleted { operation_id } => {
            snapshot.compaction = RuntimeCompactionLifecycle::Completed { operation_id };
        }
        AgentRunRuntimeCommittedChangePayload::CompactionFailed {
            operation_id,
            reason,
        } => {
            snapshot.compaction = RuntimeCompactionLifecycle::Failed {
                operation_id,
                reason,
            };
        }
        AgentRunRuntimeCommittedChangePayload::CompactionLost {
            operation_id,
            reason,
        } => {
            snapshot.compaction = RuntimeCompactionLifecycle::Lost {
                operation_id,
                reason,
            };
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentRunTargetApiProjection {
    pub source_coordinate: String,
    pub revision: u64,
    pub cursor: Option<String>,
    pub feed: Vec<RuntimeFeedItem>,
    pub compaction: RuntimeCompactionLifecycle,
    pub availability: RuntimeCommandAvailability,
}

impl From<AgentRunRuntimeFeedSnapshot> for AgentRunTargetApiProjection {
    fn from(snapshot: AgentRunRuntimeFeedSnapshot) -> Self {
        Self {
            source_coordinate: snapshot.source_coordinate,
            revision: snapshot.snapshot_revision,
            cursor: snapshot.cursor,
            feed: snapshot.items,
            compaction: snapshot.compaction,
            availability: snapshot.availability,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> AgentRunRuntimeFeedSnapshot {
        AgentRunRuntimeFeedSnapshot::new(
            "runtime-child".to_owned(),
            4,
            7,
            Some("cursor-7".to_owned()),
            vec![RuntimeFeedItem {
                item_id: "item-child".to_owned(),
                turn_id: "turn-child".to_owned(),
                kind: "message".to_owned(),
                text: "child-visible".to_owned(),
            }],
            RuntimeCompactionLifecycle::Idle,
        )
    }

    fn change(
        sequence: u64,
        previous_snapshot_revision: u64,
        payload: AgentRunRuntimeCommittedChangePayload,
    ) -> AgentRunRuntimeCommittedChange {
        AgentRunRuntimeCommittedChange {
            sequence,
            previous_snapshot_revision,
            snapshot_revision: previous_snapshot_revision + 1,
            payload,
        }
    }

    #[test]
    fn target_feed_only_contains_the_runtime_snapshot_items() {
        let projection = AgentRunTargetApiProjection::from(snapshot());
        assert_eq!(projection.feed.len(), 1);
        assert_eq!(projection.feed[0].text, "child-visible");
        assert!(
            !serde_json::to_string(&projection)
                .expect("json")
                .contains("ancestor")
        );
    }

    #[test]
    fn cursor_gap_requires_authoritative_snapshot_reload() {
        let outcome = reduce_runtime_change_page(
            snapshot(),
            AgentRunRuntimeChangePage {
                source_coordinate: "runtime-child".to_owned(),
                after_cursor: Some("cursor-7".to_owned()),
                next_cursor: Some("cursor-11".to_owned()),
                gap: true,
                changes: Vec::new(),
            },
        )
        .expect("reduce");
        assert_eq!(outcome, RuntimeFeedReduceOutcome::SnapshotReloadRequired);
    }

    #[test]
    fn compaction_lifecycle_and_availability_follow_committed_changes() {
        let started = reduce_runtime_change_page(
            snapshot(),
            AgentRunRuntimeChangePage {
                source_coordinate: "runtime-child".to_owned(),
                after_cursor: Some("cursor-7".to_owned()),
                next_cursor: Some("cursor-8".to_owned()),
                gap: false,
                changes: vec![change(
                    8,
                    4,
                    AgentRunRuntimeCommittedChangePayload::CompactionStarted {
                        operation_id: "compact-1".to_owned(),
                    },
                )],
            },
        )
        .expect("started");
        let RuntimeFeedReduceOutcome::Applied(started) = started else {
            panic!("applied");
        };
        assert!(!started.availability.submit_input);

        let completed = reduce_runtime_change_page(
            started,
            AgentRunRuntimeChangePage {
                source_coordinate: "runtime-child".to_owned(),
                after_cursor: Some("cursor-8".to_owned()),
                next_cursor: Some("cursor-9".to_owned()),
                gap: false,
                changes: vec![change(
                    9,
                    5,
                    AgentRunRuntimeCommittedChangePayload::CompactionCompleted {
                        operation_id: "compact-1".to_owned(),
                    },
                )],
            },
        )
        .expect("completed");
        let RuntimeFeedReduceOutcome::Applied(completed) = completed else {
            panic!("applied");
        };
        assert!(completed.availability.submit_input);
        assert!(matches!(
            completed.compaction,
            RuntimeCompactionLifecycle::Completed { .. }
        ));
    }

    #[test]
    fn failed_and_lost_are_distinct_availability_states() {
        assert!(
            RuntimeCommandAvailability::from_compaction(&RuntimeCompactionLifecycle::Failed {
                operation_id: "c1".to_owned(),
                reason: "rejected".to_owned(),
            })
            .submit_input
        );
        assert!(
            !RuntimeCommandAvailability::from_compaction(&RuntimeCompactionLifecycle::Lost {
                operation_id: "c2".to_owned(),
                reason: "unknown final state".to_owned(),
            })
            .submit_input
        );
    }
}
