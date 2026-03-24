use uuid::Uuid;

use super::entity::SessionBinding;
use super::value_objects::SessionOwnerType;
use crate::common::error::DomainError;

/// 项目下 session binding 的完整归属上下文（用于聚合查询，避免 N+1）
#[derive(Debug, Clone)]
pub struct ProjectSessionBinding {
    pub binding: SessionBinding,
    /// Story 标题（owner_type = story/task 时有值）
    pub story_title: Option<String>,
    /// Story ID（owner_type = task 时有值）
    pub story_id: Option<Uuid>,
    /// owner 实体标题（story: story.title, task: task.title, project: project.name）
    pub owner_title: Option<String>,
}

/// SessionBinding 仓储接口（Port）
#[async_trait::async_trait]
pub trait SessionBindingRepository: Send + Sync {
    async fn create(&self, binding: &SessionBinding) -> Result<(), DomainError>;

    async fn delete(&self, id: Uuid) -> Result<(), DomainError>;

    async fn delete_by_session_and_owner(
        &self,
        session_id: &str,
        owner_type: SessionOwnerType,
        owner_id: Uuid,
    ) -> Result<(), DomainError>;

    async fn list_by_owner(
        &self,
        owner_type: SessionOwnerType,
        owner_id: Uuid,
    ) -> Result<Vec<SessionBinding>, DomainError>;

    async fn list_by_session(&self, session_id: &str) -> Result<Vec<SessionBinding>, DomainError>;

    async fn find_by_owner_and_label(
        &self,
        owner_type: SessionOwnerType,
        owner_id: Uuid,
        label: &str,
    ) -> Result<Option<SessionBinding>, DomainError>;

    /// 返回所有存在绑定关系的 session_id 集合（去重）
    async fn list_bound_session_ids(&self) -> Result<Vec<String>, DomainError>;

    /// 批量查询项目下所有层级（project / story / task）的 session bindings，
    /// 并内联归属上下文（story_title / story_id / owner_title）。
    /// 一次查询替代原来的 N+1 串行调用。
    async fn list_by_project(
        &self,
        project_id: Uuid,
    ) -> Result<Vec<ProjectSessionBinding>, DomainError>;
}
