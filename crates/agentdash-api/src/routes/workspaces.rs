use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path as AxumPath, State};
use git2::{ErrorCode, Repository};
use rfd::FileDialog;
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
}

#[derive(Deserialize)]
pub struct UpdateWorkspaceStatusRequest {
    pub status: WorkspaceStatus,
}

#[derive(Deserialize)]
pub struct UpdateWorkspaceRequest {
    pub name: Option<String>,
    pub container_ref: Option<String>,
    pub workspace_type: Option<WorkspaceType>,
}

#[derive(Deserialize)]
pub struct DetectGitRequest {
    pub container_ref: String,
}

#[derive(Debug, Deserialize)]
pub struct PickDirectoryRequest {
    pub initial_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PickDirectoryResponse {
    pub selected: bool,
    pub container_ref: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DetectGitResponse {
    pub resolved_container_ref: String,
    pub is_git_repo: bool,
    pub source_repo: Option<String>,
    pub branch: Option<String>,
    pub commit_hash: Option<String>,
}

#[derive(Debug)]
struct GitDetectionResult {
    is_git_repo: bool,
    source_repo: Option<String>,
    branch: Option<String>,
    commit_hash: Option<String>,
}

pub async fn list_workspaces(
    State(state): State<Arc<AppState>>,
    AxumPath(project_id): AxumPath<String>,
) -> Result<Json<Vec<Workspace>>, ApiError> {
    let project_id = Uuid::parse_str(&project_id)
        .map_err(|_| ApiError::BadRequest("无效的 Project ID".into()))?;

    let workspaces = state.workspace_repo.list_by_project(project_id).await?;
    Ok(Json(workspaces))
}

pub async fn create_workspace(
    State(state): State<Arc<AppState>>,
    AxumPath(project_id): AxumPath<String>,
    Json(req): Json<CreateWorkspaceRequest>,
) -> Result<Json<Workspace>, ApiError> {
    let workspace_name = req.name.trim().to_string();
    if workspace_name.is_empty() {
        return Err(ApiError::BadRequest("工作空间名称不能为空".into()));
    }

    let project_id = Uuid::parse_str(&project_id)
        .map_err(|_| ApiError::BadRequest("无效的 Project ID".into()))?;

    // 校验 Project 存在
    state
        .project_repo
        .get_by_id(project_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Project {project_id} 不存在")))?;

    let ws_type = req.workspace_type.unwrap_or(WorkspaceType::GitWorktree);
    let (container_ref, git_config) =
        resolve_container_and_git(req.container_ref, &ws_type).await?;

    let mut workspace = Workspace::new(project_id, workspace_name, container_ref, ws_type);
    workspace.git_config = git_config;

    state.workspace_repo.create(&workspace).await?;
    auto_transition_workspace_status(&state, workspace.id).await?;
    workspace.status = WorkspaceStatus::Ready;

    Ok(Json(workspace))
}

pub async fn get_workspace(
    State(state): State<Arc<AppState>>,
    AxumPath(id): AxumPath<String>,
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

pub async fn update_workspace(
    State(state): State<Arc<AppState>>,
    AxumPath(id): AxumPath<String>,
    Json(req): Json<UpdateWorkspaceRequest>,
) -> Result<Json<Workspace>, ApiError> {
    let workspace_id =
        Uuid::parse_str(&id).map_err(|_| ApiError::BadRequest("无效的 Workspace ID".into()))?;

    let mut workspace = state
        .workspace_repo
        .get_by_id(workspace_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Workspace {id} 不存在")))?;

    if let Some(name) = req.name {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(ApiError::BadRequest("工作空间名称不能为空".into()));
        }
        workspace.name = trimmed.to_string();
    }

    let next_type = req.workspace_type.unwrap_or_else(|| workspace.workspace_type.clone());
    let next_container_ref = req
        .container_ref
        .as_ref()
        .map(|value| value.trim().to_string())
        .unwrap_or_else(|| workspace.container_ref.clone());

    let container_input = if next_container_ref.is_empty() {
        None
    } else {
        Some(next_container_ref)
    };
    let (resolved_container_ref, resolved_git_config) =
        resolve_container_and_git(container_input, &next_type).await?;

    workspace.workspace_type = next_type;
    workspace.container_ref = resolved_container_ref;
    workspace.git_config = resolved_git_config;

    state.workspace_repo.update(&workspace).await?;
    Ok(Json(workspace))
}

pub async fn update_workspace_status(
    State(state): State<Arc<AppState>>,
    AxumPath(id): AxumPath<String>,
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
    AxumPath(id): AxumPath<String>,
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

    let canonical_path = canonicalize_container_ref(container_ref)?;

    let normalized_ref = normalize_display_path(&canonical_path);
    let result = tokio::task::spawn_blocking(move || detect_git_impl(canonical_path))
        .await
        .map_err(|e| ApiError::Internal(format!("Git 识别任务异常: {e}")))??;

    Ok(Json(DetectGitResponse {
        resolved_container_ref: normalized_ref,
        is_git_repo: result.is_git_repo,
        source_repo: result.source_repo,
        branch: result.branch,
        commit_hash: result.commit_hash,
    }))
}

pub async fn pick_directory(
    Json(req): Json<PickDirectoryRequest>,
) -> Result<Json<PickDirectoryResponse>, ApiError> {
    let initial_path = req
        .initial_path
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    let picked_path = tokio::task::spawn_blocking(move || {
        let mut dialog = FileDialog::new();

        if let Some(path_str) = initial_path {
            let initial_dir = PathBuf::from(path_str);
            if initial_dir.exists() {
                dialog = dialog.set_directory(initial_dir);
            }
        }

        dialog.pick_folder()
    })
    .await
    .map_err(|err| ApiError::Internal(format!("目录选择任务异常: {err}")))?;

    match picked_path {
        Some(path) => {
            let normalized = fs::canonicalize(&path)
                .map(|value| normalize_display_path(&value))
                .unwrap_or_else(|_| normalize_display_path(&path));

            Ok(Json(PickDirectoryResponse {
                selected: true,
                container_ref: Some(normalized),
            }))
        }
        None => Ok(Json(PickDirectoryResponse {
            selected: false,
            container_ref: None,
        })),
    }
}

fn detect_git_impl(path: PathBuf) -> Result<GitDetectionResult, ApiError> {
    let repo = match Repository::discover(&path) {
        Ok(repo) => repo,
        Err(err) if err.code() == ErrorCode::NotFound => return Ok(non_git_response()),
        Err(err) => {
            return Err(ApiError::Internal(format!("Git 仓库识别失败: {err}")));
        }
    };

    let source_repo = resolve_source_repo(&repo);

    let head = repo.head().ok();
    let branch = head
        .as_ref()
        .and_then(|item| item.shorthand())
        .map(ToString::to_string)
        .filter(|value| !value.is_empty());
    let commit_hash = head
        .as_ref()
        .and_then(|item| item.target())
        .map(|oid| oid.to_string());

    Ok(GitDetectionResult {
        is_git_repo: true,
        source_repo,
        branch,
        commit_hash,
    })
}

async fn auto_transition_workspace_status(
    state: &Arc<AppState>,
    workspace_id: Uuid,
) -> Result<(), ApiError> {
    state
        .workspace_repo
        .update_status(workspace_id, WorkspaceStatus::Preparing)
        .await?;

    match state
        .workspace_repo
        .update_status(workspace_id, WorkspaceStatus::Ready)
        .await
    {
        Ok(()) => Ok(()),
        Err(err) => {
            let _ = state
                .workspace_repo
                .update_status(workspace_id, WorkspaceStatus::Error)
                .await;
            Err(err.into())
        }
    }
}

async fn resolve_container_and_git(
    container_ref: Option<String>,
    workspace_type: &WorkspaceType,
) -> Result<(String, Option<GitConfig>), ApiError> {
    let raw_ref = container_ref.unwrap_or_default();
    let trimmed_ref = raw_ref.trim();

    if trimmed_ref.is_empty() {
        if matches!(workspace_type, WorkspaceType::Ephemeral) {
            return Ok((String::new(), None));
        }
        return Err(ApiError::BadRequest("container_ref 不能为空".into()));
    }

    let canonical_path = canonicalize_container_ref(trimmed_ref)?;
    let normalized_ref = normalize_display_path(&canonical_path);

    let detected = tokio::task::spawn_blocking(move || detect_git_impl(canonical_path))
        .await
        .map_err(|e| ApiError::Internal(format!("Git 识别任务异常: {e}")))??;

    if matches!(workspace_type, WorkspaceType::GitWorktree) && !detected.is_git_repo {
        return Err(ApiError::BadRequest(format!(
            "目录不是 Git 仓库，无法创建 Git Worktree 类型工作空间: {normalized_ref}"
        )));
    }

    let git_config = if detected.is_git_repo {
        Some(GitConfig {
            source_repo: detected
                .source_repo
                .unwrap_or_else(|| normalized_ref.clone()),
            branch: detected.branch.unwrap_or_else(|| "HEAD".to_string()),
            commit_hash: detected.commit_hash,
        })
    } else {
        None
    };

    Ok((normalized_ref, git_config))
}

fn non_git_response() -> GitDetectionResult {
    GitDetectionResult {
        is_git_repo: false,
        source_repo: None,
        branch: None,
        commit_hash: None,
    }
}

fn canonicalize_container_ref(container_ref: &str) -> Result<PathBuf, ApiError> {
    let raw_path = PathBuf::from(container_ref);
    let metadata = fs::metadata(&raw_path)
        .map_err(|err| ApiError::BadRequest(format!("目录不存在或不可访问: {container_ref} ({err})")))?;
    if !metadata.is_dir() {
        return Err(ApiError::BadRequest(format!(
            "路径不是目录: {container_ref}"
        )));
    }

    fs::canonicalize(&raw_path).map_err(|err| {
        ApiError::BadRequest(format!("目录路径无法解析: {container_ref} ({err})"))
    })
}

fn resolve_source_repo(repo: &Repository) -> Option<String> {
    if let Ok(origin) = repo.find_remote("origin")
        && let Some(url) = origin.url()
    {
        let value = url.trim();
        if !value.is_empty() {
            return Some(value.to_string());
        }
    }

    repository_root(repo).map(normalize_display_path)
}

fn repository_root(repo: &Repository) -> Option<&Path> {
    repo.workdir().or_else(|| repo.path().parent())
}

fn normalize_display_path(path: &Path) -> String {
    let value = path.to_string_lossy().to_string();
    if cfg!(windows) {
        value.trim_start_matches(r"\\?\").to_string()
    } else {
        value
    }
}
