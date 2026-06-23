use std::sync::Arc;

use agentdash_application::agent_run::runtime_surface::AgentRunResourceSurfaceQueryError;
use agentdash_application::agent_run::{
    AgentRunRuntimeSurface, AgentRunRuntimeSurfaceQuery, AgentRunRuntimeSurfaceQueryDeps,
    AgentRunRuntimeSurfaceQueryError, AgentRunRuntimeSurfaceQueryPort,
    AgentRunRuntimeSurfaceWithBackend, RuntimeSurfaceQueryPurpose,
};
use agentdash_integration_api::AuthIdentity;
use agentdash_spi::{RuntimeBackendAnchor, Vfs};
use uuid::Uuid;

use crate::app_state::AppState;
use crate::auth::{ProjectPermission, load_project_with_permission};
use crate::rpc::ApiError;

/// API current-surface adapter result for runtime-session consumers.
///
/// Permission stays at the API adapter boundary because route callers hold the
/// authenticated user and must preserve the existing Project View check.
#[derive(Debug, Clone)]
pub(crate) struct ApiCurrentRuntimeSurface {
    pub runtime_session_id: String,
    pub run_id: Uuid,
    pub project_id: Uuid,
    pub agent_id: Uuid,
    pub launch_evidence_frame_id: Uuid,
    pub current_surface_frame_id: Uuid,
    pub vfs: Vfs,
    pub orchestration_id: Option<Uuid>,
    pub node_path: Option<String>,
    pub node_attempt: Option<u32>,
}

#[derive(Debug, Clone)]
pub(crate) struct ApiCurrentRuntimeSurfaceWithBackend {
    pub surface: ApiCurrentRuntimeSurface,
    pub runtime_backend_anchor: RuntimeBackendAnchor,
}

pub(crate) async fn resolve_current_runtime_surface_for_api(
    state: &Arc<AppState>,
    current_user: &AuthIdentity,
    session_id: &str,
    purpose: RuntimeSurfaceQueryPurpose,
) -> Result<ApiCurrentRuntimeSurface, ApiError> {
    ensure_runtime_session_exists(state, session_id).await?;
    let surface = runtime_surface_query(state)
        .current_runtime_surface(session_id, purpose)
        .await
        .map_err(runtime_surface_query_error_to_api)?;
    load_project_with_permission(
        state.as_ref(),
        current_user,
        surface.project_id,
        ProjectPermission::View,
    )
    .await?;
    Ok(ApiCurrentRuntimeSurface::from(surface))
}

pub(crate) async fn resolve_current_runtime_surface_with_backend_for_api(
    state: &Arc<AppState>,
    current_user: &AuthIdentity,
    session_id: &str,
    purpose: RuntimeSurfaceQueryPurpose,
) -> Result<ApiCurrentRuntimeSurfaceWithBackend, ApiError> {
    ensure_runtime_session_exists(state, session_id).await?;
    let surface = runtime_surface_query(state)
        .current_runtime_surface_with_backend(session_id, purpose)
        .await
        .map_err(runtime_surface_query_error_to_api)?;
    load_project_with_permission(
        state.as_ref(),
        current_user,
        surface.surface.project_id,
        ProjectPermission::View,
    )
    .await?;
    Ok(ApiCurrentRuntimeSurfaceWithBackend::from(surface))
}

pub(crate) async fn resolve_runtime_session_resource_vfs_for_api(
    state: &Arc<AppState>,
    current_user: &AuthIdentity,
    session_id: &str,
) -> Result<Vfs, ApiError> {
    ensure_runtime_session_exists(state, session_id).await?;
    let resource_surface = state
        .services
        .resource_surface_query
        .resource_surface_for_runtime_session(session_id)
        .await
        .map_err(resource_surface_query_error_to_api)?;
    load_project_with_permission(
        state.as_ref(),
        current_user,
        resource_surface.runtime.project_id,
        ProjectPermission::View,
    )
    .await?;
    Ok(resource_surface.lifecycle_surface.vfs)
}

pub(crate) async fn resolve_agent_run_resource_vfs_for_api(
    state: &Arc<AppState>,
    current_user: &AuthIdentity,
    run_id: Uuid,
    agent_id: Uuid,
    permission: ProjectPermission,
) -> Result<Vfs, ApiError> {
    let resource_surface = state
        .services
        .resource_surface_query
        .resource_surface_for_agent_run(run_id, agent_id)
        .await
        .map_err(resource_surface_query_error_to_api)?;
    load_project_with_permission(
        state.as_ref(),
        current_user,
        resource_surface.runtime.project_id,
        permission,
    )
    .await?;
    Ok(resource_surface.lifecycle_surface.vfs)
}

