use std::io;

use agentdash_agent_types::{
    AgentContextEnvelope, AgentInputMessage, AgentMessage, ContentPart, ProjectedEntry,
    ProjectedTranscript, ProjectionKind,
};
use agentdash_spi::{
    SESSION_PROJECTION_KIND_MODEL_CONTEXT, SessionCompactionRecord, SessionCompactionStatus,
    SessionProjectionHeadRecord, SessionProjectionSegmentRecord,
};

use super::compaction_checkpoint::{
    CompactionCheckpointError, projection_entries_from_checkpoint_records,
    suffix_start_event_seq_from_compaction,
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

        let mut entries = projection_entries_from_checkpoint_records(&compaction, &segments)
            .map_err(checkpoint_error_to_io)?;
        let suffix_start_event_seq =
            suffix_start_event_seq_from_compaction(&compaction, head.head_event_seq)
                .map_err(checkpoint_error_to_io)?;
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

fn checkpoint_error_to_io(error: CompactionCheckpointError) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
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
