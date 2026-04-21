use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::context_container::ContextContainerDefinition;
use crate::context_source::ContextSourceRef;
use crate::session_composition::SessionComposition;

/// Story 状态枚举
/// 生命周期: Created → ContextReady → Decomposed → Executing → Completed/Failed/Cancelled
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StoryStatus {
    Created,
    ContextReady,
    Decomposed,
    Executing,
    Completed,
    Failed,
    Cancelled,
}

/// Story 优先级枚举
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum StoryPriority {
    P0,
    P1,
    #[default]
    P2,
    P3,
}

/// Story 类型枚举
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum StoryType {
    #[default]
    Feature,
    Bugfix,
    Refactor,
    Docs,
    Test,
    Other,
}

/// 结构化 Story 上下文
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StoryContext {
    /// 声明式上下文来源
    #[serde(default)]
    pub source_refs: Vec<ContextSourceRef>,
    /// Story 级上下文容器定义
    #[serde(default)]
    pub context_containers: Vec<ContextContainerDefinition>,
    /// 显式禁用的项目级容器 ID
    #[serde(default)]
    pub disabled_container_ids: Vec<String>,
    /// Story 级会话编排配置
    #[serde(default)]
    pub session_composition: Option<SessionComposition>,
}

/// 状态变更类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    StoryCreated,
    StoryUpdated,
    StoryStatusChanged,
    StoryDeleted,
    TaskCreated,
    TaskUpdated,
    TaskStatusChanged,
    TaskDeleted,
    TaskArtifactAdded,
}

/// StateChange — 不可变的状态变更日志
///
/// 所有操作都记录为 StateChange，用于实现：
/// 1. 完整历史追溯
/// 2. Resume 机制（基于 since_id 的增量恢复）
/// 3. NDJSON 流式推送
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateChange {
    /// 单调递增 ID，用于 Resume 的游标定位
    pub id: i64,
    pub project_id: Uuid,
    pub entity_id: Uuid,
    pub kind: ChangeKind,
    /// 变更载荷（差异数据）
    pub payload: serde_json::Value,
    /// 可选 backend 来源；当变更无法归属到具体 Workspace/Backend 时为空。
    pub backend_id: Option<String>,
    pub created_at: DateTime<Utc>,
}
