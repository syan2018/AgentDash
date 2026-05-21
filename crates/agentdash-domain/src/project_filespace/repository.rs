use async_trait::async_trait;
use uuid::Uuid;

use crate::DomainError;

use super::entity::{ProjectFilespace, ProjectVfsMountBinding};

#[async_trait]
pub trait ProjectFilespaceRepository: Send + Sync {
    async fn create(&self, filespace: &ProjectFilespace) -> Result<(), DomainError>;
    async fn get_by_id(&self, id: Uuid) -> Result<Option<ProjectFilespace>, DomainError>;
    async fn get_by_project_and_key(
        &self,
        project_id: Uuid,
        key: &str,
    ) -> Result<Option<ProjectFilespace>, DomainError>;
    async fn list_by_project(&self, project_id: Uuid)
    -> Result<Vec<ProjectFilespace>, DomainError>;
    async fn update(&self, filespace: &ProjectFilespace) -> Result<(), DomainError>;
    async fn delete(&self, project_id: Uuid, id: Uuid) -> Result<(), DomainError>;
}

#[async_trait]
pub trait ProjectVfsMountBindingRepository: Send + Sync {
    async fn create(&self, binding: &ProjectVfsMountBinding) -> Result<(), DomainError>;
    async fn get_by_id(&self, id: Uuid) -> Result<Option<ProjectVfsMountBinding>, DomainError>;
    async fn get_by_project_and_mount_id(
        &self,
        project_id: Uuid,
        mount_id: &str,
    ) -> Result<Option<ProjectVfsMountBinding>, DomainError>;
    async fn list_by_project(
        &self,
        project_id: Uuid,
    ) -> Result<Vec<ProjectVfsMountBinding>, DomainError>;
    async fn update(&self, binding: &ProjectVfsMountBinding) -> Result<(), DomainError>;
    async fn delete(&self, project_id: Uuid, id: Uuid) -> Result<(), DomainError>;
}
