use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use agentdash_application::backend::{BackendAuthorizationService, BackendPermission};
use agentdash_application::runtime_gateway::{
    RuntimeActionKey, RuntimeActor, RuntimeContext, RuntimeInvocationRequest,
    WORKSPACE_BROWSE_DIRECTORY_ACTION, WORKSPACE_DETECT_ACTION, WorkspaceBrowseDirectoryInput,
    WorkspaceBrowseDirectoryOutput, WorkspaceDetectInput,
};
use agentdash_application::workspace::{
    WorkspaceBindingSyncResult, WorkspaceDetectionResult, WorkspaceInventoryCandidate,
    list_project_workspace_candidates, sync_project_backend_workspace_bindings,
};
use agentdash_domain::backend::{
    BackendWorkspaceInventory, BackendWorkspaceInventorySource, BackendWorkspaceInventoryStatus,
    ProjectBackendAccess, ProjectBackendAccessMode, ProjectBackendAccessStatus,
};
use agentdash_domain::workspace::{
    WorkspaceIdentityKind, identity_payload_from_detected_facts, normalize_path_key,
};

use crate::app_state::AppState;
use crate::auth::{CurrentUser, ProjectPermission, load_project_with_permission};
use crate::routes::backends::{BrowseDirectoryEntryResponse, BrowseDirectoryResponse};
use crate::rpc::ApiError;

