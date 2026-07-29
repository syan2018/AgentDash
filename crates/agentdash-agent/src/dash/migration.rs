use agentdash_agent_protocol::{ContextFrame, ContextFrameSection};
use serde_json::Value;
use thiserror::Error;

use super::{
    AgentHistory, AgentHistoryEntry, CompactionId, ContextRevision, DashAgentRepositoryState,
    HistoryEntryId, HistoryPayload, compaction_context_revision,
};

pub const DASH_REPOSITORY_SCHEMA_VERSION: i16 = 2;

pub fn migrate_dash_repository(
    mut repository: Value,
) -> Result<DashAgentRepositoryState, DashRepositoryMigrationError> {
    let history = repository
        .pointer_mut("/store/history")
        .ok_or(DashRepositoryMigrationError::MissingField("store.history"))?;
    migrate_history(history)?;

    repository
        .pointer_mut("/store")
        .and_then(Value::as_object_mut)
        .ok_or(DashRepositoryMigrationError::InvalidField("store"))?
        .remove("changes");
    repository
        .pointer_mut("/store/lifecycle")
        .and_then(Value::as_object_mut)
        .ok_or(DashRepositoryMigrationError::InvalidField(
            "store.lifecycle",
        ))?
        .remove("effects");

    let repository: DashAgentRepositoryState = serde_json::from_value(repository)
        .map_err(DashRepositoryMigrationError::DecodeFinalRepository)?;
    let state = repository
        .history()
        .state()
        .map_err(DashRepositoryMigrationError::InvalidFinalHistory)?;
    if let Some(compaction_id) = state.active_compaction {
        let compaction = state
            .compactions
            .get(&compaction_id)
            .expect("active compaction is present");
        if compaction.side_effect_started_at_ms.is_none() {
            return Err(DashRepositoryMigrationError::AmbiguousActiveCompaction(
                compaction_id,
            ));
        }
    }
    repository
        .store()
        .changes()
        .map_err(DashRepositoryMigrationError::InvalidFinalStore)?;
    Ok(repository)
}

fn migrate_history(history: &mut Value) -> Result<(), DashRepositoryMigrationError> {
    let history_object = history
        .as_object_mut()
        .ok_or(DashRepositoryMigrationError::InvalidField("store.history"))?;
    let entries = history_object
        .get_mut("entries")
        .and_then(Value::as_array_mut)
        .ok_or(DashRepositoryMigrationError::InvalidField(
            "store.history.entries",
        ))?;
    let mut migrated = Vec::with_capacity(entries.len());
    let mut started = std::collections::BTreeMap::<CompactionId, String>::new();

    for raw_entry in entries.iter_mut() {
        let payload = raw_entry
            .get_mut("payload")
            .and_then(Value::as_object_mut)
            .ok_or(DashRepositoryMigrationError::InvalidField(
                "store.history.entries[].payload",
            ))?;
        let payload_type = payload.get("type").and_then(Value::as_str).ok_or(
            DashRepositoryMigrationError::MissingField("store.history.entries[].payload.type"),
        )?;

        match payload_type {
            "compaction_queued" => {
                payload.remove("operation_id");
            }
            "compaction_started" => {
                payload.remove("operation_id");
                let compaction_id = parse_compaction_id(payload)?;
                let source_digest = digest_entries(&migrated);
                payload.insert(
                    "source_digest".to_owned(),
                    Value::String(source_digest.clone()),
                );
                started.insert(compaction_id, source_digest);
            }
            "compaction_applied" => {
                let compaction_id = parse_compaction_id(payload)?;
                let (mut summary_frame, retained_from) = parse_applied(payload)?;
                let source_digest = started.get(&compaction_id).ok_or_else(|| {
                    DashRepositoryMigrationError::AppliedWithoutStarted(compaction_id.clone())
                })?;
                let summary = compaction_summary(&summary_frame).ok_or_else(|| {
                    DashRepositoryMigrationError::MissingCompactionSummary(compaction_id.clone())
                })?;
                let context_revision = compaction_context_revision(
                    &compaction_id,
                    source_digest,
                    &summary,
                    retained_from.as_ref(),
                );
                align_summary_frame_revision(&mut summary_frame, &compaction_id, &context_revision);
                *payload = serde_json::to_value(HistoryPayload::CompactionApplied {
                    compaction_id,
                    context_revision,
                    summary_frame: Box::new(summary_frame),
                    retained_from,
                })
                .expect("final compaction payload serialization is infallible")
                .as_object()
                .expect("history payload serializes as an object")
                .clone();
            }
            _ => {}
        }

        let entry: AgentHistoryEntry =
            serde_json::from_value(raw_entry.clone()).map_err(|error| {
                DashRepositoryMigrationError::DecodeHistoryEntry {
                    sequence: raw_entry
                        .get("sequence")
                        .and_then(Value::as_u64)
                        .unwrap_or_default(),
                    error,
                }
            })?;
        migrated.push(entry);
    }

    *entries = migrated
        .iter()
        .map(|entry| {
            serde_json::to_value(entry).expect("final history entry serialization is infallible")
        })
        .collect();
    migrate_lineage_digest(history_object, &migrated)?;

    let final_history: AgentHistory = serde_json::from_value(Value::Object(history_object.clone()))
        .map_err(DashRepositoryMigrationError::DecodeFinalHistory)?;
    final_history
        .state()
        .map_err(DashRepositoryMigrationError::InvalidFinalHistory)?;
    Ok(())
}

