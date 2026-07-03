use std::collections::HashMap;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path as AxumPath, State};
use serde_json::json;
use uuid::Uuid;

use agentdash_application::workspace::{
    BindDiscoveredWorkspaceBindingCommand, BindDiscoveredWorkspaceBindingsInput,
    CreateWorkspacePlacementInput, UpdateWorkspacePlacementInput, WorkspaceDetectionResult,
    WorkspacePlacementService,
};
use agentdash_application_runtime_gateway::{
    RuntimeActionKey, RuntimeActor, RuntimeContext, RuntimeInvocationRequest,
    WORKSPACE_DETECT_ACTION, WORKSPACE_DETECT_GIT_ACTION, WORKSPACE_DISCOVER_BY_IDENTITY_ACTION,
    WorkspaceDetectGitInput, WorkspaceDetectGitOutput, WorkspaceDetectInput,
    WorkspaceDiscoverByIdentityInput, WorkspaceDiscoverByIdentityOutput,
    WorkspaceDiscoverByIdentityWorkspaceInput,
};
use agentdash_contracts::backend::BackendWorkspaceInventoryResponse;
use agentdash_contracts::common_response::{DeletedIdResponse, UpdatedIdResponse};
use agentdash_contracts::workspace::{
    BindDiscoveredWorkspaceBindingsRequest, BindDiscoveredWorkspaceBindingsResponse,
    DiscoverLocalWorkspaceBindingsRequest, DiscoverLocalWorkspaceBindingsResponse,
    DiscoveredWorkspaceBindingCandidate, WorkspaceIdentityDiscoverySkipped,
};
use agentdash_domain::backend::BackendType;
use agentdash_domain::workspace::{
    WorkspaceBinding, WorkspaceBindingStatus, WorkspaceResolutionPolicy, identity_payload_matches,
};

use crate::app_state::AppState;
use crate::auth::{
    CurrentUser, ProjectPermission, load_project_with_permission,
    load_workspace_and_project_with_permission,
};
use crate::dto::{
    CreateWorkspaceRequest, DetectGitRequest, DetectGitResponse, DetectWorkspaceRequest,
    DetectWorkspaceResponse, UpdateWorkspaceRequest, UpdateWorkspaceStatusRequest,
    WorkspaceBindingInput, WorkspaceBindingResponse, WorkspaceResponse,
};
use crate::routes::backend_access::ensure_project_backend_access;
use crate::rpc::ApiError;
use crate::workspace_placement_runtime::RuntimeGatewayWorkspacePlacementRuntime;

pub async fn list_workspaces(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    AxumPath(project_id): AxumPath<String>,
) -> Result<Json<Vec<WorkspaceResponse>>, ApiError> {
    let project_id = parse_project_id(&project_id)?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Use,
    )
    .await?;

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

pub fn router() -> axum::Router<std::sync::Arc<crate::app_state::AppState>> {
    axum::Router::new()
        .route(
            "/projects/{project_id}/workspaces",
            axum::routing::get(list_workspaces).post(create_workspace),
        )
        .route(
            "/projects/{project_id}/workspaces/detect",
            axum::routing::post(detect_workspace),
        )
        .route(
            "/projects/{project_id}/workspaces/discover-local-bindings",
            axum::routing::post(discover_local_bindings),
        )
        .route(
            "/projects/{project_id}/workspaces/bind-discovered",
            axum::routing::post(bind_discovered),
        )
        .route("/workspaces/detect-git", axum::routing::post(detect_git))
        .route(
            "/workspaces/{id}",
            axum::routing::get(get_workspace)
                .put(update_workspace)
                .delete(delete_workspace),
        )
        .route(
            "/workspaces/{id}/status",
            axum::routing::patch(update_workspace_status),
        )
}

