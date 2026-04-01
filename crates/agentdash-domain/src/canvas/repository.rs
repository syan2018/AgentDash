use uuid::Uuid;

use super::entity::Canvas;
use crate::common::error::DomainError;

#[async_trait::async_trait]
pub trait CanvasRepository: Send + Sync {
    async fn create(&self, canvas: &Canvas) -> Result<(), DomainError>;
    async fn get_by_id(&self, id: Uuid) -> Result<Option<Canvas>, DomainError>;
    async fn get_by_mount_id(
        &self,
        project_id: Uuid,
        mount_id: &str,
    ) -> Result<Option<Canvas>, DomainError>;
    async fn list_by_project(&self, project_id: Uuid) -> Result<Vec<Canvas>, DomainError>;
    async fn update(&self, canvas: &Canvas) -> Result<(), DomainError>;
    async fn delete(&self, id: Uuid) -> Result<(), DomainError>;
}
