use std::collections::HashSet;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::HeaderMap;

use agentdash_contracts::backend::{
    BackendActiveSessionResponse, BackendResponse, BackendRuntimeExecutorResponse,
    BackendRuntimeSummaryResponse, CapabilityHealthAction, CapabilityHealthDomain,
    CapabilityHealthItem, CapabilityHealthStatus,
};
use agentdash_contracts::common_response::DeletedIdResponse;
use agentdash_domain::DomainError;
use agentdash_domain::backend::{
    BackendExecutionLease, BackendRepository, ProjectBackendAccessRepository, RuntimeHealth,
};
use agentdash_domain::project::ProjectRepository;

use crate::app_state::AppState;
use crate::auth::CurrentUser;
use crate::dto::{
    BackendWithStatus, BrowseDirectoryEntryResponse, BrowseDirectoryRequest,
    BrowseDirectoryResponse, CreateBackendRequest, EnsureLocalRuntimeRequest,
    EnsureLocalRuntimeResponse, RuntimeHealthResponse, backend_capabilities_response,
    backend_response,
};
use crate::relay::registry::OnlineBackendInfo;
use crate::routes::release_info;
use crate::rpc::ApiError;
use agentdash_application::backend::{
    BackendAuthorizationService, BackendPermission, BackendRuntimeExecutorSnapshot,
    BackendRuntimeExecutorSummary, BackendRuntimeOnlineSnapshot, BackendRuntimeSummary,
    CreateBackendInput, EnsureLocalRuntimeInput, LocalRuntimeScopeInput, add_backend_record,
    can_manage_global_backend_scope, ensure_local_runtime_record, list_backend_runtime_summaries,
    remove_backend_record,
};
use agentdash_relay::CapabilitiesPayload;
use agentdash_application_runtime_gateway::{
    RuntimeActionKey, RuntimeActor, RuntimeContext, RuntimeInvocationRequest,
    WORKSPACE_BROWSE_DIRECTORY_ACTION, WorkspaceBrowseDirectoryInput,
    WorkspaceBrowseDirectoryOutput,
};

fn backend_authz(
    state: &AppState,
) -> BackendAuthorizationService<
    '_,
    dyn BackendRepository,
    dyn ProjectRepository,
    dyn ProjectBackendAccessRepository,
> {
    BackendAuthorizationService::new(
        state.repos.backend_repo.as_ref(),
        state.repos.project_repo.as_ref(),
        state.repos.project_backend_access_repo.as_ref(),
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
    let backend_authz = backend_authz(state.as_ref());
    let backends = backend_authz
        .filter_backends(
            &current_user,
            state.repos.backend_repo.list_backends().await?,
        )
        .await?;
    let online_list = state.services.backend_registry.list_online().await;
    let summaries = list_backend_runtime_summaries(
        state.repos.runtime_health_repo.as_ref(),
        state.repos.backend_execution_lease_repo.as_ref(),
        backends,
        online_list
            .into_iter()
            .map(online_backend_snapshot)
            .collect(),
        can_manage_global_backend_scope(&current_user),
    )
    .await?;
    let result = summaries
        .into_iter()
        .map(backend_with_status_from_summary)
        .collect();

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
    let backends = backend_authz
        .filter_backends(
            &current_user,
            state.repos.backend_repo.list_backends().await?,
        )
        .await?;
    let online_list = state.services.backend_registry.list_online().await;
    let summaries = list_backend_runtime_summaries(
        state.repos.runtime_health_repo.as_ref(),
        state.repos.backend_execution_lease_repo.as_ref(),
        backends,
        online_list
            .into_iter()
            .map(online_backend_snapshot)
            .collect(),
        can_manage_global_backend_scope(&current_user),
    )
    .await?
    .into_iter()
    .map(backend_runtime_summary_response)
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
        status: health.status.into(),
        online,
        version: health.version,
        capabilities: health.capabilities,
        device: health.device,
        connected_at: health.connected_at,
        last_seen_at: health.last_seen_at,
        disconnected_at: health.disconnected_at,
        disconnect_reason: health.disconnect_reason,
        created_at: health.created_at,
        updated_at: health.updated_at,
    }
}

fn active_session_response(lease: BackendExecutionLease) -> BackendActiveSessionResponse {
    BackendActiveSessionResponse {
        lease_id: lease.id.to_string(),
        session_id: lease.session_id,
        turn_id: lease.turn_id,
        executor_id: lease.executor_id,
        workspace_id: lease.workspace_id.map(|id| id.to_string()),
        root_ref: lease.root_ref,
        selection_mode: lease.selection_mode.into(),
        state: lease.state.into(),
        claimed_at: lease.claimed_at,
        activated_at: lease.activated_at,
        last_seen_at: lease.last_seen_at,
    }
}

fn online_backend_snapshot(online: OnlineBackendInfo) -> BackendRuntimeOnlineSnapshot {
    let capabilities = online.capabilities;
    BackendRuntimeOnlineSnapshot {
        backend_id: online.backend_id,
        name: online.name,
        executors: capabilities
            .executors
            .iter()
            .map(|executor| BackendRuntimeExecutorSnapshot {
                executor_id: executor.id.clone(),
                name: executor.name.clone(),
                variants: executor.variants.clone(),
                available: executor.available,
            })
            .collect(),
        capabilities,
    }
}

