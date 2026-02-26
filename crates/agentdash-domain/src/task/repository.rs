use uuid::Uuid;

use crate::common::error::DomainError;
use super::entity::Task;

/// Task 仓储接口（Port）
#[async_trait::async_trait]
pub trait TaskRepository: Send + Sync {
    async fn list_by_story(&self, story_id: Uuid) -> Result<Vec<Task>, DomainError>;
}
