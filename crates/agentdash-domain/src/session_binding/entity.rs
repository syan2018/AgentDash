use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::value_objects::SessionOwnerType;

/// SessionBinding — Session 归属关系
///
/// 统一管理 Story/Task 与 Session 的关联关系。
/// 作为唯一的 source of truth，取代在各实体上直接嵌入 session_id 的模式。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionBinding {
    pub id: Uuid,
    /// ExecutorHub session ID（如 "sess-1234..."）
    pub session_id: String,
    /// 归属实体类型
    pub owner_type: SessionOwnerType,
    /// 归属实体 ID（Story.id 或 Task.id）
    pub owner_id: Uuid,
    /// 自由标签，约定值: "execution", "companion", "planning", "review" 等
    pub label: String,
    pub created_at: DateTime<Utc>,
}

impl SessionBinding {
    pub fn new(
        session_id: String,
        owner_type: SessionOwnerType,
        owner_id: Uuid,
        label: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            session_id,
            owner_type,
            owner_id,
            label: label.into(),
            created_at: Utc::now(),
        }
    }
}
