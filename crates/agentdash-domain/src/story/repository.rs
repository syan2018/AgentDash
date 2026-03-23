use uuid::Uuid;

use super::entity::Story;
use super::value_objects::{ChangeKind, StateChange};
use crate::common::error::DomainError;

/// Story 仓储接口（Port）
///
/// 领域层定义接口，基础设施层提供实现。
#[async_trait::async_trait]
pub trait StoryRepository: Send + Sync {
    async fn create(&self, story: &Story) -> Result<(), DomainError>;
    async fn get_by_id(&self, id: Uuid) -> Result<Option<Story>, DomainError>;
    async fn list_by_project(&self, project_id: Uuid) -> Result<Vec<Story>, DomainError>;
    async fn update(&self, story: &Story) -> Result<(), DomainError>;
    async fn delete(&self, id: Uuid) -> Result<(), DomainError>;
    async fn get_changes_since(
        &self,
        since_id: i64,
        limit: i64,
    ) -> Result<Vec<StateChange>, DomainError>;
    async fn latest_event_id(&self) -> Result<i64, DomainError>;
    async fn append_change(
        &self,
        entity_id: Uuid,
        kind: ChangeKind,
        payload: serde_json::Value,
        backend_id: Option<&str>,
    ) -> Result<(), DomainError>;
}
