use serde_json::{Map, Value, json};
use uuid::Uuid;

use agent_client_protocol::{SessionId, SessionInfoUpdate, SessionNotification, SessionUpdate};

use crate::session::SessionMeta;
use crate::task::meta::build_task_lifecycle_meta;
use agentdash_domain::DomainError;
use agentdash_domain::project::Project;
use agentdash_domain::session_binding::SessionOwnerType;
use agentdash_domain::story::{ChangeKind, Story};
use agentdash_domain::task::{Artifact, ArtifactType, Task, TaskStatus};
use agentdash_domain::workspace::Workspace;
use agentdash_spi::ConnectorError;

use crate::repository_set::RepositorySet;
use crate::task::artifact::upsert_tool_execution_artifact;
use crate::task::execution::TaskExecutionError;
use crate::workspace::{BackendAvailability, resolve_workspace_binding};

// ─── error mappers ──────────────────────────────────────────

pub fn map_domain_error(err: DomainError) -> TaskExecutionError {
    match &err {
        DomainError::NotFound { .. } => TaskExecutionError::NotFound(err.to_string()),
        DomainError::InvalidTransition { .. } => TaskExecutionError::BadRequest(err.to_string()),
        DomainError::InvalidConfig(_) => TaskExecutionError::BadRequest(err.to_string()),
        _ => TaskExecutionError::Internal(err.to_string()),
    }
}

pub fn map_internal_error<E: ToString>(err: E) -> TaskExecutionError {
    TaskExecutionError::Internal(err.to_string())
}

pub fn map_connector_error(err: ConnectorError) -> TaskExecutionError {
    match err {
        ConnectorError::InvalidConfig(message) => TaskExecutionError::BadRequest(message),
        ConnectorError::Runtime(message) => TaskExecutionError::Conflict(message),
        other => TaskExecutionError::Internal(other.to_string()),
    }
}

pub fn normalize_backend_id(raw: &str) -> Result<&str, TaskExecutionError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(TaskExecutionError::BadRequest(
            "backend_id 不能为空".to_string(),
        ));
    }
    Ok(trimmed)
}

// ─── repo-based data operations ─────────────────────────────

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

    if previous_task.status == next_status {
        return Ok(false);
    }

    let previous_status = previous_task.status.clone();
    story.update_task(task_id, |task| {
        task.status = next_status.clone();
    });
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
            "executor_session_id": previous_task.executor_session_id,
            "from": previous_status,
            "to": next_status,
            "context": context,
        }),
    )
    .await?;

    Ok(true)
}

pub struct ToolCallArtifactInput<'a> {
    pub task_id: Uuid,
    pub session_id: &'a str,
    pub turn_id: &'a str,
    pub tool_call_id: &'a str,
    pub patch: Map<String, Value>,
    pub backend_id: &'a str,
    pub reason: &'a str,
}

pub async fn persist_tool_call_artifact(
    repos: &RepositorySet,
    input: ToolCallArtifactInput<'_>,
) -> Result<(), DomainError> {
    let Some(mut story) = repos.story_repo.find_by_task_id(input.task_id).await? else {
        return Ok(());
    };
    let Some(task_snapshot) = story.find_task(input.task_id).cloned() else {
        return Ok(());
    };

    let mut updated_task = task_snapshot.clone();
    let changed = upsert_tool_execution_artifact(
        &mut updated_task,
        input.session_id,
        input.turn_id,
        input.tool_call_id,
        input.patch,
    )?;
    if !changed {
        return Ok(());
    }

    story.update_task(input.task_id, |task| {
        *task = updated_task.clone();
    });
    repos.story_repo.update(&story).await?;
    append_task_change(
        repos,
        updated_task.id,
        input.backend_id,
        ChangeKind::TaskArtifactAdded,
        json!({
            "reason": input.reason,
            "task_id": updated_task.id,
            "story_id": updated_task.story_id,
            "session_id": input.session_id,
            "turn_id": input.turn_id,
            "tool_call_id": input.tool_call_id,
            "artifact_type": "tool_execution",
        }),
    )
    .await?;

    Ok(())
}

