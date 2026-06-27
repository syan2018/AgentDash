use std::collections::HashMap;

use super::entity::{
    BackendConfig, BackendExecutionLease, BackendExecutionTerminalKind, BackendWorkspaceInventory,
    LocalBackendClaim, ProjectBackendAccess, ProjectBackendAccessStatus, RunnerRegistrationToken,
    RuntimeHealth, RuntimeHealthOnlineUpdate, UserPreferences, ViewConfig,
};
use crate::common::error::DomainError;
use uuid::Uuid;

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

#[async_trait::async_trait]
pub trait BackendExecutionLeaseRepository: Send + Sync {
    async fn claim(&self, lease: &BackendExecutionLease) -> Result<(), DomainError>;
    async fn activate(
        &self,
        lease_id: Uuid,
        activated_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), DomainError>;
    async fn release(
        &self,
        lease_id: Uuid,
        terminal_kind: Option<BackendExecutionTerminalKind>,
        reason: Option<String>,
        released_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), DomainError>;
    async fn fail(
        &self,
        lease_id: Uuid,
        reason: Option<String>,
        failed_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), DomainError>;
    async fn mark_lost_by_backend(
        &self,
        backend_id: &str,
        reason: Option<String>,
        lost_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<u64, DomainError>;
    async fn get_by_id(&self, lease_id: Uuid)
    -> Result<Option<BackendExecutionLease>, DomainError>;
    async fn list_active(&self) -> Result<Vec<BackendExecutionLease>, DomainError>;
    async fn count_active_by_backend(
        &self,
        backend_ids: &[String],
    ) -> Result<HashMap<String, i64>, DomainError>;
}

#[async_trait::async_trait]
pub trait ProjectBackendAccessRepository: Send + Sync {
    async fn create(&self, access: &ProjectBackendAccess) -> Result<(), DomainError>;
    async fn update(&self, access: &ProjectBackendAccess) -> Result<(), DomainError>;
    async fn get_by_id(&self, id: Uuid) -> Result<Option<ProjectBackendAccess>, DomainError>;
    async fn list_by_project(
        &self,
        project_id: Uuid,
    ) -> Result<Vec<ProjectBackendAccess>, DomainError>;
    async fn list_active_by_project(
        &self,
        project_id: Uuid,
    ) -> Result<Vec<ProjectBackendAccess>, DomainError>;
    async fn get_active_for_project_backend(
        &self,
        project_id: Uuid,
        backend_id: &str,
    ) -> Result<Option<ProjectBackendAccess>, DomainError>;
    /// 列出指向某个 backend 的所有 active grant（跨 project 的反向视图）。
    ///
    /// 鉴权路径据此判断一个 User-scoped backend 通过哪些 project 的 active grant 被授权。
    async fn list_active_by_backend(
        &self,
        backend_id: &str,
    ) -> Result<Vec<ProjectBackendAccess>, DomainError>;
    /// 批量列出指向多个 backend 的所有 active grant，供列表路径一次预取，避免 N+1。
    async fn list_active_by_backends(
        &self,
        backend_ids: &[String],
    ) -> Result<Vec<ProjectBackendAccess>, DomainError>;
    async fn set_status(
        &self,
        id: Uuid,
        status: ProjectBackendAccessStatus,
    ) -> Result<(), DomainError>;
}

#[async_trait::async_trait]
pub trait BackendWorkspaceInventoryRepository: Send + Sync {
    async fn upsert(&self, item: &BackendWorkspaceInventory) -> Result<(), DomainError>;
    async fn upsert_many(&self, items: &[BackendWorkspaceInventory]) -> Result<(), DomainError>;
    async fn list_by_backend(
        &self,
        backend_id: &str,
    ) -> Result<Vec<BackendWorkspaceInventory>, DomainError>;
    async fn list_by_backends(
        &self,
        backend_ids: &[String],
    ) -> Result<Vec<BackendWorkspaceInventory>, DomainError>;
}

#[async_trait::async_trait]
pub trait RunnerRegistrationTokenRepository: Send + Sync {
    async fn create(&self, token: &RunnerRegistrationToken) -> Result<(), DomainError>;
    async fn update(&self, token: &RunnerRegistrationToken) -> Result<(), DomainError>;
    async fn get_by_id(&self, id: &str) -> Result<Option<RunnerRegistrationToken>, DomainError>;
    async fn list_by_project(
        &self,
        project_id: Uuid,
    ) -> Result<Vec<RunnerRegistrationToken>, DomainError>;
    async fn revoke(
        &self,
        id: &str,
        revoked_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), DomainError>;
    async fn record_usage(
        &self,
        id: &str,
        backend_id: &str,
        used_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), DomainError>;
}
