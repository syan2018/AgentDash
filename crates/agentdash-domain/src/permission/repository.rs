use uuid::Uuid;

use crate::common::error::DomainError;

use super::entity::PermissionGrant;
use super::value_objects::GrantStatus;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionGrantStatusFilter {
    Exact(GrantStatus),
    Pending,
    Active,
    Terminal,
}

#[async_trait::async_trait]
pub trait PermissionGrantRepository: Send + Sync {
    async fn create(&self, grant: &PermissionGrant) -> Result<(), DomainError>;

    async fn update(&self, grant: &PermissionGrant) -> Result<(), DomainError>;

    async fn find_by_id(&self, id: Uuid) -> Result<Option<PermissionGrant>, DomainError>;

    /// 查询 effect_frame_id 下的 grants，可按精确状态或状态组过滤。
    async fn list_by_frame(
        &self,
        effect_frame_id: Uuid,
        status_filter: Option<PermissionGrantStatusFilter>,
    ) -> Result<Vec<PermissionGrant>, DomainError>;

    /// 查询 LifecycleRun 下的 grants，可按精确状态或状态组过滤。
    async fn list_by_run(
        &self,
        run_id: Uuid,
        status_filter: Option<PermissionGrantStatusFilter>,
    ) -> Result<Vec<PermissionGrant>, DomainError>;

    /// 查询 effect_frame_id 下所有活跃 grant（status = applied | scope_escalated）。
    async fn list_active_by_frame(
        &self,
        effect_frame_id: Uuid,
    ) -> Result<Vec<PermissionGrant>, DomainError>;

    /// 查询 LifecycleRun 下所有活跃 grant。
    async fn list_active_by_run(&self, run_id: Uuid) -> Result<Vec<PermissionGrant>, DomainError>;

    /// 查询 effect_frame_id 下有 scope_escalation_intent 且状态为 Applied 的 grant。
    async fn find_active_escalation_grant(
        &self,
        effect_frame_id: Uuid,
        target_subject_kind: &str,
    ) -> Result<Option<PermissionGrant>, DomainError>;

    /// 查询已到期且可进入 Expired 终态的 active grants，过期效果由 application service 按单 grant 分类应用。
    async fn list_overdue_active(
        &self,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<PermissionGrant>, DomainError>;
}
