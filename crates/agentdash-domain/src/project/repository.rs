use uuid::Uuid;

use super::entity::Project;
use crate::common::error::DomainError;

/// Project 仓储接口（Port）
#[async_trait::async_trait]
pub trait ProjectRepository: Send + Sync {
    async fn create(&self, project: &Project) -> Result<(), DomainError>;
    async fn get_by_id(&self, id: Uuid) -> Result<Option<Project>, DomainError>;
    async fn list_all(&self) -> Result<Vec<Project>, DomainError>;
    async fn update(&self, project: &Project) -> Result<(), DomainError>;
    async fn delete(&self, id: Uuid) -> Result<(), DomainError>;
}
