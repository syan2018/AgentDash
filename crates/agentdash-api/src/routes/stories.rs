use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use serde::Deserialize;
use uuid::Uuid;

use agentdash_domain::story::Story;
use agentdash_domain::task::{AgentBinding, Task};

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
}

#[derive(Deserialize, Default)]
pub struct CreateTaskAgentBindingRequest {
    pub agent_type: Option<String>,
    pub agent_pid: Option<String>,
    pub preset_name: Option<String>,
}

#[derive(Deserialize)]
pub struct CreateTaskRequest {
    pub title: String,
    pub description: Option<String>,
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

    let story = Story::new(
        project_id,
        req.backend_id,
        req.title,
        req.description.unwrap_or_default(),
    );
    state.story_repo.create(&story).await?;
    Ok(Json(story))
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

    state.task_repo.create(&task).await?;
    Ok(Json(task))
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

fn to_agent_binding(input: Option<CreateTaskAgentBindingRequest>) -> AgentBinding {
    if let Some(value) = input {
        AgentBinding {
            agent_type: normalize_option(value.agent_type),
            agent_pid: normalize_option(value.agent_pid),
            preset_name: normalize_option(value.preset_name),
        }
    } else {
        AgentBinding::default()
    }
}
