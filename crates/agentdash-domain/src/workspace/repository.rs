use uuid::Uuid;

use super::entity::Workspace;
use super::value_objects::WorkspaceStatus;
use crate::common::error::DomainError;

/// Workspace 仓储接口（Port）
#[async_trait::async_trait]
pub trait WorkspaceRepository: Send + Sync {
    async fn create(&self, workspace: &Workspace) -> Result<(), DomainError>;
    async fn get_by_id(&self, id: Uuid) -> Result<Option<Workspace>, DomainError>;
    async fn list_by_project(&self, project_id: Uuid) -> Result<Vec<Workspace>, DomainError>;
    async fn update(&self, workspace: &Workspace) -> Result<(), DomainError>;
    async fn update_status(&self, id: Uuid, status: WorkspaceStatus) -> Result<(), DomainError>;
    async fn delete(&self, id: Uuid) -> Result<(), DomainError>;
}
