use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use serde::Deserialize;
use uuid::Uuid;

use agentdash_domain::workspace::{Workspace, WorkspaceType, WorkspaceStatus, GitConfig};

use crate::app_state::AppState;
use crate::rpc::ApiError;

#[derive(Deserialize)]
pub struct CreateWorkspaceRequest {
    pub name: String,
    pub container_ref: Option<String>,
    pub workspace_type: Option<WorkspaceType>,
    pub git_config: Option<GitConfig>,
}

#[derive(Deserialize)]
pub struct UpdateWorkspaceStatusRequest {
    pub status: WorkspaceStatus,
}

pub async fn list_workspaces(
    State(state): State<Arc<AppState>>,
    Path(project_id): Path<String>,
) -> Result<Json<Vec<Workspace>>, ApiError> {
    let project_id = Uuid::parse_str(&project_id)
        .map_err(|_| ApiError::BadRequest("无效的 Project ID".into()))?;

    let workspaces = state.workspace_repo.list_by_project(project_id).await?;
    Ok(Json(workspaces))
}

pub async fn create_workspace(
    State(state): State<Arc<AppState>>,
    Path(project_id): Path<String>,
    Json(req): Json<CreateWorkspaceRequest>,
) -> Result<Json<Workspace>, ApiError> {
    let project_id = Uuid::parse_str(&project_id)
        .map_err(|_| ApiError::BadRequest("无效的 Project ID".into()))?;

    // 校验 Project 存在
    state
        .project_repo
        .get_by_id(project_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Project {project_id} 不存在")))?;

    let ws_type = req.workspace_type.unwrap_or(WorkspaceType::GitWorktree);
    let container_ref = req.container_ref.unwrap_or_default();

    let mut workspace = Workspace::new(project_id, req.name, container_ref, ws_type);
    workspace.git_config = req.git_config;

    state.workspace_repo.create(&workspace).await?;
    Ok(Json(workspace))
}

pub async fn get_workspace(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Workspace>, ApiError> {
    let workspace_id = Uuid::parse_str(&id)
        .map_err(|_| ApiError::BadRequest("无效的 Workspace ID".into()))?;

    let workspace = state
        .workspace_repo
        .get_by_id(workspace_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Workspace {id} 不存在")))?;

    Ok(Json(workspace))
}

pub async fn update_workspace_status(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<UpdateWorkspaceStatusRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let workspace_id = Uuid::parse_str(&id)
        .map_err(|_| ApiError::BadRequest("无效的 Workspace ID".into()))?;

    state
        .workspace_repo
        .update_status(workspace_id, req.status)
        .await?;

    Ok(Json(serde_json::json!({ "updated": id })))
}

pub async fn delete_workspace(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let workspace_id = Uuid::parse_str(&id)
        .map_err(|_| ApiError::BadRequest("无效的 Workspace ID".into()))?;

    state.workspace_repo.delete(workspace_id).await?;
    Ok(Json(serde_json::json!({ "deleted": id })))
}
