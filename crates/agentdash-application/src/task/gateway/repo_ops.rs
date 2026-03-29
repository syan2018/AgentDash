use serde_json::{Map, Value, json};
use uuid::Uuid;

use agentdash_domain::project::Project;
use agentdash_domain::session_binding::SessionOwnerType;
use agentdash_domain::story::{ChangeKind, Story};
use agentdash_domain::task::{Artifact, ArtifactType, Task, TaskStatus};
use agentdash_domain::workspace::Workspace;
use agentdash_domain::DomainError;
use agentdash_spi::ConnectorError;
use crate::session::SessionMeta;

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

pub async fn get_task(
    repos: &RepositorySet,
    task_id: Uuid,
) -> Result<Task, TaskExecutionError> {
    repos
        .task_repo
        .get_by_id(task_id)
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
    let task = repos
        .task_repo
        .get_by_id(task_id)
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
        .story_repo
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
    let mut task = match repos.task_repo.get_by_id(task_id).await? {
        Some(task) => task,
        None => return Ok(false),
    };

    if task.status == next_status {
        return Ok(false);
    }

    let previous_status = task.status.clone();
    task.status = next_status.clone();
    repos.task_repo.update(&task).await?;

    append_task_change(
        repos,
        task.id,
        backend_id,
        ChangeKind::TaskStatusChanged,
        json!({
            "reason": reason,
            "task_id": task.id,
            "story_id": task.story_id,
            "session_id": task.session_id,
            "executor_session_id": task.executor_session_id,
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
    let mut task = match repos.task_repo.get_by_id(input.task_id).await? {
        Some(task) => task,
        None => return Ok(()),
    };

    let changed = upsert_tool_execution_artifact(
        &mut task, input.session_id, input.turn_id, input.tool_call_id, input.patch,
    );
    if !changed {
        return Ok(());
    }

    repos.task_repo.update(&task).await?;
    append_task_change(
        repos,
        task.id,
        input.backend_id,
        ChangeKind::TaskArtifactAdded,
        json!({
            "reason": input.reason,
            "task_id": task.id,
            "story_id": task.story_id,
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
    let mut task = match repos.task_repo.get_by_id(task_id).await? {
        Some(task) => task,
        None => return Ok(()),
    };

    task.artifacts.push(Artifact {
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
    });

    repos.task_repo.update(&task).await?;
    append_task_change(
        repos,
        task.id,
        backend_id,
        ChangeKind::TaskArtifactAdded,
        json!({
            "reason": "turn_failed_error_summary",
            "task_id": task.id,
            "story_id": task.story_id,
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
    let Some(mut task) = repos.task_repo.get_by_id(task_id).await? else {
        return Ok(());
    };

    if task.executor_session_id.as_deref() == Some(executor_session_id) {
        return Ok(());
    }

    task.executor_session_id = Some(executor_session_id.to_string());
    repos.task_repo.update(&task).await?;

    append_task_change(
        repos,
        task.id,
        backend_id,
        ChangeKind::TaskUpdated,
        json!({
            "reason": "executor_session_bound",
            "task_id": task.id,
            "story_id": task.story_id,
            "session_id": session_id,
            "turn_id": turn_id,
            "executor_session_id": executor_session_id,
        }),
    )
    .await?;

    Ok(())
}

/// 清理 Task 的 session 绑定 — OneShot 模式完成或失败后调用
pub async fn clear_task_session_binding(
    repos: &RepositorySet,
    task_id: Uuid,
    backend_id: &str,
    reason: &str,
) {
    let result: Result<(), DomainError> = async {
        let mut task = match repos.task_repo.get_by_id(task_id).await? {
            Some(task) => task,
            None => return Ok(()),
        };

        let cleared_session_id = task.session_id.take();
        let cleared_executor_session_id = task.executor_session_id.take();

        if cleared_session_id.is_none() && cleared_executor_session_id.is_none() {
            return Ok(());
        }

        repos.task_repo.update(&task).await?;
        repos
            .story_repo
            .append_change(
                task.project_id,
                task.id,
                ChangeKind::TaskUpdated,
                json!({
                    "reason": format!("session_cleared_{reason}"),
                    "task_id": task.id,
                    "project_id": task.project_id,
                    "story_id": task.story_id,
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
        SessionOwnerType::Task => repos
            .task_repo
            .get_by_id(owner_id)
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
