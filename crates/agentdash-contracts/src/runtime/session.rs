use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use agentdash_agent_protocol::BackboneEnvelope;
use agentdash_agent_types::{
    AgentContextEnvelope, AgentInputMessage, AgentMessage, ContentPart, MessageRef,
    ProjectionSourceRange, estimate_content_tokens, estimate_message_tokens,
};
use agentdash_spi::hooks::{
    ContextFrame, ContextFrameSection, RuntimeSkillEntry, RuntimeToolSchemaEntry,
};
use agentdash_spi::session_persistence::{
    PersistedSessionEvent, SessionLineageRecord, SessionLineageRelationKind, SessionLineageStatus,
};

const PROJECTION_PREVIEW_MAX_CHARS: usize = 360;
const TEXT_TOKEN_CHARS_PER_TOKEN: u64 = 4;

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
pub struct SessionCommandStateResponse {
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub turn_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct DeleteSessionResponse {
    pub deleted: bool,
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct ApproveToolCallResponse {
    pub approved: bool,
    pub session_id: String,
    pub tool_call_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct RejectToolCallResponse {
    pub rejected: bool,
    pub session_id: String,
    pub tool_call_id: String,
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
pub struct SessionContextUsageItemResponse {
    pub kind: String,
    pub label: String,
    pub name: String,
    #[ts(type = "number")]
    pub token_estimate: u64,
    pub source: String,
    pub deferred: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub source_event_seq: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub turn_id: Option<String>,
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
    pub items: Vec<SessionContextUsageItemResponse>,
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
        Self::from_envelope_and_context_items(envelope, Vec::new())
    }
}

impl SessionProjectionViewResponse {
    pub fn from_envelope_and_context_items(
        envelope: AgentContextEnvelope,
        context_items: Vec<SessionContextUsageItemResponse>,
    ) -> Self {
        let message_count = u64::try_from(envelope.messages.len()).unwrap_or(u64::MAX);
        let segments: Vec<_> = envelope
            .messages
            .into_iter()
            .enumerate()
            .map(|(index, message)| projection_segment_from_message(index, message))
            .collect();
        let context_item_token_estimate = context_items_token_estimate(&context_items);
        let context_usage = context_usage_analysis(&segments, context_items);
        let token_estimate = envelope
            .token_estimate
            .map(|tokens| tokens.saturating_add(context_item_token_estimate));
        Self {
            session_id: envelope.session_id,
            projection_kind: envelope.projection_kind.as_str().to_string(),
            projection_version: envelope.projection_version,
            head_event_seq: envelope.head_event_seq,
            active_compaction_id: envelope.active_compaction_id,
            token_estimate,
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
    context_items: Vec<SessionContextUsageItemResponse>,
) -> SessionContextUsageAnalysisResponse {
    let summary_tokens = sum_segment_tokens(segments, |segment| {
        segment.role == "compaction_summary" || segment.origin == "projection"
    });
    let raw_message_tokens = sum_segment_tokens(segments, |segment| {
        segment.role != "compaction_summary" && segment.origin != "projection"
    });
    let attachment_tokens = segments
        .iter()
        .map(|segment| segment.attachment_tokens)
        .fold(0_u64, u64::saturating_add);
    let message_tokens = raw_message_tokens.saturating_sub(attachment_tokens);
    let mut categories = context_item_categories(&context_items);
    categories.extend([
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
    ]);
    SessionContextUsageAnalysisResponse {
        categories,
        items: context_items,
        messages: message_context_breakdown(segments),
        top_tools: top_tools(segments),
        top_attachments: top_attachments(segments),
    }
}

fn context_item_categories(
    items: &[SessionContextUsageItemResponse],
) -> Vec<SessionContextUsageCategoryResponse> {
    [
        ("system_developer", "System / Developer"),
        ("system_tools", "System Tools"),
        ("mcp_tools", "MCP Tools"),
        ("agents", "Agents"),
        ("memory", "Memory"),
        ("skills", "Skills"),
    ]
    .into_iter()
    .map(|(kind, label)| {
        let category_items = items
            .iter()
            .filter(|item| item.kind == kind)
            .collect::<Vec<_>>();
        let token_estimate = category_items
            .iter()
            .filter(|item| !item.deferred)
            .map(|item| item.token_estimate)
            .fold(0_u64, u64::saturating_add);
        let source = context_item_category_source(&category_items);
        let deferred =
            !category_items.is_empty() && category_items.iter().all(|item| item.deferred);
        context_category(kind, label, token_estimate, &source, deferred)
    })
    .collect()
}

fn context_items_token_estimate(items: &[SessionContextUsageItemResponse]) -> u64 {
    items
        .iter()
        .filter(|item| !item.deferred)
        .map(|item| item.token_estimate)
        .fold(0_u64, u64::saturating_add)
}

fn context_item_category_source(items: &[&SessionContextUsageItemResponse]) -> String {
    let mut sources = items
        .iter()
        .map(|item| item.source.as_str())
        .filter(|source| !source.is_empty())
        .collect::<Vec<_>>();
    sources.sort_unstable();
    sources.dedup();
    match sources.as_slice() {
        [] => "projected".to_string(),
        [source] => (*source).to_string(),
        _ => "mixed".to_string(),
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

pub fn context_usage_items_from_context_frame(
    frame: &ContextFrame,
    source_event_seq: Option<u64>,
    turn_id: Option<String>,
) -> Vec<SessionContextUsageItemResponse> {
    frame
        .sections
        .iter()
        .flat_map(|section| {
            context_usage_items_from_section(frame, section, source_event_seq, &turn_id)
        })
        .collect()
}

fn context_usage_items_from_section(
    frame: &ContextFrame,
    section: &ContextFrameSection,
    source_event_seq: Option<u64>,
    turn_id: &Option<String>,
) -> Vec<SessionContextUsageItemResponse> {
    match section {
        ContextFrameSection::Identity {
            title,
            effective_prompt,
            ..
        } => vec![context_usage_item(
            "system_developer",
            "System / Developer",
            title,
            effective_prompt,
            "context_frame",
            false,
            source_event_seq,
            turn_id,
        )],
        ContextFrameSection::AssignmentContext {
            title, fragments, ..
        } => {
            let text = fragments
                .iter()
                .map(|fragment| fragment.content.as_str())
                .collect::<Vec<_>>()
                .join("\n\n");
            vec![context_usage_item(
                "system_developer",
                "System / Developer",
                title,
                non_empty_or(&text, &frame.rendered_text),
                "context_frame",
                false,
                source_event_seq,
                turn_id,
            )]
        }
        ContextFrameSection::ContinuationContext {
            title,
            owner_context,
            ..
        } => owner_context
            .as_deref()
            .map(|owner| {
                context_usage_item(
                    "system_developer",
                    "System / Developer",
                    title,
                    owner,
                    "context_frame",
                    false,
                    source_event_seq,
                    turn_id,
                )
            })
            .into_iter()
            .collect(),
        ContextFrameSection::HookInjection {
            title, injections, ..
        } => {
            let text = injections
                .iter()
                .map(|injection| injection.content.as_str())
                .collect::<Vec<_>>()
                .join("\n\n");
            vec![context_usage_item(
                "system_developer",
                "System / Developer",
                title,
                non_empty_or(&text, &frame.rendered_text),
                "context_frame",
                false,
                source_event_seq,
                turn_id,
            )]
        }
        ContextFrameSection::SystemNotice {
            title,
            summary,
            body,
        } => vec![context_usage_item(
            "system_developer",
            "System / Developer",
            title,
            body.as_deref().unwrap_or(summary),
            "context_frame",
            false,
            source_event_seq,
            turn_id,
        )],
        ContextFrameSection::PendingAction {
            title,
            instructions,
            injections,
            ..
        } => {
            let mut text_parts = instructions.iter().map(String::as_str).collect::<Vec<_>>();
            text_parts.extend(
                injections
                    .iter()
                    .map(|injection| injection.content.as_str()),
            );
            vec![context_usage_item(
                "system_developer",
                "System / Developer",
                title,
                &text_parts.join("\n\n"),
                "context_frame",
                false,
                source_event_seq,
                turn_id,
            )]
        }
        ContextFrameSection::AutoResume { title, prompt, .. } => vec![context_usage_item(
            "system_developer",
            "System / Developer",
            title,
            prompt,
            "context_frame",
            false,
            source_event_seq,
            turn_id,
        )],
        ContextFrameSection::UserPreferences { title, items, .. } => vec![context_usage_item(
            "memory",
            "Memory",
            title,
            &items.join("\n"),
            "context_frame",
            false,
            source_event_seq,
            turn_id,
        )],
        ContextFrameSection::ProjectGuidelines { title, entries, .. } => {
            let text = entries
                .iter()
                .map(|entry| format!("{}\n{}", entry.path, entry.content))
                .collect::<Vec<_>>()
                .join("\n\n");
            vec![context_usage_item(
                "memory",
                "Memory",
                title,
                &text,
                "context_frame",
                false,
                source_event_seq,
                turn_id,
            )]
        }
        ContextFrameSection::ToolSchema { tools } => tools
            .iter()
            .map(|tool| tool_schema_usage_item(tool, source_event_seq, turn_id))
            .collect(),
        ContextFrameSection::ToolSchemaDelta { added_tools } => added_tools
            .iter()
            .map(|tool| tool_schema_usage_item(tool, source_event_seq, turn_id))
            .collect(),
        ContextFrameSection::SkillDelta {
            added_skills,
            changed_skills,
            ..
        } => added_skills
            .iter()
            .chain(changed_skills.iter())
            .map(|skill| skill_usage_item(skill, source_event_seq, turn_id))
            .collect(),
        _ => Vec::new(),
    }
}

fn tool_schema_usage_item(
    tool: &RuntimeToolSchemaEntry,
    source_event_seq: Option<u64>,
    turn_id: &Option<String>,
) -> SessionContextUsageItemResponse {
    let source = tool.source.as_deref().unwrap_or("tool_schema");
    let kind = if source.starts_with("mcp:") || source.starts_with("platform_mcp:") {
        "mcp_tools"
    } else {
        "system_tools"
    };
    let label = if kind == "mcp_tools" {
        "MCP Tools"
    } else {
        "System Tools"
    };
    let mut text = format!("{}\n{}", tool.name, tool.description);
    if let Some(capability_key) = tool.capability_key.as_deref() {
        text.push_str("\n");
        text.push_str(capability_key);
    }
    if let Some(tool_path) = tool.tool_path.as_deref() {
        text.push_str("\n");
        text.push_str(tool_path);
    }
    text.push_str("\n");
    text.push_str(&tool.parameters_schema.to_string());
    context_usage_item(
        kind,
        label,
        &tool.name,
        &text,
        "tool_schema",
        false,
        source_event_seq,
        turn_id,
    )
}

fn skill_usage_item(
    skill: &RuntimeSkillEntry,
    source_event_seq: Option<u64>,
    turn_id: &Option<String>,
) -> SessionContextUsageItemResponse {
    let name = skill
        .display_name
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&skill.name);
    let text = [
        name,
        skill.description.as_str(),
        skill.file_path.as_str(),
        skill.capability_key.as_str(),
        skill.provider_key.as_str(),
    ]
    .join("\n");
    context_usage_item(
        "skills",
        "Skills",
        name,
        &text,
        "skill_registry",
        skill.disable_model_invocation,
        source_event_seq,
        turn_id,
    )
}

fn context_usage_item(
    kind: &str,
    label: &str,
    name: &str,
    text: &str,
    source: &str,
    deferred: bool,
    source_event_seq: Option<u64>,
    turn_id: &Option<String>,
) -> SessionContextUsageItemResponse {
    SessionContextUsageItemResponse {
        kind: kind.to_string(),
        label: label.to_string(),
        name: name.trim().to_string(),
        token_estimate: estimate_text_tokens(text),
        source: source.to_string(),
        deferred,
        source_event_seq,
        turn_id: turn_id.clone(),
    }
}

fn non_empty_or<'a>(value: &'a str, fallback: &'a str) -> &'a str {
    if value.trim().is_empty() {
        fallback
    } else {
        value
    }
}

fn estimate_text_tokens(text: &str) -> u64 {
    let text = text.trim();
    if text.is_empty() {
        return 0;
    }
    let chars = u64::try_from(text.chars().count()).unwrap_or(u64::MAX);
    chars
        .saturating_add(TEXT_TOKEN_CHARS_PER_TOKEN - 1)
        .saturating_div(TEXT_TOKEN_CHARS_PER_TOKEN)
        .max(1)
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
    use agentdash_spi::hooks::{
        ContextFrame, ContextFrameSection, RuntimeEventSource, RuntimeSkillEntry,
        RuntimeToolSchemaEntry,
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
        let messages_category = view
            .context_usage
            .categories
            .iter()
            .find(|category| category.kind == "messages")
            .expect("messages category should exist");
        assert_eq!(
            messages_category.token_estimate,
            view.segments[0]
                .token_estimate
                .expect("segment token estimate")
                .saturating_sub(view.context_usage.messages.attachment_tokens)
        );
        assert_eq!(view.context_usage.top_attachments.len(), 1);
        assert!(
            view.context_usage.top_attachments[0]
                .name
                .contains("image/png")
        );
    }

    #[test]
    fn projection_view_aggregates_context_frame_usage_items() {
        let envelope = AgentContextEnvelope {
            session_id: "sess-1".to_string(),
            projection_kind: ProjectionKind::ModelContext,
            projection_version: 0,
            head_event_seq: 10,
            active_compaction_id: None,
            token_estimate: Some(20),
            messages: Vec::new(),
        };
        let frame = ContextFrame {
            id: "frame-1".to_string(),
            kind: "system_guidelines".to_string(),
            source: RuntimeEventSource::RuntimeContextUpdate,
            phase_node: None,
            apply_mode: None,
            delivery_status: "prepared_for_connector".to_string(),
            delivery_channel: "connector_context".to_string(),
            message_role: "system".to_string(),
            rendered_text: String::new(),
            created_at_ms: 1,
            sections: vec![
                ContextFrameSection::Identity {
                    title: "Identity".to_string(),
                    summary: "identity".to_string(),
                    base_prompt: "base".to_string(),
                    agent_prompt: None,
                    mode: "default".to_string(),
                    effective_prompt: "You are Codex.".to_string(),
                },
                ContextFrameSection::ProjectGuidelines {
                    title: "Project Guidelines".to_string(),
                    summary: "guidelines".to_string(),
                    entries: vec![agentdash_spi::hooks::ProjectGuidelineEntry {
                        path: "AGENTS.md".to_string(),
                        content: "Use Chinese for user-facing replies.".to_string(),
                    }],
                },
                ContextFrameSection::ToolSchema {
                    tools: vec![
                        RuntimeToolSchemaEntry {
                            name: "read_file".to_string(),
                            description: "Read files".to_string(),
                            parameters_schema: serde_json::json!({"type": "object"}),
                            capability_key: None,
                            source: Some("platform:read".to_string()),
                            tool_path: None,
                        },
                        RuntimeToolSchemaEntry {
                            name: "workflow_search".to_string(),
                            description: "Search workflow state".to_string(),
                            parameters_schema: serde_json::json!({"type": "object"}),
                            capability_key: None,
                            source: Some("mcp:workflow".to_string()),
                            tool_path: None,
                        },
                    ],
                },
                ContextFrameSection::SkillDelta {
                    added_skills: vec![RuntimeSkillEntry {
                        name: "trellis-start".to_string(),
                        capability_key: "skill:trellis-start".to_string(),
                        provider_key: "local".to_string(),
                        local_name: "trellis-start".to_string(),
                        display_name: None,
                        description: "Start a Trellis session".to_string(),
                        file_path: ".agents/skills/trellis-start/SKILL.md".to_string(),
                        base_dir: None,
                        exposure: Default::default(),
                        disable_model_invocation: false,
                    }],
                    removed_skills: Vec::new(),
                    changed_skills: Vec::new(),
                },
            ],
        };
        let items = super::context_usage_items_from_context_frame(
            &frame,
            Some(8),
            Some("turn-1".to_string()),
        );

        let view = SessionProjectionViewResponse::from_envelope_and_context_items(envelope, items);

        assert_eq!(view.context_usage.items.len(), 5);
        assert!(
            view.token_estimate.expect("combined token estimate") > 20,
            "top-level token estimate should include non-message context items"
        );
        for kind in [
            "system_developer",
            "memory",
            "system_tools",
            "mcp_tools",
            "skills",
        ] {
            let category = view
                .context_usage
                .categories
                .iter()
                .find(|category| category.kind == kind)
                .expect("category should exist");
            assert!(category.token_estimate > 0);
            assert_ne!(category.source, "not_loaded");
            assert!(!category.deferred);
        }
    }
}