pub async fn create_workspace(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    AxumPath(project_id): AxumPath<String>,
    Json(req): Json<CreateWorkspaceRequest>,
) -> Result<Json<WorkspaceResponse>, ApiError> {
    let project_id = parse_project_id(&project_id)?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Configure,
    )
    .await?;

    let workspace_name = normalize_workspace_name(&req.name)?;
    let raw_bindings = if let Some(bindings) = req.bindings {
        bindings
    } else if let Some(shortcut_binding) = req.shortcut_binding {
        vec![shortcut_binding]
    } else {
        Vec::new()
    };
    let bindings = raw_bindings
        .into_iter()
        .map(|binding| binding_input_to_binding(Uuid::nil(), binding))
        .collect::<Result<Vec<_>, _>>()?;
    let placement_runtime = Arc::new(RuntimeGatewayWorkspacePlacementRuntime::new(
        state.services.runtime_gateway.clone(),
    ));
    let stored = WorkspacePlacementService::new(state.repos.clone(), placement_runtime)
        .create_workspace(CreateWorkspacePlacementInput {
            project_id,
            user_id: Some(current_user.user_id),
            name: workspace_name,
            identity_kind: req.identity_kind,
            identity_payload: req.identity_payload,
            resolution_policy: req
                .resolution_policy
                .unwrap_or(WorkspaceResolutionPolicy::PreferOnline),
            default_binding_id: req.default_binding_id,
            bindings,
            mount_capabilities: req.mount_capabilities.unwrap_or_default(),
        })
        .await?;
    Ok(Json(WorkspaceResponse::from(stored)))
}