#[derive(Debug, Deserialize)]
pub struct CreateProjectBackendAccessRequest {
    pub backend_id: String,
    pub priority: Option<i32>,
    pub root_policy: Option<Value>,
    pub capability_policy: Option<Value>,
    pub note: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateProjectBackendAccessRequest {
    pub status: Option<ProjectBackendAccessStatus>,
    pub access_mode: Option<ProjectBackendAccessMode>,
    pub priority: Option<i32>,
    pub root_policy: Option<Value>,
    pub capability_policy: Option<Value>,
    pub note: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ProjectBackendAccessResponse {
    pub id: Uuid,
    pub project_id: Uuid,
    pub backend_id: String,
    pub status: ProjectBackendAccessStatus,
    pub access_mode: ProjectBackendAccessMode,
    pub priority: i32,
    pub root_policy: Value,
    pub capability_policy: Value,
    pub note: Option<String>,
    pub created_by: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
pub struct BackendWorkspaceInventoryResponse {
    pub id: Uuid,
    pub backend_id: String,
    pub root_ref: String,
    pub identity_kind: WorkspaceIdentityKind,
    pub identity_payload: Value,
    pub detected_facts: Value,
    pub status: BackendWorkspaceInventoryStatus,
    pub source: BackendWorkspaceInventorySource,
    pub last_seen_at: chrono::DateTime<chrono::Utc>,
    pub last_error: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
pub struct InventoryRefreshResponse {
    pub access_id: Uuid,
    pub backend_id: String,
    pub refreshed: usize,
    pub failed: usize,
    pub items: Vec<BackendWorkspaceInventoryResponse>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct BrowseAccessDirectoryRequest {
    pub path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RegisterBackendWorkspaceInventoryRequest {
    pub root_ref: String,
}

pub async fn list_project_backend_access(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(project_id): Path<String>,
) -> Result<Json<Vec<ProjectBackendAccessResponse>>, ApiError> {
    let project_id = parse_project_id(&project_id)?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::View,
    )
    .await?;

    let accesses = state
        .repos
        .project_backend_access_repo
        .list_by_project(project_id)
        .await?;
    Ok(Json(
        accesses
            .into_iter()
            .map(ProjectBackendAccessResponse::from)
            .collect(),
    ))
}

pub async fn create_project_backend_access(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(project_id): Path<String>,
    Json(req): Json<CreateProjectBackendAccessRequest>,
) -> Result<Json<ProjectBackendAccessResponse>, ApiError> {
    let project_id = parse_project_id(&project_id)?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Edit,
    )
    .await?;
    let backend_id = normalize_required("backend_id", &req.backend_id)?;
    let backend = state.repos.backend_repo.get_backend(&backend_id).await?;
    BackendAuthorizationService::new(
        state.repos.backend_repo.as_ref(),
        state.repos.project_repo.as_ref(),
    )
    .require_config(&current_user, &backend, BackendPermission::Manage)
    .await?;

    let existing = state
        .repos
        .project_backend_access_repo
        .list_by_project(project_id)
        .await?
        .into_iter()
        .find(|access| access.backend_id == backend_id);
    if let Some(mut access) = existing {
        access.status = ProjectBackendAccessStatus::Active;
        access.priority = req.priority.unwrap_or(access.priority);
        if let Some(root_policy) = req.root_policy {
            access.root_policy = root_policy;
        }
        if let Some(capability_policy) = req.capability_policy {
            access.capability_policy = capability_policy;
        }
        if let Some(note) = req.note {
            access.note = normalize_optional(note);
        }
        state
            .repos
            .project_backend_access_repo
            .update(&access)
            .await?;
        let stored = state
            .repos
            .project_backend_access_repo
            .get_by_id(access.id)
            .await?
            .ok_or_else(|| ApiError::Internal("ProjectBackendAccess 更新后读取失败".into()))?;
        return Ok(Json(ProjectBackendAccessResponse::from(stored)));
    }

    let mut access =
        ProjectBackendAccess::new(project_id, backend_id, Some(current_user.user_id.clone()));
    access.priority = req.priority.unwrap_or_default();
    if let Some(root_policy) = req.root_policy {
        access.root_policy = root_policy;
    }
    if let Some(capability_policy) = req.capability_policy {
        access.capability_policy = capability_policy;
    }
    access.note = req.note.and_then(normalize_optional);
    state
        .repos
        .project_backend_access_repo
        .create(&access)
        .await?;
    Ok(Json(ProjectBackendAccessResponse::from(access)))
}

pub async fn update_project_backend_access(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((project_id, access_id)): Path<(String, String)>,
    Json(req): Json<UpdateProjectBackendAccessRequest>,
) -> Result<Json<ProjectBackendAccessResponse>, ApiError> {
    let project_id = parse_project_id(&project_id)?;
    let access_id = parse_uuid(&access_id, "ProjectBackendAccess ID")?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Edit,
    )
    .await?;
    let mut access = load_access_for_project(&state, project_id, access_id).await?;
    if let Some(status) = req.status {
        access.status = status;
    }
    if let Some(access_mode) = req.access_mode {
        access.access_mode = access_mode;
    }
    if let Some(priority) = req.priority {
        access.priority = priority;
    }
    if let Some(root_policy) = req.root_policy {
        access.root_policy = root_policy;
    }
    if let Some(capability_policy) = req.capability_policy {
        access.capability_policy = capability_policy;
    }
    if let Some(note) = req.note {
        access.note = normalize_optional(note);
    }
    state
        .repos
        .project_backend_access_repo
        .update(&access)
        .await?;
    let stored = state
        .repos
        .project_backend_access_repo
        .get_by_id(access.id)
        .await?
        .ok_or_else(|| ApiError::Internal("ProjectBackendAccess 更新后读取失败".into()))?;
    Ok(Json(ProjectBackendAccessResponse::from(stored)))
}

pub async fn revoke_project_backend_access(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((project_id, access_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let project_id = parse_project_id(&project_id)?;
    let access_id = parse_uuid(&access_id, "ProjectBackendAccess ID")?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Edit,
    )
    .await?;
    let access = load_access_for_project(&state, project_id, access_id).await?;
    state
        .repos
        .project_backend_access_repo
        .set_status(access.id, ProjectBackendAccessStatus::Revoked)
        .await?;
    Ok(Json(serde_json::json!({ "revoked": access.id })))
}

pub async fn list_project_backend_inventory(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((project_id, access_id)): Path<(String, String)>,
) -> Result<Json<Vec<BackendWorkspaceInventoryResponse>>, ApiError> {
    let project_id = parse_project_id(&project_id)?;
    let access_id = parse_uuid(&access_id, "ProjectBackendAccess ID")?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::View,
    )
    .await?;
    let access = load_access_for_project(&state, project_id, access_id).await?;
    let items = state
        .repos
        .backend_workspace_inventory_repo
        .list_by_backend(&access.backend_id)
        .await?;
    Ok(Json(
        items
            .into_iter()
            .map(BackendWorkspaceInventoryResponse::from)
            .collect(),
    ))
}

pub async fn refresh_project_backend_inventory(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((project_id, access_id)): Path<(String, String)>,
) -> Result<Json<InventoryRefreshResponse>, ApiError> {
    let project_id = parse_project_id(&project_id)?;
    let access_id = parse_uuid(&access_id, "ProjectBackendAccess ID")?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Edit,
    )
    .await?;
    let access = load_access_for_project(&state, project_id, access_id).await?;
    if !access.is_active() {
        return Err(ApiError::Conflict("ProjectBackendAccess 当前未启用".into()));
    }
    let online = state
        .services
        .backend_registry
        .list_online()
        .await
        .into_iter()
        .find(|backend| backend.backend_id == access.backend_id)
        .ok_or_else(|| ApiError::Conflict(format!("backend `{}` 当前不在线", access.backend_id)))?;
    let mut items = Vec::new();
    let mut warnings = Vec::new();
    for root in online.workspace_roots {
        match invoke_workspace_detect(
            &state,
            Some(current_user.user_id.as_str()),
            project_id,
            &access.backend_id,
            &root,
        )
        .await
        {
            Ok(detected) => {
                items.push(inventory_from_detected(
                    access.backend_id.clone(),
                    root,
                    detected,
                    BackendWorkspaceInventoryStatus::Available,
                    BackendWorkspaceInventorySource::ManualRefresh,
                    None,
                ));
            }
            Err(error) => {
                warnings.push(format!("{}: {}", root, error));
                items.push(error_inventory(
                    access.backend_id.clone(),
                    root,
                    error.to_string(),
                ));
            }
        }
    }
    state
        .repos
        .backend_workspace_inventory_repo
        .upsert_many(&items)
        .await?;
    let refreshed = items
        .iter()
        .filter(|item| item.status == BackendWorkspaceInventoryStatus::Available)
        .count();
    let failed = items.len().saturating_sub(refreshed);
    Ok(Json(InventoryRefreshResponse {
        access_id: access.id,
        backend_id: access.backend_id,
        refreshed,
        failed,
        items: items
            .into_iter()
            .map(BackendWorkspaceInventoryResponse::from)
            .collect(),
        warnings,
    }))
}

pub async fn register_project_backend_inventory(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((project_id, access_id)): Path<(String, String)>,
    Json(req): Json<RegisterBackendWorkspaceInventoryRequest>,
) -> Result<Json<BackendWorkspaceInventoryResponse>, ApiError> {
    let project_id = parse_project_id(&project_id)?;
    let access_id = parse_uuid(&access_id, "ProjectBackendAccess ID")?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Edit,
    )
    .await?;
    let access = load_access_for_project(&state, project_id, access_id).await?;
    if !access.is_active() {
        return Err(ApiError::Conflict("ProjectBackendAccess 当前未启用".into()));
    }

    let root_ref = normalize_required("root_ref", &req.root_ref)?;
    let detected = invoke_workspace_detect(
        &state,
        Some(current_user.user_id.as_str()),
        project_id,
        &access.backend_id,
        &root_ref,
    )
    .await?;
    let item = inventory_from_detected(
        access.backend_id,
        root_ref,
        detected,
        BackendWorkspaceInventoryStatus::Available,
        BackendWorkspaceInventorySource::CapabilityExpansionAck,
        None,
    );
    state
        .repos
        .backend_workspace_inventory_repo
        .upsert(&item)
        .await?;
    Ok(Json(BackendWorkspaceInventoryResponse::from(item)))
}

pub async fn list_workspace_candidates(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(project_id): Path<String>,
) -> Result<Json<Vec<WorkspaceInventoryCandidate>>, ApiError> {
    let project_id = parse_project_id(&project_id)?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::View,
    )
    .await?;
    Ok(Json(
        list_project_workspace_candidates(&state.repos, project_id).await?,
    ))
}

