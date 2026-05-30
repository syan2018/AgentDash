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
use agentdash_domain::story::StoryRepository;
use agentdash_domain::task::Task;
use agentdash_domain::workflow::{
    LifecycleRunLinkRepository, LifecycleRunRepository, RunLinkSubjectKind,
};
use uuid::Uuid;

/// 从 LifecycleRunLink 查询 Task 的执行 session ID。
///
/// 通过 `lifecycle_run_link_repo.list_by_subject(Task, task_id)` 找到关联的 run，
/// 再从 run 取 session_id。
pub async fn find_task_execution_session_id(
    lifecycle_run_link_repo: &dyn LifecycleRunLinkRepository,
    lifecycle_run_repo: &dyn LifecycleRunRepository,
    task_id: Uuid,
) -> Result<Option<String>, DomainError> {
    let links = lifecycle_run_link_repo
        .list_by_subject(RunLinkSubjectKind::Task, task_id)
        .await?;
    let Some(link) = links.first() else {
        return Ok(None);
    };
    let run = lifecycle_run_repo.get_by_id(link.run_id).await?;
    Ok(run.and_then(|r| r.session_id))
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
