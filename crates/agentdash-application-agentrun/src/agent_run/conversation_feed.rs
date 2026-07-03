//! AgentRun 会话 feed 读模型。
//!
//! `conversation/feed` 只表达当前 RuntimeSession 事件库外的 inherited projection seed。
//! 当前 RuntimeSession 的 durable history、ContextFrame、fork marker、工具事件和 live stream
//! 由 AgentRun scoped runtime events/stream 端点提供；调用方应从 `runtime_replay_start_seq`
//! 开始回放 runtime events。

use agentdash_agent_types::{AgentContextEnvelope, AgentMessage, ContentPart, ProjectedEntry};
use agentdash_domain::workflow::{LifecycleAgent, LifecycleRun};
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct AgentConversationFeedInput {
    pub run: LifecycleRun,
    pub agent: LifecycleAgent,
    pub runtime_session_id: String,
    pub envelope: AgentContextEnvelope,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentConversationFeedModel {
    pub run_id: String,
    pub agent_id: String,
    pub runtime_session_id: String,
    pub projection_kind: String,
    pub projection_version: u64,
    pub head_event_seq: u64,
    pub runtime_replay_start_seq: u64,
    pub active_compaction_id: Option<String>,
    pub messages: Vec<AgentConversationFeedMessageModel>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentConversationMessageRefModel {
    pub turn_id: String,
    pub entry_index: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentConversationSourceRangeModel {
    pub start_event_seq: u64,
    pub end_event_seq: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentConversationMessageRoleModel {
    User,
    Assistant,
    ToolResult,
    CompactionSummary,
}

impl AgentConversationMessageRoleModel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::ToolResult => "tool_result",
            Self::CompactionSummary => "compaction_summary",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentConversationFeedMessageModel {
    pub message_ref: AgentConversationMessageRefModel,
    pub role: AgentConversationMessageRoleModel,
    pub text: String,
    pub content_parts: Vec<AgentConversationContentPartModel>,
    pub tool_calls: Vec<AgentConversationToolCallModel>,
    pub tool_result: Option<AgentConversationToolResultModel>,
    pub origin: String,
    pub synthetic: bool,
    pub projection_kind: String,
    pub source_event_seq: Option<u64>,
    pub source_range: Option<AgentConversationSourceRangeModel>,
    pub projection_segment_id: Option<String>,
    pub timestamp_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AgentConversationContentPartModel {
    Text {
        text: String,
    },
    Image {
        mime_type: String,
        data: String,
    },
    Reasoning {
        text: String,
        id: Option<String>,
        signature: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentConversationToolCallModel {
    pub id: String,
    pub call_id: Option<String>,
    pub name: String,
    pub arguments: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentConversationToolResultModel {
    pub tool_call_id: String,
    pub call_id: Option<String>,
    pub tool_name: Option<String>,
    pub details: Option<Value>,
    pub is_error: bool,
}

pub struct AgentConversationFeedProjector;

impl AgentConversationFeedProjector {
    pub fn derive(input: AgentConversationFeedInput) -> AgentConversationFeedModel {
        let AgentConversationFeedInput {
            run,
            agent,
            runtime_session_id,
            envelope,
        } = input;
        let projection_kind = envelope.projection_kind.as_str().to_string();
        let projection_version = envelope.projection_version;
        let head_event_seq = envelope.head_event_seq;
        let active_compaction_id = envelope.active_compaction_id.clone();
        let messages = envelope
            .into_projected_transcript()
            .entries
            .into_iter()
            .filter(is_fork_inherited_projection_entry)
            .filter_map(conversation_feed_message)
            .collect::<Vec<_>>();

        AgentConversationFeedModel {
            run_id: run.id.to_string(),
            agent_id: agent.id.to_string(),
            runtime_session_id,
            projection_kind,
            projection_version,
            head_event_seq,
            runtime_replay_start_seq: 0,
            active_compaction_id,
            messages,
        }
    }
}

fn is_fork_inherited_projection_entry(entry: &ProjectedEntry) -> bool {
    entry.synthetic
        && entry.origin.as_str() == "projection"
        && entry
            .provenance
            .get("source")
            .and_then(Value::as_str)
            .is_some_and(|value| value == "projection_segment")
        && entry
            .provenance
            .get("strategy")
            .and_then(Value::as_str)
            .is_some_and(|value| value == "fork_initial_projection")
        && entry
            .provenance
            .get("trigger")
            .and_then(Value::as_str)
            .is_some_and(|value| value == "session_fork")
}

fn conversation_feed_message(entry: ProjectedEntry) -> Option<AgentConversationFeedMessageModel> {
    let role = conversation_message_role(&entry.message);
    let text = conversation_message_text(&entry.message);
    let content_parts = conversation_content_parts(&entry.message);
    let tool_calls = conversation_tool_calls(&entry.message);
    let tool_result = conversation_tool_result(&entry.message);
    if text.trim().is_empty()
        && content_parts.is_empty()
        && tool_calls.is_empty()
        && tool_result.is_none()
    {
        return None;
    }
    let timestamp_ms = conversation_message_timestamp_ms(&entry.message);
    Some(AgentConversationFeedMessageModel {
        message_ref: AgentConversationMessageRefModel {
            turn_id: entry.message_ref.turn_id,
            entry_index: entry.message_ref.entry_index,
        },
        role,
        text,
        content_parts,
        tool_calls,
        tool_result,
        origin: entry.origin.as_str().to_string(),
        synthetic: entry.synthetic,
        projection_kind: entry.projection_kind.as_str().to_string(),
        source_event_seq: entry.source_event_seq,
        source_range: entry
            .source_range
            .map(|range| AgentConversationSourceRangeModel {
                start_event_seq: range.start_event_seq,
                end_event_seq: range.end_event_seq,
            }),
        projection_segment_id: entry.projection_segment_id,
        timestamp_ms,
    })
}

fn conversation_content_parts(message: &AgentMessage) -> Vec<AgentConversationContentPartModel> {
    let content = match message {
        AgentMessage::User { content, .. }
        | AgentMessage::Assistant { content, .. }
        | AgentMessage::ToolResult { content, .. } => content,
        AgentMessage::CompactionSummary { .. } => return Vec::new(),
    };
    content
        .iter()
        .map(|part| match part {
            ContentPart::Text { text } => {
                AgentConversationContentPartModel::Text { text: text.clone() }
            }
            ContentPart::Image { mime_type, data } => AgentConversationContentPartModel::Image {
                mime_type: mime_type.clone(),
                data: data.clone(),
            },
            ContentPart::Reasoning {
                text,
                id,
                signature,
            } => AgentConversationContentPartModel::Reasoning {
                text: text.clone(),
                id: id.clone(),
                signature: signature.clone(),
            },
        })
        .collect()
}

fn conversation_tool_calls(message: &AgentMessage) -> Vec<AgentConversationToolCallModel> {
    let AgentMessage::Assistant { tool_calls, .. } = message else {
        return Vec::new();
    };
    tool_calls
        .iter()
        .map(|tool_call| AgentConversationToolCallModel {
            id: tool_call.id.clone(),
            call_id: tool_call.call_id.clone(),
            name: tool_call.name.clone(),
            arguments: tool_call.arguments.clone(),
        })
        .collect()
}

fn conversation_tool_result(message: &AgentMessage) -> Option<AgentConversationToolResultModel> {
    let AgentMessage::ToolResult {
        tool_call_id,
        call_id,
        tool_name,
        details,
        is_error,
        ..
    } = message
    else {
        return None;
    };
    Some(AgentConversationToolResultModel {
        tool_call_id: tool_call_id.clone(),
        call_id: call_id.clone(),
        tool_name: tool_name.clone(),
        details: details.clone(),
        is_error: *is_error,
    })
}

fn conversation_message_role(message: &AgentMessage) -> AgentConversationMessageRoleModel {
    match message {
        AgentMessage::User { .. } => AgentConversationMessageRoleModel::User,
        AgentMessage::Assistant { .. } => AgentConversationMessageRoleModel::Assistant,
        AgentMessage::ToolResult { .. } => AgentConversationMessageRoleModel::ToolResult,
        AgentMessage::CompactionSummary { .. } => {
            AgentConversationMessageRoleModel::CompactionSummary
        }
    }
}

fn conversation_message_text(message: &AgentMessage) -> String {
    match message {
        AgentMessage::User { content, .. }
        | AgentMessage::Assistant { content, .. }
        | AgentMessage::ToolResult { content, .. } => content
            .iter()
            .filter_map(content_part_text)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        AgentMessage::CompactionSummary { summary, .. } => summary.trim().to_string(),
    }
}

fn content_part_text(part: &ContentPart) -> Option<&str> {
    match part {
        ContentPart::Text { text } | ContentPart::Reasoning { text, .. } => Some(text.as_str()),
        ContentPart::Image { .. } => None,
    }
}

fn conversation_message_timestamp_ms(message: &AgentMessage) -> Option<u64> {
    match message {
        AgentMessage::User { timestamp, .. }
        | AgentMessage::Assistant { timestamp, .. }
        | AgentMessage::ToolResult { timestamp, .. }
        | AgentMessage::CompactionSummary { timestamp, .. } => *timestamp,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_agent_types::{
        AgentInputMessage, MessageRef, ProjectionKind, ProjectionOrigin, ProjectionSourceRange,
    };
    use agentdash_domain::workflow::{AgentSource, LifecycleAgent, LifecycleRun};

    #[test]
    fn feed_keeps_only_fork_inherited_projection_messages() {
        let run = LifecycleRun::new_plain(uuid::Uuid::new_v4());
        let agent = LifecycleAgent::new_root(run.id, run.project_id, AgentSource::ProjectAgent);
        let model = AgentConversationFeedProjector::derive(AgentConversationFeedInput {
            run,
            agent,
            runtime_session_id: "sess-child".to_string(),
            envelope: AgentContextEnvelope {
                session_id: "sess-child".to_string(),
                projection_kind: ProjectionKind::ModelContext,
                projection_version: 1,
                head_event_seq: 8,
                active_compaction_id: Some("fork-initial-sess-child".to_string()),
                token_estimate: None,
                messages: vec![
                    fork_projection_message("parent-turn", 0, "parent answer"),
                    current_runtime_message("child-turn", 0, 7, "child answer"),
                    ordinary_compaction_projection_message("summary"),
                ],
            },
        });

        assert_eq!(model.messages.len(), 1);
        assert_eq!(model.messages[0].message_ref.turn_id, "parent-turn");
        assert_eq!(model.messages[0].text, "parent answer");
    }

    fn fork_projection_message(turn_id: &str, entry_index: u32, text: &str) -> AgentInputMessage {
        let mut message = projected_message(turn_id, entry_index, text);
        message.projection_segment_id = Some("fork-initial-sess-child-context".to_string());
        message.provenance = serde_json::json!({
            "source": "projection_segment",
            "segment_id": "fork-initial-sess-child-context",
            "strategy": "fork_initial_projection",
            "trigger": "session_fork",
            "phase": "session_branching",
        });
        message
    }

    fn ordinary_compaction_projection_message(text: &str) -> AgentInputMessage {
        let mut message = projected_message("_projection:summary", 0, text);
        message.projection_segment_id = Some("summary-segment".to_string());
        message.source_range = Some(ProjectionSourceRange {
            start_event_seq: 1,
            end_event_seq: 6,
        });
        message.provenance = serde_json::json!({
            "source": "projection_segment",
            "segment_id": "summary-segment",
            "strategy": "summary_prefix",
            "trigger": "auto",
        });
        message
    }

    fn projected_message(turn_id: &str, entry_index: u32, text: &str) -> AgentInputMessage {
        AgentInputMessage {
            message_ref: MessageRef {
                turn_id: turn_id.to_string(),
                entry_index,
            },
            projection_kind: ProjectionKind::ModelContext,
            message: AgentMessage::assistant(text),
            origin: ProjectionOrigin::Projection,
            synthetic: true,
            source_event_seq: None,
            source_range: None,
            projection_segment_id: None,
            provenance: Value::Null,
        }
    }

    fn current_runtime_message(
        turn_id: &str,
        entry_index: u32,
        source_event_seq: u64,
        text: &str,
    ) -> AgentInputMessage {
        AgentInputMessage {
            message_ref: MessageRef {
                turn_id: turn_id.to_string(),
                entry_index,
            },
            projection_kind: ProjectionKind::ModelContext,
            message: AgentMessage::assistant(text),
            origin: ProjectionOrigin::Event,
            synthetic: false,
            source_event_seq: Some(source_event_seq),
            source_range: Some(ProjectionSourceRange {
                start_event_seq: source_event_seq,
                end_event_seq: source_event_seq,
            }),
            projection_segment_id: None,
            provenance: Value::Null,
        }
    }
}
