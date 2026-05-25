use std::io;

use agentdash_agent_types::{
    AgentContextEnvelope, AgentInputMessage, AgentMessage, MessageRef, ProjectedEntry,
    ProjectedTranscript, ProjectionKind, ProjectionOrigin, ProjectionSourceRange,
};
use agentdash_spi::{
    SESSION_PROJECTION_KIND_MODEL_CONTEXT, SessionCompactionRecord, SessionCompactionStatus,
    SessionProjectionHeadRecord, SessionProjectionSegmentRecord,
};

use super::continuation::build_raw_projected_transcript_from_filtered_events;
use super::persistence::{PersistedSessionEvent, SessionStoreSet};

#[derive(Clone)]
pub struct ContextProjector {
    stores: SessionStoreSet,
}

impl ContextProjector {
    pub fn new(stores: SessionStoreSet) -> Self {
        Self { stores }
    }

    pub async fn build_model_context(
        &self,
        session_id: &str,
        branch_id: Option<&str>,
    ) -> io::Result<AgentContextEnvelope> {
        let events = self.stores.events.list_all_events(session_id).await?;
        let head = self
            .stores
            .projections
            .read_projection_head(session_id, branch_id, SESSION_PROJECTION_KIND_MODEL_CONTEXT)
            .await?;

        match head {
            Some(head) => {
                self.build_from_projection_head(session_id, branch_id, &events, head)
                    .await
            }
            None => Ok(envelope_from_transcript(
                session_id,
                branch_id,
                0,
                latest_event_seq(&events),
                None,
                None,
                build_raw_projected_transcript_from_filtered_events(events.iter()),
            )),
        }
    }

    pub async fn build_model_context_at_event(
        &self,
        session_id: &str,
        branch_id: Option<&str>,
        head_event_seq: u64,
    ) -> io::Result<AgentContextEnvelope> {
        let events = self.stores.events.list_all_events(session_id).await?;
        let head = self
            .stores
            .projections
            .read_projection_head(session_id, branch_id, SESSION_PROJECTION_KIND_MODEL_CONTEXT)
            .await?;

        if let Some(mut head) = head {
            if let Some(active_compaction_id) = head.active_compaction_id.as_deref() {
                let compaction = self
                    .stores
                    .compactions
                    .get_compaction(session_id, active_compaction_id)
                    .await?;
                if compaction
                    .as_ref()
                    .is_some_and(|record| compaction_covers_head(record, head_event_seq))
                {
                    head.head_event_seq = head_event_seq;
                    return self
                        .build_from_projection_head(session_id, branch_id, &events, head)
                        .await;
                }
            } else {
                head.head_event_seq = head_event_seq;
                return self
                    .build_from_projection_head(session_id, branch_id, &events, head)
                    .await;
            }
        }

        Ok(envelope_from_transcript(
            session_id,
            branch_id,
            0,
            head_event_seq,
            None,
            None,
            build_raw_projected_transcript_from_filtered_events(
                events
                    .iter()
                    .filter(|event| event.event_seq <= head_event_seq),
            ),
        ))
    }

