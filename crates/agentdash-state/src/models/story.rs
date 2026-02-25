use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Story 状态枚举
/// 生命周期: Created → ContextReady → Decomposed → Executing → Completed/Failed
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "TEXT", rename_all = "snake_case")]
pub enum StoryStatus {
    Created,
    ContextReady,
    Decomposed,
    Executing,
    Completed,
    Failed,
}

/// Story — 用户价值单元
///
/// 从用户角度描述需求的工作单元，维护完整上下文，编排 Task 执行。
/// Story 严格归属于创建它的 Backend。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Story {
    pub id: Uuid,
    pub backend_id: String,
    pub title: String,
    pub description: String,
    pub status: StoryStatus,
    /// 设计上下文（PRD、规范引用等），JSON 格式
    pub context: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Story {
    pub fn new(backend_id: String, title: String, description: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            backend_id,
            title,
            description,
            status: StoryStatus::Created,
            context: serde_json::Value::Object(serde_json::Map::new()),
            created_at: now,
            updated_at: now,
        }
    }
}
