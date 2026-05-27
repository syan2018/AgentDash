use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use agentdash_agent_protocol::BackboneEnvelope;
use agentdash_agent_types::{
    AgentContextEnvelope, AgentInputMessage, AgentMessage, ContentPart, MessageRef,
    ProjectionSourceRange, estimate_content_tokens, estimate_message_tokens,
};
use agentdash_spi::session_persistence::{
    PersistedSessionEvent, SessionLineageRecord, SessionLineageRelationKind, SessionLineageStatus,
};

const PROJECTION_PREVIEW_MAX_CHARS: usize = 360;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct SessionEventResponse {
    pub session_id: String,
    #[ts(type = "number")]
    pub event_seq: u64,
    #[ts(type = "number")]
    pub occurred_at_ms: i64,
    #[ts(type = "number")]
    pub committed_at_ms: i64,
    pub session_update_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub turn_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub entry_index: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub tool_call_id: Option<String>,
    #[ts(type = "BackboneEnvelope")]
    pub notification: BackboneEnvelope,
}

impl From<PersistedSessionEvent> for SessionEventResponse {
    fn from(event: PersistedSessionEvent) -> Self {
        Self {
            session_id: event.session_id,
            event_seq: event.event_seq,
            occurred_at_ms: event.occurred_at_ms,
            committed_at_ms: event.committed_at_ms,
            session_update_type: event.session_update_type,
            turn_id: event.turn_id,
            entry_index: event.entry_index,
            tool_call_id: event.tool_call_id,
            notification: event.notification,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct SessionEventsPageResponse {
    #[ts(type = "number")]
    pub snapshot_seq: u64,
    pub events: Vec<SessionEventResponse>,
    pub has_more: bool,
    #[ts(type = "number")]
    pub next_after_seq: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionNdjsonEnvelope {
    Connected {
        #[ts(type = "number")]
        last_event_id: u64,
    },
    Event {
        #[serde(flatten)]
        event: Box<SessionEventResponse>,
    },
    Heartbeat {
        #[ts(type = "number")]
        timestamp: i64,
    },
}

impl SessionNdjsonEnvelope {
    pub fn connected(last_event_id: u64) -> Self {
        Self::Connected { last_event_id }
    }

    pub fn event(event: PersistedSessionEvent) -> Self {
        Self::Event {
            event: Box::new(event.into()),
        }
    }

    pub fn heartbeat_now() -> Self {
        Self::Heartbeat {
            timestamp: Utc::now().timestamp_millis(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct SessionProjectionSourceRangeResponse {
    #[ts(type = "number")]
    pub start_event_seq: u64,
    #[ts(type = "number")]
    pub end_event_seq: u64,
}

impl From<ProjectionSourceRange> for SessionProjectionSourceRangeResponse {
    fn from(range: ProjectionSourceRange) -> Self {
        Self {
            start_event_seq: range.start_event_seq,
            end_event_seq: range.end_event_seq,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct SessionProjectionMessageRefResponse {
    pub turn_id: String,
    pub entry_index: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct SessionProjectionSegmentProvenanceResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub compaction_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub projection_version: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub segment_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub strategy: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub trigger: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub phase: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct SessionProjectionSegmentViewResponse {
    pub id: String,
    #[ts(type = "number")]
    pub sort_order: u32,
    pub segment_type: String,
    pub role: String,
    pub origin: String,
    pub synthetic: bool,
    pub projection_kind: String,
    pub message_ref: SessionProjectionMessageRefResponse,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub source_event_seq: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub source_range: Option<SessionProjectionSourceRangeResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub projection_segment_id: Option<String>,
    pub preview: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub token_estimate: Option<u64>,
    #[serde(default, skip_serializing_if = "is_zero")]
    #[ts(type = "number")]
    pub attachment_tokens: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachment_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_names: Vec<String>,
    pub provenance: SessionProjectionSegmentProvenanceResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct SessionContextUsageCategoryResponse {
    pub kind: String,
    pub label: String,
    #[ts(type = "number")]
    pub token_estimate: u64,
    pub source: String,
    pub deferred: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct SessionMessageContextBreakdownResponse {
    #[ts(type = "number")]
    pub user_message_tokens: u64,
    #[ts(type = "number")]
    pub assistant_message_tokens: u64,
    #[ts(type = "number")]
    pub tool_call_tokens: u64,
    #[ts(type = "number")]
    pub tool_result_tokens: u64,
    #[ts(type = "number")]
    pub attachment_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct SessionToolContextContributionResponse {
    pub name: String,
    #[ts(type = "number")]
    pub call_tokens: u64,
    #[ts(type = "number")]
    pub result_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct SessionAttachmentContextContributionResponse {
    pub name: String,
    #[ts(type = "number")]
    pub tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct SessionContextUsageAnalysisResponse {
    pub categories: Vec<SessionContextUsageCategoryResponse>,
    pub messages: SessionMessageContextBreakdownResponse,
    pub top_tools: Vec<SessionToolContextContributionResponse>,
    pub top_attachments: Vec<SessionAttachmentContextContributionResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct SessionProjectionViewResponse {
    pub session_id: String,
    pub projection_kind: String,
    #[ts(type = "number")]
    pub projection_version: u64,
    #[ts(type = "number")]
    pub head_event_seq: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub active_compaction_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub token_estimate: Option<u64>,
    #[ts(type = "number")]
    pub message_count: u64,
    pub segments: Vec<SessionProjectionSegmentViewResponse>,
    pub context_usage: SessionContextUsageAnalysisResponse,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionLineageRelationKindDto {
    Fork,
    Companion,
    SpawnedAgent,
    RollbackBranch,
}

impl From<SessionLineageRelationKind> for SessionLineageRelationKindDto {
    fn from(value: SessionLineageRelationKind) -> Self {
        match value {
            SessionLineageRelationKind::Fork => Self::Fork,
            SessionLineageRelationKind::Companion => Self::Companion,
            SessionLineageRelationKind::SpawnedAgent => Self::SpawnedAgent,
            SessionLineageRelationKind::RollbackBranch => Self::RollbackBranch,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionLineageStatusDto {
    Open,
    Closed,
    Archived,
}

impl From<SessionLineageStatus> for SessionLineageStatusDto {
    fn from(value: SessionLineageStatus) -> Self {
        match value {
            SessionLineageStatus::Open => Self::Open,
            SessionLineageStatus::Closed => Self::Closed,
            SessionLineageStatus::Archived => Self::Archived,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct SessionMessageRefDto {
    pub turn_id: String,
    pub entry_index: u32,
}

impl From<MessageRef> for SessionMessageRefDto {
    fn from(value: MessageRef) -> Self {
        Self {
            turn_id: value.turn_id,
            entry_index: value.entry_index,
        }
    }
}

impl From<SessionMessageRefDto> for MessageRef {
    fn from(value: SessionMessageRefDto) -> Self {
        Self {
            turn_id: value.turn_id,
            entry_index: value.entry_index,
        }
    }
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct CreateSessionForkRequest {
    #[serde(default)]
    #[ts(optional)]
    pub title: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub fork_point_ref: Option<SessionMessageRefDto>,
    #[serde(default)]
    #[ts(optional)]
    pub fork_point_compaction_id: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub metadata_json: Option<Value>,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct RollbackSessionProjectionRequest {
    #[ts(type = "number")]
    pub target_event_seq: u64,
    #[serde(default)]
    #[ts(optional)]
    pub active_compaction_id: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct SessionLineageRecordResponse {
    pub child_session_id: String,
    pub parent_session_id: String,
    pub relation_kind: SessionLineageRelationKindDto,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub fork_point_event_seq: Option<u64>,
    pub fork_point_ref_json: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub fork_point_compaction_id: Option<String>,
    pub status: SessionLineageStatusDto,
    #[ts(type = "number")]
    pub created_at_ms: i64,
    #[ts(type = "number")]
    pub updated_at_ms: i64,
    pub metadata_json: Value,
}

impl From<SessionLineageRecord> for SessionLineageRecordResponse {
    fn from(value: SessionLineageRecord) -> Self {
        Self {
            child_session_id: value.child_session_id,
            parent_session_id: value.parent_session_id,
            relation_kind: value.relation_kind.into(),
            fork_point_event_seq: value.fork_point_event_seq,
            fork_point_ref_json: value.fork_point_ref_json,
            fork_point_compaction_id: value.fork_point_compaction_id,
            status: value.status.into(),
            created_at_ms: value.created_at_ms,
            updated_at_ms: value.updated_at_ms,
            metadata_json: value.metadata_json,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct SessionForkChildSessionResponse {
    pub id: String,
    pub title: String,
    #[ts(type = "number")]
    pub created_at: i64,
    #[ts(type = "number")]
    pub updated_at: i64,
    #[ts(type = "number")]
    pub last_event_seq: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct SessionForkResponse {
    pub parent_session_id: String,
    pub child_session: SessionForkChildSessionResponse,
    pub lineage: SessionLineageRecordResponse,
    pub child_initial_compaction_id: String,
    #[ts(type = "number")]
    pub projection_version: u64,
    #[ts(type = "number")]
    pub head_event_seq: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct SessionLineageViewResponse {
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub lineage: Option<SessionLineageRecordResponse>,
    pub ancestors: Vec<SessionLineageRecordResponse>,
    pub children: Vec<SessionLineageRecordResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct SessionProjectionRollbackResponse {
    pub session_id: String,
    pub event: SessionEventResponse,
    #[ts(type = "number")]
    pub head_event_seq: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub active_compaction_id: Option<String>,
    #[ts(type = "number")]
    pub projection_version: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub updated_by_event_seq: Option<u64>,
}

impl From<AgentContextEnvelope> for SessionProjectionViewResponse {
    fn from(envelope: AgentContextEnvelope) -> Self {
        let message_count = u64::try_from(envelope.messages.len()).unwrap_or(u64::MAX);
        let segments: Vec<_> = envelope
            .messages
            .into_iter()
            .enumerate()
            .map(|(index, message)| projection_segment_from_message(index, message))
            .collect();
        let context_usage = context_usage_analysis(&segments);
        Self {
            session_id: envelope.session_id,
            projection_kind: envelope.projection_kind.as_str().to_string(),
            projection_version: envelope.projection_version,
            head_event_seq: envelope.head_event_seq,
            active_compaction_id: envelope.active_compaction_id,
            token_estimate: envelope.token_estimate,
            message_count,
            segments,
            context_usage,
        }
    }
}

fn projection_segment_from_message(
    index: usize,
    message: AgentInputMessage,
) -> SessionProjectionSegmentViewResponse {
    let provenance = projection_provenance(&message.provenance);
    let segment_type =
        provenance
            .segment_type
            .clone()
            .unwrap_or_else(|| match message.origin.as_str() {
                "projection" => "projection_segment".to_string(),
                _ => "original_event".to_string(),
            });
    let id = message
        .projection_segment_id
        .clone()
        .unwrap_or_else(|| format!("{}:{}", segment_type, index));
    let sort_order = u32::try_from(index).unwrap_or(u32::MAX);
    let role = message_role(&message.message).to_string();
    let preview = message_preview(&message.message);
    let token_estimate = Some(estimate_message_tokens(&message.message));
    let tool_names = message_tool_names(&message.message);
    let attachment_tokens = message_attachment_tokens(&message.message);
    let attachment_names = message_attachment_names(&message.message);
    SessionProjectionSegmentViewResponse {
        id,
        sort_order,
        segment_type,
        role,
        origin: message.origin.as_str().to_string(),
        synthetic: message.synthetic,
        projection_kind: message.projection_kind.as_str().to_string(),
        message_ref: SessionProjectionMessageRefResponse {
            turn_id: message.message_ref.turn_id,
            entry_index: message.message_ref.entry_index,
        },
        source_event_seq: message.source_event_seq,
        source_range: message.source_range.map(Into::into),
        projection_segment_id: message.projection_segment_id,
        preview,
        token_estimate,
        attachment_tokens,
        attachment_names,
        tool_names,
        provenance,
    }
}

fn projection_provenance(value: &serde_json::Value) -> SessionProjectionSegmentProvenanceResponse {
    SessionProjectionSegmentProvenanceResponse {
        compaction_id: read_string(value, "compaction_id"),
        projection_version: read_u64(value, "projection_version"),
        segment_type: read_string(value, "segment_type"),
        strategy: read_string(value, "strategy"),
        trigger: read_string(value, "trigger"),
        phase: read_string(value, "phase"),
    }
}

fn read_string(value: &serde_json::Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn read_u64(value: &serde_json::Value, key: &str) -> Option<u64> {
    value.get(key).and_then(serde_json::Value::as_u64)
}

fn message_role(message: &AgentMessage) -> &'static str {
    match message {
        AgentMessage::User { .. } => "user",
        AgentMessage::Assistant { .. } => "assistant",
        AgentMessage::ToolResult { .. } => "tool_result",
        AgentMessage::CompactionSummary { .. } => "compaction_summary",
    }
}

fn message_tool_names(message: &AgentMessage) -> Vec<String> {
    match message {
        AgentMessage::Assistant { tool_calls, .. } => tool_calls
            .iter()
            .map(|call| call.name.trim())
            .filter(|name| !name.is_empty())
            .map(ToString::to_string)
            .collect(),
        AgentMessage::ToolResult { tool_name, .. } => tool_name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(|name| vec![name.to_string()])
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn message_attachment_tokens(message: &AgentMessage) -> u64 {
    let content = match message {
        AgentMessage::User { content, .. }
        | AgentMessage::Assistant { content, .. }
        | AgentMessage::ToolResult { content, .. } => content,
        AgentMessage::CompactionSummary { .. } => return 0,
    };
    content
        .iter()
        .filter(|part| matches!(part, ContentPart::Image { .. }))
        .map(|part| estimate_content_tokens(std::slice::from_ref(part)))
        .fold(0_u64, u64::saturating_add)
}

fn message_attachment_names(message: &AgentMessage) -> Vec<String> {
    let content = match message {
        AgentMessage::User { content, .. }
        | AgentMessage::Assistant { content, .. }
        | AgentMessage::ToolResult { content, .. } => content,
        AgentMessage::CompactionSummary { .. } => return Vec::new(),
    };
    content
        .iter()
        .enumerate()
        .filter_map(|(index, part)| match part {
            ContentPart::Image { mime_type, .. } => Some(format!("{mime_type} image #{index}")),
            _ => None,
        })
        .collect()
}

fn context_usage_analysis(
    segments: &[SessionProjectionSegmentViewResponse],
) -> SessionContextUsageAnalysisResponse {
    let summary_tokens = sum_segment_tokens(segments, |segment| {
        segment.role == "compaction_summary" || segment.origin == "projection"
    });
    let message_tokens = sum_segment_tokens(segments, |segment| {
        segment.role != "compaction_summary" && segment.origin != "projection"
    });
    let attachment_tokens = segments
        .iter()
        .map(|segment| segment.attachment_tokens)
        .fold(0_u64, u64::saturating_add);
    let categories = vec![
        context_category(
            "system_developer",
            "System / Developer",
            0,
            "not_loaded",
            true,
        ),
        context_category("system_tools", "System Tools", 0, "not_loaded", true),
        context_category("mcp_tools", "MCP Tools", 0, "not_loaded", true),
        context_category("agents", "Agents", 0, "not_loaded", true),
        context_category("memory", "Memory", 0, "not_loaded", true),
        context_category("skills", "Skills", 0, "not_loaded", true),
        context_category(
            "messages",
            "Messages",
            message_tokens,
            "local_estimate",
            false,
        ),
        context_category(
            "attachments",
            "Attachments",
            attachment_tokens,
            "local_estimate",
            false,
        ),
        context_category(
            "compaction_summary",
            "Compaction Summary",
            summary_tokens,
            "projected",
            false,
        ),
    ];
    SessionContextUsageAnalysisResponse {
        categories,
        messages: message_context_breakdown(segments),
        top_tools: top_tools(segments),
        top_attachments: top_attachments(segments),
    }
}

fn context_category(
    kind: &str,
    label: &str,
    token_estimate: u64,
    source: &str,
    deferred: bool,
) -> SessionContextUsageCategoryResponse {
    SessionContextUsageCategoryResponse {
        kind: kind.to_string(),
        label: label.to_string(),
        token_estimate,
        source: source.to_string(),
        deferred,
    }
}

fn sum_segment_tokens(
    segments: &[SessionProjectionSegmentViewResponse],
    predicate: impl Fn(&SessionProjectionSegmentViewResponse) -> bool,
) -> u64 {
    segments
        .iter()
        .filter(|segment| predicate(segment))
        .filter_map(|segment| segment.token_estimate)
        .fold(0_u64, u64::saturating_add)
}

fn message_context_breakdown(
    segments: &[SessionProjectionSegmentViewResponse],
) -> SessionMessageContextBreakdownResponse {
    SessionMessageContextBreakdownResponse {
        user_message_tokens: sum_segment_tokens(segments, |segment| segment.role == "user"),
        assistant_message_tokens: sum_segment_tokens(segments, |segment| {
            segment.role == "assistant"
        }),
        tool_call_tokens: sum_tool_call_tokens(segments),
        tool_result_tokens: sum_segment_tokens(segments, |segment| segment.role == "tool_result"),
        attachment_tokens: segments
            .iter()
            .map(|segment| segment.attachment_tokens)
            .fold(0_u64, u64::saturating_add),
    }
}

fn sum_tool_call_tokens(segments: &[SessionProjectionSegmentViewResponse]) -> u64 {
    segments
        .iter()
        .filter(|segment| segment.role == "assistant" && !segment.tool_names.is_empty())
        .filter_map(|segment| segment.token_estimate)
        .fold(0_u64, u64::saturating_add)
}

fn top_tools(
    segments: &[SessionProjectionSegmentViewResponse],
) -> Vec<SessionToolContextContributionResponse> {
    let mut values: Vec<SessionToolContextContributionResponse> = Vec::new();
    for segment in segments {
        let Some(tokens) = segment.token_estimate else {
            continue;
        };
        for name in &segment.tool_names {
            let Some(row) = values.iter_mut().find(|row| row.name == *name) else {
                values.push(SessionToolContextContributionResponse {
                    name: name.clone(),
                    call_tokens: if segment.role == "assistant" {
                        tokens
                    } else {
                        0
                    },
                    result_tokens: if segment.role == "tool_result" {
                        tokens
                    } else {
                        0
                    },
                });
                continue;
            };
            if segment.role == "assistant" {
                row.call_tokens = row.call_tokens.saturating_add(tokens);
            } else if segment.role == "tool_result" {
                row.result_tokens = row.result_tokens.saturating_add(tokens);
            }
        }
    }
    values.sort_by_key(|row| std::cmp::Reverse(row.call_tokens.saturating_add(row.result_tokens)));
    values.truncate(5);
    values
}

fn top_attachments(
    segments: &[SessionProjectionSegmentViewResponse],
) -> Vec<SessionAttachmentContextContributionResponse> {
    let mut values = Vec::new();
    for segment in segments {
        for name in &segment.attachment_names {
            values.push(SessionAttachmentContextContributionResponse {
                name: name.clone(),
                tokens: segment.attachment_tokens,
            });
        }
    }
    values.sort_by_key(|row| std::cmp::Reverse(row.tokens));
    values.truncate(5);
    values
}

fn is_zero(value: &u64) -> bool {
    *value == 0
}

fn message_preview(message: &AgentMessage) -> String {
    let text = message
        .first_text()
        .map(ToString::to_string)
        .or_else(|| assistant_tool_call_preview(message))
        .unwrap_or_else(|| message_role(message).to_string());
    truncate_preview(&text)
}

fn assistant_tool_call_preview(message: &AgentMessage) -> Option<String> {
    let AgentMessage::Assistant { tool_calls, .. } = message else {
        return None;
    };
    if tool_calls.is_empty() {
        return None;
    }
    let names = tool_calls
        .iter()
        .map(|tool| tool.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    Some(format!("tool_calls: {names}"))
}

fn truncate_preview(value: &str) -> String {
    let mut chars = value.chars();
    let preview = chars
        .by_ref()
        .take(PROJECTION_PREVIEW_MAX_CHARS)
        .collect::<String>();
    if chars.next().is_some() {
        format!("{preview}...")
    } else {
        preview
    }
}

#[cfg(test)]
mod projection_tests {
    use agentdash_agent_types::{
        AgentContextEnvelope, AgentInputMessage, AgentMessage, ContentPart, MessageRef,
        ProjectionKind, ProjectionOrigin, ProjectionSourceRange,
    };

    use super::SessionProjectionViewResponse;

    #[test]
    fn projection_view_marks_summary_as_synthetic_projection() {
        let envelope = AgentContextEnvelope {
            session_id: "sess-1".to_string(),
            projection_kind: ProjectionKind::ModelContext,
            projection_version: 2,
            head_event_seq: 42,
            active_compaction_id: Some("compaction-1".to_string()),
            token_estimate: Some(128),
            messages: vec![AgentInputMessage {
                message_ref: MessageRef {
                    turn_id: "_projection:summary".to_string(),
                    entry_index: 0,
                },
                projection_kind: ProjectionKind::CompactionSummary,
                message: AgentMessage::compaction_summary("压缩后的历史摘要", 48000, 12),
                origin: ProjectionOrigin::Projection,
                synthetic: true,
                source_event_seq: None,
                source_range: Some(ProjectionSourceRange {
                    start_event_seq: 1,
                    end_event_seq: 30,
                }),
                projection_segment_id: Some("segment-1".to_string()),
                provenance: serde_json::json!({
                    "compaction_id": "compaction-1",
                    "projection_version": 2,
                    "segment_type": "summary_chunk",
                    "strategy": "summary_prefix",
                    "trigger": "auto",
                    "phase": "pre_provider"
                }),
            }],
        };

        let view = SessionProjectionViewResponse::from(envelope);

        assert_eq!(view.projection_kind, "model_context");
        assert_eq!(view.projection_version, 2);
        assert_eq!(view.message_count, 1);
        assert_eq!(view.segments[0].origin, "projection");
        assert!(view.segments[0].synthetic);
        assert_eq!(view.segments[0].segment_type, "summary_chunk");
        assert!(view.segments[0].token_estimate.is_some());
        assert!(
            view.context_usage
                .categories
                .iter()
                .any(|category| category.kind == "compaction_summary")
        );
        assert_eq!(view.context_usage.messages.user_message_tokens, 0);
        assert_eq!(
            view.segments[0].provenance.compaction_id.as_deref(),
            Some("compaction-1")
        );
    }

    #[test]
    fn projection_view_reports_attachment_breakdown_from_image_parts() {
        let envelope = AgentContextEnvelope {
            session_id: "sess-1".to_string(),
            projection_kind: ProjectionKind::ModelContext,
            projection_version: 0,
            head_event_seq: 1,
            active_compaction_id: None,
            token_estimate: None,
            messages: vec![AgentInputMessage {
                message_ref: MessageRef {
                    turn_id: "turn-1".to_string(),
                    entry_index: 0,
                },
                projection_kind: ProjectionKind::ModelContext,
                message: AgentMessage::User {
                    content: vec![
                        ContentPart::text("看这张图"),
                        ContentPart::Image {
                            mime_type: "image/png".to_string(),
                            data: "AAECAw==".to_string(),
                        },
                    ],
                    timestamp: None,
                },
                origin: ProjectionOrigin::Event,
                synthetic: false,
                source_event_seq: Some(1),
                source_range: None,
                projection_segment_id: None,
                provenance: serde_json::Value::Null,
            }],
        };

        let view = SessionProjectionViewResponse::from(envelope);

        assert!(view.context_usage.messages.attachment_tokens > 0);
        assert_eq!(view.context_usage.top_attachments.len(), 1);
        assert!(
            view.context_usage.top_attachments[0]
                .name
                .contains("image/png")
        );
    }
}
