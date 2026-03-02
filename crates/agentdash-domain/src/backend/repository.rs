use super::entity::{BackendConfig, UserPreferences, ViewConfig};
use crate::common::error::DomainError;

/// Backend 仓储接口（Port）
#[async_trait::async_trait]
pub trait BackendRepository: Send + Sync {
    async fn add_backend(&self, config: &BackendConfig) -> Result<(), DomainError>;
    async fn list_backends(&self) -> Result<Vec<BackendConfig>, DomainError>;
    async fn get_backend(&self, id: &str) -> Result<BackendConfig, DomainError>;
    async fn remove_backend(&self, id: &str) -> Result<(), DomainError>;
    async fn list_views(&self) -> Result<Vec<ViewConfig>, DomainError>;
    async fn save_view(&self, view: &ViewConfig) -> Result<(), DomainError>;
    async fn get_preferences(&self) -> Result<UserPreferences, DomainError>;
    async fn save_preferences(&self, prefs: &UserPreferences) -> Result<(), DomainError>;
}
