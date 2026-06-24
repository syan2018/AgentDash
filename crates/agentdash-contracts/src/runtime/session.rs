use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use agentdash_agent_protocol::BackboneEnvelope;
use agentdash_agent_types::{MessageRef, ProjectionSourceRange};
use agentdash_spi::session_persistence::{
    PersistedSessionEvent, SessionLineageRecord, SessionLineageRelationKind, SessionLineageStatus,
};

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
    EphemeralEvent {
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

    pub fn ephemeral_event(event: PersistedSessionEvent) -> Self {
        Self::EphemeralEvent {
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

fn is_zero(value: &u64) -> bool {
    *value == 0
}
