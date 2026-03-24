use crate::common::error::DomainError;

use super::entity::{Group, User};

#[async_trait::async_trait]
pub trait UserDirectoryRepository: Send + Sync {
    async fn upsert_user(&self, user: &User) -> Result<(), DomainError>;
    async fn get_user_by_id(&self, user_id: &str) -> Result<Option<User>, DomainError>;
    async fn get_group_by_id(&self, group_id: &str) -> Result<Option<Group>, DomainError>;
    async fn list_users(&self) -> Result<Vec<User>, DomainError>;
    async fn list_groups(&self) -> Result<Vec<Group>, DomainError>;
    async fn list_groups_for_user(&self, user_id: &str) -> Result<Vec<Group>, DomainError>;
    async fn replace_groups_for_user(
        &self,
        user_id: &str,
        groups: &[Group],
    ) -> Result<(), DomainError>;
}
