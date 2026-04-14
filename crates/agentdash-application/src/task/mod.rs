pub mod artifact;
pub mod config;
pub mod context_builder;
pub mod execution;
pub mod gateway;
pub mod lock;
pub mod management;
pub mod meta;
pub mod restart_tracker;
pub mod service;
pub mod session_runtime_inputs;
pub mod state_reconciler;
pub mod tools;

use agentdash_domain::common::error::DomainError;
use agentdash_domain::session_binding::{SessionBindingRepository, SessionOwnerType};
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
