use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use agentdash_domain::workspace::{GitConfig, Workspace, WorkspaceStatus, WorkspaceType};

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

#[derive(Deserialize)]
pub struct DetectGitRequest {
    pub container_ref: String,
}

#[derive(Debug, Serialize)]
pub struct DetectGitResponse {
    pub is_git_repo: bool,
    pub source_repo: Option<String>,
    pub branch: Option<String>,
    pub commit_hash: Option<String>,
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
    let workspace_id =
        Uuid::parse_str(&id).map_err(|_| ApiError::BadRequest("无效的 Workspace ID".into()))?;

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
    let workspace_id =
        Uuid::parse_str(&id).map_err(|_| ApiError::BadRequest("无效的 Workspace ID".into()))?;

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
    let workspace_id =
        Uuid::parse_str(&id).map_err(|_| ApiError::BadRequest("无效的 Workspace ID".into()))?;

    state.workspace_repo.delete(workspace_id).await?;
    Ok(Json(serde_json::json!({ "deleted": id })))
}

pub async fn detect_git(
    Json(req): Json<DetectGitRequest>,
) -> Result<Json<DetectGitResponse>, ApiError> {
    let container_ref = req.container_ref.trim();
    if container_ref.is_empty() {
        return Err(ApiError::BadRequest("container_ref 不能为空".into()));
    }

    let path = PathBuf::from(container_ref);
    if !path.exists() || !path.is_dir() {
        return Err(ApiError::BadRequest(format!(
            "目录不存在或不可访问: {container_ref}"
        )));
    }

    let result = tokio::task::spawn_blocking(move || detect_git_impl(path))
        .await
        .map_err(|e| ApiError::Internal(format!("Git 识别任务异常: {e}")))?;

    Ok(Json(result))
}

fn detect_git_impl(path: PathBuf) -> DetectGitResponse {
    if !is_inside_git_repo(&path) {
        return DetectGitResponse {
            is_git_repo: false,
            source_repo: None,
            branch: None,
            commit_hash: None,
        };
    }

    let source_repo = run_git_command(&path, &["remote", "get-url", "origin"])
        .or_else(|| run_git_command(&path, &["rev-parse", "--show-toplevel"]));

    let branch = run_git_command(&path, &["branch", "--show-current"]);
    let commit_hash = run_git_command(&path, &["rev-parse", "HEAD"]);

    DetectGitResponse {
        is_git_repo: true,
        source_repo,
        branch,
        commit_hash,
    }
}

fn is_inside_git_repo(path: &PathBuf) -> bool {
    matches!(
        run_git_command(path, &["rev-parse", "--is-inside-work-tree"]).as_deref(),
        Some("true")
    )
}

fn run_git_command(path: &PathBuf, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if value.is_empty() { None } else { Some(value) }
}
