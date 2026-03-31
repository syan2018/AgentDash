use crate::common::error::DomainError;

use super::entity::AuthSession;

#[async_trait::async_trait]
pub trait AuthSessionRepository: Send + Sync {
    async fn upsert_session(&self, session: &AuthSession) -> Result<(), DomainError>;
    async fn get_by_token_hash(&self, token_hash: &str) -> Result<Option<AuthSession>, DomainError>;
    async fn revoke_by_token_hash(&self, token_hash: &str, revoked_at: i64)
    -> Result<bool, DomainError>;
    async fn delete_expired_before(&self, epoch_secs: i64) -> Result<u64, DomainError>;
}
