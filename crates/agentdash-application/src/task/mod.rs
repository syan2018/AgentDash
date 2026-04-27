pub mod artifact;
pub mod config;
pub mod context_builder;
pub mod execution;
pub mod gateway;
pub mod lock;
pub mod meta;
pub mod service;
pub mod view_projector;

use agentdash_domain::common::error::DomainError;
use agentdash_domain::session_binding::{SessionBindingRepository, SessionOwnerType};
use agentdash_domain::story::StoryRepository;
use agentdash_domain::task::Task;
use uuid::Uuid;

/// 从 SessionBinding 查询 Task 的执行 session ID。
///
/// Task 的 session 归属统一通过 `SessionBinding(owner_type=task, label="execution")` 管理，
/// 不再在 Task entity 上持有 session_id。
pub async fn find_task_execution_session_id(
    session_binding_repo: &dyn SessionBindingRepository,
    task_id: Uuid,
) -> Result<Option<String>, DomainError> {
    let binding = session_binding_repo
        .find_by_owner_and_label(SessionOwnerType::Task, task_id, "execution")
        .await?;
    Ok(binding.map(|b| b.session_id))
}

/// 通过 Story aggregate 读取指定 Task（只读副本）。
///
/// M1-b 过渡期辅助函数：原 `TaskRepository::get_by_id` 的替代。
/// 返回 `None` 表示 task 所属 story 不存在或 task 已被移除。
pub async fn load_task(
    story_repo: &dyn StoryRepository,
    task_id: Uuid,
) -> Result<Option<Task>, DomainError> {
    let Some(story) = story_repo.find_by_task_id(task_id).await? else {
        return Ok(None);
    };
    Ok(story.find_task(task_id).cloned())
}