fn runtime_surface_query(state: &Arc<AppState>) -> AgentRunRuntimeSurfaceQuery {
    AgentRunRuntimeSurfaceQuery::new(AgentRunRuntimeSurfaceQueryDeps {
        anchor_repo: state.repos.execution_anchor_repo.clone(),
        run_repo: state.repos.lifecycle_run_repo.clone(),
        agent_repo: state.repos.lifecycle_agent_repo.clone(),
        frame_repo: state.repos.agent_frame_repo.clone(),
    })
}

async fn ensure_runtime_session_exists(
    state: &Arc<AppState>,
    session_id: &str,
) -> Result<(), ApiError> {
    state
        .services
        .session_core
        .get_session_meta(session_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("会话 {session_id} 不存在")))?;
    Ok(())
}

fn runtime_surface_query_error_to_api(error: AgentRunRuntimeSurfaceQueryError) -> ApiError {
    if let Some(anchor_error) = error.as_runtime_backend_anchor_error() {
        return ApiError::Conflict(anchor_error.to_string());
    }
    match error {
        AgentRunRuntimeSurfaceQueryError::MissingAnchor {
            runtime_session_id, ..
        } => ApiError::NotFound(format!(
            "runtime_session 缺少 RuntimeSessionExecutionAnchor: {runtime_session_id}"
        )),
        AgentRunRuntimeSurfaceQueryError::MissingLifecycleRun { run_id, .. } => {
            ApiError::NotFound(format!("lifecycle_run 不存在: {run_id}"))
        }
        AgentRunRuntimeSurfaceQueryError::MissingLifecycleAgent { agent_id, .. } => {
            ApiError::NotFound(format!("lifecycle_agent 不存在: {agent_id}"))
        }
        AgentRunRuntimeSurfaceQueryError::MissingCurrentFrame { agent_id, .. } => {
            ApiError::NotFound(format!(
                "lifecycle_agent {agent_id} 没有可用 current runtime surface"
            ))
        }
        AgentRunRuntimeSurfaceQueryError::AnchorControlPlaneMismatch { .. }
        | AgentRunRuntimeSurfaceQueryError::MissingSurfaceClosure { .. } => {
            ApiError::Conflict(error.to_string())
        }
        AgentRunRuntimeSurfaceQueryError::Repository { message, .. } => ApiError::Internal(message),
        AgentRunRuntimeSurfaceQueryError::MissingRuntimeBackendAnchor { .. }
        | AgentRunRuntimeSurfaceQueryError::BackendAnchorDerivation { .. } => {
            ApiError::Conflict(error.to_string())
        }
    }
}

fn resource_surface_query_error_to_api(error: AgentRunResourceSurfaceQueryError) -> ApiError {
    match error {
        AgentRunResourceSurfaceQueryError::RuntimeSurface(error) => {
            runtime_surface_query_error_to_api(error)
        }
        AgentRunResourceSurfaceQueryError::MissingDeliveryAnchor { agent_id, .. } => {
            ApiError::NotFound(format!(
                "lifecycle_agent {agent_id} 没有可用 delivery runtime surface"
            ))
        }
        AgentRunResourceSurfaceQueryError::ControlPlaneMismatch { .. }
        | AgentRunResourceSurfaceQueryError::Projection { .. } => {
            ApiError::Conflict(error.to_string())
        }
        AgentRunResourceSurfaceQueryError::Repository { message, .. } => {
            ApiError::Internal(message)
        }
    }
}

impl From<AgentRunRuntimeSurface> for ApiCurrentRuntimeSurface {
    fn from(surface: AgentRunRuntimeSurface) -> Self {
        Self {
            runtime_session_id: surface.runtime_session_id,
            run_id: surface.run_id,
            project_id: surface.project_id,
            agent_id: surface.agent_id,
            launch_evidence_frame_id: surface.launch_evidence_frame_id,
            current_surface_frame_id: surface.current_surface_frame_id,
            vfs: surface.vfs,
            orchestration_id: surface.provenance.orchestration_id,
            node_path: surface.provenance.node_path,
            node_attempt: surface.provenance.node_attempt,
        }
    }
}

impl From<AgentRunRuntimeSurfaceWithBackend> for ApiCurrentRuntimeSurfaceWithBackend {
    fn from(surface_with_backend: AgentRunRuntimeSurfaceWithBackend) -> Self {
        Self {
            surface: ApiCurrentRuntimeSurface::from(surface_with_backend.surface),
            runtime_backend_anchor: surface_with_backend.runtime_backend_anchor,
        }
    }
}