pub async fn sync_workspace_bindings(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(project_id): Path<String>,
) -> Result<Json<WorkspaceBindingSyncResult>, ApiError> {
    let project_id = parse_project_id(&project_id)?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Edit,
    )
    .await?;
    Ok(Json(
        sync_project_backend_workspace_bindings(&state.repos, project_id).await?,
    ))
}

pub async fn browse_project_backend_access(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((project_id, access_id)): Path<(String, String)>,
    Json(req): Json<BrowseAccessDirectoryRequest>,
) -> Result<Json<BrowseDirectoryResponse>, ApiError> {
    let project_id = parse_project_id(&project_id)?;
    let access_id = parse_uuid(&access_id, "ProjectBackendAccess ID")?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::View,
    )
    .await?;
    let access = load_access_for_project(&state, project_id, access_id).await?;
    if !access.is_active() {
        return Err(ApiError::Conflict("ProjectBackendAccess 当前未启用".into()));
    }
    let input = serde_json::to_value(WorkspaceBrowseDirectoryInput {
        backend_id: access.backend_id.clone(),
        path: req.path,
    })
    .map_err(|error| {
        ApiError::BadRequest(format!("workspace.browse_directory 输入非法: {error}"))
    })?;
    let request = RuntimeInvocationRequest::new(
        RuntimeActionKey::parse(WORKSPACE_BROWSE_DIRECTORY_ACTION).map_err(|error| {
            ApiError::Internal(format!("内置 Runtime Action Key 非法: {error}"))
        })?,
        RuntimeActor::PlatformUser {
            user_id: Some(current_user.user_id),
        },
        RuntimeContext::Setup {
            project_id: Some(project_id),
            workspace_id: None,
            backend_id: Some(access.backend_id),
            root_ref: None,
        },
        input,
    );
    let invocation = state.services.runtime_gateway.invoke(request).await?;
    let output = serde_json::from_value::<WorkspaceBrowseDirectoryOutput>(invocation.output.output)
        .map_err(|error| {
            ApiError::Internal(format!(
                "workspace.browse_directory 返回值解析失败: {error}"
            ))
        })?;
    Ok(Json(BrowseDirectoryResponse {
        current_path: output.current_path,
        entries: output
            .entries
            .into_iter()
            .map(|entry| BrowseDirectoryEntryResponse {
                name: entry.name,
                path: entry.path,
                is_dir: entry.is_dir,
            })
            .collect(),
    }))
}

