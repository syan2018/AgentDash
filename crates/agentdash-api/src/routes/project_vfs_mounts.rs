use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use agentdash_domain::common::MountCapability;
use agentdash_domain::inline_file::InlineFileOwnerKind;
use agentdash_domain::project_vfs_mount::{ProjectVfsMount, ProjectVfsMountContent};
use agentdash_domain::shared_library::InstalledAssetSource;

use crate::app_state::AppState;
use crate::auth::{CurrentUser, ProjectPermission, load_project_with_permission};
use crate::rpc::ApiError;

#[derive(Debug, Deserialize)]
pub struct ProjectPath {
    pub project_id: String,
}

#[derive(Debug, Deserialize)]
pub struct VfsMountPath {
    pub project_id: String,
    pub mount_id: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateProjectVfsMountRequest {
    pub mount_id: String,
    pub display_name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub capabilities: Vec<MountCapability>,
    pub content: ProjectVfsMountContent,
}

#[derive(Debug, Deserialize)]
pub struct UpdateProjectVfsMountRequest {
    pub mount_id: String,
    pub display_name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub capabilities: Vec<MountCapability>,
    pub content: ProjectVfsMountContent,
}

#[derive(Debug, Serialize)]
pub struct ProjectVfsMountResponse {
    pub project_id: Uuid,
    pub mount_id: String,
    pub display_name: String,
    pub description: Option<String>,
    pub capabilities: Vec<MountCapability>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installed_source: Option<InstalledAssetSource>,
    pub content: ProjectVfsMountContent,
    pub surface_ref: String,
    pub created_at: chrono::DateTime<Utc>,
    pub updated_at: chrono::DateTime<Utc>,
}

pub async fn list_vfs_mounts(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<ProjectPath>,
) -> Result<Json<Vec<ProjectVfsMountResponse>>, ApiError> {
    let project_id = parse_uuid(&path.project_id, "project_id")?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::View,
    )
    .await?;

    let mounts = state
        .repos
        .project_vfs_mount_repo
        .list_by_project(project_id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    Ok(Json(mounts.into_iter().map(Into::into).collect()))
}

pub async fn create_vfs_mount(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<ProjectPath>,
    Json(req): Json<CreateProjectVfsMountRequest>,
) -> Result<Json<ProjectVfsMountResponse>, ApiError> {
    let project_id = parse_uuid(&path.project_id, "project_id")?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Edit,
    )
    .await?;

    let mount_id = normalize_identifier(&req.mount_id, "mount_id")?;
    let display_name = normalize_display_name(&req.display_name, &mount_id);
    let description = normalize_optional(req.description);
    let content = normalize_content(req.content)?;
    let capabilities = normalize_capabilities(req.capabilities);

