//! Task ↔ Session 绑定桥接。
//!
//! 职责：封装 Task execution child session 绑定的创建 / 清理流程，
//! 包括与 `SessionHub`、`SessionBindingRepository` 的协同。
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
    session_hub: &crate::session::SessionHub,
    task: &Task,
) -> Result<SessionMeta, TaskExecutionError> {
    let title = format!("Task: {}", task.title.trim());
    session_hub
        .create_session(title.trim())
        .await
        .map_err(map_internal_error)
}

/// 清理 Task 的 session 绑定 — OneShot 模式完成或失败后调用。
///
/// 删除 SessionBinding(owner_type=task, label="execution")。执行器原生 resume id
/// 归属在 SessionMeta，不回写 Task。
pub async fn clear_task_session_binding(
    repos: &RepositorySet,
    task_id: Uuid,
    backend_id: &str,
    reason: &str,
) {
    let result: Result<(), DomainError> = async {
        use agentdash_domain::session_binding::SessionOwnerType;

        let Some(story) = repos.story_repo.find_by_task_id(task_id).await? else {
            return Ok(());
        };
        if story.find_task(task_id).is_none() {
            return Ok(());
        }

        let execution_binding = repos
            .session_binding_repo
            .find_by_owner_and_label(SessionOwnerType::Task, task_id, "execution")
            .await?;

        let cleared_session_id = execution_binding.as_ref().map(|b| b.session_id.clone());

        if cleared_session_id.is_none() {
            return Ok(());
        }

        if let Some(binding) = &execution_binding {
            repos
                .session_binding_repo
                .delete_by_session_and_owner(&binding.session_id, SessionOwnerType::Task, task_id)
                .await?;
        }

        let project_id = story.project_id;
        let story_id = story.id;
        repos
            .state_change_repo
            .append_change(
                project_id,
                task_id,
                ChangeKind::TaskUpdated,
                json!({
                    "reason": format!("session_cleared_{reason}"),
                    "task_id": task_id,
                    "project_id": project_id,
                    "story_id": story_id,
                    "cleared_session_id": cleared_session_id,
                }),
                Some(backend_id),
            )
            .await?;

        tracing::info!(
            task_id = %task_id,
            reason = reason,
            "已清理 Task session 绑定"
        );

        Ok(())
    }
    .await;

    if let Err(err) = result {
        tracing::warn!(
            task_id = %task_id,
            reason = reason,
            error = %err,
            "清理 Task session 绑定失败（不阻塞主流程）"
        );
    }
}
