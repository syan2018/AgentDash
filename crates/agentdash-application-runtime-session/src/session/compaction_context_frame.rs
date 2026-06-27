use agentdash_spi::hooks::{ContextFrame, ContextFrameSection, RuntimeEventSource};

use crate::session::context_frame::{self, ContextFramePayload};

#[derive(Debug, Clone)]
struct CompactionSummaryFrame {
    summary: String,
    tokens_before: u64,
    messages_compacted: u32,
    compaction_id: Option<String>,
    projection_version: Option<u64>,
    strategy: Option<String>,
    trigger: Option<String>,
    phase: Option<String>,
    source_start_event_seq: Option<u64>,
    source_end_event_seq: Option<u64>,
    first_kept_event_seq: Option<u64>,
    compacted_until_ref: Option<serde_json::Value>,
    timestamp_ms: Option<u64>,
}

impl CompactionSummaryFrame {
    fn from_event_value(value: &serde_json::Value) -> Option<Self> {
        let summary = value.get("summary")?.as_str()?.trim().to_string();
        if summary.is_empty() {
            return None;
        }
        let messages_compacted = value
            .get("messages_compacted")
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
            .unwrap_or_default();
        Some(Self {
            summary,
            tokens_before: value
                .get("tokens_before")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or_default(),
            messages_compacted,
            compaction_id: string_field(value, "compaction_id"),
            projection_version: value
                .get("projection_version")
                .and_then(serde_json::Value::as_u64),
            strategy: string_field(value, "strategy"),
            trigger: string_field(value, "trigger"),
            phase: string_field(value, "phase"),
            source_start_event_seq: value
                .get("source_start_event_seq")
                .and_then(serde_json::Value::as_u64),
            source_end_event_seq: value
                .get("source_end_event_seq")
                .and_then(serde_json::Value::as_u64),
            first_kept_event_seq: value
                .get("first_kept_event_seq")
                .and_then(serde_json::Value::as_u64),
            compacted_until_ref: value.get("compacted_until_ref").cloned(),
            timestamp_ms: value
                .get("timestamp_ms")
                .and_then(serde_json::Value::as_u64),
        })
    }
}

impl ContextFramePayload for CompactionSummaryFrame {
    fn id(&self, created_at_ms: i64) -> String {
        format!("compaction-summary-{created_at_ms}")
    }

    fn kind(&self) -> &'static str {
        "compaction_summary"
    }

    fn source(&self) -> RuntimeEventSource {
        RuntimeEventSource::RuntimeContextUpdate
    }

    fn delivery_status(&self) -> String {
        "applied_to_compacted_context".to_string()
    }

    fn delivery_channel(&self) -> &'static str {
        "continuation"
    }

    fn message_role(&self) -> &'static str {
        "system"
    }

    fn sections(&self) -> Vec<ContextFrameSection> {
        vec![ContextFrameSection::CompactionSummary {
            title: "Compaction Summary".to_string(),
            summary: self.summary.clone(),
            tokens_before: self.tokens_before,
            messages_compacted: self.messages_compacted,
            compaction_id: self.compaction_id.clone(),
            projection_version: self.projection_version,
            strategy: self.strategy.clone(),
            trigger: self.trigger.clone(),
            phase: self.phase.clone(),
            source_start_event_seq: self.source_start_event_seq,
            source_end_event_seq: self.source_end_event_seq,
            first_kept_event_seq: self.first_kept_event_seq,
            compacted_until_ref: self.compacted_until_ref.clone(),
            timestamp_ms: self.timestamp_ms,
        }]
    }

    fn rendered_text(&self) -> String {
        let mut lines = vec![
            "## Compaction Summary".to_string(),
            format!("messages_compacted: {}", self.messages_compacted),
            format!("tokens_before: {}", self.tokens_before),
        ];
        if let Some(timestamp_ms) = self.timestamp_ms {
            lines.push(format!("timestamp_ms: {timestamp_ms}"));
        }
        if let Some(compaction_id) = self.compaction_id.as_ref() {
            lines.push(format!("compaction_id: {compaction_id}"));
        }
        if let Some(projection_version) = self.projection_version {
            lines.push(format!("projection_version: {projection_version}"));
        }
        if let Some(strategy) = self.strategy.as_ref() {
            lines.push(format!("strategy: {strategy}"));
        }
        if let Some(trigger) = self.trigger.as_ref() {
            lines.push(format!("trigger: {trigger}"));
        }
        if let Some(phase) = self.phase.as_ref() {
            lines.push(format!("phase: {phase}"));
        }
        if let Some(source_start_event_seq) = self.source_start_event_seq {
            lines.push(format!("source_start_event_seq: {source_start_event_seq}"));
        }
        if let Some(source_end_event_seq) = self.source_end_event_seq {
            lines.push(format!("source_end_event_seq: {source_end_event_seq}"));
        }
        if let Some(first_kept_event_seq) = self.first_kept_event_seq {
            lines.push(format!("first_kept_event_seq: {first_kept_event_seq}"));
        }
        if let Some(compacted_until_ref) = self.compacted_until_ref.as_ref() {
            lines.push(format!("compacted_until_ref: {}", compacted_until_ref));
        }
        lines.push(String::new());
        lines.push(self.summary.clone());
        lines.join("\n")
    }
}

pub(crate) fn build_compaction_context_frame(value: &serde_json::Value) -> Option<ContextFrame> {
    let metadata = CompactionSummaryFrame::from_event_value(value)?;
    Some(context_frame::build_context_frame(&metadata))
}

fn string_field(value: &serde_json::Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToString::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compaction_frame_preserves_summary_and_boundary() {
        let frame = build_compaction_context_frame(&serde_json::json!({
            "summary": "压缩后的历史摘要",
            "tokens_before": 48000,
            "messages_compacted": 12,
            "compacted_until_ref": { "turn_id": "turn-1", "entry_index": 3 },
            "timestamp_ms": 1710000000000_u64,
        }))
        .expect("compaction frame");

        assert_eq!(frame.kind, "compaction_summary");
        assert_eq!(frame.delivery_channel, "continuation");
        assert!(frame.rendered_text.contains("压缩后的历史摘要"));
        assert!(matches!(
            frame.sections.first(),
            Some(ContextFrameSection::CompactionSummary {
                messages_compacted: 12,
                ..
            })
        ));
    }
}
