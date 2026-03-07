use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use serde::Deserialize;
use uuid::Uuid;

use agentdash_domain::context_source::ContextSourceRef;
use agentdash_domain::story::{ChangeKind, Story, StoryPriority, StoryStatus, StoryType};
use agentdash_domain::task::{AgentBinding, Task, TaskStatus};

use crate::app_state::AppState;
use crate::rpc::ApiError;

#[derive(Deserialize)]
pub struct ListStoriesQuery {
    pub backend_id: Option<String>,
    pub project_id: Option<String>,
}

#[derive(Deserialize)]
pub struct CreateStoryRequest {
    pub project_id: String,
    pub backend_id: String,
    pub title: String,
    pub description: Option<String>,
    pub priority: Option<StoryPriority>,
    pub story_type: Option<StoryType>,
    pub tags: Option<Vec<String>>,
}

#[derive(Deserialize, Default)]
pub struct UpdateStoryRequest {
    pub title: Option<String>,
    pub description: Option<String>,
    pub backend_id: Option<String>,
    pub status: Option<StoryStatus>,
    pub priority: Option<StoryPriority>,
    pub story_type: Option<StoryType>,
    pub tags: Option<Vec<String>>,
    pub context_source_refs: Option<Vec<ContextSourceRef>>,
}

#[derive(Deserialize, Default)]
pub struct CreateTaskAgentBindingRequest {
    pub agent_type: Option<String>,
    pub agent_pid: Option<String>,
    pub preset_name: Option<String>,
    pub prompt_template: Option<String>,
    pub initial_context: Option<String>,
    pub context_sources: Option<Vec<ContextSourceRef>>,
}

#[derive(Deserialize)]
pub struct CreateTaskRequest {
    pub title: String,
    pub description: Option<String>,
    pub workspace_id: Option<String>,
    pub agent_binding: Option<CreateTaskAgentBindingRequest>,
}

#[derive(Deserialize, Default)]
pub struct UpdateTaskRequest {
    pub title: Option<String>,
    pub description: Option<String>,
    pub status: Option<TaskStatus>,
    pub workspace_id: Option<String>,
    pub agent_binding: Option<CreateTaskAgentBindingRequest>,
}

pub async fn list_stories(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ListStoriesQuery>,
) -> Result<Json<Vec<Story>>, ApiError> {
    if let Some(project_id) = &query.project_id {
        let pid = Uuid::parse_str(project_id)
            .map_err(|_| ApiError::BadRequest("无效的 Project ID".into()))?;
        let stories = state.story_repo.list_by_project(pid).await?;
        return Ok(Json(stories));
    }

    if let Some(backend_id) = &query.backend_id {
        let stories = state.story_repo.list_by_backend(backend_id).await?;
        return Ok(Json(stories));
    }

    Err(ApiError::BadRequest(
        "需要 backend_id 或 project_id 参数".into(),
    ))
}

pub async fn create_story(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateStoryRequest>,
) -> Result<Json<Story>, ApiError> {
    let project_id = Uuid::parse_str(&req.project_id)
        .map_err(|_| ApiError::BadRequest("无效的 Project ID".into()))?;
    let title = req.title.trim();
    if title.is_empty() {
        return Err(ApiError::BadRequest("Story 标题不能为空".into()));
    }
    let backend_id = req.backend_id.trim();
    if backend_id.is_empty() {
        return Err(ApiError::BadRequest("backend_id 不能为空".into()));
    }

    let story = Story::new(
        project_id,
        backend_id.to_string(),
        title.to_string(),
        req.description.unwrap_or_default(),
    );
    let mut next_story = story;
    if let Some(priority) = req.priority {
        next_story.priority = priority;
    }
    if let Some(story_type) = req.story_type {
        next_story.story_type = story_type;
    }
    if let Some(tags) = req.tags {
        next_story.tags = normalize_tags(tags);
    }

    state.story_repo.create(&next_story).await?;
    Ok(Json(next_story))
}