    pub async fn build_model_context_from_compaction(
        &self,
        session_id: &str,
        branch_id: Option<&str>,
        compaction_id: &str,
        head_event_seq: Option<u64>,
    ) -> io::Result<AgentContextEnvelope> {
        let events = self.stores.events.list_all_events(session_id).await?;
        let compaction = self
            .stores
            .compactions
            .get_compaction(session_id, compaction_id)
            .await?
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("compaction {compaction_id} 不存在"),
                )
            })?;
        validate_active_compaction(&compaction)?;
        let head = SessionProjectionHeadRecord {
            session_id: session_id.to_string(),
            branch_id: branch_id.map(ToString::to_string),
            projection_kind: SESSION_PROJECTION_KIND_MODEL_CONTEXT.to_string(),
            projection_version: compaction.projection_version,
            head_event_seq: head_event_seq
                .or(compaction.completed_event_seq)
                .or(compaction.source_end_event_seq)
                .unwrap_or_else(|| latest_event_seq(&events)),
            active_compaction_id: Some(compaction_id.to_string()),
            updated_by_event_seq: compaction.completed_event_seq,
            updated_at_ms: compaction
                .completed_at_ms
                .unwrap_or(compaction.created_at_ms),
        };
        self.build_from_projection_head(session_id, branch_id, &events, head)
            .await
    }

    pub async fn build_projected_transcript(
        &self,
        session_id: &str,
        branch_id: Option<&str>,
    ) -> io::Result<ProjectedTranscript> {
        Ok(self
            .build_model_context(session_id, branch_id)
            .await?
            .into_projected_transcript())
    }

    async fn build_from_projection_head(
        &self,
        session_id: &str,
        branch_id: Option<&str>,
        events: &[PersistedSessionEvent],
        head: SessionProjectionHeadRecord,
    ) -> io::Result<AgentContextEnvelope> {
        let Some(active_compaction_id) = head.active_compaction_id.as_deref() else {
            return Ok(envelope_from_transcript(
                session_id,
                branch_id,
                head.projection_version,
                head.head_event_seq,
                None,
                None,
                build_raw_projected_transcript_from_filtered_events(
                    events
                        .iter()
                        .filter(|event| event.event_seq <= head.head_event_seq),
                ),
            ));
        };

        let compaction = self
            .stores
            .compactions
            .get_compaction(session_id, active_compaction_id)
            .await?
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("projection head 指向不存在的 compaction {active_compaction_id}"),
                )
            })?;
        validate_active_compaction(&compaction)?;

        let segments = self
            .stores
            .projections
            .list_projection_segments(
                session_id,
                branch_id,
                SESSION_PROJECTION_KIND_MODEL_CONTEXT,
                head.projection_version,
            )
            .await?;

        let mut entries = projection_entries_from_segments(&compaction, &segments);
        let suffix_start_event_seq = compaction
            .first_kept_event_seq
            .or_else(|| {
                compaction
                    .source_end_event_seq
                    .map(|seq| seq.saturating_add(1))
            })
            .unwrap_or(head.head_event_seq.saturating_add(1));
        let suffix =
            build_raw_projected_transcript_from_filtered_events(events.iter().filter(|event| {
                event.event_seq >= suffix_start_event_seq && event.event_seq <= head.head_event_seq
            }));
        entries.extend(suffix.entries);

        Ok(envelope_from_entries(
            session_id,
            branch_id,
            head.projection_version,
            head.head_event_seq,
            Some(active_compaction_id.to_string()),
            token_estimate(&segments),
            entries,
        ))
    }
}

fn validate_active_compaction(compaction: &SessionCompactionRecord) -> io::Result<()> {
    if compaction.status != SessionCompactionStatus::ProjectionCommitted {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "active compaction {} 状态不是 projection_committed",
                compaction.id
            ),
        ));
    }
    Ok(())
}

fn compaction_covers_head(compaction: &SessionCompactionRecord, head_event_seq: u64) -> bool {
    if compaction.strategy == "fork_initial_projection" {
        return true;
    }
    compaction
        .source_end_event_seq
        .map(|source_end| source_end <= head_event_seq)
        .unwrap_or(true)
}

fn envelope_from_transcript(
    session_id: &str,
    branch_id: Option<&str>,
    projection_version: u64,
    head_event_seq: u64,
    active_compaction_id: Option<String>,
    token_estimate: Option<u64>,
    transcript: ProjectedTranscript,
) -> AgentContextEnvelope {
    envelope_from_entries(
        session_id,
        branch_id,
        projection_version,
        head_event_seq,
        active_compaction_id,
        token_estimate,
        transcript.entries,
    )
}

fn envelope_from_entries(
    session_id: &str,
    branch_id: Option<&str>,
    projection_version: u64,
    head_event_seq: u64,
    active_compaction_id: Option<String>,
    token_estimate: Option<u64>,
    entries: Vec<ProjectedEntry>,
) -> AgentContextEnvelope {
    AgentContextEnvelope {
        session_id: session_id.to_string(),
        branch_id: branch_id.map(ToString::to_string),
        projection_kind: ProjectionKind::ModelContext,
        projection_version,
        head_event_seq,
        active_compaction_id,
        token_estimate,
        messages: entries.into_iter().map(AgentInputMessage::from).collect(),
    }
}

fn projection_entries_from_segments(
    compaction: &SessionCompactionRecord,
    segments: &[SessionProjectionSegmentRecord],
) -> Vec<ProjectedEntry> {
    let mut entries = Vec::new();
    for segment in segments {
        match segment.segment_type.as_str() {
            "context_envelope" => {
                entries.extend(context_entries_from_segment(compaction, segment));
            }
            "summary_chunk" => {
                if let Some(entry) = summary_entry_from_segment(compaction, segment) {
                    entries.push(entry);
                }
            }
            _ => {}
        }
    }
    if entries.is_empty()
        && let Some(entry) = summary_entry_from_compaction(compaction)
    {
        entries.push(entry);
    }
    entries
}

