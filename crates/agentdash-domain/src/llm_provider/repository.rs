use uuid::Uuid;

use super::entity::{LlmProvider, LlmProviderUserCredential};
use crate::common::error::DomainError;

/// LLM Provider 仓储接口
#[async_trait::async_trait]
pub trait LlmProviderRepository: Send + Sync {
    async fn create(&self, provider: &LlmProvider) -> Result<(), DomainError>;
    async fn get_by_id(&self, id: Uuid) -> Result<Option<LlmProvider>, DomainError>;
    async fn list_all(&self) -> Result<Vec<LlmProvider>, DomainError>;
    async fn list_enabled(&self) -> Result<Vec<LlmProvider>, DomainError>;
    async fn update(&self, provider: &LlmProvider) -> Result<(), DomainError>;
    async fn delete(&self, id: Uuid) -> Result<(), DomainError>;
    /// 按给定 ID 顺序批量更新 sort_order
    async fn reorder(&self, ids: &[Uuid]) -> Result<(), DomainError>;
}

/// 用户 BYOK 凭据仓储接口。
#[async_trait::async_trait]
pub trait LlmProviderCredentialRepository: Send + Sync {
    async fn get_for_user_provider(
        &self,
        user_id: &str,
        provider_id: Uuid,
    ) -> Result<Option<LlmProviderUserCredential>, DomainError>;

    async fn list_for_user(
        &self,
        user_id: &str,
    ) -> Result<Vec<LlmProviderUserCredential>, DomainError>;

    async fn upsert_for_user_provider(
        &self,
        credential: &LlmProviderUserCredential,
    ) -> Result<(), DomainError>;

    async fn delete_for_user_provider(
        &self,
        user_id: &str,
        provider_id: Uuid,
    ) -> Result<bool, DomainError>;
}
