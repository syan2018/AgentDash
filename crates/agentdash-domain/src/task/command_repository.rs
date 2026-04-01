use uuid::Uuid;

use crate::common::error::DomainError;

use super::entity::Task;

/// Task 聚合命令仓储
///
/// 用于承载跨 Task / Story / StateChange 的显式事务边界，
/// 避免将跨聚合一致性语义混入 `TaskRepository`。
#[async_trait::async_trait]
pub trait TaskAggregateCommandRepository: Send + Sync {
    /// 在同一事务中创建 Task，并同步维护 Story.task_count / StateChange。
    async fn create_for_story(&self, task: &Task) -> Result<(), DomainError>;

    /// 在同一事务中删除 Task，并同步维护 Story.task_count / StateChange。
    async fn delete_for_story(&self, task_id: Uuid) -> Result<Task, DomainError>;
}
