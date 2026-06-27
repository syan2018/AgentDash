use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::value_objects::{StoryContext, StoryPriority, StoryStatus, StoryType};

/// Story — 用户价值单元
///
/// 从用户角度描述需求的工作单元，维护完整上下文，编排 Task 执行。
/// 归属于 Project。backend_id 已移除，通过 default_workspace_id → Workspace.backend_id 获取。
///
/// **Aggregate root**：Story 只保存用户价值单元的主题、上下文和流程状态。
/// Task 计划事实由 `LifecycleRun.tasks` 承担，Story 侧通过 projection 查询关联 Task。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Story {
    pub id: Uuid,
    /// 所属项目
    pub project_id: Uuid,
    /// Story 级默认 Workspace（用于 Task 执行 backend 解析继承链）
    pub default_workspace_id: Option<Uuid>,
    pub title: String,
    pub description: String,
    pub status: StoryStatus,
    pub priority: StoryPriority,
    pub story_type: StoryType,
    pub tags: Vec<String>,
    /// 结构化设计上下文
    pub context: StoryContext,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Story {
    pub fn new(project_id: Uuid, title: String, description: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            project_id,
            default_workspace_id: None,
            title,
            description,
            status: StoryStatus::Created,
            priority: StoryPriority::default(),
            story_type: StoryType::default(),
            tags: vec![],
            context: StoryContext::default(),
            created_at: now,
            updated_at: now,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn new_story_has_default_context() {
        let project_id = Uuid::new_v4();
        let story = Story::new(project_id, "S".into(), "".into());
        assert_eq!(story.project_id, project_id);
        assert!(story.context.source_refs.is_empty());
    }
}