pub async fn get_workspace(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<WorkspaceResponse>, ApiError> {
    let workspace_id = parse_workspace_id(&id)?;
    let (workspace, _) = load_workspace_and_project_with_permission(
        state.as_ref(),
        &current_user,
        workspace_id,
        ProjectPermission::Use,
    )
    .await?;

    Ok(Json(WorkspaceResponse::from(workspace)))
}

pub async fn update_workspace(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    AxumPath(id): AxumPath<String>,
    Json(req): Json<UpdateWorkspaceRequest>,
) -> Result<Json<WorkspaceResponse>, ApiError> {
    let workspace_id = parse_workspace_id(&id)?;
    let (workspace, _) = load_workspace_and_project_with_permission(
        state.as_ref(),
        &current_user,
        workspace_id,
        ProjectPermission::Configure,
    )
    .await?;

    let name = req
        .name
        .map(|name| normalize_workspace_name(&name))
        .transpose()?;
    let bindings = req
        .bindings
        .map(|bindings| {
            bindings
                .into_iter()
                .map(|binding| binding_input_to_binding(workspace.id, binding))
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?;
    let placement_runtime = Arc::new(RuntimeGatewayWorkspacePlacementRuntime::new(
        state.services.runtime_gateway.clone(),
    ));
    let stored = WorkspacePlacementService::new(state.repos.clone(), placement_runtime)
        .update_workspace(UpdateWorkspacePlacementInput {
            workspace,
            user_id: Some(current_user.user_id),
            name,
            identity_kind: req.identity_kind,
            identity_payload: req.identity_payload,
            resolution_policy: req.resolution_policy,
            default_binding_id: req.default_binding_id,
            bindings,
            mount_capabilities: req.mount_capabilities,
        })
        .await?;
    Ok(Json(WorkspaceResponse::from(stored)))
}

pub async fn update_workspace_status(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    AxumPath(id): AxumPath<String>,
    Json(req): Json<UpdateWorkspaceStatusRequest>,
) -> Result<Json<UpdatedIdResponse>, ApiError> {
    let workspace_id = parse_workspace_id(&id)?;
    let (mut workspace, _) = load_workspace_and_project_with_permission(
        state.as_ref(),
        &current_user,
        workspace_id,
        ProjectPermission::Configure,
    )
    .await?;
    workspace.status = req.status;
    workspace.refresh_default_binding();
    state.repos.workspace_repo.update(&workspace).await?;

    Ok(Json(UpdatedIdResponse { updated: id }))
}

pub async fn delete_workspace(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<DeletedIdResponse>, ApiError> {
    let workspace_id = parse_workspace_id(&id)?;
    load_workspace_and_project_with_permission(
        state.as_ref(),
        &current_user,
        workspace_id,
        ProjectPermission::Configure,
    )
    .await?;

    state.repos.workspace_repo.delete(workspace_id).await?;
    Ok(Json(DeletedIdResponse { deleted: id }))
}

pub async fn detect_workspace(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    AxumPath(project_id): AxumPath<String>,
    Json(req): Json<DetectWorkspaceRequest>,
) -> Result<Json<DetectWorkspaceResponse>, ApiError> {
    let project_id = parse_project_id(&project_id)?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Configure,
    )
    .await?;

    ensure_project_backend_access(&state, project_id, &req.backend_id).await?;
    let detected = invoke_workspace_setup_detect(
        &state,
        Some(current_user.user_id.as_str()),
        project_id,
        &req.backend_id,
        &req.root_ref,
    )
    .await?;
    let existing = state
        .repos
        .workspace_repo
        .list_by_project(project_id)
        .await?;
    let matched_workspace_ids = existing
        .into_iter()
        .filter(|workspace| {
            workspace.identity_kind == detected.identity_kind
                && identity_payload_matches(
                    workspace.identity_kind.clone(),
                    &workspace.identity_payload,
                    &detected.identity_payload,
                    Some(&detected.binding.detected_facts),
                )
        })
        .map(|workspace| workspace.id)
        .collect();

    Ok(Json(DetectWorkspaceResponse {
        identity_kind: detected.identity_kind,
        identity_payload: detected.identity_payload,
        binding: WorkspaceBindingResponse::from(detected.binding),
        confidence: detected.confidence,
        warnings: detected.warnings,
        matched_workspace_ids,
    }))
}

pub async fn detect_git(
    State(state): State<Arc<AppState>>,
    Json(req): Json<DetectGitRequest>,
) -> Result<Json<DetectGitResponse>, ApiError> {
    let root_ref = req.root_ref.trim();
    if root_ref.is_empty() {
        return Err(ApiError::BadRequest("root_ref 不能为空".into()));
    }

    let backend_id = require_backend_id(req.backend_id.as_deref())?;
    let result = invoke_workspace_setup_detect_git(&state, backend_id, root_ref).await?;

    Ok(Json(DetectGitResponse {
        resolved_root_ref: result.resolved_root_ref,
        is_git_repo: result.is_git_repo,
        source_repo: result.source_repo,
        branch: result.branch,
        commit_hash: result.commit_hash,
    }))
}

pub async fn discover_local_bindings(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    AxumPath(project_id): AxumPath<String>,
    Json(req): Json<DiscoverLocalWorkspaceBindingsRequest>,
) -> Result<Json<DiscoverLocalWorkspaceBindingsResponse>, ApiError> {
    let project_id = parse_project_id(&project_id)?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Configure,
    )
    .await?;

    let backend_id = normalize_required_string("backend_id", &req.backend_id)?;
    ensure_local_project_backend_access(&state, project_id, &backend_id).await?;

    let workspaces = state
        .repos
        .workspace_repo
        .list_by_project(project_id)
        .await?;
    let workspace_names = workspaces
        .iter()
        .map(|workspace| (workspace.id, workspace.name.clone()))
        .collect::<HashMap<_, _>>();
    if workspaces.is_empty() {
        return Ok(Json(DiscoverLocalWorkspaceBindingsResponse {
            backend_id,
            candidates: Vec::new(),
            skipped: Vec::new(),
            warnings: Vec::new(),
        }));
    }

    let input_workspaces = workspaces
        .iter()
        .map(|workspace| WorkspaceDiscoverByIdentityWorkspaceInput {
            workspace_id: workspace.id,
            identity_kind: workspace.identity_kind.clone(),
            identity_payload: workspace.identity_payload.clone(),
        })
        .collect();
    let discovered = invoke_workspace_discover_by_identity(
        &state,
        Some(current_user.user_id.as_str()),
        project_id,
        &backend_id,
        input_workspaces,
    )
    .await?;

    let candidates = discovered
        .candidates
        .into_iter()
        .filter_map(|candidate| {
            let workspace_name = workspace_names.get(&candidate.workspace_id)?.clone();
            Some(DiscoveredWorkspaceBindingCandidate {
                workspace_id: candidate.workspace_id.to_string(),
                workspace_name,
                root_ref: candidate.root_ref,
                identity_kind: agentdash_contracts::workspace::WorkspaceIdentityKind::from(
                    candidate.identity_kind,
                ),
                identity_payload: candidate.identity_payload,
                detected_facts: candidate.detected_facts,
                confidence: candidate.confidence,
                display_name: candidate.display_name,
                client_name: candidate.client_name,
                server_address: candidate.server_address,
                stream: candidate.stream,
                warnings: candidate.warnings,
            })
        })
        .collect();
    let skipped = discovered
        .skipped
        .into_iter()
        .filter_map(|skipped| {
            let workspace_name = workspace_names.get(&skipped.workspace_id)?.clone();
            Some(WorkspaceIdentityDiscoverySkipped {
                workspace_id: skipped.workspace_id.to_string(),
                workspace_name,
                identity_kind: agentdash_contracts::workspace::WorkspaceIdentityKind::from(
                    skipped.identity_kind,
                ),
                reason: skipped.reason,
                message: skipped.message,
            })
        })
        .collect();

    Ok(Json(DiscoverLocalWorkspaceBindingsResponse {
        backend_id,
        candidates,
        skipped,
        warnings: discovered.warnings,
    }))
}

pub async fn bind_discovered(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    AxumPath(project_id): AxumPath<String>,
    Json(req): Json<BindDiscoveredWorkspaceBindingsRequest>,
) -> Result<Json<BindDiscoveredWorkspaceBindingsResponse>, ApiError> {
    let project_id = parse_project_id(&project_id)?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Configure,
    )
    .await?;

    let bindings = req
        .bindings
        .into_iter()
        .map(|binding| {
            let workspace_id = parse_workspace_id(&binding.workspace_id)?;
            Ok(BindDiscoveredWorkspaceBindingCommand {
                workspace_id,
                backend_id: binding.backend_id,
                root_ref: binding.root_ref,
            })
        })
        .collect::<Result<Vec<_>, ApiError>>()?;
    let placement_runtime = Arc::new(RuntimeGatewayWorkspacePlacementRuntime::new(
        state.services.runtime_gateway.clone(),
    ));
    let result = WorkspacePlacementService::new(state.repos.clone(), placement_runtime)
        .bind_discovered(BindDiscoveredWorkspaceBindingsInput {
            project_id,
            user_id: Some(current_user.user_id),
            bindings,
        })
        .await?;

    Ok(Json(BindDiscoveredWorkspaceBindingsResponse {
        backend_id: result.backend_id,
        workspaces: result
            .workspaces
            .into_iter()
            .map(WorkspaceResponse::from)
            .collect(),
        bound_workspace_ids: result
            .bound_workspace_ids
            .into_iter()
            .map(|id| id.to_string())
            .collect(),
        created_bindings: result.created_bindings,
        updated_bindings: result.updated_bindings,
        inventory_items: result
            .inventory_items
            .into_iter()
            .map(BackendWorkspaceInventoryResponse::from)
            .collect(),
        warnings: result.warnings,
    }))
}

async fn ensure_local_project_backend_access(
    state: &Arc<AppState>,
    project_id: Uuid,
    backend_id: &str,
) -> Result<agentdash_domain::backend::ProjectBackendAccess, ApiError> {
    let access = ensure_project_backend_access(state, project_id, backend_id).await?;
    if !access.is_active() {
        return Err(ApiError::Conflict("ProjectBackendAccess 当前未启用".into()));
    }
    let backend = state
        .repos
        .backend_repo
        .get_backend(&access.backend_id)
        .await?;
    if backend.backend_type != BackendType::Local {
        return Err(ApiError::BadRequest(
            "本机 Workspace discovery 仅支持 local backend".into(),
        ));
    }
    Ok(access)
}

fn normalize_required_string(field: &str, raw: &str) -> Result<String, ApiError> {
    let value = raw.trim();
    if value.is_empty() {
        return Err(ApiError::BadRequest(format!("{field} 不能为空")));
    }
    Ok(value.to_string())
}

async fn invoke_workspace_discover_by_identity(
    state: &Arc<AppState>,
    user_id: Option<&str>,
    project_id: Uuid,
    backend_id: &str,
    workspaces: Vec<WorkspaceDiscoverByIdentityWorkspaceInput>,
) -> Result<WorkspaceDiscoverByIdentityOutput, ApiError> {
    let input = serde_json::to_value(WorkspaceDiscoverByIdentityInput {
        backend_id: backend_id.to_string(),
        workspaces,
    })
    .map_err(|error| {
        ApiError::BadRequest(format!("workspace.discover_by_identity 输入非法: {error}"))
    })?;
    let request = RuntimeInvocationRequest::new(
        RuntimeActionKey::parse(WORKSPACE_DISCOVER_BY_IDENTITY_ACTION).map_err(|error| {
            ApiError::Internal(format!("内置 Runtime Action Key 非法: {error}"))
        })?,
        RuntimeActor::PlatformUser {
            user_id: user_id.map(str::to_string),
        },
        RuntimeContext::Setup {
            project_id: Some(project_id),
            workspace_id: None,
            backend_id: Some(backend_id.to_string()),
            root_ref: None,
        },
        input,
    );
    let invocation = state.services.runtime_gateway.invoke(request).await?;
    serde_json::from_value::<WorkspaceDiscoverByIdentityOutput>(invocation.output.output).map_err(
        |error| {
            ApiError::Internal(format!(
                "workspace.discover_by_identity 返回值解析失败: {error}"
            ))
        },
    )
}

fn binding_input_to_binding(
    workspace_id: Uuid,
    binding: WorkspaceBindingInput,
) -> Result<WorkspaceBinding, ApiError> {
    let backend_id = binding.backend_id.trim().to_string();
    if backend_id.is_empty() {
        return Err(ApiError::BadRequest("binding.backend_id 不能为空".into()));
    }
    let root_ref = binding.root_ref.trim().to_string();
    if root_ref.is_empty() {
        return Err(ApiError::BadRequest("binding.root_ref 不能为空".into()));
    }

    let mut created = WorkspaceBinding::new(
        workspace_id,
        backend_id,
        root_ref,
        binding.detected_facts.unwrap_or_else(|| json!({})),
    );
    if let Some(id) = binding.id {
        created.id = id;
    }
    created.status = binding.status.unwrap_or(WorkspaceBindingStatus::Pending);
    created.priority = binding.priority.unwrap_or_default();
    Ok(created)
}

async fn invoke_workspace_setup_detect(
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
            project_id: (project_id != Uuid::nil()).then_some(project_id),
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

fn normalize_workspace_name(raw: &str) -> Result<String, ApiError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ApiError::BadRequest("工作空间名称不能为空".into()));
    }
    Ok(trimmed.to_string())
}