fn parse_compaction_id(
    payload: &serde_json::Map<String, Value>,
) -> Result<CompactionId, DashRepositoryMigrationError> {
    serde_json::from_value(
        payload
            .get("compaction_id")
            .cloned()
            .ok_or(DashRepositoryMigrationError::MissingField("compaction_id"))?,
    )
    .map_err(|_| DashRepositoryMigrationError::InvalidField("compaction_id"))
}

fn parse_applied(
    payload: &serde_json::Map<String, Value>,
) -> Result<(ContextFrame, Option<HistoryEntryId>), DashRepositoryMigrationError> {
    let source = payload
        .get("checkpoint")
        .and_then(Value::as_object)
        .unwrap_or(payload);
    let summary_frame = source
        .get("summary_frame")
        .or_else(|| source.get("context_frame"))
        .cloned()
        .ok_or(DashRepositoryMigrationError::MissingField(
            "compaction_applied.summary_frame",
        ))?;
    let summary_frame = serde_json::from_value(summary_frame)
        .map_err(DashRepositoryMigrationError::DecodeSummaryFrame)?;
    let retained_from = source
        .get("retained_from")
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|_| {
            DashRepositoryMigrationError::InvalidField("compaction_applied.retained_from")
        })?
        .flatten();
    Ok((summary_frame, retained_from))
}

fn compaction_summary(frame: &ContextFrame) -> Option<String> {
    frame.sections.iter().find_map(|section| match section {
        ContextFrameSection::CompactionSummary { summary, .. } => Some(summary.clone()),
        _ => None,
    })
}

fn align_summary_frame_revision(
    frame: &mut ContextFrame,
    compaction_id: &CompactionId,
    revision: &ContextRevision,
) {
    frame.id = format!("compaction-summary:{}:{}", compaction_id.0, revision.0);
    frame.delivery_metadata.cache_key = Some(compaction_id.0.clone());
    frame.delivery_metadata.cache_revision = Some(revision.0.clone());
}

