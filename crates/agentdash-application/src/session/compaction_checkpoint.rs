use std::fmt;

use agentdash_agent_protocol::{BackboneEvent, PlatformEvent};
use agentdash_agent_types::{
    AgentMessage, MessageRef, ProjectedEntry, ProjectedTranscript, ProjectionKind,
    ProjectionOrigin, ProjectionSourceRange,
};
use agentdash_spi::{
    PersistedSessionEvent, SessionCompactionRecord, SessionProjectionSegmentRecord,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum CompactionCheckpointSource {
    ProjectionSegment,
    CompactionRecord,
    ContextEvent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CompactionCheckpointProvenance {
    pub source: CompactionCheckpointSource,
    pub compaction_id: Option<String>,
    pub segment_id: Option<String>,
    pub projection_version: Option<u64>,
    pub segment_type: Option<String>,
    pub strategy: Option<String>,
    pub trigger: Option<String>,
    pub phase: Option<String>,
}

impl CompactionCheckpointProvenance {
    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "source": match self.source {
                CompactionCheckpointSource::ProjectionSegment => "projection_segment",
                CompactionCheckpointSource::CompactionRecord => "compaction_record",
                CompactionCheckpointSource::ContextEvent => "context_event",
            },
            "compaction_id": self.compaction_id.clone(),
            "segment_id": self.segment_id.clone(),
            "projection_version": self.projection_version,
            "segment_type": self.segment_type.clone(),
            "strategy": self.strategy.clone(),
            "trigger": self.trigger.clone(),
            "phase": self.phase.clone(),
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct CompactionCheckpoint {
    pub summary: String,
    pub tokens_before: u64,
    pub messages_compacted: u32,
    pub compacted_until_ref: Option<MessageRef>,
    pub source_range: Option<ProjectionSourceRange>,
    pub first_kept_event_seq: Option<u64>,
    pub timestamp_ms: Option<u64>,
    pub compaction_id: Option<String>,
    pub segment_id: Option<String>,
    pub projection_version: Option<u64>,
    pub provenance: CompactionCheckpointProvenance,
    message_ref: MessageRef,
    origin: ProjectionOrigin,
    synthetic: bool,
}

impl CompactionCheckpoint {
    pub(super) fn suffix_start_event_seq(&self, head_event_seq: u64) -> u64 {
        self.first_kept_event_seq
            .or_else(|| {
                self.source_range
                    .as_ref()
                    .map(|range| range.end_event_seq.saturating_add(1))
            })
            .unwrap_or(head_event_seq.saturating_add(1))
    }

    pub(super) fn to_projected_entry(&self) -> Option<ProjectedEntry> {
        self.to_projected_entry_with_boundary(self.compacted_until_ref.clone())
    }

    fn to_projected_entry_with_boundary(
        &self,
        compacted_until_ref: Option<MessageRef>,
    ) -> Option<ProjectedEntry> {
        if self.summary.trim().is_empty() || self.messages_compacted == 0 {
            return None;
        }

        let mut entry = ProjectedEntry::projection(
            self.message_ref.clone(),
            ProjectionKind::CompactionSummary,
            AgentMessage::CompactionSummary {
                summary: self.summary.clone(),
                tokens_before: self.tokens_before,
                messages_compacted: self.messages_compacted,
                compacted_until_ref,
                timestamp: self.timestamp_ms,
            },
            self.segment_id.clone(),
            self.source_range.clone(),
        );
        entry.origin = self.origin;
        entry.synthetic = self.synthetic;
        entry.provenance = self.provenance.to_json();
        Some(entry)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum CompactionCheckpointError {
    InvalidMessageRef {
        key: String,
    },
    InvalidSourceRange {
        start: u64,
        end: u64,
    },
    ProjectionVersionMismatch {
        compaction_id: String,
        compaction_version: u64,
        segment_id: String,
        segment_version: u64,
    },
    ProjectionKindMismatch {
        compaction_id: String,
        compaction_kind: String,
        segment_id: String,
        segment_kind: String,
    },
    SegmentCompactionMismatch {
        compaction_id: String,
        segment_id: String,
        generated_by_compaction_id: String,
    },
}

impl fmt::Display for CompactionCheckpointError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidMessageRef { key } => {
                write!(f, "compaction checkpoint 的 {key} 不是合法 MessageRef")
            }
            Self::InvalidSourceRange { start, end } => write!(
                f,
                "compaction checkpoint source range 非法: start_event_seq={start}, end_event_seq={end}"
            ),
            Self::ProjectionVersionMismatch {
                compaction_id,
                compaction_version,
                segment_id,
                segment_version,
            } => write!(
                f,
                "projection segment {segment_id} 版本 {segment_version} 与 compaction {compaction_id} 版本 {compaction_version} 不一致"
            ),
            Self::ProjectionKindMismatch {
                compaction_id,
                compaction_kind,
                segment_id,
                segment_kind,
            } => write!(
                f,
                "projection segment {segment_id} kind {segment_kind} 与 compaction {compaction_id} kind {compaction_kind} 不一致"
            ),
            Self::SegmentCompactionMismatch {
                compaction_id,
                segment_id,
                generated_by_compaction_id,
            } => write!(
                f,
                "projection segment {segment_id} 归属 {generated_by_compaction_id} 与 compaction {compaction_id} 不一致"
            ),
        }
    }
}

impl std::error::Error for CompactionCheckpointError {}

pub(super) fn projection_entries_from_checkpoint_records(
    compaction: &SessionCompactionRecord,
    segments: &[SessionProjectionSegmentRecord],
) -> Result<Vec<ProjectedEntry>, CompactionCheckpointError> {
    let mut entries = Vec::new();
    for segment in segments
        .iter()
        .filter(|segment| segment.segment_type == "summary_chunk")
    {
        if let Some(entry) = checkpoint_from_projection_segment(compaction, segment)?
            .and_then(|checkpoint| checkpoint.to_projected_entry())
        {
            entries.push(entry);
        }
    }

    if entries.is_empty()
        && let Some(entry) = checkpoint_from_compaction_record(compaction)?
            .and_then(|checkpoint| checkpoint.to_projected_entry())
    {
        entries.push(entry);
    }
    Ok(entries)
}

pub(super) fn suffix_start_event_seq_from_compaction(
    compaction: &SessionCompactionRecord,
    head_event_seq: u64,
) -> Result<u64, CompactionCheckpointError> {
    let source_range = source_range(
        compaction.source_start_event_seq,
        compaction.source_end_event_seq,
    )?;
    Ok(compaction
        .first_kept_event_seq
        .or_else(|| source_range.map(|range| range.end_event_seq.saturating_add(1)))
        .unwrap_or(head_event_seq.saturating_add(1)))
}

pub(super) fn checkpoint_from_projection_segment(
    compaction: &SessionCompactionRecord,
    segment: &SessionProjectionSegmentRecord,
) -> Result<Option<CompactionCheckpoint>, CompactionCheckpointError> {
    validate_segment(compaction, segment)?;
    let Some(summary) =
        segment_summary_text(segment).or_else(|| non_empty_string(&compaction.summary))
    else {
        return Ok(None);
    };
    let source_range =
        match source_range(segment.source_start_event_seq, segment.source_end_event_seq)? {
            Some(range) => Some(range),
            None => source_range(
                compaction.source_start_event_seq,
                compaction.source_end_event_seq,
            )?,
        };
    let messages_compacted = read_u32(&segment.source_refs_json, "messages_compacted")
        .or_else(|| read_u32(&compaction.token_stats_json, "messages_compacted"))
        .unwrap_or_default();
    let compacted_until_ref =
        read_message_ref_optional(&segment.source_refs_json, "compacted_until_ref")?.or(
            read_message_ref_optional(
                &compaction.replacement_projection_json,
                "compacted_until_ref",
            )?,
        );

    Ok(Some(CompactionCheckpoint {
        summary,
        tokens_before: tokens_before(compaction),
        messages_compacted,
        compacted_until_ref,
        source_range,
        first_kept_event_seq: compaction.first_kept_event_seq,
        timestamp_ms: Some(segment.created_at_ms.max(0) as u64),
        compaction_id: Some(compaction.id.clone()),
        segment_id: Some(segment.id.clone()),
        projection_version: Some(compaction.projection_version),
        provenance: CompactionCheckpointProvenance {
            source: CompactionCheckpointSource::ProjectionSegment,
            compaction_id: Some(compaction.id.clone()),
            segment_id: Some(segment.id.clone()),
            projection_version: Some(compaction.projection_version),
            segment_type: Some(segment.segment_type.clone()),
            strategy: Some(compaction.strategy.clone()),
            trigger: Some(compaction.trigger.clone()),
            phase: compaction.phase.clone(),
        },
        message_ref: MessageRef {
            turn_id: format!("_projection:{}", segment.id),
            entry_index: segment.sort_order.try_into().unwrap_or(u32::MAX),
        },
        origin: ProjectionOrigin::from_label(&segment.origin),
        synthetic: segment.synthetic,
    }))
}

pub(super) fn checkpoint_from_compaction_record(
    compaction: &SessionCompactionRecord,
) -> Result<Option<CompactionCheckpoint>, CompactionCheckpointError> {
    let Some(summary) = non_empty_string(&compaction.summary) else {
        return Ok(None);
    };
    let source_range = source_range(
        compaction.source_start_event_seq,
        compaction.source_end_event_seq,
    )?;
    let compacted_until_ref = read_message_ref_optional(
        &compaction.replacement_projection_json,
        "compacted_until_ref",
    )?;

    Ok(Some(CompactionCheckpoint {
        summary,
        tokens_before: tokens_before(compaction),
        messages_compacted: read_u32(&compaction.token_stats_json, "messages_compacted")
            .unwrap_or_default(),
        compacted_until_ref,
        source_range,
        first_kept_event_seq: compaction.first_kept_event_seq,
        timestamp_ms: compaction
            .completed_at_ms
            .or(Some(compaction.created_at_ms))
            .map(|value| value.max(0) as u64),
        compaction_id: Some(compaction.id.clone()),
        segment_id: None,
        projection_version: Some(compaction.projection_version),
        provenance: CompactionCheckpointProvenance {
            source: CompactionCheckpointSource::CompactionRecord,
            compaction_id: Some(compaction.id.clone()),
            segment_id: None,
            projection_version: Some(compaction.projection_version),
            segment_type: None,
            strategy: Some(compaction.strategy.clone()),
            trigger: Some(compaction.trigger.clone()),
            phase: compaction.phase.clone(),
        },
        message_ref: MessageRef {
            turn_id: format!("_compaction:{}", compaction.id),
            entry_index: 0,
        },
        origin: ProjectionOrigin::Projection,
        synthetic: true,
    }))
}

pub(super) fn latest_context_compacted_checkpoint(
    events: &[PersistedSessionEvent],
) -> Option<CompactionCheckpoint> {
    events.iter().rev().find_map(|event| {
        checkpoint_from_context_compacted_event(event)
            .ok()
            .flatten()
    })
}

pub(super) fn checkpoint_from_context_compacted_event(
    event: &PersistedSessionEvent,
) -> Result<Option<CompactionCheckpoint>, CompactionCheckpointError> {
    let BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate { key, value }) =
        &event.notification.event
    else {
        return Ok(None);
    };
    if key != "context_compacted" {
        return Ok(None);
    }
    checkpoint_from_context_compacted_value(value)
}

pub(super) fn checkpoint_from_context_compacted_value(
    value: &serde_json::Value,
) -> Result<Option<CompactionCheckpoint>, CompactionCheckpointError> {
    let Some(summary) = value
        .get("summary")
        .and_then(serde_json::Value::as_str)
        .and_then(non_empty_string)
    else {
        return Ok(None);
    };

    Ok(Some(CompactionCheckpoint {
        summary,
        tokens_before: value
            .get("tokens_before")
            .and_then(serde_json::Value::as_u64)
            .or_else(|| value.get("before").and_then(serde_json::Value::as_u64))
            .unwrap_or_default(),
        messages_compacted: read_u32(value, "messages_compacted").unwrap_or_default(),
        compacted_until_ref: read_message_ref_optional(value, "compacted_until_ref")?,
        source_range: None,
        first_kept_event_seq: None,
        timestamp_ms: value
            .get("timestamp_ms")
            .and_then(serde_json::Value::as_u64),
        compaction_id: None,
        segment_id: None,
        projection_version: value
            .get("projection_version")
            .and_then(serde_json::Value::as_u64),
        provenance: CompactionCheckpointProvenance {
            source: CompactionCheckpointSource::ContextEvent,
            compaction_id: value
                .get("compaction_id")
                .and_then(serde_json::Value::as_str)
                .map(ToString::to_string),
            segment_id: None,
            projection_version: value
                .get("projection_version")
                .and_then(serde_json::Value::as_u64),
            segment_type: None,
            strategy: value
                .get("strategy")
                .and_then(serde_json::Value::as_str)
                .map(ToString::to_string),
            trigger: value
                .get("trigger")
                .and_then(serde_json::Value::as_str)
                .map(ToString::to_string),
            phase: value
                .get("phase")
                .and_then(serde_json::Value::as_str)
                .map(ToString::to_string),
        },
        message_ref: MessageRef {
            turn_id: "_compaction_summary".to_string(),
            entry_index: 0,
        },
        origin: ProjectionOrigin::Projection,
        synthetic: true,
    }))
}

pub(super) fn apply_checkpoint_to_projected_entries(
    raw_entries: Vec<ProjectedEntry>,
    checkpoint: Option<CompactionCheckpoint>,
) -> ProjectedTranscript {
    let Some(checkpoint) = checkpoint else {
        return ProjectedTranscript {
            entries: raw_entries,
        };
    };
    if checkpoint.summary.trim().is_empty() || checkpoint.messages_compacted == 0 {
        return ProjectedTranscript {
            entries: raw_entries,
        };
    }
    let Some(boundary_ref) = checkpoint.compacted_until_ref.as_ref() else {
        return ProjectedTranscript {
            entries: raw_entries,
        };
    };

    let cut = raw_entries
        .iter()
        .position(|entry| entry.message_ref == *boundary_ref)
        .map(|pos| pos + 1)
        .unwrap_or(0);
    let derived_ref = if cut > 0 {
        Some(raw_entries[cut - 1].message_ref.clone())
    } else {
        Some(boundary_ref.clone())
    };
    let Some(summary_entry) = checkpoint.to_projected_entry_with_boundary(derived_ref) else {
        return ProjectedTranscript {
            entries: raw_entries,
        };
    };

    let mut entries = vec![summary_entry];
    entries.extend(raw_entries.into_iter().skip(cut));
    ProjectedTranscript { entries }
}

fn validate_segment(
    compaction: &SessionCompactionRecord,
    segment: &SessionProjectionSegmentRecord,
) -> Result<(), CompactionCheckpointError> {
    if compaction.projection_version != segment.projection_version {
        return Err(CompactionCheckpointError::ProjectionVersionMismatch {
            compaction_id: compaction.id.clone(),
            compaction_version: compaction.projection_version,
            segment_id: segment.id.clone(),
            segment_version: segment.projection_version,
        });
    }
    if compaction.projection_kind != segment.projection_kind {
        return Err(CompactionCheckpointError::ProjectionKindMismatch {
            compaction_id: compaction.id.clone(),
            compaction_kind: compaction.projection_kind.clone(),
            segment_id: segment.id.clone(),
            segment_kind: segment.projection_kind.clone(),
        });
    }
    if let Some(generated_by) = segment.generated_by_compaction_id.as_deref()
        && generated_by != compaction.id
    {
        return Err(CompactionCheckpointError::SegmentCompactionMismatch {
            compaction_id: compaction.id.clone(),
            segment_id: segment.id.clone(),
            generated_by_compaction_id: generated_by.to_string(),
        });
    }
    Ok(())
}

fn segment_summary_text(segment: &SessionProjectionSegmentRecord) -> Option<String> {
    segment
        .content_json
        .get("content")
        .and_then(serde_json::Value::as_str)
        .or_else(|| {
            segment
                .content_json
                .get("summary")
                .and_then(serde_json::Value::as_str)
        })
        .or_else(|| segment.content_json.as_str())
        .and_then(non_empty_string)
}

fn non_empty_string(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn source_range(
    start: Option<u64>,
    end: Option<u64>,
) -> Result<Option<ProjectionSourceRange>, CompactionCheckpointError> {
    match (start, end) {
        (Some(start), Some(end)) if end >= start => Ok(Some(ProjectionSourceRange {
            start_event_seq: start,
            end_event_seq: end,
        })),
        (Some(start), Some(end)) => {
            Err(CompactionCheckpointError::InvalidSourceRange { start, end })
        }
        _ => Ok(None),
    }
}

fn read_u32(value: &serde_json::Value, key: &str) -> Option<u32> {
    value
        .get(key)
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
}

fn read_message_ref_optional(
    value: &serde_json::Value,
    key: &str,
) -> Result<Option<MessageRef>, CompactionCheckpointError> {
    let Some(raw) = value.get(key) else {
        return Ok(None);
    };
    if raw.is_null() {
        return Ok(None);
    }
    serde_json::from_value(raw.clone()).map(Some).map_err(|_| {
        CompactionCheckpointError::InvalidMessageRef {
            key: key.to_string(),
        }
    })
}

fn tokens_before(compaction: &SessionCompactionRecord) -> u64 {
    compaction
        .token_stats_json
        .get("before")
        .and_then(serde_json::Value::as_u64)
        .or_else(|| {
            compaction
                .token_stats_json
                .get("tokens_before")
                .and_then(serde_json::Value::as_u64)
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use agentdash_agent_protocol::{BackboneEnvelope, PlatformEvent, SourceInfo};
    use agentdash_spi::{SESSION_PROJECTION_KIND_MODEL_CONTEXT, SessionCompactionStatus};

    use super::*;

    fn compaction() -> SessionCompactionRecord {
        SessionCompactionRecord {
            id: "compaction-1".to_string(),
            session_id: "session-1".to_string(),
            branch_id: None,
            projection_kind: SESSION_PROJECTION_KIND_MODEL_CONTEXT.to_string(),
            projection_version: 7,
            lifecycle_item_id: "compact-item".to_string(),
            start_event_seq: 1,
            completed_event_seq: Some(9),
            failed_event_seq: None,
            status: SessionCompactionStatus::ProjectionCommitted,
            trigger: "auto".to_string(),
            reason: Some("token_pressure".to_string()),
            phase: Some("pre_provider".to_string()),
            strategy: "summary_prefix".to_string(),
            budget_scope: Some("model_context".to_string()),
            base_head_event_seq: Some(8),
            source_start_event_seq: Some(1),
            source_end_event_seq: Some(8),
            first_kept_event_seq: Some(9),
            summary: "compaction summary".to_string(),
            replacement_projection_json: serde_json::json!({
                "compacted_until_ref": { "turn_id": "fallback-turn", "entry_index": 1 },
            }),
            token_stats_json: serde_json::json!({
                "tokens_before": 48000,
                "messages_compacted": 99,
            }),
            diagnostics_json: serde_json::json!({}),
            created_by: Some("agent".to_string()),
            created_at_ms: 1000,
            completed_at_ms: Some(2000),
        }
    }

    fn segment() -> SessionProjectionSegmentRecord {
        SessionProjectionSegmentRecord {
            id: "segment-1".to_string(),
            session_id: "session-1".to_string(),
            branch_id: None,
            projection_kind: SESSION_PROJECTION_KIND_MODEL_CONTEXT.to_string(),
            projection_version: 7,
            sort_order: 0,
            segment_type: "summary_chunk".to_string(),
            origin: "projection".to_string(),
            synthetic: true,
            source_start_event_seq: Some(2),
            source_end_event_seq: Some(4),
            source_refs_json: serde_json::json!({
                "messages_compacted": 2,
                "compacted_until_ref": { "turn_id": "segment-turn", "entry_index": 0 },
            }),
            generated_by_compaction_id: Some("compaction-1".to_string()),
            content_json: serde_json::json!({ "content": "segment summary" }),
            token_estimate: Some(128),
            created_at_ms: 1500,
        }
    }

    fn context_event(value: serde_json::Value) -> PersistedSessionEvent {
        PersistedSessionEvent {
            session_id: "session-1".to_string(),
            event_seq: 10,
            occurred_at_ms: 10,
            committed_at_ms: 10,
            session_update_type: "platform".to_string(),
            turn_id: Some("turn-1".to_string()),
            entry_index: None,
            tool_call_id: None,
            notification: BackboneEnvelope::new(
                BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate {
                    key: "context_compacted".to_string(),
                    value,
                }),
                "session-1",
                SourceInfo {
                    connector_id: "test".to_string(),
                    connector_type: "unit".to_string(),
                    executor_id: None,
                },
            ),
        }
    }

    #[test]
    fn segment_metadata_takes_priority_over_compaction_metadata() {
        let checkpoint = checkpoint_from_projection_segment(&compaction(), &segment())
            .expect("checkpoint should parse")
            .expect("checkpoint should exist");

        assert_eq!(checkpoint.summary, "segment summary");
        assert_eq!(checkpoint.messages_compacted, 2);
        assert_eq!(
            checkpoint.compacted_until_ref,
            Some(MessageRef {
                turn_id: "segment-turn".to_string(),
                entry_index: 0,
            })
        );
        assert_eq!(
            checkpoint
                .source_range
                .as_ref()
                .map(|range| (range.start_event_seq, range.end_event_seq)),
            Some((2, 4))
        );
        assert_eq!(checkpoint.suffix_start_event_seq(10), 9);
    }

    #[test]
    fn compaction_metadata_is_projection_fallback() {
        let mut segment = segment();
        segment.source_start_event_seq = None;
        segment.source_end_event_seq = None;
        segment.source_refs_json = serde_json::json!({});
        segment.content_json = serde_json::json!({});

        let checkpoint = checkpoint_from_projection_segment(&compaction(), &segment)
            .expect("checkpoint should parse")
            .expect("checkpoint should exist");
        let entry = checkpoint
            .to_projected_entry()
            .expect("summary entry should build");

        assert_eq!(checkpoint.summary, "compaction summary");
        assert_eq!(checkpoint.messages_compacted, 99);
        assert_eq!(
            checkpoint.compacted_until_ref,
            Some(MessageRef {
                turn_id: "fallback-turn".to_string(),
                entry_index: 1,
            })
        );
        assert_eq!(
            checkpoint
                .source_range
                .as_ref()
                .map(|range| (range.start_event_seq, range.end_event_seq)),
            Some((1, 8))
        );
        assert_eq!(entry.projection_segment_id.as_deref(), Some("segment-1"));
    }

    #[test]
    fn context_compacted_event_payload_parses_checkpoint() {
        let event = context_event(serde_json::json!({
            "summary": "event summary",
            "tokens_before": 42000,
            "messages_compacted": 3,
            "compacted_until_ref": { "turn_id": "event-turn", "entry_index": 2 },
            "timestamp_ms": 123_u64,
        }));

        let checkpoint = checkpoint_from_context_compacted_event(&event)
            .expect("checkpoint should parse")
            .expect("checkpoint should exist");
        assert_eq!(checkpoint.summary, "event summary");
        assert_eq!(checkpoint.tokens_before, 42000);
        assert_eq!(checkpoint.messages_compacted, 3);
        assert_eq!(checkpoint.timestamp_ms, Some(123));
        assert_eq!(
            checkpoint.compacted_until_ref,
            Some(MessageRef {
                turn_id: "event-turn".to_string(),
                entry_index: 2,
            })
        );
    }

    #[test]
    fn invalid_message_ref_returns_error_and_latest_discovery_skips_it() {
        let bad = context_event(serde_json::json!({
            "summary": "bad",
            "messages_compacted": 1,
            "compacted_until_ref": { "turn_id": 1 },
        }));
        let good = context_event(serde_json::json!({
            "summary": "good",
            "messages_compacted": 1,
            "compacted_until_ref": { "turn_id": "ok", "entry_index": 0 },
        }));

        assert!(matches!(
            checkpoint_from_context_compacted_event(&bad),
            Err(CompactionCheckpointError::InvalidMessageRef { .. })
        ));
        let checkpoint = latest_context_compacted_checkpoint(&[good, bad])
            .expect("latest valid checkpoint should be discovered");
        assert_eq!(checkpoint.summary, "good");
    }

    #[test]
    fn null_message_ref_is_treated_as_absent_boundary() {
        let event = context_event(serde_json::json!({
            "summary": "event summary",
            "messages_compacted": 1,
            "compacted_until_ref": null,
        }));

        let checkpoint = checkpoint_from_context_compacted_event(&event)
            .expect("checkpoint should parse")
            .expect("checkpoint should exist");
        assert_eq!(checkpoint.compacted_until_ref, None);
        assert!(checkpoint.to_projected_entry().is_some());
        assert!(apply_checkpoint_to_projected_entries(Vec::new(), Some(checkpoint)).is_empty());
    }

    #[test]
    fn invalid_source_range_is_rejected() {
        let mut segment = segment();
        segment.source_start_event_seq = Some(5);
        segment.source_end_event_seq = Some(4);

        assert!(matches!(
            checkpoint_from_projection_segment(&compaction(), &segment),
            Err(CompactionCheckpointError::InvalidSourceRange { start: 5, end: 4 })
        ));
    }

    #[test]
    fn fallback_summary_entry_preserves_boundary_metadata() {
        let checkpoint = checkpoint_from_compaction_record(&compaction())
            .expect("checkpoint should parse")
            .expect("checkpoint should exist");
        let entry = checkpoint
            .to_projected_entry()
            .expect("summary entry should build");

        match entry.message {
            AgentMessage::CompactionSummary {
                messages_compacted,
                compacted_until_ref,
                ..
            } => {
                assert_eq!(messages_compacted, 99);
                assert_eq!(
                    compacted_until_ref,
                    Some(MessageRef {
                        turn_id: "fallback-turn".to_string(),
                        entry_index: 1,
                    })
                );
            }
            other => panic!("unexpected message: {other:?}"),
        }
    }
}
