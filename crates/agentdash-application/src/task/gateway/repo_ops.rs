//! Task 聚合的核心 Repository 操作。
//!
//! 职责：面向 Story aggregate（Task child entity）的查询与命令型写入，
//! 包括状态 force set 与 StateChange 追加。
//!
//! 不在本文件范围：
//! - 错误映射 helpers → [`super::errors`]
//! - session 绑定桥接   → [`super::session_bridge`]
//! - workspace / backend / scope 解析 → [`super::resolve`]
//! - artifact 持久化   → [`super::artifact_ops`]
//! - ACP meta / overview 桥接 → [`super::meta_bridge`]

use serde_json::{Value, json};
use uuid::Uuid;

use agentdash_domain::DomainError;
use agentdash_domain::story::ChangeKind;
use agentdash_domain::task::{Task, TaskStatus};

use crate::repository_set::RepositorySet;
use crate::task::execution::TaskExecutionError;

use super::errors::map_domain_error;

pub async fn get_task(repos: &RepositorySet, task_id: Uuid) -> Result<Task, TaskExecutionError> {
    crate::task::load_task(repos.story_repo.as_ref(), task_id)
        .await
        .map_err(map_domain_error)?
        .ok_or_else(|| TaskExecutionError::NotFound(format!("Task {task_id} 不存在")))
}

pub async fn append_task_change(
    repos: &RepositorySet,
    task_id: Uuid,
    backend_id: &str,
    kind: ChangeKind,
    payload: Value,
) -> Result<(), DomainError> {
    let task = crate::task::load_task(repos.story_repo.as_ref(), task_id)
        .await?
        .ok_or_else(|| DomainError::NotFound {
            entity: "task",
            id: task_id.to_string(),
        })?;
    let backend_id_opt = if backend_id.trim().is_empty() {
        None
    } else {
        Some(backend_id)
    };
    repos
        .state_change_repo
        .append_change(task.project_id, task_id, kind, payload, backend_id_opt)
        .await
}

pub async fn update_task_status(
    repos: &RepositorySet,
    task_id: Uuid,
    backend_id: &str,
    next_status: TaskStatus,
    reason: &str,
    context: Value,
) -> Result<bool, DomainError> {
    let Some(mut story) = repos.story_repo.find_by_task_id(task_id).await? else {
        return Ok(false);
    };

    let Some(previous_task) = story.find_task(task_id).cloned() else {
        return Ok(false);
    };

    if previous_task.status() == &next_status {
        return Ok(false);
    }

    let previous_status = previous_task.status().clone();
    // M2：命令型状态写入走 `force_set_task_status`（非 projector 路径）；
    // runtime 真相由 LifecycleRunService → projector 产出。
    story.force_set_task_status(task_id, next_status.clone());
    repos.story_repo.update(&story).await?;

    append_task_change(
        repos,
        previous_task.id,
        backend_id,
        ChangeKind::TaskStatusChanged,
        json!({
            "reason": reason,
            "task_id": previous_task.id,
            "story_id": previous_task.story_id,
            "from": previous_status,
            "to": next_status,
            "context": context,
        }),
    )
    .await?;

    Ok(true)
}