fn migrate_lineage_digest(
    history: &mut serde_json::Map<String, Value>,
    entries: &[AgentHistoryEntry],
) -> Result<(), DashRepositoryMigrationError> {
    let Some(lineage) = history.get_mut("lineage") else {
        return Ok(());
    };
    if lineage.is_null() {
        return Ok(());
    }
    let lineage = lineage
        .as_object_mut()
        .ok_or(DashRepositoryMigrationError::InvalidField(
            "store.history.lineage",
        ))?;
    let source_head: Option<HistoryEntryId> = lineage
        .get("source_head")
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|_| {
            DashRepositoryMigrationError::InvalidField("store.history.lineage.source_head")
        })?
        .flatten();
    let prefix_len = match source_head {
        Some(source_head) => entries
            .iter()
            .position(|entry| entry.entry_id == source_head)
            .map(|index| index + 1)
            .ok_or(DashRepositoryMigrationError::UnknownLineageHead(
                source_head,
            ))?,
        None => 0,
    };
    lineage.insert(
        "source_digest".to_owned(),
        Value::String(digest_entries(&entries[..prefix_len])),
    );
    Ok(())
}

fn digest_entries(entries: &[AgentHistoryEntry]) -> String {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    for entry in entries {
        let bytes = serde_json::to_vec(entry).expect("history entry serialization is infallible");
        hasher.update((bytes.len() as u64).to_be_bytes());
        hasher.update(bytes);
    }
    format!("{:x}", hasher.finalize())
}

#[derive(Debug, Error)]
pub enum DashRepositoryMigrationError {
    #[error("Dash repository migration is missing required field {0}")]
    MissingField(&'static str),
    #[error("Dash repository migration found invalid field {0}")]
    InvalidField(&'static str),
    #[error("decode migrated history entry at sequence {sequence}: {error}")]
    DecodeHistoryEntry {
        sequence: u64,
        #[source]
        error: serde_json::Error,
    },
    #[error("decode compaction summary frame: {0}")]
    DecodeSummaryFrame(serde_json::Error),
    #[error("compaction {0:?} applied without a preceding started fact")]
    AppliedWithoutStarted(CompactionId),
    #[error("compaction {0:?} applied without a canonical summary")]
    MissingCompactionSummary(CompactionId),
    #[error("fork lineage references unknown source head {0:?}")]
    UnknownLineageHead(HistoryEntryId),
    #[error("decode final Dash history: {0}")]
    DecodeFinalHistory(serde_json::Error),
    #[error("final Dash history is invalid: {0}")]
    InvalidFinalHistory(super::HistoryError),
    #[error("decode final Dash repository: {0}")]
    DecodeFinalRepository(serde_json::Error),
    #[error("final Dash store is invalid: {0}")]
    InvalidFinalStore(super::StoreError),
    #[error(
        "active compaction {0:?} has no durable side-effect boundary; migration cannot infer retry safety"
    )]
    AmbiguousActiveCompaction(CompactionId),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dash::{
        AgentSessionId, BranchId, CompactionMode, DashAgentStore, HistoryContribution,
        accepted_compaction_summary_frame,
    };

