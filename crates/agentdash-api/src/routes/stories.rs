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
    pub backend_id: String,
}

#[derive(Deserialize)]
pub struct CreateStoryRequest {
    pub backend_id: String,
    pub title: String,
    pub description: Option<String>,
}

pub async fn list_stories(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ListStoriesQuery>,
) -> Result<Json<Vec<Story>>, ApiError> {
    let stories = state.story_repo.list_by_backend(&query.backend_id).await?;
    Ok(Json(stories))
}

pub async fn create_story(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateStoryRequest>,
) -> Result<Json<Story>, ApiError> {
    let story = Story::new(
        req.backend_id,
        req.title,
        req.description.unwrap_or_default(),
    );
    state.story_repo.create(&story).await?;
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