pub async fn persist_turn_failure_artifact(
    repos: &RepositorySet,
    task_id: Uuid,
    backend_id: &str,
    session_id: &str,
    turn_id: &str,
    error_message: &str,
) -> Result<(), DomainError> {
    let Some(mut story) = repos.story_repo.find_by_task_id(task_id).await? else {
        return Ok(());
    };
    if story.find_task(task_id).is_none() {
        return Ok(());
    }

    let new_artifact = Artifact {
        id: Uuid::new_v4(),
        artifact_type: ArtifactType::LogOutput,
        content: json!({
            "kind": "turn_error",
            "session_id": session_id,
            "turn_id": turn_id,
            "message": error_message,
            "created_at": chrono::Utc::now().to_rfc3339(),
        }),
        created_at: chrono::Utc::now(),
    };
    story.update_task(task_id, |task| {
        task.artifacts.push(new_artifact.clone());
    });
    let (story_id, project_id) = (story.id, story.project_id);
    let _ = (story_id, project_id);
    repos.story_repo.update(&story).await?;
    append_task_change(
        repos,
        task_id,
        backend_id,
        ChangeKind::TaskArtifactAdded,
        json!({
            "reason": "turn_failed_error_summary",
            "task_id": task_id,
            "story_id": story.id,
            "session_id": session_id,
            "turn_id": turn_id,
            "artifact_type": "log_output",
        }),
    )
    .await?;

    Ok(())
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
        task.executor_session_id = Some(executor_session_id.to_string());
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
            task.executor_session_id = None;
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

pub async fn load_related_context(
    repos: &RepositorySet,
    task: &Task,
) -> Result<(Story, Project, Option<Workspace>), TaskExecutionError> {
    let story = repos
        .story_repo
        .get_by_id(task.story_id)
        .await
        .map_err(map_domain_error)?
        .ok_or_else(|| {
            TaskExecutionError::NotFound(format!("Task 所属 Story {} 不存在", task.story_id))
        })?;

    let project = repos
        .project_repo
        .get_by_id(task.project_id)
        .await
        .map_err(map_domain_error)?
        .ok_or_else(|| {
            TaskExecutionError::NotFound(format!("Task 所属 Project {} 不存在", task.project_id))
        })?;

    let workspace = resolve_effective_task_workspace(repos, task, &story, &project).await?;

    Ok((story, project, workspace))
}

pub async fn resolve_effective_task_workspace(
    repos: &RepositorySet,
    task: &Task,
    story: &Story,
    project: &Project,
) -> Result<Option<Workspace>, TaskExecutionError> {
    if let Some(workspace_id) = task.workspace_id {
        return repos
            .workspace_repo
            .get_by_id(workspace_id)
            .await
            .map_err(map_domain_error)?
            .ok_or_else(|| {
                TaskExecutionError::NotFound(format!("Task 关联 Workspace {workspace_id} 不存在"))
            })
            .map(Some);
    }

    if let Some(default_ws_id) = story.default_workspace_id {
        return repos
            .workspace_repo
            .get_by_id(default_ws_id)
            .await
            .map_err(map_domain_error)?
            .ok_or_else(|| {
                TaskExecutionError::NotFound(format!("Story 默认 Workspace {default_ws_id} 不存在"))
            })
            .map(Some);
    }

    if let Some(default_ws_id) = project.config.default_workspace_id {
        return repos
            .workspace_repo
            .get_by_id(default_ws_id)
            .await
            .map_err(map_domain_error)?
            .ok_or_else(|| {
                TaskExecutionError::NotFound(format!(
                    "Project 默认 Workspace {default_ws_id} 不存在"
                ))
            })
            .map(Some);
    }

    Ok(None)
}

pub async fn resolve_task_backend_id(
    repos: &RepositorySet,
    availability: &dyn BackendAvailability,
    task: &Task,
) -> Result<String, TaskExecutionError> {
    let story = repos
        .story_repo
        .get_by_id(task.story_id)
        .await
        .map_err(map_domain_error)?
        .ok_or_else(|| TaskExecutionError::NotFound(format!("Story {} 不存在", task.story_id)))?;

    let project = repos
        .project_repo
        .get_by_id(story.project_id)
        .await
        .map_err(map_domain_error)?
        .ok_or_else(|| {
            TaskExecutionError::NotFound(format!("Story 所属 Project {} 不存在", story.project_id))
        })?;

    let workspace = resolve_effective_task_workspace(repos, task, &story, &project)
        .await?
        .ok_or_else(|| {
            TaskExecutionError::BadRequest(
                "Task 执行需要绑定 Workspace：请为 Task、Story 或 Project 配置默认 Workspace"
                    .to_string(),
            )
        })?;
    let binding = resolve_workspace_binding(availability, &workspace)
        .await
        .map_err(map_internal_error)?;
    if let Ok(bid) = normalize_backend_id(&binding.backend_id) {
        return Ok(bid.to_string());
    }

    Err(TaskExecutionError::BadRequest(
        "Task 执行需要绑定 Workspace：请为 Task、Story 或 Project 配置默认 Workspace".to_string(),
    ))
}

pub async fn resolve_project_scope_for_owner(
    repos: &RepositorySet,
    owner_type: SessionOwnerType,
    owner_id: Uuid,
) -> Result<Uuid, TaskExecutionError> {
    match owner_type {
        SessionOwnerType::Project => Ok(owner_id),
        SessionOwnerType::Story => repos
            .story_repo
            .get_by_id(owner_id)
            .await
            .map_err(map_domain_error)?
            .map(|story| story.project_id)
            .ok_or_else(|| TaskExecutionError::NotFound(format!("Story {owner_id} 不存在"))),
        SessionOwnerType::Task => crate::task::load_task(repos.story_repo.as_ref(), owner_id)
            .await
            .map_err(map_domain_error)?
            .map(|task| task.project_id)
            .ok_or_else(|| TaskExecutionError::NotFound(format!("Task {owner_id} 不存在"))),
    }
}

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

pub fn bridge_task_status_event_to_session_notification(
    session_id: &str,
    turn_id: &str,
    event_type: &str,
    message: &str,
    data: Value,
) -> SessionNotification {
    let meta = build_task_lifecycle_meta(turn_id, event_type, message, data);
    SessionNotification::new(
        SessionId::new(session_id.to_string()),
        SessionUpdate::SessionInfoUpdate(SessionInfoUpdate::new().meta(meta)),
    )
}

pub async fn get_session_overview(
    session_hub: &crate::session::SessionHub,
    session_id: &str,
) -> Result<Option<crate::task::execution::SessionOverview>, TaskExecutionError> {
    let meta = session_hub
        .get_session_meta(session_id)
        .await
        .map_err(map_internal_error)?;
    Ok(meta.map(|value| crate::task::execution::SessionOverview {
        title: value.title,
        updated_at: value.updated_at,
    }))
}
