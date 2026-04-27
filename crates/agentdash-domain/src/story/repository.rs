use uuid::Uuid;

use super::entity::Story;
use crate::common::error::DomainError;

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
}
