//! Task ↔ Session 绑定桥接。
//!
//! 职责：封装 Task execution child session 绑定的创建 / 清理流程，
//! 包括与 `SessionCoreService`、`SessionBindingRepository` 的协同。
//!
//! 注意：这里只负责维护 Task → SessionBinding 的归属关系与 StateChange 记录，
//! 不参与 Task 生命周期决策（那是 projector / service 层的职责）。

use serde_json::json;
use uuid::Uuid;

use agentdash_domain::DomainError;
use agentdash_domain::story::ChangeKind;
use agentdash_domain::task::Task;

use crate::repository_set::RepositorySet;
use crate::session::SessionMeta;
use crate::task::execution::TaskExecutionError;

use super::errors::map_internal_error;
pub async fn create_task_session(
    session_core: &crate::session::SessionCoreService,
    task: &Task,
) -> Result<SessionMeta, TaskExecutionError> {
    let title = format!("Task: {}", task.title.trim());
    session_core
        .create_session(title.trim())
        .await
        .map_err(map_internal_error)
}

/// 清理 Task 的 session 绑定 — OneShot 模式完成或失败后调用。
///
/// 删除 SessionBinding(owner_type=task, label="execution")。执行器原生 resume id
/// 归属在 SessionMeta，不回写 Task。
/// Stub: session binding 已移除，此函数保留签名供调用方兼容。
/// TODO: migrate to LifecycleRunLink-based session dissociation
pub async fn clear_task_session_binding(
    _repos: &RepositorySet,
    task_id: Uuid,
    _backend_id: &str,
    reason: &str,
) {
    tracing::debug!(
        task_id = %task_id,
        reason = reason,
        "clear_task_session_binding: session binding 已移除，跳过"
    );
}
