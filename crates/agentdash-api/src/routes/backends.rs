use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::HeaderMap;

use agentdash_contracts::core::DeletedIdResponse;
use agentdash_domain::DomainError;
use agentdash_domain::backend::{
    BackendConfig, BackendExecutionLease, BackendRepository, BackendShareScopeKind, BackendType,
    BackendVisibility, RuntimeHealth,
};
use agentdash_domain::project::ProjectRepository;

use crate::app_state::AppState;
use crate::auth::CurrentUser;
use crate::dto::{
    BackendActiveSessionResponse, BackendRuntimeExecutorResponse, BackendRuntimeSummaryResponse,
    BackendWithStatus, BrowseDirectoryEntryResponse, BrowseDirectoryRequest,
    BrowseDirectoryResponse, CreateBackendRequest, EnsureLocalRuntimeRequest,
    EnsureLocalRuntimeResponse, RuntimeHealthResponse,
};
use crate::relay::registry::OnlineBackendInfo;
use crate::rpc::ApiError;
use agentdash_application::backend::{
    BackendAuthorizationService, BackendPermission, CreateBackendInput, EnsureLocalRuntimeInput,
    LocalRuntimeScopeInput, add_backend_record, can_manage_global_backend_scope,
    ensure_local_runtime_record, remove_backend_record,
};
use agentdash_application::runtime_gateway::{
    RuntimeActionKey, RuntimeActor, RuntimeContext, RuntimeInvocationRequest,
    WORKSPACE_BROWSE_DIRECTORY_ACTION, WorkspaceBrowseDirectoryInput,
    WorkspaceBrowseDirectoryOutput,
};

fn backend_authz(
    state: &AppState,
) -> BackendAuthorizationService<'_, dyn BackendRepository, dyn ProjectRepository> {
    BackendAuthorizationService::new(
        state.repos.backend_repo.as_ref(),
        state.repos.project_repo.as_ref(),
    )
}

pub fn router() -> axum::Router<std::sync::Arc<crate::app_state::AppState>> {
    axum::Router::new()
        .route(
            "/backends",
            axum::routing::get(list_backends).post(add_backend),
        )
        .route(
            "/local-runtime/ensure",
            axum::routing::post(ensure_local_runtime),
        )
        .route(
            "/backends/runtime-health",
            axum::routing::get(list_runtime_health),
        )
        .route(
            "/backends/runtime-summary",
            axum::routing::get(list_runtime_summary),
        )
        .route(
            "/backends/{id}",
            axum::routing::get(get_backend).delete(remove_backend),
        )
        .route(
            "/backends/{id}/runtime-health",
            axum::routing::get(get_runtime_health),
        )
        .route("/backends/online", axum::routing::get(list_online_backends))
        .route(
            "/backends/{backend_id}/browse",
            axum::routing::post(browse_directory),
        )
}

