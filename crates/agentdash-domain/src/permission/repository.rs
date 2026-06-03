use uuid::Uuid;

use crate::common::error::DomainError;

use super::entity::PermissionGrant;

#[async_trait::async_trait]
pub trait PermissionGrantRepository: Send + Sync {
    async fn create(&self, grant: &PermissionGrant) -> Result<(), DomainError>;

    async fn update(&self, grant: &PermissionGrant) -> Result<(), DomainError>;

    async fn find_by_id(&self, id: Uuid) -> Result<Option<PermissionGrant>, DomainError>;

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

    /// 批量标记过期的 grant（TTL 到期），返回受影响行数。
    async fn expire_overdue(&self) -> Result<u64, DomainError>;
}
