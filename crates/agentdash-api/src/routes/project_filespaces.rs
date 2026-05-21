use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use agentdash_domain::common::MountCapability;
use agentdash_domain::inline_file::InlineFileOwnerKind;
use agentdash_domain::project_filespace::{
    ProjectFilespace, ProjectVfsMountBinding, ProjectVfsMountSource,
};

use crate::app_state::AppState;
use crate::auth::{CurrentUser, ProjectPermission, load_project_with_permission};
use crate::rpc::ApiError;

#[derive(Debug, Deserialize)]
pub struct ProjectPath {
    pub project_id: String,
}

#[derive(Debug, Deserialize)]
pub struct FilespacePath {
    pub project_id: String,
    pub filespace_id: String,
}

#[derive(Debug, Deserialize)]
pub struct MountBindingPath {
    pub project_id: String,
    pub binding_id: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateProjectFilespaceRequest {
    pub key: String,
    pub display_name: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateProjectFilespaceRequest {
    pub key: String,
    pub display_name: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateProjectVfsMountBindingRequest {
    pub mount_id: String,
    pub display_name: String,
    pub source: ProjectVfsMountSource,
    #[serde(default)]
    pub capabilities: Vec<MountCapability>,
    #[serde(default)]
    pub default_write: bool,
}

#[derive(Debug, Serialize)]
pub struct ProjectFilespaceResponse {
    pub id: Uuid,
    pub project_id: Uuid,
    pub key: String,
    pub display_name: String,
    pub description: Option<String>,
    pub surface_ref: String,
    pub created_at: chrono::DateTime<Utc>,
    pub updated_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct ProjectVfsMountBindingResponse {
    pub id: Uuid,
    pub project_id: Uuid,
    pub mount_id: String,
    pub display_name: String,
    pub source: ProjectVfsMountSource,
    pub capabilities: Vec<MountCapability>,
    pub default_write: bool,
    pub created_at: chrono::DateTime<Utc>,
    pub updated_at: chrono::DateTime<Utc>,
}

pub async fn list_filespaces(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<ProjectPath>,
) -> Result<Json<Vec<ProjectFilespaceResponse>>, ApiError> {
    let project_id = parse_uuid(&path.project_id, "project_id")?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::View,
    )
    .await?;

    let filespaces = state
        .repos
        .project_filespace_repo
        .list_by_project(project_id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    Ok(Json(filespaces.into_iter().map(Into::into).collect()))
}

pub async fn create_filespace(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<ProjectPath>,
    Json(req): Json<CreateProjectFilespaceRequest>,
) -> Result<Json<ProjectFilespaceResponse>, ApiError> {
    let project_id = parse_uuid(&path.project_id, "project_id")?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Edit,
    )
    .await?;

    let key = normalize_identifier(&req.key, "key")?;
    let display_name = normalize_display_name(&req.display_name, &key);
    let filespace = ProjectFilespace {
        id: Uuid::new_v4(),
        project_id,
        key: key.clone(),
        display_name: display_name.clone(),
        description: normalize_optional(req.description),
        installed_source: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    state
        .repos
        .project_filespace_repo
        .create(&filespace)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;

    let binding =
        ProjectVfsMountBinding::new_filespace(project_id, key, display_name, filespace.id);
    state
        .repos
        .project_vfs_mount_binding_repo
        .create(&binding)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;

    Ok(Json(filespace.into()))
}

pub async fn update_filespace(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<FilespacePath>,
    Json(req): Json<UpdateProjectFilespaceRequest>,
) -> Result<Json<ProjectFilespaceResponse>, ApiError> {
    let project_id = parse_uuid(&path.project_id, "project_id")?;
    let filespace_id = parse_uuid(&path.filespace_id, "filespace_id")?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Edit,
    )
    .await?;
    let mut filespace = state
        .repos
        .project_filespace_repo
        .get_by_id(filespace_id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?
        .ok_or_else(|| ApiError::NotFound("Project Filespace 不存在".into()))?;
    if filespace.project_id != project_id {
        return Err(ApiError::Conflict(
            "filespace_id 与 project_id 不属于同一 Project".into(),
        ));
    }
    filespace.key = normalize_identifier(&req.key, "key")?;
    filespace.display_name = normalize_display_name(&req.display_name, &filespace.key);
    filespace.description = normalize_optional(req.description);
    filespace.updated_at = Utc::now();
    state
        .repos
        .project_filespace_repo
        .update(&filespace)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    Ok(Json(filespace.into()))
}

pub async fn delete_filespace(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<FilespacePath>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let project_id = parse_uuid(&path.project_id, "project_id")?;
    let filespace_id = parse_uuid(&path.filespace_id, "filespace_id")?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Edit,
    )
    .await?;
    state
        .repos
        .inline_file_repo
        .delete_by_owner(InlineFileOwnerKind::ProjectFilespace, filespace_id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    let bindings = state
        .repos
        .project_vfs_mount_binding_repo
        .list_by_project(project_id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    for binding in bindings {
        if matches!(
            binding.source,
            ProjectVfsMountSource::Filespace { filespace_id: id } if id == filespace_id
        ) {
            state
                .repos
                .project_vfs_mount_binding_repo
                .delete(project_id, binding.id)
                .await
                .map_err(|error| ApiError::Internal(error.to_string()))?;
        }
    }
    state
        .repos
        .project_filespace_repo
        .delete(project_id, filespace_id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn list_mount_bindings(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<ProjectPath>,
) -> Result<Json<Vec<ProjectVfsMountBindingResponse>>, ApiError> {
    let project_id = parse_uuid(&path.project_id, "project_id")?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::View,
    )
    .await?;
    let bindings = state
        .repos
        .project_vfs_mount_binding_repo
        .list_by_project(project_id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    Ok(Json(bindings.into_iter().map(Into::into).collect()))
}

pub async fn update_mount_binding(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<MountBindingPath>,
    Json(req): Json<UpdateProjectVfsMountBindingRequest>,
) -> Result<Json<ProjectVfsMountBindingResponse>, ApiError> {
    let project_id = parse_uuid(&path.project_id, "project_id")?;
    let binding_id = parse_uuid(&path.binding_id, "binding_id")?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Edit,
    )
    .await?;
    let mut binding = state
        .repos
        .project_vfs_mount_binding_repo
        .get_by_id(binding_id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?
        .ok_or_else(|| ApiError::NotFound("Project VFS Mount 不存在".into()))?;
    if binding.project_id != project_id {
        return Err(ApiError::Conflict(
            "binding_id 与 project_id 不属于同一 Project".into(),
        ));
    }
    binding.mount_id = normalize_identifier(&req.mount_id, "mount_id")?;
    binding.display_name = normalize_display_name(&req.display_name, &binding.mount_id);
    binding.source = req.source;
    binding.capabilities = normalize_capabilities(req.capabilities);
    binding.default_write =
        req.default_write && binding.capabilities.contains(&MountCapability::Write);
    binding.updated_at = Utc::now();
    state
        .repos
        .project_vfs_mount_binding_repo
        .update(&binding)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    Ok(Json(binding.into()))
}

impl From<ProjectFilespace> for ProjectFilespaceResponse {
    fn from(filespace: ProjectFilespace) -> Self {
        Self {
            surface_ref: format!(
                "project-filespace:{}:{}",
                filespace.project_id, filespace.id
            ),
            id: filespace.id,
            project_id: filespace.project_id,
            key: filespace.key,
            display_name: filespace.display_name,
            description: filespace.description,
            created_at: filespace.created_at,
            updated_at: filespace.updated_at,
        }
    }
}

impl From<ProjectVfsMountBinding> for ProjectVfsMountBindingResponse {
    fn from(binding: ProjectVfsMountBinding) -> Self {
        Self {
            id: binding.id,
            project_id: binding.project_id,
            mount_id: binding.mount_id,
            display_name: binding.display_name,
            source: binding.source,
            capabilities: binding.capabilities,
            default_write: binding.default_write,
            created_at: binding.created_at,
            updated_at: binding.updated_at,
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
