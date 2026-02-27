use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use serde::Deserialize;
use uuid::Uuid;

use agentdash_domain::story::Story;
use agentdash_domain::task::Task;

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

    Err(ApiError::BadRequest("需要 backend_id 或 project_id 参数".into()))
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
    let story_id = Uuid::parse_str(&id)
        .map_err(|_| ApiError::BadRequest("无效的 Story ID".into()))?;

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
    let story_id = Uuid::parse_str(&id).map_err(|_| ApiError::BadRequest("无效的 Story ID".into()))?;
    let tasks = state.task_repo.list_by_story(story_id).await?;
    Ok(Json(tasks))
}
