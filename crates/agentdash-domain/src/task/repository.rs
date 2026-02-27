use uuid::Uuid;

use crate::common::error::DomainError;
use super::entity::Task;
use super::value_objects::TaskStatus;

/// Task 仓储接口（Port）
#[async_trait::async_trait]
pub trait TaskRepository: Send + Sync {
    async fn create(&self, task: &Task) -> Result<(), DomainError>;
    async fn get_by_id(&self, id: Uuid) -> Result<Option<Task>, DomainError>;
    async fn list_by_story(&self, story_id: Uuid) -> Result<Vec<Task>, DomainError>;
    async fn list_by_workspace(&self, workspace_id: Uuid) -> Result<Vec<Task>, DomainError>;
    async fn update(&self, task: &Task) -> Result<(), DomainError>;
    async fn update_status(&self, id: Uuid, status: TaskStatus) -> Result<(), DomainError>;
    async fn delete(&self, id: Uuid) -> Result<(), DomainError>;
}
