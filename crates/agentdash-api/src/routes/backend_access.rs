use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use uuid::Uuid;

use agentdash_application::backend::{
    BackendAuthorizationService, BackendPermission, EnsureProjectBackendAccessGrantInput,
    ProjectBackendAccessGrantSource, ensure_project_backend_access_grant,
};
use agentdash_application::workspace::{
    RegisterBackendInventoryInput,
    WorkspaceBindingSyncResult as ApplicationWorkspaceBindingSyncResult,
    WorkspaceInventoryCandidate as ApplicationWorkspaceInventoryCandidate,
    WorkspacePlacementService,
};
use agentdash_application::workspace::{
    list_project_workspace_candidates, sync_project_backend_workspace_bindings,
};
use agentdash_application_runtime_gateway::{
    WORKSPACE_BROWSE_DIRECTORY_ACTION, WorkspaceBrowseDirectoryInput,
    WorkspaceBrowseDirectoryOutput,
};
use agentdash_contracts::backend::{
    BackendWorkspaceInventoryResponse, CreateProjectBackendAccessRequest,
    ProjectBackendAccessMode as ProjectBackendAccessModeDto, ProjectBackendAccessResponse,
    ProjectBackendAccessStatus as ProjectBackendAccessStatusDto,
    RegisterBackendWorkspaceInventoryRequest, UpdateProjectBackendAccessRequest,
};
use agentdash_contracts::common_response::RevokedIdResponse;
use agentdash_contracts::workspace::{WorkspaceBindingSyncResult, WorkspaceInventoryCandidate};
use agentdash_domain::backend::{
    ProjectBackendAccess, ProjectBackendAccessMode, ProjectBackendAccessStatus,
};

use crate::app_state::AppState;
use crate::auth::{CurrentUser, ProjectPermission, load_project_with_permission};
use crate::dto::{
    BrowseAccessDirectoryRequest, BrowseDirectoryEntryResponse, BrowseDirectoryResponse,
};
use crate::operation_runtime::{SetupOperationScope, invoke_setup_operation};
use crate::rpc::ApiError;
use crate::workspace_placement_runtime::RuntimeGatewayWorkspacePlacementRuntime;

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
        ProjectPermission::Use,
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
        ProjectPermission::Configure,
    )
    .await?;
    let backend_id = normalize_required("backend_id", &req.backend_id)?;
    let backend = state.repos.backend_repo.get_backend(&backend_id).await?;
    BackendAuthorizationService::new(
        state.repos.backend_repo.as_ref(),
        state.repos.project_repo.as_ref(),
        state.repos.project_backend_access_repo.as_ref(),
    )
    .require_config(&current_user, &backend, BackendPermission::Manage)
    .await?;

    let result = ensure_project_backend_access_grant(
        state.repos.project_backend_access_repo.as_ref(),
        EnsureProjectBackendAccessGrantInput {
            project_id,
            backend_id,
            source: ProjectBackendAccessGrantSource::UserGrant,
            created_by_user_id: Some(current_user.user_id.clone()),
            priority: req.priority,
            root_policy: req.root_policy,
            capability_policy: req.capability_policy,
            note: req.note,
        },
    )
    .await?;
    Ok(Json(ProjectBackendAccessResponse::from(result.access)))
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
        ProjectPermission::Configure,
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
        ProjectPermission::Configure,
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
        ProjectPermission::Use,
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
        ProjectPermission::Configure,
    )
    .await?;
    let placement_runtime = Arc::new(RuntimeGatewayWorkspacePlacementRuntime::new(
        state.services.operation_gateway.clone(),
        current_user.clone(),
    ));
    let item = WorkspacePlacementService::new(state.repos.clone(), placement_runtime)
        .register_backend_inventory(RegisterBackendInventoryInput {
            project_id,
            access_id,
            user_id: Some(current_user.user_id),
            root_ref: req.root_ref,
        })
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
        ProjectPermission::Use,
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
        ProjectPermission::Configure,
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
        ProjectPermission::Use,
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
    let output = invoke_setup_operation(
        state.as_ref(),
        &current_user,
        WORKSPACE_BROWSE_DIRECTORY_ACTION,
        input,
        SetupOperationScope {
            project_id: Some(project_id),
            workspace_id: None,
            backend_id: Some(access.backend_id),
        },
    )
    .await?;
    let output =
        serde_json::from_value::<WorkspaceBrowseDirectoryOutput>(output).map_err(|error| {
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
        ProjectBackendAccessModeDto::ExplicitGrant => ProjectBackendAccessMode::ExplicitGrant,
    }
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
