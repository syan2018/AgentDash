use serde::{Deserialize, Serialize};

use crate::model::message::{AgentMessage, MessageRef};

// ─── ProjectionKind ────────────────────────────────────────

/// 投影视图或条目类别。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionKind {
    /// 直接从原始 transcript 事件还原
    Transcript,
    /// 压缩摘要（不对应单条原始事件，而是多条事件的聚合投影）
    CompactionSummary,
    /// 模型可见上下文投影
    #[default]
    ModelContext,
    /// 前端 timeline 投影
    Timeline,
    /// 审计回放投影
    Audit,
    /// 团队接手 / 分支交接投影
    Handoff,
}

impl ProjectionKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Transcript => "transcript",
            Self::CompactionSummary => "compaction_summary",
            Self::ModelContext => "model_context",
            Self::Timeline => "timeline",
            Self::Audit => "audit",
            Self::Handoff => "handoff",
        }
    }
}

// ─── Projection Provenance ──────────────────────────────────

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionOrigin {
    #[default]
    Event,
    Projection,
}

impl ProjectionOrigin {
    pub fn from_label(value: &str) -> Self {
        match value {
            "projection" => Self::Projection,
            _ => Self::Event,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Event => "event",
            Self::Projection => "projection",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectionSourceRange {
    pub start_event_seq: u64,
    pub end_event_seq: u64,
}

// ─── ProjectedEntry ────────────────────────────────────────

/// 带身份和投影语义的单条消息。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectedEntry {
    pub message_ref: MessageRef,
    pub projection_kind: ProjectionKind,
    pub message: AgentMessage,
    #[serde(default)]
    pub origin: ProjectionOrigin,
    #[serde(default)]
    pub synthetic: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_event_seq: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_range: Option<ProjectionSourceRange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projection_segment_id: Option<String>,
    #[serde(default)]
    pub provenance: serde_json::Value,
}

impl ProjectedEntry {
    pub fn event(
        message_ref: MessageRef,
        projection_kind: ProjectionKind,
        message: AgentMessage,
        source_event_seq: Option<u64>,
    ) -> Self {
        let source_range = source_event_seq.map(|seq| ProjectionSourceRange {
            start_event_seq: seq,
            end_event_seq: seq,
        });
        Self {
            message_ref,
            projection_kind,
            message,
            origin: ProjectionOrigin::Event,
            synthetic: false,
            source_event_seq,
            source_range,
            projection_segment_id: None,
            provenance: serde_json::Value::Null,
        }
    }

    pub fn projection(
        message_ref: MessageRef,
        projection_kind: ProjectionKind,
        message: AgentMessage,
        projection_segment_id: Option<String>,
        source_range: Option<ProjectionSourceRange>,
    ) -> Self {
        Self {
            message_ref,
            projection_kind,
            message,
            origin: ProjectionOrigin::Projection,
            synthetic: true,
            source_event_seq: None,
            source_range,
            projection_segment_id,
            provenance: serde_json::Value::Null,
        }
    }
}

// ─── ProjectedTranscript ───────────────────────────────────

/// 从持久化事件重建的投影 transcript。
///
/// 与 `Vec<AgentMessage>` 不同，每条消息都携带稳定引用和投影来源标记，
/// 可用于 ref-based compaction cut、restore 对齐和 branch lineage。
#[derive(Debug, Clone, Default)]
pub struct ProjectedTranscript {
    pub entries: Vec<ProjectedEntry>,
}

impl ProjectedTranscript {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// 降级为裸 AgentMessage 列表 — 用于注入到不需要身份的 runtime 路径。
    pub fn into_messages(self) -> Vec<AgentMessage> {
        self.entries.into_iter().map(|e| e.message).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

// ─── Agent Context Envelope ────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentInputMessage {
    pub message_ref: MessageRef,
    pub projection_kind: ProjectionKind,
    pub message: AgentMessage,
    #[serde(default)]
    pub origin: ProjectionOrigin,
    #[serde(default)]
    pub synthetic: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_event_seq: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_range: Option<ProjectionSourceRange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projection_segment_id: Option<String>,
    #[serde(default)]
    pub provenance: serde_json::Value,
}

impl From<ProjectedEntry> for AgentInputMessage {
    fn from(entry: ProjectedEntry) -> Self {
        Self {
            message_ref: entry.message_ref,
            projection_kind: entry.projection_kind,
            message: entry.message,
            origin: entry.origin,
            synthetic: entry.synthetic,
            source_event_seq: entry.source_event_seq,
            source_range: entry.source_range,
            projection_segment_id: entry.projection_segment_id,
            provenance: entry.provenance,
        }
    }
}

impl From<AgentInputMessage> for ProjectedEntry {
    fn from(message: AgentInputMessage) -> Self {
        Self {
            message_ref: message.message_ref,
            projection_kind: message.projection_kind,
            message: message.message,
            origin: message.origin,
            synthetic: message.synthetic,
            source_event_seq: message.source_event_seq,
            source_range: message.source_range,
            projection_segment_id: message.projection_segment_id,
            provenance: message.provenance,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentContextEnvelope {
    pub session_id: String,
    pub projection_kind: ProjectionKind,
    pub projection_version: u64,
    pub head_event_seq: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_compaction_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_estimate: Option<u64>,
    #[serde(default)]
    pub messages: Vec<AgentInputMessage>,
}

impl AgentContextEnvelope {
    pub fn into_messages(self) -> Vec<AgentMessage> {
        self.messages
            .into_iter()
            .map(|message| message.message)
            .collect()
    }

    pub fn into_projected_transcript(self) -> ProjectedTranscript {
        ProjectedTranscript {
            entries: self
                .messages
                .into_iter()
                .map(ProjectedEntry::from)
                .collect(),
        }
    }
}
