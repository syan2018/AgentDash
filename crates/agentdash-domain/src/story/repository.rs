use uuid::Uuid;

use super::entity::Story;
use crate::common::error::DomainError;
use crate::task::Task;

/// Story 仓储接口（Port）
///
/// 领域层定义接口，基础设施层提供实现。
///
/// **M1-b 更新**：Task 已合入 Story aggregate（`stories.tasks` JSONB 列）。
/// 新增 `find_by_task_id` 方法以便调用者从 task_id 反查所属 Story —— 原
/// `TaskRepository::get_by_id` 的语义下沉到 Story aggregate 内部的 `find_task`。
#[async_trait::async_trait]
pub trait StoryRepository: Send + Sync {
    async fn create(&self, story: &Story) -> Result<(), DomainError>;
    async fn get_by_id(&self, id: Uuid) -> Result<Option<Story>, DomainError>;
    async fn list_by_project(&self, project_id: Uuid) -> Result<Vec<Story>, DomainError>;
    async fn update(&self, story: &Story) -> Result<(), DomainError>;
    async fn delete(&self, id: Uuid) -> Result<(), DomainError>;

    /// 从 task_id 反查所属 Story（含聚合内 tasks）。
    ///
    /// 用于替代原 `TaskRepository::get_by_id` 的语义：基础设施层按 `stories.tasks @>`
    /// JSONB containment 查找 task 所属 Story。
    async fn find_by_task_id(&self, task_id: Uuid) -> Result<Option<Story>, DomainError>;

    /// 在 Story aggregate 内新增单个 Task。
    ///
    /// PostgreSQL 实现会在同一事务内锁定 Story 行、更新 `stories.tasks`，并追加
    /// `TaskCreated` / `StoryUpdated` state_changes。默认实现供测试仓储复用。
    async fn add_task_to_story(&self, story_id: Uuid, task: &Task) -> Result<(), DomainError> {
        let mut story = self
            .get_by_id(story_id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity: "story",
                id: story_id.to_string(),
            })?;
        story.add_task(task.clone());
        self.update(&story).await
    }

    /// 在 Story aggregate 内批量新增 Task。
    ///
    /// PostgreSQL 实现会将整个批次作为一个事务提交；默认实现逐个追加。
    async fn add_tasks_to_story(&self, story_id: Uuid, tasks: &[Task]) -> Result<(), DomainError> {
        for task in tasks {
            self.add_task_to_story(story_id, task).await?;
        }
        Ok(())
    }

    /// 从 Story aggregate 内删除 Task，并返回被删除的 Task。
    ///
    /// PostgreSQL 实现会在同一事务内锁定 Story 行、更新 `stories.tasks`，并追加
    /// `TaskDeleted` / `StoryUpdated` state_changes。默认实现供测试仓储复用。
    async fn remove_task_from_story(&self, task_id: Uuid) -> Result<Task, DomainError> {
        let mut story =
            self.find_by_task_id(task_id)
                .await?
                .ok_or_else(|| DomainError::NotFound {
                    entity: "task",
                    id: task_id.to_string(),
                })?;
        let removed = story
            .remove_task(task_id)
            .ok_or_else(|| DomainError::NotFound {
                entity: "task",
                id: task_id.to_string(),
            })?;
        self.update(&story).await?;
        Ok(removed)
    }
}
