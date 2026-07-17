use agentdash_agent_protocol::{
    ContextDeliveryChannel, ContextDeliveryStatus, ContextFrame, ContextFrameKind,
    ContextFrameSection, ContextFrameSource, ContextMessageRole,
};
use serde::{Deserialize, Serialize};

use super::{ContextFrameFacts, ContextProjectionIdentity, ContextProjector};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompactionPresentationFacts {
    pub summary: String,
    pub tokens_before: u64,
    pub messages_compacted: u32,
    pub compaction_id: Option<String>,
    pub projection_version: Option<u64>,
    pub strategy: Option<String>,
    pub trigger: Option<String>,
    pub phase: Option<String>,
    pub source_start_event_seq: Option<u64>,
    pub source_end_event_seq: Option<u64>,
    pub first_kept_event_seq: Option<u64>,
    pub compacted_until_ref: Option<serde_json::Value>,
    pub timestamp_ms: Option<u64>,
}

#[must_use]
pub fn project_compaction_summary(
    identity: &ContextProjectionIdentity,
    facts: &CompactionPresentationFacts,
) -> Option<ContextFrame> {
    let summary = facts.summary.trim();
    if summary.is_empty() {
        return None;
    }
    let mut lines = vec![
        "## Compaction Summary".to_string(),
        format!("messages_compacted: {}", facts.messages_compacted),
        format!("tokens_before: {}", facts.tokens_before),
    ];
    push(&mut lines, "timestamp_ms", facts.timestamp_ms);
    push_ref(&mut lines, "compaction_id", facts.compaction_id.as_deref());
    push(&mut lines, "projection_version", facts.projection_version);
    push_ref(&mut lines, "strategy", facts.strategy.as_deref());
    push_ref(&mut lines, "trigger", facts.trigger.as_deref());
    push_ref(&mut lines, "phase", facts.phase.as_deref());
    push(
        &mut lines,
        "source_start_event_seq",
        facts.source_start_event_seq,
    );
    push(
        &mut lines,
        "source_end_event_seq",
        facts.source_end_event_seq,
    );
    push(
        &mut lines,
        "first_kept_event_seq",
        facts.first_kept_event_seq,
    );
    if let Some(reference) = facts.compacted_until_ref.as_ref() {
        lines.push(format!("compacted_until_ref: {reference}"));
    }
    lines.extend([
        String::new(),
        "以下是之前对话的压缩摘要，用于延续工作上下文。摘要中的路径、函数名等具体信息可能已过时，请在执行前验证。".to_string(),
        String::new(),
        summary.to_string(),
    ]);
    Some(
        ContextProjector::project(
            identity,
            [ContextFrameFacts {
                kind: ContextFrameKind::CompactionSummary,
                source: ContextFrameSource::RuntimeContextUpdate,
                phase_node: None,
                apply_mode: None,
                delivery_status: ContextDeliveryStatus::AppliedToCompactedContext,
                delivery_channel: ContextDeliveryChannel::Continuation,
                message_role: ContextMessageRole::System,
                rendered_text: lines.join("\n"),
                sections: vec![ContextFrameSection::CompactionSummary {
                    title: "Compaction Summary".to_string(),
                    summary: summary.to_string(),
                    tokens_before: facts.tokens_before,
                    messages_compacted: facts.messages_compacted,
                    compaction_id: facts.compaction_id.clone(),
                    projection_version: facts.projection_version,
                    strategy: facts.strategy.clone(),
                    trigger: facts.trigger.clone(),
                    phase: facts.phase.clone(),
                    source_start_event_seq: facts.source_start_event_seq,
                    source_end_event_seq: facts.source_end_event_seq,
                    first_kept_event_seq: facts.first_kept_event_seq,
                    compacted_until_ref: facts.compacted_until_ref.clone(),
                    timestamp_ms: facts.timestamp_ms,
                }],
            }],
        )
        .bootstrap_frames
        .remove(0),
    )
}

fn push<T: std::fmt::Display>(lines: &mut Vec<String>, key: &str, value: Option<T>) {
    if let Some(value) = value {
        lines.push(format!("{key}: {value}"));
    }
}

fn push_ref(lines: &mut Vec<String>, key: &str, value: Option<&str>) {
    if let Some(value) = value {
        lines.push(format!("{key}: {value}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_compaction_facts_preserve_main_payload_without_placeholders() {
        let frame = project_compaction_summary(
            &ContextProjectionIdentity {
                operation_id: "compact-1".to_string(),
                source_frame_id: "checkpoint-1".to_string(),
                source_frame_revision: 2,
                recorded_at_ms: 20,
            },
            &CompactionPresentationFacts {
                summary: "summary".to_string(),
                tokens_before: 48_000,
                messages_compacted: 12,
                compaction_id: Some("compact-1".to_string()),
                projection_version: Some(2),
                strategy: Some("summary_prefix".to_string()),
                trigger: Some("automatic".to_string()),
                phase: Some("pre_provider".to_string()),
                source_start_event_seq: Some(1),
                source_end_event_seq: Some(8),
                first_kept_event_seq: Some(9),
                compacted_until_ref: Some(serde_json::json!({"turn_id":"turn-1","entry_index":3})),
                timestamp_ms: Some(1_710_000_000_000),
            },
        )
        .expect("compaction frame");
        assert_eq!(
            frame.delivery_status,
            ContextDeliveryStatus::AppliedToCompactedContext
        );
        assert_eq!(frame.delivery_channel, ContextDeliveryChannel::Continuation);
        assert_eq!(frame.message_role, ContextMessageRole::System);
        assert!(frame.rendered_text.contains("tokens_before: 48000"));
        assert!(frame.rendered_text.ends_with("summary"));
    }

    #[test]
    fn missing_summary_does_not_fabricate_a_compaction_frame() {
        let facts = CompactionPresentationFacts {
            summary: String::new(),
            tokens_before: 0,
            messages_compacted: 0,
            compaction_id: None,
            projection_version: None,
            strategy: None,
            trigger: None,
            phase: None,
            source_start_event_seq: None,
            source_end_event_seq: None,
            first_kept_event_seq: None,
            compacted_until_ref: None,
            timestamp_ms: None,
        };
        assert!(
            project_compaction_summary(
                &ContextProjectionIdentity {
                    operation_id: "compact".to_string(),
                    source_frame_id: "checkpoint".to_string(),
                    source_frame_revision: 1,
                    recorded_at_ms: 0,
                },
                &facts,
            )
            .is_none()
        );
    }
}