    if state
        .repos
        .project_vfs_mount_repo
        .get_by_project_and_mount_id(project_id, &mount_id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?
        .is_some()
    {
        return Err(ApiError::Conflict(format!(
            "mount_id `{mount_id}` 已被占用"
        )));
    }

    let now = Utc::now();
    let mount = ProjectVfsMount {
        id: Uuid::new_v4(),
        project_id,
        mount_id,
        display_name,
        description,
        capabilities,
        installed_source: None,
        content,
        created_at: now,
        updated_at: now,
    };
    state
        .repos
        .project_vfs_mount_repo
        .create(&mount)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    Ok(Json(mount.into()))
}

pub async fn get_vfs_mount(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<VfsMountPath>,
) -> Result<Json<ProjectVfsMountResponse>, ApiError> {
    let project_id = parse_uuid(&path.project_id, "project_id")?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::View,
    )
    .await?;
    let mount_id = path.mount_id.trim();
    let mount = state
        .repos
        .project_vfs_mount_repo
        .get_by_project_and_mount_id(project_id, mount_id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?
        .ok_or_else(|| ApiError::NotFound("Project VFS Mount 不存在".into()))?;
    Ok(Json(mount.into()))
}

pub async fn update_vfs_mount(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<VfsMountPath>,
    Json(req): Json<UpdateProjectVfsMountRequest>,
) -> Result<Json<ProjectVfsMountResponse>, ApiError> {
    let project_id = parse_uuid(&path.project_id, "project_id")?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Edit,
    )
    .await?;
    let old_mount_id = path.mount_id.trim().to_string();
    let mut mount = state
        .repos
        .project_vfs_mount_repo
        .get_by_project_and_mount_id(project_id, &old_mount_id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?
        .ok_or_else(|| ApiError::NotFound("Project VFS Mount 不存在".into()))?;

    let new_mount_id = normalize_identifier(&req.mount_id, "mount_id")?;
    if new_mount_id != old_mount_id
        && state
            .repos
            .project_vfs_mount_repo
            .get_by_project_and_mount_id(project_id, &new_mount_id)
            .await
            .map_err(|error| ApiError::Internal(error.to_string()))?
            .is_some()
    {
        return Err(ApiError::Conflict(format!(
            "mount_id `{new_mount_id}` 已被占用"
        )));
    }

    let new_content = normalize_content(req.content)?;
    if std::mem::discriminant(&mount.content) != std::mem::discriminant(&new_content) {
        return Err(ApiError::BadRequest(
            "Project VFS Mount content kind 不可在更新中切换".into(),
        ));
    }

    mount.mount_id = new_mount_id;
    mount.display_name = normalize_display_name(&req.display_name, &mount.mount_id);
    mount.description = normalize_optional(req.description);
    mount.content = new_content;
    mount.capabilities = normalize_capabilities(req.capabilities);
    mount.updated_at = Utc::now();
    state
        .repos
        .project_vfs_mount_repo
        .update(&mount)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    Ok(Json(mount.into()))
}

pub async fn delete_vfs_mount(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<VfsMountPath>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let project_id = parse_uuid(&path.project_id, "project_id")?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Edit,
    )
    .await?;
    let mount_id = path.mount_id.trim();
    let mount = state
        .repos
        .project_vfs_mount_repo
        .get_by_project_and_mount_id(project_id, mount_id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?
        .ok_or_else(|| ApiError::NotFound("Project VFS Mount 不存在".into()))?;
    state
        .repos
        .inline_file_repo
        .delete_by_owner(InlineFileOwnerKind::ProjectVfsMount, mount.id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    state
        .repos
        .project_vfs_mount_repo
        .delete(project_id, mount_id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

impl From<ProjectVfsMount> for ProjectVfsMountResponse {
    fn from(mount: ProjectVfsMount) -> Self {
        let surface_ref = format!("project-vfs-mount:{}:{}", mount.project_id, mount.mount_id);
        Self {
            project_id: mount.project_id,
            mount_id: mount.mount_id,
            display_name: mount.display_name,
            description: mount.description,
            capabilities: mount.capabilities,
            installed_source: mount.installed_source,
            content: mount.content,
            surface_ref,
            created_at: mount.created_at,
            updated_at: mount.updated_at,
        }
    }
}

fn parse_uuid(raw: &str, field: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(raw).map_err(|_| ApiError::BadRequest(format!("{field} 非法")))
}

fn normalize_identifier(raw: &str, field: &str) -> Result<String, ApiError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ApiError::BadRequest(format!("{field} 不能为空")));
    }
    if trimmed.chars().any(char::is_whitespace)
        || trimmed.chars().any(|ch| matches!(ch, '/' | '\\' | ':'))
    {
        return Err(ApiError::BadRequest(format!(
            "{field} 不能包含空白、`/`、`\\` 或 `:`"
        )));
    }
    if trimmed.eq_ignore_ascii_case("main") {
        return Err(ApiError::BadRequest(format!("{field} 不能使用保留字 main")));
    }
    Ok(trimmed.to_string())
}

fn normalize_display_name(raw: &str, fallback: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value.and_then(|item| {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn normalize_capabilities(capabilities: Vec<MountCapability>) -> Vec<MountCapability> {
    let mut normalized = Vec::new();
    for capability in capabilities {
        if capability == MountCapability::Exec {
            continue;
        }
        if !normalized.contains(&capability) {
            normalized.push(capability);
        }
    }
    if normalized.is_empty() {
        normalized.extend([
            MountCapability::Read,
            MountCapability::List,
            MountCapability::Search,
        ]);
    }
    normalized
}

fn normalize_content(content: ProjectVfsMountContent) -> Result<ProjectVfsMountContent, ApiError> {
    match content {
        ProjectVfsMountContent::Inline => Ok(ProjectVfsMountContent::Inline),
        ProjectVfsMountContent::ExternalService {
            service_id,
            root_ref,
        } => {
            let service_id = service_id.trim();
            let root_ref = root_ref.trim();
            if service_id.is_empty() {
                return Err(ApiError::BadRequest("service_id 不能为空".into()));
            }
            if root_ref.is_empty() {
                return Err(ApiError::BadRequest("root_ref 不能为空".into()));
            }
            Ok(ProjectVfsMountContent::ExternalService {
                service_id: service_id.to_string(),
                root_ref: root_ref.to_string(),
            })
        }
    }
}
