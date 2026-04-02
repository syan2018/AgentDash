use uuid::Uuid;

use super::entity::LlmProvider;
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
