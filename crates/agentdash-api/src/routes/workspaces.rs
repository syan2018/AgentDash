use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path as AxumPath, State};
use chrono::Utc;
use serde_json::{Value, json};
use uuid::Uuid;

use agentdash_application::workspace::WorkspaceDetectionResult;
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
use agentdash_domain::backend::{
    BackendType, BackendWorkspaceInventory, BackendWorkspaceInventorySource,
    BackendWorkspaceInventoryStatus,
};
use agentdash_domain::workspace::{
    P4WorkspaceIdentityContract, P4WorkspaceMatchMode, Workspace, WorkspaceBinding,
    WorkspaceBindingStatus, WorkspaceIdentityKind, WorkspaceResolutionPolicy, WorkspaceStatus,
    identity_payload_matches, normalize_identity_payload,
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
        ProjectPermission::View,
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
        ProjectPermission::Edit,
    )
    .await?;

    let workspace_name = normalize_workspace_name(&req.name)?;
    let (identity_kind, identity_payload, initial_bindings) = derive_workspace_shape(
        &state,
        project_id,
        Some(current_user.user_id.as_str()),
        req.identity_kind,
        req.identity_payload,
        req.bindings,
        req.shortcut_binding,
    )
    .await?;

    let mut workspace = Workspace::new(
        project_id,
        workspace_name,
        identity_kind,
        identity_payload,
        req.resolution_policy
            .unwrap_or(WorkspaceResolutionPolicy::PreferOnline),
    );
    workspace.set_bindings(initial_bindings);
    workspace.default_binding_id = req.default_binding_id.or(workspace.default_binding_id);
    workspace.mount_capabilities = req.mount_capabilities.unwrap_or_default();
    workspace.status = derive_workspace_status(&workspace.bindings);
    workspace.refresh_default_binding();

    state.repos.workspace_repo.create(&workspace).await?;
    let stored = state
        .repos
        .workspace_repo
        .get_by_id(workspace.id)
        .await?
        .ok_or_else(|| ApiError::Internal("Workspace 创建后读取失败".into()))?;
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
        ProjectPermission::View,
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
    let (mut workspace, _) = load_workspace_and_project_with_permission(
        state.as_ref(),
        &current_user,
        workspace_id,
        ProjectPermission::Edit,
    )
    .await?;

    if let Some(name) = req.name {
        workspace.name = normalize_workspace_name(&name)?;
    }
    if let Some(identity_kind) = req.identity_kind {
        workspace.identity_kind = identity_kind;
    }
    if let Some(identity_payload) = req.identity_payload {
        workspace.identity_payload = normalize_workspace_identity_payload(
            workspace.identity_kind.clone(),
            identity_payload,
        )?;
    }
    if let Some(resolution_policy) = req.resolution_policy {
        workspace.resolution_policy = resolution_policy;
    }
    if let Some(bindings) = req.bindings {
        let next_bindings = bindings
            .into_iter()
            .map(|binding| binding_input_to_binding(workspace.id, binding))
            .collect::<Result<Vec<_>, _>>()?;
        for binding in &next_bindings {
            ensure_project_backend_access(&state, workspace.project_id, &binding.backend_id)
                .await?;
        }
        workspace.set_bindings(next_bindings);
    }
    if let Some(default_binding_id) = req.default_binding_id {
        workspace.default_binding_id = Some(default_binding_id);
    }
    if let Some(mount_capabilities) = req.mount_capabilities {
        workspace.mount_capabilities = mount_capabilities;
    }
    workspace.status = derive_workspace_status(&workspace.bindings);
    workspace.refresh_default_binding();

    state.repos.workspace_repo.update(&workspace).await?;
    let stored = state
        .repos
        .workspace_repo
        .get_by_id(workspace.id)
        .await?
        .ok_or_else(|| ApiError::Internal("Workspace 更新后读取失败".into()))?;
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
        ProjectPermission::Edit,
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
        ProjectPermission::Edit,
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
        ProjectPermission::Edit,
    )
    .await?;

    ensure_project_backend_access(&state, project_id, &req.backend_id).await?;
    let detected = invoke_workspace_detect(
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
    let result = invoke_workspace_detect_git(&state, backend_id, root_ref).await?;

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
        ProjectPermission::Edit,
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
        ProjectPermission::Edit,
    )
    .await?;

    if req.bindings.is_empty() {
        return Err(ApiError::BadRequest("bindings 不能为空".into()));
    }
    let mut commands = Vec::new();
    let mut seen = HashSet::new();
    for binding in req.bindings {
        let workspace_id = parse_workspace_id(&binding.workspace_id)?;
        let backend_id = normalize_required_string("binding.backend_id", &binding.backend_id)?;
        let root_ref = normalize_required_string("binding.root_ref", &binding.root_ref)?;
        let key = (workspace_id, binding_unique_key(&backend_id, &root_ref));
        if seen.insert(key) {
            commands.push(BindDiscoveredCommand {
                workspace_id,
                backend_id,
                root_ref,
            });
        }
    }

    let backend_id = commands
        .first()
        .map(|command| command.backend_id.clone())
        .ok_or_else(|| ApiError::BadRequest("bindings 不能为空".into()))?;
    if commands
        .iter()
        .any(|command| command.backend_id != backend_id)
    {
        return Err(ApiError::BadRequest(
            "bind-discovered 单次请求只能绑定同一个 backend".into(),
        ));
    }
    let access = ensure_local_project_backend_access(&state, project_id, &backend_id).await?;

    let mut workspaces = state
        .repos
        .workspace_repo
        .list_by_project(project_id)
        .await?
        .into_iter()
        .map(|workspace| (workspace.id, workspace))
        .collect::<HashMap<_, _>>();
    let mut touched_workspace_ids = HashSet::new();
    let mut created_bindings = 0;
    let mut updated_bindings = 0;
    let mut inventory_items = Vec::new();
    let mut warnings = Vec::new();

    for command in commands {
        let workspace = workspaces
            .get_mut(&command.workspace_id)
            .ok_or_else(|| ApiError::NotFound("Workspace 不存在或不属于当前 Project".into()))?;
        let detected = invoke_workspace_detect(
            &state,
            Some(current_user.user_id.as_str()),
            project_id,
            &command.backend_id,
            &command.root_ref,
        )
        .await?;
        if detected.identity_kind != workspace.identity_kind
            || !discovery_identity_payload_matches(
                workspace.identity_kind.clone(),
                &workspace.identity_payload,
                &detected.identity_payload,
                Some(&detected.binding.detected_facts),
            )
        {
            return Err(ApiError::BadRequest(format!(
                "目录 `{}` 与 Workspace `{}` 的 identity 不匹配",
                command.root_ref, workspace.name
            )));
        }

        warnings.extend(detected.warnings.clone());
        let inventory = inventory_from_detected(
            access.backend_id.clone(),
            detected.binding.root_ref.clone(),
            detected.clone(),
            BackendWorkspaceInventoryStatus::Available,
            BackendWorkspaceInventorySource::IdentityDiscovery,
            None,
        );
        state
            .repos
            .backend_workspace_inventory_repo
            .upsert(&inventory)
            .await?;

        let priority = access.priority;
        let now = Utc::now();
        if let Some(existing) = workspace.bindings.iter_mut().find(|binding| {
            binding.backend_id == command.backend_id
                && binding.root_ref == detected.binding.root_ref
        }) {
            existing.status = WorkspaceBindingStatus::Ready;
            existing.detected_facts = detected.binding.detected_facts;
            existing.last_verified_at = Some(inventory.last_seen_at);
            existing.priority = priority;
            existing.updated_at = now;
            updated_bindings += 1;
        } else {
            let mut binding = WorkspaceBinding::new(
                workspace.id,
                command.backend_id,
                detected.binding.root_ref,
                detected.binding.detected_facts,
            );
            binding.status = WorkspaceBindingStatus::Ready;
            binding.last_verified_at = Some(inventory.last_seen_at);
            binding.priority = priority;
            workspace.bindings.push(binding);
            created_bindings += 1;
        }
        workspace.status = derive_workspace_status(&workspace.bindings);
        workspace.refresh_default_binding();
        touched_workspace_ids.insert(workspace.id);
        inventory_items.push(BackendWorkspaceInventoryResponse::from(inventory));
    }

    let mut stored_workspaces = Vec::new();
    let mut bound_workspace_ids = touched_workspace_ids.into_iter().collect::<Vec<_>>();
    bound_workspace_ids.sort_unstable();
    for workspace_id in &bound_workspace_ids {
        let workspace = workspaces
            .get(workspace_id)
            .ok_or_else(|| ApiError::Internal("Workspace 更新缓存缺失".into()))?;
        state.repos.workspace_repo.update(workspace).await?;
        let stored = state
            .repos
            .workspace_repo
            .get_by_id(*workspace_id)
            .await?
            .ok_or_else(|| ApiError::Internal("Workspace 更新后读取失败".into()))?;
        stored_workspaces.push(WorkspaceResponse::from(stored));
    }

    Ok(Json(BindDiscoveredWorkspaceBindingsResponse {
        backend_id,
        workspaces: stored_workspaces,
        bound_workspace_ids: bound_workspace_ids
            .into_iter()
            .map(|id| id.to_string())
            .collect(),
        created_bindings,
        updated_bindings,
        inventory_items,
        warnings,
    }))
}