    #[test]
    fn migrates_nested_checkpoint_and_removes_duplicate_repository_paths() {
        let repository = completed_compaction_repository();
        let mut raw = serde_json::to_value(&repository).unwrap();
        raw.pointer_mut("/store")
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("changes".to_owned(), serde_json::json!([{"legacy": true}]));
        raw.pointer_mut("/store/lifecycle")
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("effects".to_owned(), serde_json::json!({"legacy": {}}));

        let entries = raw
            .pointer_mut("/store/history/entries")
            .unwrap()
            .as_array_mut()
            .unwrap();
        let queued = entries[0]["payload"].as_object_mut().unwrap();
        queued.insert(
            "operation_id".to_owned(),
            Value::String("compaction-effect".to_owned()),
        );
        let started = entries[1]["payload"].as_object_mut().unwrap();
        started.insert(
            "operation_id".to_owned(),
            Value::String("compaction-effect".to_owned()),
        );
        started.insert(
            "source_digest".to_owned(),
            Value::String("stale-digest".to_owned()),
        );
        let applied = entries[2]["payload"].as_object_mut().unwrap();
        let checkpoint = serde_json::json!({
            "operation_id": "compaction-effect",
            "context_revision": applied.remove("context_revision").unwrap(),
            "summary_frame": applied.remove("summary_frame").unwrap(),
            "retained_from": applied.remove("retained_from").unwrap(),
            "source_digest": "stale-digest",
            "summary": "summary",
            "compacted_entry_ids": [],
            "retained_entry_ids": [],
            "tool_pairs": []
        });
        applied.insert("checkpoint".to_owned(), checkpoint);

        let migrated = migrate_dash_repository(raw).unwrap();
        let migrated_json = serde_json::to_value(&migrated).unwrap();
        assert!(migrated_json.pointer("/store/changes").is_none());
        assert!(migrated_json.pointer("/store/lifecycle/effects").is_none());
        assert!(
            migrated_json
                .pointer("/store/history/entries/0/payload/operation_id")
                .is_none()
        );
        assert!(
            migrated_json
                .pointer("/store/history/entries/2/payload/checkpoint")
                .is_none()
        );
        let state = migrated.history().state().unwrap();
        assert_eq!(
            state
                .compactions
                .get(&CompactionId::new("compaction-1"))
                .unwrap()
                .status,
            super::super::ActivityStatus::Completed
        );
    }

    #[test]
    fn rejects_active_compaction_without_side_effect_boundary() {
        let mut history =
            AgentHistory::empty(AgentSessionId::new("session-1"), BranchId::new("branch-1"));
        append_queued_and_started(&mut history);
        let repository = DashAgentRepositoryState::new(DashAgentStore::new(history).unwrap());

        let error = migrate_dash_repository(serde_json::to_value(repository).unwrap()).unwrap_err();
        assert!(matches!(
            error,
            DashRepositoryMigrationError::AmbiguousActiveCompaction(_)
        ));
    }

    fn completed_compaction_repository() -> DashAgentRepositoryState {
        let mut history =
            AgentHistory::empty(AgentSessionId::new("session-1"), BranchId::new("branch-1"));
        let source_digest = append_queued_and_started(&mut history);
        let compaction_id = CompactionId::new("compaction-1");
        let revision = compaction_context_revision(&compaction_id, &source_digest, "summary", None);
        let frame = accepted_compaction_summary_frame(
            &compaction_id,
            &revision,
            "summary",
            CompactionMode::Manual,
            10,
            2,
            None,
            None,
            None,
            3,
        );
        history
            .append_batch(vec![
                HistoryContribution {
                    entry_id: HistoryEntryId::new("applied"),
                    payload: HistoryPayload::CompactionApplied {
                        compaction_id: compaction_id.clone(),
                        context_revision: revision,
                        summary_frame: Box::new(frame),
                        retained_from: None,
                    },
                },
                HistoryContribution {
                    entry_id: HistoryEntryId::new("completed"),
                    payload: HistoryPayload::CompactionCompleted {
                        compaction_id,
                        completed_at_ms: 4,
                    },
                },
            ])
            .unwrap();
        DashAgentRepositoryState::new(DashAgentStore::new(history).unwrap())
    }

    fn append_queued_and_started(history: &mut AgentHistory) -> String {
        let compaction_id = CompactionId::new("compaction-1");
        history
            .append(HistoryContribution {
                entry_id: HistoryEntryId::new("queued"),
                payload: HistoryPayload::CompactionQueued {
                    compaction_id: compaction_id.clone(),
                    mode: CompactionMode::Manual,
                    queued_at_ms: 1,
                },
            })
            .unwrap();
        let source_digest = history.digest();
        history
            .append(HistoryContribution {
                entry_id: HistoryEntryId::new("started"),
                payload: HistoryPayload::CompactionStarted {
                    compaction_id,
                    mode: CompactionMode::Manual,
                    source_head: history.head().cloned(),
                    source_digest: source_digest.clone(),
                    started_at_ms: 2,
                },
            })
            .unwrap();
        source_digest
    }
}
