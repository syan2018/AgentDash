pub mod artifact;
pub mod config;
pub mod context_builder;
pub mod execution;
pub mod gateway;
pub mod lock;
pub mod meta;
pub mod plan;
pub(crate) mod runtime_coordinate;
pub mod service;
pub mod view_projector;

use agentdash_domain::common::error::DomainError;
use agentdash_domain::task::Task;
use uuid::Uuid;

/// 通过 Story aggregate 读取指定 Task（只读副本）。
///
/// M1-b 过渡期辅助函数：原 `TaskRepository::get_by_id` 的替代。
/// 返回 `None` 表示 task 所属 story 不存在或 task 已被移除。
pub async fn load_task(
    _story_repo: &dyn agentdash_domain::story::StoryRepository,
    task_id: Uuid,
) -> Result<Option<Task>, DomainError> {
    tracing::debug!(
        task_id = %task_id,
        "legacy Story-owned Task loader is disabled; Task truth lives in LifecycleRun.tasks"
    );
    Ok(None)
}
