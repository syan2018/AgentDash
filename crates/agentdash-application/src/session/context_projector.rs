use std::io;

use agentdash_agent_types::{
    AgentContextEnvelope, AgentInputMessage, AgentMessage, ContentPart, MessageRef, ProjectedEntry,
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
            None => {
                let transcript = build_raw_projected_transcript_from_filtered_events(events.iter());
                let token_estimate = entries_token_estimate(&transcript.entries);
                Ok(envelope_from_transcript(
                    session_id,
                    branch_id,
                    0,
                    latest_event_seq(&events),
                    None,
                    token_estimate,
                    transcript,
                ))
            }
        }
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
            let transcript = build_raw_projected_transcript_from_filtered_events(
                events
                    .iter()
                    .filter(|event| event.event_seq <= head.head_event_seq),
            );
            let token_estimate = entries_token_estimate(&transcript.entries);
            return Ok(envelope_from_transcript(
                session_id,
                branch_id,
                head.projection_version,
                head.head_event_seq,
                None,
                token_estimate,
                transcript,
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
        let token_estimate = token_estimate(&segments, &entries, &suffix.entries);
        entries.extend(suffix.entries);

        Ok(envelope_from_entries(
            session_id,
            branch_id,
            head.projection_version,
            head.head_event_seq,
            Some(active_compaction_id.to_string()),
            token_estimate,
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
    let mut entries = segments
        .iter()
        .filter(|segment| segment.segment_type == "summary_chunk")
        .filter_map(|segment| summary_entry_from_segment(compaction, segment))
        .collect::<Vec<_>>();
    if entries.is_empty()
        && let Some(entry) = summary_entry_from_compaction(compaction)
    {
        entries.push(entry);
    }
    entries
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
            messages_compacted: checkpoint_messages_compacted(compaction, Some(segment)),
            compacted_until_ref: checkpoint_compacted_until_ref(compaction, Some(segment)),
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
            messages_compacted: checkpoint_messages_compacted(compaction, None),
            compacted_until_ref: checkpoint_compacted_until_ref(compaction, None),
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

fn checkpoint_messages_compacted(
    compaction: &SessionCompactionRecord,
    segment: Option<&SessionProjectionSegmentRecord>,
) -> u32 {
    segment
        .and_then(|segment| read_u32(&segment.source_refs_json, "messages_compacted"))
        .or_else(|| read_u32(&compaction.token_stats_json, "messages_compacted"))
        .unwrap_or_default()
}

fn checkpoint_compacted_until_ref(
    compaction: &SessionCompactionRecord,
    segment: Option<&SessionProjectionSegmentRecord>,
) -> Option<MessageRef> {
    segment
        .and_then(|segment| read_message_ref(&segment.source_refs_json, "compacted_until_ref"))
        .or_else(|| {
            read_message_ref(
                &compaction.replacement_projection_json,
                "compacted_until_ref",
            )
        })
}

fn read_u32(value: &serde_json::Value, key: &str) -> Option<u32> {
    value
        .get(key)
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
}

fn read_message_ref(value: &serde_json::Value, key: &str) -> Option<MessageRef> {
    serde_json::from_value(value.get(key)?.clone()).ok()
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

fn token_estimate(
    segments: &[SessionProjectionSegmentRecord],
    projection_entries: &[ProjectedEntry],
    suffix_entries: &[ProjectedEntry],
) -> Option<u64> {
    let mut total = 0_u64;
    let mut has_estimate = false;
    for segment in segments {
        if let Some(value) = segment.token_estimate {
            has_estimate = true;
            total = total.saturating_add(value);
        }
    }
    if !has_estimate && !projection_entries.is_empty() {
        has_estimate = true;
        total = total.saturating_add(entries_token_total(projection_entries));
    }
    for entry in suffix_entries {
        has_estimate = true;
        total = total.saturating_add(estimate_message_tokens(&entry.message));
    }
    has_estimate.then_some(total)
}

fn entries_token_estimate(entries: &[ProjectedEntry]) -> Option<u64> {
    if entries.is_empty() {
        return None;
    }
    Some(entries_token_total(entries))
}

fn entries_token_total(entries: &[ProjectedEntry]) -> u64 {
    entries.iter().fold(0_u64, |total, entry| {
        total.saturating_add(estimate_message_tokens(&entry.message))
    })
}

fn estimate_message_tokens(message: &AgentMessage) -> u64 {
    match message {
        AgentMessage::User { content, .. } => estimate_content_tokens(content),
        AgentMessage::Assistant {
            content,
            tool_calls,
            error_message,
            ..
        } => {
            let tool_chars = tool_calls.iter().fold(0_usize, |acc, call| {
                acc.saturating_add(call.id.chars().count())
                    .saturating_add(
                        call.call_id
                            .as_deref()
                            .map(|value| value.chars().count())
                            .unwrap_or(0),
                    )
                    .saturating_add(call.name.chars().count())
                    .saturating_add(json_chars(&call.arguments))
            });
            estimate_content_tokens(content)
                .saturating_add(chars_to_tokens(tool_chars))
                .saturating_add(error_message.as_deref().map(text_tokens).unwrap_or(0))
        }
        AgentMessage::ToolResult {
            tool_call_id,
            call_id,
            tool_name,
            content,
            details,
            ..
        } => {
            let metadata_chars = tool_call_id
                .chars()
                .count()
                .saturating_add(
                    call_id
                        .as_deref()
                        .map(|value| value.chars().count())
                        .unwrap_or(0),
                )
                .saturating_add(
                    tool_name
                        .as_deref()
                        .map(|value| value.chars().count())
                        .unwrap_or(0),
                )
                .saturating_add(details.as_ref().map(json_chars).unwrap_or(0));
            estimate_content_tokens(content).saturating_add(chars_to_tokens(metadata_chars))
        }
        AgentMessage::CompactionSummary { summary, .. } => text_tokens(summary),
    }
}

fn estimate_content_tokens(content: &[ContentPart]) -> u64 {
    let chars = content.iter().fold(0_usize, |acc, part| {
        acc.saturating_add(match part {
            ContentPart::Text { text } | ContentPart::Reasoning { text, .. } => {
                text.chars().count()
            }
            ContentPart::Image { mime_type, .. } => mime_type.chars().count().saturating_add(1024),
        })
    });
    chars_to_tokens(chars).saturating_add(4)
}

fn text_tokens(value: &str) -> u64 {
    chars_to_tokens(value.chars().count()).saturating_add(4)
}

fn json_chars(value: &serde_json::Value) -> usize {
    serde_json::to_string(value)
        .map(|value| value.chars().count())
        .unwrap_or_default()
}

fn chars_to_tokens(chars: usize) -> u64 {
    u64::try_from(chars).unwrap_or(u64::MAX).saturating_add(3) / 4
}

fn latest_event_seq(events: &[PersistedSessionEvent]) -> u64 {
    events
        .iter()
        .map(|event| event.event_seq)
        .max()
        .unwrap_or_default()
}
