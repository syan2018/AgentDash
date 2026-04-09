use serde::{Deserialize, Serialize};

use crate::message::{AgentMessage, MessageRef};

// ─── ProjectionKind ────────────────────────────────────────

/// 消息投影来源 — 标记一条 projected message 是原始 transcript 还是合成产物。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionKind {
    /// 直接从原始 transcript 事件还原
    Transcript,
    /// 压缩摘要（不对应单条原始事件，而是多条事件的聚合投影）
    CompactionSummary,
}

// ─── ProjectedEntry ────────────────────────────────────────

/// 带身份和投影语义的单条消息。
#[derive(Debug, Clone)]
pub struct ProjectedEntry {
    pub message_ref: MessageRef,
    pub projection_kind: ProjectionKind,
    pub message: AgentMessage,
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
