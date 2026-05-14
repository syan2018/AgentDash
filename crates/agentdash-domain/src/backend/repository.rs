use super::entity::{
    BackendConfig, LocalBackendClaim, RuntimeHealth, RuntimeHealthOnlineUpdate, UserPreferences,
    ViewConfig,
};
use crate::common::error::DomainError;

/// Backend 仓储接口（Port）
#[async_trait::async_trait]
pub trait BackendRepository: Send + Sync {
    async fn add_backend(&self, config: &BackendConfig) -> Result<(), DomainError>;
    async fn list_backends(&self) -> Result<Vec<BackendConfig>, DomainError>;
    async fn get_backend(&self, id: &str) -> Result<BackendConfig, DomainError>;
    async fn get_backend_by_auth_token(&self, token: &str) -> Result<BackendConfig, DomainError>;
    async fn ensure_local_backend(
        &self,
        claim: &LocalBackendClaim,
    ) -> Result<BackendConfig, DomainError>;
    async fn remove_backend(&self, id: &str) -> Result<(), DomainError>;
    async fn list_views(&self) -> Result<Vec<ViewConfig>, DomainError>;
    async fn save_view(&self, view: &ViewConfig) -> Result<(), DomainError>;
    async fn get_preferences(&self) -> Result<UserPreferences, DomainError>;
    async fn save_preferences(&self, prefs: &UserPreferences) -> Result<(), DomainError>;
}

#[async_trait::async_trait]
pub trait RuntimeHealthRepository: Send + Sync {
    async fn upsert_online(&self, update: &RuntimeHealthOnlineUpdate) -> Result<(), DomainError>;
    async fn update_capabilities(
        &self,
        backend_id: &str,
        capabilities: serde_json::Value,
    ) -> Result<(), DomainError>;
    async fn mark_seen(
        &self,
        backend_id: &str,
        seen_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), DomainError>;
    async fn mark_offline(
        &self,
        backend_id: &str,
        disconnected_at: chrono::DateTime<chrono::Utc>,
        reason: Option<String>,
    ) -> Result<(), DomainError>;
    async fn get_runtime_health(
        &self,
        backend_id: &str,
    ) -> Result<Option<RuntimeHealth>, DomainError>;
    async fn list_runtime_health(&self) -> Result<Vec<RuntimeHealth>, DomainError>;
}
