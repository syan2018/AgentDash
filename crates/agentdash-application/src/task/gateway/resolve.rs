//! 解析族 helpers：workspace / backend / project scope。
//!
//! 职责：把 Task 执行所需的上下文（Story、Project、Workspace、backend_id 等）
//! 从 Repository 聚合解析出来。纯查询，不做状态写入。

use uuid::Uuid;

use agentdash_domain::project::Project;
use agentdash_domain::session_binding::SessionOwnerType;
use agentdash_domain::story::Story;
use agentdash_domain::task::Task;
use agentdash_domain::workspace::Workspace;

use crate::repository_set::RepositorySet;
use crate::task::execution::TaskExecutionError;
use crate::workspace::{BackendAvailability, resolve_workspace_binding};

use super::errors::{map_domain_error, map_internal_error};

pub fn normalize_backend_id(raw: &str) -> Result<&str, TaskExecutionError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(TaskExecutionError::BadRequest(
            "backend_id 不能为空".to_string(),
        ));
    }
    Ok(trimmed)
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