#[derive(Debug)]
struct BindDiscoveredCommand {
    workspace_id: Uuid,
    backend_id: String,
    root_ref: String,
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

fn discovery_identity_payload_matches(
    kind: WorkspaceIdentityKind,
    expected_payload: &Value,
    actual_payload: &Value,
    actual_binding_facts: Option<&Value>,
) -> bool {
    if identity_payload_matches(
        kind.clone(),
        expected_payload,
        actual_payload,
        actual_binding_facts,
    ) {
        return true;
    }

    if kind != WorkspaceIdentityKind::P4Workspace {
        return false;
    }
    let Some(relaxed_payload) = relaxed_p4_discovery_payload(expected_payload) else {
        return false;
    };
    identity_payload_matches(
        WorkspaceIdentityKind::P4Workspace,
        &relaxed_payload,
        actual_payload,
        actual_binding_facts,
    )
}

fn relaxed_p4_discovery_payload(expected_payload: &Value) -> Option<Value> {
    let normalized =
        normalize_identity_payload(WorkspaceIdentityKind::P4Workspace, expected_payload).ok()?;
    let mut contract = serde_json::from_value::<P4WorkspaceIdentityContract>(normalized).ok()?;
    if contract.match_mode != P4WorkspaceMatchMode::ServerStreamClient {
        return None;
    }
    contract.match_mode = P4WorkspaceMatchMode::ServerStream;
    contract.client_name = None;
    serde_json::to_value(contract).ok()
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

async fn derive_workspace_shape(
    state: &Arc<AppState>,
    project_id: Uuid,
    user_id: Option<&str>,
    identity_kind: Option<WorkspaceIdentityKind>,
    identity_payload: Option<Value>,
    bindings: Option<Vec<WorkspaceBindingInput>>,
    shortcut_binding: Option<WorkspaceBindingInput>,
) -> Result<(WorkspaceIdentityKind, Value, Vec<WorkspaceBinding>), ApiError> {
    let raw_bindings = if let Some(bindings) = bindings {
        bindings
    } else if let Some(shortcut_binding) = shortcut_binding {
        vec![shortcut_binding]
    } else {
        Vec::new()
    };

    let mut parsed_bindings = raw_bindings
        .into_iter()
        .map(|binding| binding_input_to_binding(Uuid::nil(), binding))
        .collect::<Result<Vec<_>, _>>()?;
    ensure_unique_bindings(&parsed_bindings)?;

    if let Some(identity_kind) = identity_kind {
        let identity_payload = identity_payload.ok_or_else(|| {
            ApiError::BadRequest("显式提供 identity_kind 时，identity_payload 不能为空".into())
        })?;
        return Ok((
            identity_kind.clone(),
            normalize_workspace_identity_payload(identity_kind, identity_payload)?,
            parsed_bindings,
        ));
    }

    let Some(first_binding) = parsed_bindings.first() else {
        return Err(ApiError::BadRequest(
            "创建 Workspace 时，必须提供 identity 或至少一个 binding".into(),
        ));
    };

    if project_id != Uuid::nil() {
        ensure_project_backend_access(state, project_id, &first_binding.backend_id).await?;
    }
    let detected = invoke_workspace_detect(
        state,
        user_id,
        project_id,
        &first_binding.backend_id,
        &first_binding.root_ref,
    )
    .await?;
    let replacement_binding = WorkspaceBinding {
        id: first_binding.id,
        workspace_id: Uuid::nil(),
        backend_id: detected.binding.backend_id,
        root_ref: detected.binding.root_ref,
        status: detected.binding.status,
        detected_facts: detected.binding.detected_facts,
        last_verified_at: detected.binding.last_verified_at,
        priority: first_binding.priority,
        created_at: first_binding.created_at,
        updated_at: first_binding.updated_at,
    };
    parsed_bindings[0] = replacement_binding;
    ensure_unique_bindings(&parsed_bindings)?;

    let detected_identity_kind = detected.identity_kind.clone();
    Ok((
        detected.identity_kind,
        identity_payload
            .map(|payload| {
                normalize_workspace_identity_payload(detected_identity_kind.clone(), payload)
            })
            .transpose()?
            .unwrap_or(detected.identity_payload),
        parsed_bindings,
    ))
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

fn ensure_unique_bindings(bindings: &[WorkspaceBinding]) -> Result<(), ApiError> {
    let mut seen = HashSet::new();
    for binding in bindings {
        let key = binding_unique_key(&binding.backend_id, &binding.root_ref);
        if !seen.insert(key) {
            return Err(ApiError::BadRequest(
                "同一个 Workspace 中不能重复绑定相同 backend/root".into(),
            ));
        }
    }
    Ok(())
}

fn binding_unique_key(backend_id: &str, root_ref: &str) -> String {
    let root = root_ref.trim().replace('\\', "/");
    let root = root.trim_end_matches('/');
    format!("{}:{root}", backend_id.trim())
}

fn derive_workspace_status(bindings: &[WorkspaceBinding]) -> WorkspaceStatus {
    if bindings
        .iter()
        .any(|binding| matches!(binding.status, WorkspaceBindingStatus::Ready))
    {
        WorkspaceStatus::Ready
    } else if bindings
        .iter()
        .any(|binding| matches!(binding.status, WorkspaceBindingStatus::Error))
    {
        WorkspaceStatus::Error
    } else {
        WorkspaceStatus::Pending
    }
}

fn normalize_workspace_identity_payload(
    kind: WorkspaceIdentityKind,
    payload: Value,
) -> Result<Value, ApiError> {
    normalize_identity_payload(kind, &payload).map_err(ApiError::BadRequest)
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

async fn invoke_workspace_detect_git(
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
