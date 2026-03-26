use uuid::Uuid;

use super::entity::{Agent, ProjectAgentLink};
use crate::common::error::DomainError;

/// Agent 仓储接口
#[async_trait::async_trait]
pub trait AgentRepository: Send + Sync {
    async fn create(&self, agent: &Agent) -> Result<(), DomainError>;
    async fn get_by_id(&self, id: Uuid) -> Result<Option<Agent>, DomainError>;
    async fn list_all(&self) -> Result<Vec<Agent>, DomainError>;
    async fn update(&self, agent: &Agent) -> Result<(), DomainError>;
    async fn delete(&self, id: Uuid) -> Result<(), DomainError>;
}

/// Project ↔ Agent 关联仓储接口
#[async_trait::async_trait]
pub trait ProjectAgentLinkRepository: Send + Sync {
    async fn create(&self, link: &ProjectAgentLink) -> Result<(), DomainError>;
    async fn get_by_id(&self, id: Uuid) -> Result<Option<ProjectAgentLink>, DomainError>;

    /// 按 project + agent 唯一键查找
    async fn find_by_project_and_agent(
        &self,
        project_id: Uuid,
        agent_id: Uuid,
    ) -> Result<Option<ProjectAgentLink>, DomainError>;

    /// 列出项目下所有关联
    async fn list_by_project(&self, project_id: Uuid) -> Result<Vec<ProjectAgentLink>, DomainError>;

    /// 列出 Agent 关联的所有项目
    async fn list_by_agent(&self, agent_id: Uuid) -> Result<Vec<ProjectAgentLink>, DomainError>;

    async fn update(&self, link: &ProjectAgentLink) -> Result<(), DomainError>;
    async fn delete(&self, id: Uuid) -> Result<(), DomainError>;

    /// 删除 project + agent 唯一键对应的关联
    async fn delete_by_project_and_agent(
        &self,
        project_id: Uuid,
        agent_id: Uuid,
    ) -> Result<(), DomainError>;
}