fn backend_with_status_from_summary(summary: BackendRuntimeSummary) -> BackendWithStatus {
    BackendWithStatus {
        online: summary.online,
        runtime_health: summary
            .runtime_health
            .map(|health| runtime_health_response(health, summary.online)),
        capabilities: summary.capabilities.map(backend_capabilities_response),
        backend: backend_response(summary.backend),
    }
}

fn backend_runtime_summary_response(
    summary: BackendRuntimeSummary,
) -> BackendRuntimeSummaryResponse {
    let capability_health =
        build_capability_health(&summary.capabilities, summary.online, &summary.executors);
    BackendRuntimeSummaryResponse {
        backend_id: summary.backend_id,
        name: summary.name,
        enabled: summary.enabled,
        online: summary.online,
        runtime_health: summary
            .runtime_health
            .map(|health| runtime_health_response(health, summary.online)),
        active_session_count: summary.active_session_count,
        active_sessions: summary
            .active_sessions
            .into_iter()
            .map(active_session_response)
            .collect(),
        executors: summary
            .executors
            .into_iter()
            .map(|executor| BackendRuntimeExecutorResponse {
                executor_id: executor.executor_id,
                name: executor.name,
                variants: executor.variants,
                available: executor.available,
                active_session_count: executor.active_session_count,
                allocatable: executor.allocatable,
            })
            .collect(),
        capability_health,
        allocatable: summary.allocatable,
    }
}

fn build_capability_health(
    capabilities: &Option<CapabilitiesPayload>,
    online: bool,
    executors: &[BackendRuntimeExecutorSummary],
) -> Vec<CapabilityHealthItem> {
    let mut items: Vec<CapabilityHealthItem> = Vec::new();

    // MCP health from relay payload
    if let Some(caps) = capabilities {
        for relay_item in &caps.capability_health {
            if let (Ok(domain), Ok(status)) = (
                relay_item.domain.parse::<CapabilityHealthDomain>(),
                relay_item.status.parse::<CapabilityHealthStatus>(),
            ) {
                items.push(CapabilityHealthItem {
                    id: relay_item.id.clone(),
                    domain,
                    status,
                    label: relay_item.label.clone(),
                    summary: relay_item.summary.clone(),
                    actions: relay_item
                        .actions
                        .iter()
                        .map(|a| CapabilityHealthAction {
                            kind: a.kind.clone(),
                            label: a.label.clone(),
                        })
                        .collect(),
                });
            }
        }
    }

    // Executor health derived from summary
    for executor in executors {
        let (status, summary) = if !online {
            (CapabilityHealthStatus::Unavailable, "Runtime 离线".to_string())
        } else if !executor.available {
            (CapabilityHealthStatus::Degraded, "不可用".to_string())
        } else if !executor.allocatable {
            (CapabilityHealthStatus::Degraded, "不可分配".to_string())
        } else {
            (CapabilityHealthStatus::Ready, "就绪".to_string())
        };
        if status != CapabilityHealthStatus::Ready {
            items.push(CapabilityHealthItem {
                id: format!("executor:{}", executor.executor_id),
                domain: CapabilityHealthDomain::Executor,
                status,
                label: executor.name.clone(),
                summary,
                actions: Vec::new(),
            });
        }
    }

    items
}

pub async fn get_backend(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<BackendResponse>, ApiError> {
    let backend_authz = backend_authz(state.as_ref());
    let backend = backend_authz
        .require_backend(&current_user, &id, BackendPermission::View)
        .await?;
    Ok(Json(backend_response(backend)))
}

pub async fn add_backend(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Json(req): Json<CreateBackendRequest>,
) -> Result<Json<BackendResponse>, ApiError> {
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
    Ok(Json(backend_response(config)))
}

pub async fn ensure_local_runtime(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    headers: HeaderMap,
    Json(req): Json<EnsureLocalRuntimeRequest>,
) -> Result<Json<EnsureLocalRuntimeResponse>, ApiError> {
    let relay_ws_url = release_info::configured_relay_ws_url_from_env()
        .unwrap_or_else(|| relay_ws_url_from_headers(&headers));
    let result = ensure_local_runtime_record(
        &state.repos,
        EnsureLocalRuntimeInput {
            current_user_id: current_user.user_id.clone(),
            machine_id: req.machine_id,
            machine_label: req.machine_label,
            profile_id: req.profile_id,
            scope: req.scope.map(|scope| LocalRuntimeScopeInput {
                kind: scope.kind,
                id: scope.id,
            }),
            capability_slot: req.capability_slot,
            name: req.name,
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
        share_scope_kind: result.share_scope_kind,
        share_scope_id: result.share_scope_id,
        capability_slot: result.capability_slot,
        registration_source: result.registration_source,
        claimed_at: result.claimed_at,
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
