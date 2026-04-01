use uuid::Uuid;

use super::value_objects::{ChangeKind, StateChange};
use crate::common::error::DomainError;

/// 状态变更日志仓储接口。
///
/// 独立承载 `state_changes` 事件流的追加与查询，避免业务聚合仓储混入事件存储职责。
#[async_trait::async_trait]
pub trait StateChangeRepository: Send + Sync {
    async fn get_changes_since(
        &self,
        since_id: i64,
        limit: i64,
    ) -> Result<Vec<StateChange>, DomainError>;
    async fn get_changes_since_by_project(
        &self,
        project_id: Uuid,
        since_id: i64,
        limit: i64,
    ) -> Result<Vec<StateChange>, DomainError>;
    async fn latest_event_id(&self) -> Result<i64, DomainError>;
    async fn latest_event_id_by_project(&self, project_id: Uuid) -> Result<i64, DomainError>;
    async fn append_change(
        &self,
        project_id: Uuid,
        entity_id: Uuid,
        kind: ChangeKind,
        payload: serde_json::Value,
        backend_id: Option<&str>,
    ) -> Result<(), DomainError>;
}
