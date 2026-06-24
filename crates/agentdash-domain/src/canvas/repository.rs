use uuid::Uuid;

use super::entity::Canvas;
use super::value_objects::CanvasScope;
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
    async fn list_personal_by_owner(
        &self,
        project_id: Uuid,
        owner_user_id: &str,
    ) -> Result<Vec<Canvas>, DomainError> {
        let canvases = self.list_by_project(project_id).await?;
        Ok(canvases
            .into_iter()
            .filter(|canvas| {
                canvas.scope == CanvasScope::Personal
                    && canvas.owner_user_id.as_deref() == Some(owner_user_id)
            })
            .collect())
    }

    async fn list_project_shared(&self, project_id: Uuid) -> Result<Vec<Canvas>, DomainError> {
        let canvases = self.list_by_project(project_id).await?;
        Ok(canvases
            .into_iter()
            .filter(|canvas| canvas.scope == CanvasScope::Project)
            .collect())
    }

    async fn find_published_from(
        &self,
        _source_canvas_id: Uuid,
    ) -> Result<Option<Canvas>, DomainError> {
        Ok(None)
    }

    async fn update(&self, canvas: &Canvas) -> Result<(), DomainError>;
    async fn delete(&self, id: Uuid) -> Result<(), DomainError>;
}