pub async fn get_story(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Story>, ApiError> {
    let story_id =
        Uuid::parse_str(&id).map_err(|_| ApiError::BadRequest("无效的 Story ID".into()))?;

    let story = state
        .story_repo
        .get_by_id(story_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Story {id} 不存在")))?;

    Ok(Json(story))
}

pub async fn update_story(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<UpdateStoryRequest>,
) -> Result<Json<Story>, ApiError> {
    let story_id =
        Uuid::parse_str(&id).map_err(|_| ApiError::BadRequest("无效的 Story ID".into()))?;

    let mut story = state
        .story_repo
        .get_by_id(story_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Story {id} 不存在")))?;

    if let Some(title) = req.title {
        let trimmed = title.trim();
        if trimmed.is_empty() {
            return Err(ApiError::BadRequest("Story 标题不能为空".into()));
        }
        story.title = trimmed.to_string();
    }
    if let Some(description) = req.description {
        story.description = description;
    }
    if let Some(backend_id) = req.backend_id {
        let trimmed = backend_id.trim();
        if trimmed.is_empty() {
            return Err(ApiError::BadRequest("backend_id 不能为空".into()));
        }
        story.backend_id = trimmed.to_string();
    }
    if let Some(status) = req.status {
        story.status = status;
    }
    if let Some(priority) = req.priority {
        story.priority = priority;
    }
    if let Some(story_type) = req.story_type {
        story.story_type = story_type;
    }
    if let Some(tags) = req.tags {
        story.tags = normalize_tags(tags);
    }
    if let Some(context_source_refs) = req.context_source_refs {
        story.context.source_refs = context_source_refs;
    }

    state.story_repo.update(&story).await?;
    Ok(Json(story))
}

pub async fn delete_story(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let story_id =
        Uuid::parse_str(&id).map_err(|_| ApiError::BadRequest("无效的 Story ID".into()))?;

    let story = state
        .story_repo
        .get_by_id(story_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Story {id} 不存在")))?;

    let tasks = state.task_repo.list_by_story(story_id).await?;
    for task in &tasks {
        state.task_repo.delete(task.id).await?;
        state
            .story_repo
            .append_change(
                task.id,
                ChangeKind::TaskDeleted,
                serde_json::json!({
                    "task_id": task.id,
                    "story_id": story_id,
                    "reason": "cascade_delete_with_story"
                }),
                &story.backend_id,
            )
            .await
            .ok();
    }

    state.story_repo.delete(story_id).await?;

    state
        .story_repo
        .append_change(
            story_id,
            ChangeKind::StoryDeleted,
            serde_json::json!({
                "story_id": story_id,
                "reason": "story_deleted_by_user"
            }),
            &story.backend_id,
        )
        .await
        .ok();

    Ok(Json(serde_json::json!({ "deleted": id })))
}

pub async fn list_tasks(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Vec<Task>>, ApiError> {
    let story_id =
        Uuid::parse_str(&id).map_err(|_| ApiError::BadRequest("无效的 Story ID".into()))?;
    let tasks = state.task_repo.list_by_story(story_id).await?;
    Ok(Json(tasks))
}

pub async fn create_task(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<CreateTaskRequest>,
) -> Result<Json<Task>, ApiError> {
    let story_id =
        Uuid::parse_str(&id).map_err(|_| ApiError::BadRequest("无效的 Story ID".into()))?;

    let title = req.title.trim();
    if title.is_empty() {
        return Err(ApiError::BadRequest("Task 标题不能为空".into()));
    }

    let story = state
        .story_repo
        .get_by_id(story_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Story {id} 不存在")))?;

    let project = state
        .project_repo
        .get_by_id(story.project_id)
        .await?
        .ok_or_else(|| {
            ApiError::NotFound(format!("Story 所属 Project {} 不存在", story.project_id))
        })?;

    let workspace_id = match req.workspace_id.as_deref() {
        Some(raw) if !raw.trim().is_empty() => {
            let ws_id = Uuid::parse_str(raw.trim())
                .map_err(|_| ApiError::BadRequest("无效的 Workspace ID".into()))?;

            let workspace = state
                .workspace_repo
                .get_by_id(ws_id)
                .await?
                .ok_or_else(|| ApiError::NotFound(format!("Workspace {ws_id} 不存在")))?;

            if workspace.project_id != story.project_id {
                return Err(ApiError::Conflict(
                    "Workspace 与 Story 不属于同一 Project".into(),
                ));
            }

            Some(ws_id)
        }
        _ => None,
    };

    let mut agent_binding = to_agent_binding(req.agent_binding);

    if let Some(preset_name) = agent_binding.preset_name.clone() {
        let preset = project
            .config
            .agent_presets
            .iter()
            .find(|p| p.name == preset_name)
            .ok_or_else(|| ApiError::BadRequest(format!("Project 中不存在预设: {preset_name}")))?;

        if agent_binding.agent_type.is_none() {
            agent_binding.agent_type = Some(preset.agent_type.clone());
        }
    }

    if agent_binding.agent_type.is_none() {
        agent_binding.agent_type = project.config.default_agent_type.clone();
    }

    if agent_binding.agent_type.is_none() && agent_binding.preset_name.is_none() {
        return Err(ApiError::UnprocessableEntity(
            "请指定 Agent 类型或预设，或在 Project 配置中设置 default_agent_type".into(),
        ));
    }

    let mut task = Task::new(
        story_id,
        title.to_string(),
        req.description.unwrap_or_default(),
    );
    task.workspace_id = workspace_id;
    task.agent_binding = agent_binding;

    state
        .sqlite_task_repo
        .create_task_with_story_update(&task)
        .await?;

    Ok(Json(task))
}

pub async fn get_task(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Task>, ApiError> {
    let task_id =
        Uuid::parse_str(&id).map_err(|_| ApiError::BadRequest("无效的 Task ID".into()))?;

    let task = state
        .task_repo
        .get_by_id(task_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Task {id} 不存在")))?;

    Ok(Json(task))
}

pub async fn update_task(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<UpdateTaskRequest>,
) -> Result<Json<Task>, ApiError> {
    let task_id =
        Uuid::parse_str(&id).map_err(|_| ApiError::BadRequest("无效的 Task ID".into()))?;

    let mut task = state
        .task_repo
        .get_by_id(task_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Task {id} 不存在")))?;

    let old_status = task.status.clone();

    let story = state
        .story_repo
        .get_by_id(task.story_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Task 所属 Story {} 不存在", task.story_id)))?;

    if let Some(title) = req.title {
        let trimmed = title.trim();
        if trimmed.is_empty() {
            return Err(ApiError::BadRequest("Task 标题不能为空".into()));
        }
        task.title = trimmed.to_string();
    }
    if let Some(description) = req.description {
        task.description = description;
    }
    if let Some(status) = req.status {
        task.status = status;
    }

    if let Some(workspace_id_raw) = req.workspace_id {
        let normalized = workspace_id_raw.trim();
        task.workspace_id = if normalized.is_empty() {
            None
        } else {
            let ws_id = Uuid::parse_str(normalized)
                .map_err(|_| ApiError::BadRequest("无效的 Workspace ID".into()))?;
            let workspace = state
                .workspace_repo
                .get_by_id(ws_id)
                .await?
                .ok_or_else(|| ApiError::NotFound(format!("Workspace {ws_id} 不存在")))?;
            if workspace.project_id != story.project_id {
                return Err(ApiError::Conflict(
                    "Workspace 与 Task 所属 Story 不属于同一 Project".into(),
                ));
            }
            Some(ws_id)
        };
    }

    if let Some(agent_binding_req) = req.agent_binding {
        task.agent_binding = to_agent_binding(Some(agent_binding_req));
    }

    state.task_repo.update(&task).await?;

    let change_kind = if task.status != old_status {
        ChangeKind::TaskStatusChanged
    } else {
        ChangeKind::TaskUpdated
    };
    state
        .story_repo
        .append_change(
            task.id,
            change_kind,
            serde_json::to_value(&task).unwrap_or_default(),
            &story.backend_id,
        )
        .await
        .ok();

    Ok(Json(task))
}

pub async fn delete_task(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let task_id =
        Uuid::parse_str(&id).map_err(|_| ApiError::BadRequest("无效的 Task ID".into()))?;

    state
        .sqlite_task_repo
        .delete_task_with_story_update(task_id)
        .await?;

    Ok(Json(serde_json::json!({ "deleted": id })))
}

fn normalize_option(value: Option<String>) -> Option<String> {
    value.and_then(|v| {
        let trimmed = v.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn normalize_tags(tags: Vec<String>) -> Vec<String> {
    tags.into_iter()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

fn to_agent_binding(input: Option<CreateTaskAgentBindingRequest>) -> AgentBinding {
    if let Some(value) = input {
        AgentBinding {
            agent_type: normalize_option(value.agent_type),
            agent_pid: normalize_option(value.agent_pid),
            preset_name: normalize_option(value.preset_name),
            prompt_template: normalize_option(value.prompt_template),
            initial_context: normalize_option(value.initial_context),
            context_sources: value.context_sources.unwrap_or_default(),
        }
    } else {
        AgentBinding::default()
    }
}