fn parse_project_id(raw: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(raw).map_err(|_| ApiError::BadRequest("无效的 Project ID".into()))
}

fn parse_workspace_id(raw: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(raw).map_err(|_| ApiError::BadRequest("无效的 Workspace ID".into()))
}

fn require_backend_id(raw: Option<&str>) -> Result<&str, ApiError> {
    raw.map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::BadRequest("detect_git 必须显式提供 backend_id".into()))
}

async fn invoke_workspace_setup_detect_git(
    state: &Arc<AppState>,
    backend_id: &str,
    root_ref: &str,
) -> Result<WorkspaceDetectGitOutput, ApiError> {
    let input = serde_json::to_value(WorkspaceDetectGitInput {
        backend_id: backend_id.to_string(),
        root_ref: root_ref.to_string(),
    })
    .map_err(|error| ApiError::BadRequest(format!("workspace.detect_git 输入非法: {error}")))?;
    let request = RuntimeInvocationRequest::new(
        RuntimeActionKey::parse(WORKSPACE_DETECT_GIT_ACTION).map_err(|error| {
            ApiError::Internal(format!("内置 Runtime Action Key 非法: {error}"))
        })?,
        RuntimeActor::PlatformUser { user_id: None },
        RuntimeContext::Setup {
            project_id: None,
            workspace_id: None,
            backend_id: Some(backend_id.to_string()),
            root_ref: Some(root_ref.to_string()),
        },
        input,
    );
    let invocation = state.services.runtime_gateway.invoke(request).await?;
    serde_json::from_value::<WorkspaceDetectGitOutput>(invocation.output.output).map_err(|error| {
        ApiError::Internal(format!("workspace.detect_git 返回值解析失败: {error}"))
    })
}
