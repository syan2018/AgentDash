use uuid::Uuid;

use super::entity::SessionBinding;
use super::value_objects::SessionOwnerType;
use crate::common::error::DomainError;

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
}
