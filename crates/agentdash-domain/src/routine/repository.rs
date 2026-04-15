use uuid::Uuid;

use super::entity::{Routine, RoutineExecution};
use crate::common::error::DomainError;

/// Routine 仓储接口
#[async_trait::async_trait]
pub trait RoutineRepository: Send + Sync {
    async fn create(&self, routine: &Routine) -> Result<(), DomainError>;
    async fn get_by_id(&self, id: Uuid) -> Result<Option<Routine>, DomainError>;
    async fn list_by_project(&self, project_id: Uuid) -> Result<Vec<Routine>, DomainError>;
    /// 列出所有已启用的指定触发类型的 Routine
    async fn list_enabled_by_trigger_type(
        &self,
        trigger_type: &str,
    ) -> Result<Vec<Routine>, DomainError>;
    async fn update(&self, routine: &Routine) -> Result<(), DomainError>;
    async fn delete(&self, id: Uuid) -> Result<(), DomainError>;
    /// 通过 webhook endpoint_id 查找 Routine
    async fn find_by_endpoint_id(&self, endpoint_id: &str) -> Result<Option<Routine>, DomainError>;
}

/// RoutineExecution 仓储接口
#[async_trait::async_trait]
pub trait RoutineExecutionRepository: Send + Sync {
    async fn create(&self, execution: &RoutineExecution) -> Result<(), DomainError>;
    async fn get_by_id(&self, id: Uuid) -> Result<Option<RoutineExecution>, DomainError>;
    async fn update(&self, execution: &RoutineExecution) -> Result<(), DomainError>;
    async fn list_by_routine(
        &self,
        routine_id: Uuid,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<RoutineExecution>, DomainError>;
    /// 查找指定 entity_key 最近的活跃 session
    async fn find_latest_by_entity_key(
        &self,
        routine_id: Uuid,
        entity_key: &str,
    ) -> Result<Option<RoutineExecution>, DomainError>;
}
