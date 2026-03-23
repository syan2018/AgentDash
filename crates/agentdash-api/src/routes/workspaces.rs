use std::sync::Arc;

use axum::Json;
use axum::extract::{Path as AxumPath, State};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use agentdash_domain::workspace::{GitConfig, Workspace, WorkspaceStatus, WorkspaceType};
use agentdash_relay::{CommandWorkspaceDetectGitPayload, RelayMessage};

use crate::app_state::AppState;
use crate::dto::WorkspaceResponse;
use crate::rpc::ApiError;

#[derive(Deserialize)]
pub struct CreateWorkspaceRequest {
    pub name: String,
    pub backend_id: String,
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
    pub backend_id: Option<String>,
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

const PICK_DIRECTORY_UNAVAILABLE_MESSAGE: &str = "当前部署不支持浏览目录；请手动输入目标 Backend 机器上的绝对路径。若后续需要“浏览当前用户机器目录”，应通过独立本机桥接能力接入，而不是复用 cloud API。";

pub async fn list_workspaces(
    State(state): State<Arc<AppState>>,
    AxumPath(project_id): AxumPath<String>,
) -> Result<Json<Vec<WorkspaceResponse>>, ApiError> {
    let project_id = Uuid::parse_str(&project_id)
        .map_err(|_| ApiError::BadRequest("无效的 Project ID".into()))?;

    let workspaces = state
        .repos
        .workspace_repo
        .list_by_project(project_id)
        .await?;
    Ok(Json(
        workspaces
            .into_iter()
            .map(WorkspaceResponse::from)
            .collect(),
    ))
}

pub async fn create_workspace(
    State(state): State<Arc<AppState>>,
    AxumPath(project_id): AxumPath<String>,
    Json(req): Json<CreateWorkspaceRequest>,
) -> Result<Json<WorkspaceResponse>, ApiError> {
    let workspace_name = req.name.trim().to_string();
    if workspace_name.is_empty() {
        return Err(ApiError::BadRequest("工作空间名称不能为空".into()));
    }

    let backend_id = req.backend_id.trim().to_string();
    if backend_id.is_empty() {
        return Err(ApiError::BadRequest("创建 Workspace 必须显式指定 backend_id".into()));
    }

    let project_id = Uuid::parse_str(&project_id)
        .map_err(|_| ApiError::BadRequest("无效的 Project ID".into()))?;

    let ws_type = req.workspace_type.unwrap_or(WorkspaceType::GitWorktree);
    let (container_ref, git_config) =
        resolve_container_and_git(&state, &backend_id, req.container_ref, &ws_type).await?;

    let mut workspace = Workspace::new(
        project_id,
        backend_id,
        workspace_name,
        container_ref,
        ws_type,
    );
    workspace.git_config = git_config;

    state.repos.workspace_repo.create(&workspace).await?;
    auto_transition_workspace_status(&state, workspace.id).await?;
    workspace.status = WorkspaceStatus::Ready;

    Ok(Json(WorkspaceResponse::from(workspace)))
}

pub async fn get_workspace(
    State(state): State<Arc<AppState>>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<WorkspaceResponse>, ApiError> {
    let workspace_id =
        Uuid::parse_str(&id).map_err(|_| ApiError::BadRequest("无效的 Workspace ID".into()))?;

    let workspace = state
        .repos
        .workspace_repo
        .get_by_id(workspace_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Workspace {id} 不存在")))?;

    Ok(Json(WorkspaceResponse::from(workspace)))
}

pub async fn update_workspace(
    State(state): State<Arc<AppState>>,
    AxumPath(id): AxumPath<String>,
    Json(req): Json<UpdateWorkspaceRequest>,
) -> Result<Json<WorkspaceResponse>, ApiError> {
    let workspace_id =
        Uuid::parse_str(&id).map_err(|_| ApiError::BadRequest("无效的 Workspace ID".into()))?;

    let mut workspace = state
        .repos
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

    let next_type = req
        .workspace_type
        .unwrap_or_else(|| workspace.workspace_type.clone());
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
        resolve_container_and_git(&state, &workspace.backend_id, container_input, &next_type)
            .await?;

    workspace.workspace_type = next_type;
    workspace.container_ref = resolved_container_ref;
    workspace.git_config = resolved_git_config;

    state.repos.workspace_repo.update(&workspace).await?;
    Ok(Json(WorkspaceResponse::from(workspace)))
}

pub async fn update_workspace_status(
    State(state): State<Arc<AppState>>,
    AxumPath(id): AxumPath<String>,
    Json(req): Json<UpdateWorkspaceStatusRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let workspace_id =
        Uuid::parse_str(&id).map_err(|_| ApiError::BadRequest("无效的 Workspace ID".into()))?;

    state
        .repos
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

    state.repos.workspace_repo.delete(workspace_id).await?;
    Ok(Json(serde_json::json!({ "deleted": id })))
}

pub async fn detect_git(
    State(state): State<Arc<AppState>>,
    Json(req): Json<DetectGitRequest>,
) -> Result<Json<DetectGitResponse>, ApiError> {
    let container_ref = req.container_ref.trim();
    if container_ref.is_empty() {
        return Err(ApiError::BadRequest("container_ref 不能为空".into()));
    }

    let backend_id = require_detect_git_backend_id(req.backend_id.as_deref())?;
    let result = detect_git_via_backend(&state, backend_id, container_ref).await?;

    Ok(Json(DetectGitResponse {
        resolved_container_ref: container_ref.to_string(),
        is_git_repo: result.is_git_repo,
        source_repo: result.source_repo,
        branch: result.branch,
        commit_hash: result.commit_hash,
    }))
}

pub async fn pick_directory(
    Json(req): Json<PickDirectoryRequest>,
) -> Result<Json<PickDirectoryResponse>, ApiError> {
    let has_initial_path = req
        .initial_path
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| !value.is_empty());
    tracing::warn!(
        has_initial_path,
        "pick_directory 已被禁用；请改为手动输入 backend 侧绝对路径"
    );
    Err(pick_directory_unavailable_error())
}

async fn auto_transition_workspace_status(
    state: &Arc<AppState>,
    workspace_id: Uuid,
) -> Result<(), ApiError> {
    state
        .repos
        .workspace_repo
        .update_status(workspace_id, WorkspaceStatus::Preparing)
        .await?;

    match state
        .repos
        .workspace_repo
        .update_status(workspace_id, WorkspaceStatus::Ready)
        .await
    {
        Ok(()) => Ok(()),
        Err(err) => {
            let _ = state
                .repos
                .workspace_repo
                .update_status(workspace_id, WorkspaceStatus::Error)
                .await;
            Err(err.into())
        }
    }
}

async fn resolve_container_and_git(
    state: &Arc<AppState>,
    backend_id: &str,
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

    let normalized_ref = trimmed_ref.to_string();
    let detected = detect_git_via_backend(state, backend_id, trimmed_ref).await?;

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

async fn detect_git_via_backend(
    state: &Arc<AppState>,
    backend_id: &str,
    container_ref: &str,
) -> Result<GitDetectionResult, ApiError> {
    let backend_id = backend_id.trim();
    if backend_id.is_empty() {
        return Err(ApiError::BadRequest("backend_id 不能为空".into()));
    }
    if !state.services.backend_registry.is_online(backend_id).await {
        return Err(ApiError::Conflict(format!(
            "目标 Backend 当前不在线: {backend_id}"
        )));
    }

    let cmd = RelayMessage::CommandWorkspaceDetectGit {
        id: RelayMessage::new_id("workspace-detect-git"),
        payload: CommandWorkspaceDetectGitPayload {
            path: container_ref.to_string(),
        },
    };
    let resp = state
        .services
        .backend_registry
        .send_command(backend_id, cmd)
        .await
        .map_err(|e| ApiError::Internal(format!("relay workspace_detect_git 失败: {e}")))?;

    match resp {
        RelayMessage::ResponseWorkspaceDetectGit {
            payload: Some(payload),
            error: None,
            ..
        } => Ok(GitDetectionResult {
            is_git_repo: payload.is_git,
            source_repo: payload
                .remote_url
                .clone()
                .or_else(|| payload.is_git.then(|| container_ref.to_string())),
            branch: payload.current_branch.or(payload.default_branch),
            commit_hash: None,
        }),
        RelayMessage::ResponseWorkspaceDetectGit {
            error: Some(err), ..
        } => Err(ApiError::Internal(format!(
            "远程 workspace_detect_git 错误: {}",
            err.message
        ))),
        _ => Err(ApiError::Internal(
            "远程 workspace_detect_git 返回了意外响应".into(),
        )),
    }
}

fn require_detect_git_backend_id(raw: Option<&str>) -> Result<&str, ApiError> {
    let backend_id = raw.map(str::trim).filter(|value| !value.is_empty());
    backend_id.ok_or_else(|| {
        ApiError::BadRequest(
            "detect_git 必须显式提供 backend_id；container_ref 始终表示目标 Backend 机器上的绝对路径"
                .into(),
        )
    })
}

fn pick_directory_unavailable_error() -> ApiError {
    ApiError::Conflict(PICK_DIRECTORY_UNAVAILABLE_MESSAGE.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn require_detect_git_backend_id_rejects_missing_or_blank() {
        assert!(require_detect_git_backend_id(None).is_err());
        assert!(require_detect_git_backend_id(Some("")).is_err());
        assert!(require_detect_git_backend_id(Some("   ")).is_err());
        assert_eq!(
            require_detect_git_backend_id(Some(" backend-a ")).expect("应提取 backend_id"),
            "backend-a"
        );
    }

    #[tokio::test]
    async fn pick_directory_returns_explicit_conflict() {
        let result = pick_directory(Json(PickDirectoryRequest {
            initial_path: Some("F:/Projects".into()),
        }))
        .await;

        match result {
            Err(ApiError::Conflict(message)) => {
                assert_eq!(message, PICK_DIRECTORY_UNAVAILABLE_MESSAGE);
            }
            other => panic!("预期 Conflict，实际得到: {:?}", other.map(|_| ())),
        }
    }
}