pub async fn ensure_project_backend_access(
    state: &Arc<AppState>,
    project_id: Uuid,
    backend_id: &str,
) -> Result<ProjectBackendAccess, ApiError> {
    state
        .repos
        .project_backend_access_repo
        .get_active_for_project_backend(project_id, backend_id)
        .await?
        .ok_or_else(|| {
            ApiError::Forbidden(format!(
                "Project 尚未授权访问 backend `{}`",
                backend_id.trim()
            ))
        })
}

async fn load_access_for_project(
    state: &Arc<AppState>,
    project_id: Uuid,
    access_id: Uuid,
) -> Result<ProjectBackendAccess, ApiError> {
    let access = state
        .repos
        .project_backend_access_repo
        .get_by_id(access_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("ProjectBackendAccess 不存在".into()))?;
    if access.project_id != project_id {
        return Err(ApiError::NotFound("ProjectBackendAccess 不存在".into()));
    }
    Ok(access)
}

async fn invoke_workspace_detect(
    state: &Arc<AppState>,
    user_id: Option<&str>,
    project_id: Uuid,
    backend_id: &str,
    root_ref: &str,
) -> Result<WorkspaceDetectionResult, ApiError> {
    let input = serde_json::to_value(WorkspaceDetectInput {
        backend_id: backend_id.to_string(),
        root_ref: root_ref.to_string(),
    })
    .map_err(|error| ApiError::BadRequest(format!("workspace.detect 输入非法: {error}")))?;
    let request = RuntimeInvocationRequest::new(
        RuntimeActionKey::parse(WORKSPACE_DETECT_ACTION).map_err(|error| {
            ApiError::Internal(format!("内置 Runtime Action Key 非法: {error}"))
        })?,
        RuntimeActor::PlatformUser {
            user_id: user_id.map(str::to_string),
        },
        RuntimeContext::Setup {
            project_id: Some(project_id),
            workspace_id: None,
            backend_id: Some(backend_id.to_string()),
            root_ref: Some(root_ref.to_string()),
        },
        input,
    );
    let invocation = state.services.runtime_gateway.invoke(request).await?;
    serde_json::from_value::<WorkspaceDetectionResult>(invocation.output.output)
        .map_err(|error| ApiError::Internal(format!("workspace.detect 返回值解析失败: {error}")))
}