fn context_entries_from_segment(
    compaction: &SessionCompactionRecord,
    segment: &SessionProjectionSegmentRecord,
) -> Vec<ProjectedEntry> {
    let messages = segment
        .content_json
        .get("messages")
        .cloned()
        .unwrap_or_else(|| segment.content_json.clone());
    let Ok(messages) = serde_json::from_value::<Vec<AgentInputMessage>>(messages) else {
        return Vec::new();
    };
    let segment_range = source_range(segment.source_start_event_seq, segment.source_end_event_seq);
    messages
        .into_iter()
        .enumerate()
        .map(|(index, message)| {
            let original_provenance = message.provenance.clone();
            let mut entry = ProjectedEntry::from(message);
            entry.origin = ProjectionOrigin::Projection;
            entry.synthetic = true;
            entry.source_event_seq = None;
            entry.source_range = segment_range.clone();
            entry.projection_segment_id = Some(segment.id.clone());
            entry.provenance = serde_json::json!({
                "compaction_id": compaction.id,
                "projection_version": compaction.projection_version,
                "segment_type": segment.segment_type,
                "segment_index": index,
                "strategy": compaction.strategy,
                "trigger": compaction.trigger,
                "source_refs": segment.source_refs_json,
                "original_provenance": original_provenance,
            });
            entry
        })
        .collect()
}

fn summary_entry_from_segment(
    compaction: &SessionCompactionRecord,
    segment: &SessionProjectionSegmentRecord,
) -> Option<ProjectedEntry> {
    let summary = segment_summary_text(segment)
        .or_else(|| non_empty_string(&compaction.summary))
        .filter(|value| !value.trim().is_empty())?;
    let source_range = source_range(segment.source_start_event_seq, segment.source_end_event_seq)
        .or_else(|| {
            source_range(
                compaction.source_start_event_seq,
                compaction.source_end_event_seq,
            )
        });
    let message_ref = MessageRef {
        turn_id: format!("_projection:{}", segment.id),
        entry_index: segment.sort_order.try_into().unwrap_or(u32::MAX),
    };
    let mut entry = ProjectedEntry::projection(
        message_ref,
        ProjectionKind::CompactionSummary,
        AgentMessage::CompactionSummary {
            summary,
            tokens_before: tokens_before(compaction),
            messages_compacted: messages_compacted(source_range.as_ref()),
            compacted_until_ref: None,
            timestamp: Some(segment.created_at_ms.max(0) as u64),
        },
        Some(segment.id.clone()),
        source_range,
    );
    entry.origin = ProjectionOrigin::from_label(&segment.origin);
    entry.synthetic = segment.synthetic;
    entry.provenance = serde_json::json!({
        "compaction_id": compaction.id,
        "projection_version": compaction.projection_version,
        "segment_type": segment.segment_type,
        "strategy": compaction.strategy,
        "trigger": compaction.trigger,
        "phase": compaction.phase,
    });
    Some(entry)
}

fn summary_entry_from_compaction(compaction: &SessionCompactionRecord) -> Option<ProjectedEntry> {
    let summary = non_empty_string(&compaction.summary)?;
    let source_range = source_range(
        compaction.source_start_event_seq,
        compaction.source_end_event_seq,
    );
    Some(ProjectedEntry::projection(
        MessageRef {
            turn_id: format!("_compaction:{}", compaction.id),
            entry_index: 0,
        },
        ProjectionKind::CompactionSummary,
        AgentMessage::CompactionSummary {
            summary,
            tokens_before: tokens_before(compaction),
            messages_compacted: messages_compacted(source_range.as_ref()),
            compacted_until_ref: None,
            timestamp: compaction
                .completed_at_ms
                .or(Some(compaction.created_at_ms))
                .map(|value| value.max(0) as u64),
        },
        None,
        source_range,
    ))
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

fn source_range(start: Option<u64>, end: Option<u64>) -> Option<ProjectionSourceRange> {
    match (start, end) {
        (Some(start), Some(end)) if end >= start => Some(ProjectionSourceRange {
            start_event_seq: start,
            end_event_seq: end,
        }),
        _ => None,
    }
}

fn messages_compacted(source_range: Option<&ProjectionSourceRange>) -> u32 {
    source_range
        .and_then(|range| {
            range
                .end_event_seq
                .checked_sub(range.start_event_seq)?
                .checked_add(1)
        })
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or_default()
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

fn token_estimate(segments: &[SessionProjectionSegmentRecord]) -> Option<u64> {
    let mut total = 0_u64;
    let mut has_estimate = false;
    for segment in segments {
        if let Some(value) = segment.token_estimate {
            has_estimate = true;
            total = total.saturating_add(value);
        }
    }
    has_estimate.then_some(total)
}

fn latest_event_seq(events: &[PersistedSessionEvent]) -> u64 {
    events
        .iter()
        .map(|event| event.event_seq)
        .max()
        .unwrap_or_default()
}