pub async fn list_backends(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
) -> Result<Json<Vec<BackendWithStatus>>, ApiError> {
    let backends = state.repos.backend_repo.list_backends().await?;
    let backend_authz = backend_authz(state.as_ref());
    let backends = backend_authz
        .filter_backends(&current_user, backends)
        .await?;
    let online_list = state.services.backend_registry.list_online().await;
    let runtime_health = state
        .repos
        .runtime_health_repo
        .list_runtime_health()
        .await?;
    let runtime_health_by_backend = runtime_health
        .into_iter()
        .map(|health| (health.backend_id.clone(), health))
        .collect::<HashMap<_, _>>();
    let mut result = Vec::with_capacity(backends.len() + online_list.len());

    let mut seen_ids = HashSet::new();

    for b in backends {
        seen_ids.insert(b.id.clone());
        let online_info = online_list.iter().find(|o| o.backend_id == b.id);
        let runtime_health = runtime_health_by_backend
            .get(&b.id)
            .cloned()
            .map(|health| runtime_health_response(health, online_info.is_some()));
        result.push(BackendWithStatus {
            online: online_info.is_some(),
            runtime_health,
            workspace_roots: online_info.map(|o| o.workspace_roots.clone()),
            capabilities: online_info.map(|o| o.capabilities.clone()),
            config: b,
        });
    }

    for o in &online_list {
        if seen_ids.contains(&o.backend_id) {
            continue;
        }
        if !can_manage_global_backend_scope(&current_user) {
            continue;
        }
        let runtime_health = runtime_health_by_backend
            .get(&o.backend_id)
            .cloned()
            .map(|health| runtime_health_response(health, true));
        result.push(BackendWithStatus {
            online: true,
            runtime_health,
            workspace_roots: Some(o.workspace_roots.clone()),
            capabilities: Some(o.capabilities.clone()),
            config: BackendConfig {
                id: o.backend_id.clone(),
                name: o.name.clone(),
                endpoint: String::new(),
                auth_token: None,
                enabled: true,
                backend_type: BackendType::Remote,
                owner_user_id: None,
                profile_id: None,
                device_id: None,
                machine_id: None,
                machine_label: None,
                legacy_machine_ids: Vec::new(),
                visibility: BackendVisibility::Private,
                share_scope_kind: BackendShareScopeKind::User,
                share_scope_id: None,
                capability_slot: "default".to_string(),
                device: serde_json::json!({}),
                last_claimed_at: None,
            },
        });
    }

    Ok(Json(result))
}

pub async fn list_runtime_health(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
) -> Result<Json<Vec<RuntimeHealthResponse>>, ApiError> {
    let online_ids = state
        .services
        .backend_registry
        .list_online_ids()
        .await
        .into_iter()
        .collect::<HashSet<_>>();
    let backend_authz = backend_authz(state.as_ref());
    let visible_backend_ids = backend_authz.visible_backend_ids(&current_user).await?;
    let items = state
        .repos
        .runtime_health_repo
        .list_runtime_health()
        .await?
        .into_iter()
        .filter(|health| {
            can_manage_global_backend_scope(&current_user)
                || visible_backend_ids.contains(&health.backend_id)
        })
        .map(|health| {
            let online = online_ids.contains(&health.backend_id);
            runtime_health_response(health, online)
        })
        .collect();
    Ok(Json(items))
}

