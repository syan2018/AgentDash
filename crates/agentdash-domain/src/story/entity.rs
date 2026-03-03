use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::value_objects::{StoryContext, StoryPriority, StoryStatus, StoryType};

/// Story — 用户价值单元
///
/// 从用户角度描述需求的工作单元，维护完整上下文，编排 Task 执行。
/// 归属于 Project，同时保留 backend_id 作为执行后端标识。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Story {
    pub id: Uuid,
    /// 所属项目（新增）
    pub project_id: Uuid,
    /// 执行后端标识（保留，与 Project.backend_id 可独立设置）
    pub backend_id: String,
    pub title: String,
    pub description: String,
    pub status: StoryStatus,
    pub priority: StoryPriority,
    pub story_type: StoryType,
    pub tags: Vec<String>,
    /// Story 聚合信息：当前关联 Task 数量
    pub task_count: u32,
    /// 结构化设计上下文
    pub context: StoryContext,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Story {
    pub fn new(project_id: Uuid, backend_id: String, title: String, description: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            project_id,
            backend_id,
            title,
            description,
            status: StoryStatus::Created,
            priority: StoryPriority::default(),
            story_type: StoryType::default(),
            tags: vec![],
            task_count: 0,
            context: StoryContext::default(),
            created_at: now,
            updated_at: now,
        }
    }
}