fn inventory_from_detected(
    backend_id: String,
    root_ref: String,
    detected: WorkspaceDetectionResult,
    status: BackendWorkspaceInventoryStatus,
    source: BackendWorkspaceInventorySource,
    last_error: Option<String>,
) -> BackendWorkspaceInventory {
    let mut item = BackendWorkspaceInventory::available(
        backend_id,
        root_ref,
        detected.identity_kind,
        detected.identity_payload,
        detected.binding.detected_facts,
        source,
    );
    item.status = status;
    item.last_error = last_error;
    item
}

fn error_inventory(
    backend_id: String,
    root_ref: String,
    last_error: String,
) -> BackendWorkspaceInventory {
    let identity_payload = identity_payload_from_detected_facts(
        WorkspaceIdentityKind::LocalDir,
        &serde_json::json!({}),
        &root_ref,
    )
    .unwrap_or_else(|| serde_json::json!({ "match_mode": "path_key", "path_key": normalize_path_key(&root_ref) }));
    let mut item = BackendWorkspaceInventory::available(
        backend_id,
        root_ref,
        WorkspaceIdentityKind::LocalDir,
        identity_payload,
        serde_json::json!({}),
        BackendWorkspaceInventorySource::ManualRefresh,
    );
    item.status = BackendWorkspaceInventoryStatus::Error;
    item.last_error = Some(last_error);
    item
}

fn parse_project_id(raw: &str) -> Result<Uuid, ApiError> {
    parse_uuid(raw, "Project ID")
}

fn parse_uuid(raw: &str, label: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(raw).map_err(|_| ApiError::BadRequest(format!("无效的 {label}")))
}

fn normalize_required(field: &str, raw: &str) -> Result<String, ApiError> {
    let value = raw.trim();
    if value.is_empty() {
        return Err(ApiError::BadRequest(format!("{field} 不能为空")));
    }
    Ok(value.to_string())
}

fn normalize_optional(raw: String) -> Option<String> {
    let value = raw.trim();
    (!value.is_empty()).then(|| value.to_string())
}

impl From<ProjectBackendAccess> for ProjectBackendAccessResponse {
    fn from(value: ProjectBackendAccess) -> Self {
        Self {
            id: value.id,
            project_id: value.project_id,
            backend_id: value.backend_id,
            status: value.status,
            access_mode: value.access_mode,
            priority: value.priority,
            root_policy: value.root_policy,
            capability_policy: value.capability_policy,
            note: value.note,
            created_by: value.created_by,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

impl From<BackendWorkspaceInventory> for BackendWorkspaceInventoryResponse {
    fn from(value: BackendWorkspaceInventory) -> Self {
        Self {
            id: value.id,
            backend_id: value.backend_id,
            root_ref: value.root_ref,
            identity_kind: value.identity_kind,
            identity_payload: value.identity_payload,
            detected_facts: value.detected_facts,
            status: value.status,
            source: value.source,
            last_seen_at: value.last_seen_at,
            last_error: value.last_error,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}
