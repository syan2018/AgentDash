//! Task ↔ Session 绑定桥接。
//!
//! 职责：封装 Task executor session 绑定的创建 / 同步 / 清理流程，
//! 包括与 `SessionHub`、`SessionBindingRepository` 的协同。
//!
//! 注意：这里只负责把 "session 层的事实" 反映到 Task / Story 聚合与 StateChange 上，
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
use super::repo_ops::append_task_change;

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

pub async fn sync_task_executor_session_binding_from_hub(
    repos: &RepositorySet,
    session_hub: &crate::session::SessionHub,
    task_id: Uuid,
    backend_id: &str,
    session_id: &str,
    turn_id: &str,
) -> Result<(), DomainError> {
    let meta = match session_hub.get_session_meta(session_id).await {
        Ok(Some(meta)) => meta,
        Ok(None) => return Ok(()),
        Err(err) => {
            return Err(DomainError::InvalidConfig(err.to_string()));
        }
    };

    let Some(executor_session_id) = meta
        .executor_session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(());
    };

    bind_executor_session_id(
        repos,
        task_id,
        backend_id,
        session_id,
        turn_id,
        executor_session_id,
    )
    .await
}

pub async fn bind_executor_session_id(
    repos: &RepositorySet,
    task_id: Uuid,
    backend_id: &str,
    session_id: &str,
    turn_id: &str,
    executor_session_id: &str,
) -> Result<(), DomainError> {
    let Some(mut story) = repos.story_repo.find_by_task_id(task_id).await? else {
        return Ok(());
    };
    let Some(existing) = story.find_task(task_id) else {
        return Ok(());
    };
    if existing.executor_session_id.as_deref() == Some(executor_session_id) {
        return Ok(());
    }
    let story_id = story.id;

    story.update_task(task_id, |task| {
        *task.executor_session_id = Some(executor_session_id.to_string());
    });
    repos.story_repo.update(&story).await?;

    append_task_change(
        repos,
        task_id,
        backend_id,
        ChangeKind::TaskUpdated,
        json!({
            "reason": "executor_session_bound",
            "task_id": task_id,
            "story_id": story_id,
            "session_id": session_id,
            "turn_id": turn_id,
            "executor_session_id": executor_session_id,
        }),
    )
    .await?;

    Ok(())
}

/// 清理 Task 的 session 绑定 — OneShot 模式完成或失败后调用。
///
/// 删除 SessionBinding(owner_type=task, label="execution") 并清理 executor_session_id。
pub async fn clear_task_session_binding(
    repos: &RepositorySet,
    task_id: Uuid,
    backend_id: &str,
    reason: &str,
) {
    let result: Result<(), DomainError> = async {
        use agentdash_domain::session_binding::SessionOwnerType;

        let Some(mut story) = repos.story_repo.find_by_task_id(task_id).await? else {
            return Ok(());
        };
        let Some(existing_task) = story.find_task(task_id).cloned() else {
            return Ok(());
        };

        let execution_binding = repos
            .session_binding_repo
            .find_by_owner_and_label(SessionOwnerType::Task, task_id, "execution")
            .await?;

        let cleared_session_id = execution_binding.as_ref().map(|b| b.session_id.clone());
        let cleared_executor_session_id = existing_task.executor_session_id.clone();

        if cleared_session_id.is_none() && cleared_executor_session_id.is_none() {
            return Ok(());
        }

        if let Some(binding) = &execution_binding {
            repos
                .session_binding_repo
                .delete_by_session_and_owner(&binding.session_id, SessionOwnerType::Task, task_id)
                .await?;
        }

        story.update_task(task_id, |task| {
            *task.executor_session_id = None;
        });
        let project_id = story.project_id;
        let story_id = story.id;
        repos.story_repo.update(&story).await?;
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
                    "cleared_executor_session_id": cleared_executor_session_id,
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
