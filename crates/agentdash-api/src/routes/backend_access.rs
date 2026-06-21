use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use uuid::Uuid;

use agentdash_application::backend::{BackendAuthorizationService, BackendPermission};
use agentdash_application::runtime_gateway::{
    RuntimeActionKey, RuntimeActor, RuntimeContext, RuntimeInvocationRequest,
    WORKSPACE_BROWSE_DIRECTORY_ACTION, WORKSPACE_DETECT_ACTION, WorkspaceBrowseDirectoryInput,
    WorkspaceBrowseDirectoryOutput, WorkspaceDetectInput,
};
use agentdash_application::workspace::{
    WorkspaceBindingSyncResult as ApplicationWorkspaceBindingSyncResult, WorkspaceDetectionResult,
    WorkspaceInventoryCandidate as ApplicationWorkspaceInventoryCandidate,
};
use agentdash_application::workspace::{
    list_project_workspace_candidates, sync_project_backend_workspace_bindings,
};
use agentdash_contracts::backend::{
    BackendWorkspaceInventoryResponse, CreateProjectBackendAccessRequest, InventoryRefreshResponse,
    ProjectBackendAccessMode as ProjectBackendAccessModeDto, ProjectBackendAccessResponse,
    ProjectBackendAccessStatus as ProjectBackendAccessStatusDto,
    RegisterBackendWorkspaceInventoryRequest, UpdateProjectBackendAccessRequest,
};
use agentdash_contracts::common_response::RevokedIdResponse;
use agentdash_contracts::workspace::{WorkspaceBindingSyncResult, WorkspaceInventoryCandidate};
use agentdash_domain::backend::{
    BackendWorkspaceInventory, BackendWorkspaceInventorySource, BackendWorkspaceInventoryStatus,
    ProjectBackendAccess, ProjectBackendAccessMode, ProjectBackendAccessStatus,
};
use agentdash_domain::workspace::{
    WorkspaceIdentityKind, identity_payload_from_detected_facts, normalize_path_key,
};

use crate::app_state::AppState;
use crate::auth::{CurrentUser, ProjectPermission, load_project_with_permission};
use crate::dto::{
    BrowseAccessDirectoryRequest, BrowseDirectoryEntryResponse, BrowseDirectoryResponse,
};
use crate::rpc::ApiError;

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

pub fn router() -> axum::Router<std::sync::Arc<crate::app_state::AppState>> {
    axum::Router::new()
        .route(
            "/projects/{project_id}/workspaces/candidates",
            axum::routing::get(list_workspace_candidates),
        )
        .route(
            "/projects/{project_id}/workspaces/sync-backend-bindings",
            axum::routing::post(sync_workspace_bindings),
        )
        .route(
            "/projects/{project_id}/backend-access",
            axum::routing::get(list_project_backend_access).post(create_project_backend_access),
        )
        .route(
            "/projects/{project_id}/backend-access/{access_id}",
            axum::routing::patch(update_project_backend_access)
                .delete(revoke_project_backend_access),
        )
        .route(
            "/projects/{project_id}/backend-access/{access_id}/inventory",
            axum::routing::get(list_project_backend_inventory),
        )
        .route(
            "/projects/{project_id}/backend-access/{access_id}/inventory/refresh",
            axum::routing::post(refresh_project_backend_inventory),
        )
        .route(
            "/projects/{project_id}/backend-access/{access_id}/inventory/register",
            axum::routing::post(register_project_backend_inventory),
        )
        .route(
            "/projects/{project_id}/backend-access/{access_id}/browse",
            axum::routing::post(browse_project_backend_access),
        )
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
        access.status = project_backend_access_status_command(status);
    }
    if let Some(access_mode) = req.access_mode {
        access.access_mode = project_backend_access_mode_command(access_mode);
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
) -> Result<Json<RevokedIdResponse>, ApiError> {
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
    Ok(Json(RevokedIdResponse {
        revoked: access.id.to_string(),
    }))
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
        access_id: access.id.to_string(),
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
        list_project_workspace_candidates(&state.repos, project_id)
            .await?
            .into_iter()
            .map(workspace_inventory_candidate_response)
            .collect(),
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
    let result = sync_project_backend_workspace_bindings(&state.repos, project_id).await?;
    Ok(Json(workspace_binding_sync_response(result)))
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

fn workspace_binding_sync_response(
    value: ApplicationWorkspaceBindingSyncResult,
) -> WorkspaceBindingSyncResult {
    WorkspaceBindingSyncResult {
        updated_workspace_ids: value
            .updated_workspace_ids
            .into_iter()
            .map(|id| id.to_string())
            .collect(),
        created_bindings: value.created_bindings,
        updated_bindings: value.updated_bindings,
        candidates: value
            .candidates
            .into_iter()
            .map(workspace_inventory_candidate_response)
            .collect(),
        conflicts: value
            .conflicts
            .into_iter()
            .map(workspace_inventory_candidate_response)
            .collect(),
    }
}

fn workspace_inventory_candidate_response(
    value: ApplicationWorkspaceInventoryCandidate,
) -> WorkspaceInventoryCandidate {
    WorkspaceInventoryCandidate {
        backend_id: value.backend_id,
        root_ref: value.root_ref,
        identity_kind: agentdash_contracts::workspace::WorkspaceIdentityKind::from(
            value.identity_kind,
        ),
        identity_payload: value.identity_payload,
        detected_facts: value.detected_facts,
        status: agentdash_contracts::backend::BackendWorkspaceInventoryStatus::from(value.status),
        matched_workspace_ids: value
            .matched_workspace_ids
            .into_iter()
            .map(|id| id.to_string())
            .collect(),
        reason: value.reason,
    }
}

fn project_backend_access_status_command(
    value: ProjectBackendAccessStatusDto,
) -> ProjectBackendAccessStatus {
    match value {
        ProjectBackendAccessStatusDto::Active => ProjectBackendAccessStatus::Active,
        ProjectBackendAccessStatusDto::Paused => ProjectBackendAccessStatus::Paused,
        ProjectBackendAccessStatusDto::Revoked => ProjectBackendAccessStatus::Revoked,
    }
}

fn project_backend_access_mode_command(
    value: ProjectBackendAccessModeDto,
) -> ProjectBackendAccessMode {
    match value {
        ProjectBackendAccessModeDto::UseInventory => ProjectBackendAccessMode::UseInventory,
    }
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
