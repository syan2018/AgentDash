use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 状态变更类型
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "TEXT", rename_all = "snake_case")]
pub enum ChangeKind {
    StoryCreated,
    StoryUpdated,
    StoryStatusChanged,
    TaskCreated,
    TaskUpdated,
    TaskStatusChanged,
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
    pub entity_id: Uuid,
    pub kind: ChangeKind,
    /// 变更载荷（差异数据）
    pub payload: serde_json::Value,
    pub backend_id: String,
    pub created_at: DateTime<Utc>,
}
