use uuid::Uuid;

use super::entity::ProjectAgent;
use crate::common::error::DomainError;

/// ProjectAgent 仓储接口
#[async_trait::async_trait]
pub trait ProjectAgentRepository: Send + Sync {
    async fn create(&self, agent: &ProjectAgent) -> Result<(), DomainError>;
    async fn get_by_id(&self, id: Uuid) -> Result<Option<ProjectAgent>, DomainError>;

    /// 按 project + project_agent_id 查找
    async fn get_by_project_and_id(
        &self,
        project_id: Uuid,
        id: Uuid,
    ) -> Result<Option<ProjectAgent>, DomainError>;

    /// 按 project + name 查找
    async fn get_by_project_and_name(
        &self,
        project_id: Uuid,
        name: &str,
    ) -> Result<Option<ProjectAgent>, DomainError>;

    /// 列出项目下所有 Project Agent
    async fn list_by_project(&self, project_id: Uuid) -> Result<Vec<ProjectAgent>, DomainError>;

    async fn update(&self, agent: &ProjectAgent) -> Result<(), DomainError>;

    async fn delete(&self, project_id: Uuid, id: Uuid) -> Result<(), DomainError>;
}
