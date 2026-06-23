use std::sync::Arc;

use agentdash_application::agent_run::{
    AgentRunRuntimeSurface, AgentRunRuntimeSurfaceQuery, AgentRunRuntimeSurfaceQueryDeps,
    AgentRunRuntimeSurfaceQueryError, AgentRunRuntimeSurfaceQueryPort,
    AgentRunRuntimeSurfaceWithBackend, RuntimeSurfaceQueryPurpose,
};
use agentdash_application::lifecycle::surface::surface_projector::{
    AgentRunLifecycleSessionEvidenceFacts, AgentRunLifecycleSkillProjectionFacts,
    OrchestrationNodeEvidenceRef,
};
use agentdash_application::lifecycle::{
    AgentRunLifecycleSurfaceProjector, AgentRunRuntimeAddress, MessageStreamProjectionRef,
    MessageStreamTraceKind,
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
    pub launch_frame_id: Uuid,
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
    let surface = resolve_current_runtime_surface_for_api(
        state,
        current_user,
        session_id,
        RuntimeSurfaceQueryPurpose::resource_surface(),
    )
    .await?;
    project_runtime_surface_resource_vfs(state, &surface).await
}

pub(crate) async fn project_runtime_surface_resource_vfs(
    state: &Arc<AppState>,
    surface: &ApiCurrentRuntimeSurface,
) -> Result<Vfs, ApiError> {
    let node_evidence = match (
        surface.orchestration_id,
        surface.node_path.as_ref(),
        surface.node_attempt,
    ) {
        (Some(orchestration_id), Some(node_path), Some(attempt)) => {
            Some(OrchestrationNodeEvidenceRef {
                run_id: surface.run_id,
                orchestration_id,
                node_path: node_path.clone(),
                attempt,
            })
        }
        _ => None,
    };
    let resource_surface = AgentRunLifecycleSurfaceProjector::new(&state.repos)
        .project_workspace_read_surface(AgentRunLifecycleSessionEvidenceFacts {
            base_vfs: Some(surface.vfs.clone()),
            address: AgentRunRuntimeAddress {
                run_id: surface.run_id,
                agent_id: surface.agent_id,
                frame_id: surface.launch_frame_id,
            },
            message_stream: MessageStreamProjectionRef {
                runtime_session_id: surface.runtime_session_id.clone(),
                trace_kind: MessageStreamTraceKind::ConnectorRuntimeSession,
            },
            project_id: surface.project_id,
            node_evidence,
            skill_projection: AgentRunLifecycleSkillProjectionFacts::preserve_projected(),
        })
        .await
        .map_err(|error| {
            ApiError::Internal(format!("构建 AgentRun resource surface 失败: {error}"))
        })?;
    Ok(resource_surface.vfs)
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

impl From<AgentRunRuntimeSurface> for ApiCurrentRuntimeSurface {
    fn from(surface: AgentRunRuntimeSurface) -> Self {
        Self {
            runtime_session_id: surface.runtime_session_id,
            run_id: surface.run_id,
            project_id: surface.project_id,
            agent_id: surface.agent_id,
            launch_frame_id: surface.provenance.launch_frame_id,
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
