use async_trait::async_trait;
use uuid::Uuid;

use crate::DomainError;

use super::entity::ProjectVfsMount;

#[async_trait]
pub trait ProjectVfsMountRepository: Send + Sync {
    async fn create(&self, mount: &ProjectVfsMount) -> Result<(), DomainError>;
    async fn get_by_id(&self, id: Uuid) -> Result<Option<ProjectVfsMount>, DomainError>;
    async fn get_by_project_and_mount_id(
        &self,
        project_id: Uuid,
        mount_id: &str,
    ) -> Result<Option<ProjectVfsMount>, DomainError>;
    async fn list_by_project(&self, project_id: Uuid) -> Result<Vec<ProjectVfsMount>, DomainError>;
    async fn update(&self, mount: &ProjectVfsMount) -> Result<(), DomainError>;
    async fn delete(&self, project_id: Uuid, mount_id: &str) -> Result<(), DomainError>;
}