pub async fn get_runtime_health(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<RuntimeHealthResponse>, ApiError> {
    if !can_manage_global_backend_scope(&current_user) {
        let backend_authz = backend_authz(state.as_ref());
        backend_authz
            .require_backend(&current_user, &id, BackendPermission::View)
            .await?;
    }
    let health = state
        .repos
        .runtime_health_repo
        .get_runtime_health(&id)
        .await?
        .ok_or_else(|| DomainError::NotFound {
            entity: "runtime_health",
            id: id.clone(),
        })?;
    let online = state.services.backend_registry.is_online(&id).await;
    Ok(Json(runtime_health_response(health, online)))
}

pub async fn list_runtime_summary(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
) -> Result<Json<Vec<BackendRuntimeSummaryResponse>>, ApiError> {
    let backend_authz = backend_authz(state.as_ref());
    let mut backends = backend_authz
        .filter_backends(
            &current_user,
            state.repos.backend_repo.list_backends().await?,
        )
        .await?;
    let online_list = state.services.backend_registry.list_online().await;
    let runtime_health_by_backend = state
        .repos
        .runtime_health_repo
        .list_runtime_health()
        .await?
        .into_iter()
        .map(|health| (health.backend_id.clone(), health))
        .collect::<HashMap<_, _>>();
    let active_leases = state
        .repos
        .backend_execution_lease_repo
        .list_active()
        .await?;
    let active_leases_by_backend = active_leases.into_iter().fold(
        HashMap::<String, Vec<BackendExecutionLease>>::new(),
        |mut acc, lease| {
            acc.entry(lease.backend_id.clone()).or_default().push(lease);
            acc
        },
    );

    let mut seen_ids = backends
        .iter()
        .map(|backend| backend.id.clone())
        .collect::<HashSet<_>>();
    if can_manage_global_backend_scope(&current_user) {
        for online in &online_list {
            if seen_ids.insert(online.backend_id.clone()) {
                backends.push(online_backend_config(online));
            }
        }
    }

    let summaries = backends
        .into_iter()
        .map(|backend| {
            let online_info = online_list
                .iter()
                .find(|online| online.backend_id == backend.id);
            let online = online_info.is_some();
            let runtime_health = runtime_health_by_backend
                .get(&backend.id)
                .cloned()
                .map(|health| runtime_health_response(health, online));
            let active_sessions = active_leases_by_backend
                .get(&backend.id)
                .cloned()
                .unwrap_or_default();
            let executors = backend_runtime_executors(online_info, &active_sessions);
            let allocatable =
                backend.enabled && online && executors.iter().any(|executor| executor.allocatable);
            BackendRuntimeSummaryResponse {
                backend_id: backend.id,
                name: backend.name,
                enabled: backend.enabled,
                online,
                runtime_health,
                active_session_count: active_sessions.len(),
                active_sessions: active_sessions
                    .into_iter()
                    .map(active_session_response)
                    .collect(),
                executors,
                allocatable,
            }
        })
        .collect();
    Ok(Json(summaries))
}

/// 列出通过 WebSocket 连接的在线后端
pub async fn list_online_backends(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
) -> Result<Json<Vec<OnlineBackendInfo>>, ApiError> {
    let mut online = state.services.backend_registry.list_online().await;
    if !can_manage_global_backend_scope(&current_user) {
        let backend_authz = backend_authz(state.as_ref());
        let visible_backend_ids = backend_authz.visible_backend_ids(&current_user).await?;
        online.retain(|backend| visible_backend_ids.contains(&backend.backend_id));
    }
    Ok(Json(online))
}

fn runtime_health_response(health: RuntimeHealth, online: bool) -> RuntimeHealthResponse {
    RuntimeHealthResponse {
        backend_id: health.backend_id,
        profile_id: health.profile_id,
        name: health.name,
        status: health.status,
        online,
        version: health.version,
        capabilities: health.capabilities,
        workspace_roots: health.workspace_roots,
        device: health.device,
        connected_at: health.connected_at,
        last_seen_at: health.last_seen_at,
        disconnected_at: health.disconnected_at,
        disconnect_reason: health.disconnect_reason,
        created_at: health.created_at,
        updated_at: health.updated_at,
    }
}

fn online_backend_config(online: &OnlineBackendInfo) -> BackendConfig {
    BackendConfig {
        id: online.backend_id.clone(),
        name: online.name.clone(),
        endpoint: String::new(),
        auth_token: None,
        enabled: true,
        backend_type: BackendType::Remote,
        owner_user_id: None,
        profile_id: None,
        device_id: None,
        machine_id: None,
        machine_label: None,
        legacy_machine_ids: Vec::new(),
        visibility: BackendVisibility::Private,
        share_scope_kind: BackendShareScopeKind::User,
        share_scope_id: None,
        capability_slot: "default".to_string(),
        device: serde_json::json!({}),
        last_claimed_at: None,
    }
}

fn backend_runtime_executors(
    online_info: Option<&OnlineBackendInfo>,
    active_sessions: &[BackendExecutionLease],
) -> Vec<BackendRuntimeExecutorResponse> {
    let Some(online_info) = online_info else {
        return Vec::new();
    };
    online_info
        .capabilities
        .executors
        .iter()
        .map(|executor| {
            let active_session_count = active_sessions
                .iter()
                .filter(|lease| lease.executor_id.eq_ignore_ascii_case(&executor.id))
                .count();
            BackendRuntimeExecutorResponse {
                executor_id: executor.id.clone(),
                name: executor.name.clone(),
                variants: executor.variants.clone(),
                available: executor.available,
                active_session_count,
                allocatable: executor.available,
            }
        })
        .collect()
}

fn active_session_response(lease: BackendExecutionLease) -> BackendActiveSessionResponse {
    BackendActiveSessionResponse {
        lease_id: lease.id,
        session_id: lease.session_id,
        turn_id: lease.turn_id,
        executor_id: lease.executor_id,
        workspace_id: lease.workspace_id,
        root_ref: lease.root_ref,
        selection_mode: lease.selection_mode,
        state: lease.state,
        claimed_at: lease.claimed_at,
        activated_at: lease.activated_at,
        last_seen_at: lease.last_seen_at,
    }
}

pub async fn get_backend(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<BackendConfig>, ApiError> {
    let backend_authz = backend_authz(state.as_ref());
    let backend = backend_authz
        .require_backend(&current_user, &id, BackendPermission::View)
        .await?;
    Ok(Json(backend))
}

pub async fn add_backend(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Json(req): Json<CreateBackendRequest>,
) -> Result<Json<BackendConfig>, ApiError> {
    let config = add_backend_record(
        &state.repos,
        &current_user,
        CreateBackendInput {
            id: req.id,
            name: req.name,
            endpoint: req.endpoint,
            auth_token: req.auth_token,
            backend_type: req.backend_type,
        },
    )
    .await?;
    Ok(Json(config))
}

pub async fn ensure_local_runtime(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    headers: HeaderMap,
    Json(req): Json<EnsureLocalRuntimeRequest>,
) -> Result<Json<EnsureLocalRuntimeResponse>, ApiError> {
    let relay_ws_url = relay_ws_url_from_headers(&headers);
    let result = ensure_local_runtime_record(
        &state.repos,
        EnsureLocalRuntimeInput {
            current_user_id: current_user.user_id.clone(),
            machine_id: req.machine_id,
            machine_label: req.machine_label,
            legacy_machine_ids: req.legacy_machine_ids,
            profile_id: req.profile_id,
            scope: req.scope.map(|scope| LocalRuntimeScopeInput {
                kind: scope.kind,
                id: scope.id,
            }),
            capability_slot: req.capability_slot,
            name: req.name,
            workspace_roots: req.workspace_roots,
            executor_enabled: req.executor_enabled,
            client_version: req.client_version,
            device: req.device,
            rotate_token: req.rotate_token,
            relay_ws_url: relay_ws_url.clone(),
        },
    )
    .await?;

    Ok(Json(EnsureLocalRuntimeResponse {
        backend_id: result.backend.id,
        name: result.backend.name,
        relay_ws_url: result.backend.endpoint,
        auth_token: result.auth_token,
        backend_enabled: result.backend.enabled,
        profile_id: result.profile_id,
        machine_id: result.machine_id,
        machine_label: result.machine_label,
        visibility: result.backend.visibility,
        share_scope_kind: result.backend.share_scope_kind,
        share_scope_id: result.share_scope_id,
        capability_slot: result.capability_slot,
    }))
}

fn relay_ws_url_from_headers(headers: &HeaderMap) -> String {
    let host = headers
        .get("x-forwarded-host")
        .or_else(|| headers.get("host"))
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("127.0.0.1:3001");
    let proto = headers
        .get("x-forwarded-proto")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.split(',').next().unwrap_or(value).trim())
        .unwrap_or("http");
    let ws_scheme = if proto.eq_ignore_ascii_case("https") {
        "wss"
    } else {
        "ws"
    };
    format!("{ws_scheme}://{host}/ws/backend")
}

pub async fn remove_backend(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<DeletedIdResponse>, ApiError> {
    remove_backend_record(&state.repos, &current_user, &id).await?;
    Ok(Json(DeletedIdResponse { deleted: id }))
}

// ─── 目录浏览 ─────────────────────────────────────────────

pub async fn browse_directory(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(backend_id): Path<String>,
    Json(req): Json<BrowseDirectoryRequest>,
) -> Result<Json<BrowseDirectoryResponse>, ApiError> {
    let backend_id = backend_id.trim();
    if backend_id.is_empty() {
        return Err(ApiError::BadRequest("backend_id 不能为空".into()));
    }
    let backend_authz = backend_authz(state.as_ref());
    let backend = backend_authz
        .require_backend(&current_user, backend_id, BackendPermission::View)
        .await?;

    let input = serde_json::to_value(WorkspaceBrowseDirectoryInput {
        backend_id: backend.id.clone(),
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
            project_id: None,
            workspace_id: None,
            backend_id: Some(backend.id),
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
