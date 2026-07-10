use std::sync::Arc;

use agentdash_application_agentrun::agent_run::{
    AgentRunRuntimeSurfaceQueryError, AgentRunTerminalLaunchTarget,
    AgentRunTerminalLaunchTargetError, RuntimeSurfaceQueryPurpose,
    terminal_launch_target_from_current_surface,
};
use uuid::Uuid;

use crate::app_state::AppState;
use crate::rpc::ApiError;

#[derive(Debug, Clone)]
pub(crate) struct ApiTerminalLaunchTarget {
    pub project_id: Uuid,
    pub target: AgentRunTerminalLaunchTarget,
}

pub(crate) async fn resolve_terminal_launch_target_for_runtime_session(
    state: &Arc<AppState>,
    session_id: &str,
) -> Result<ApiTerminalLaunchTarget, ApiError> {
    ensure_runtime_session_exists(state, session_id).await?;
    let runtime_surface = state
        .services
        .runtime_surface_query
        .current_runtime_surface_with_backend(
            session_id,
            RuntimeSurfaceQueryPurpose::new("terminal_spawn"),
        )
        .await
        .map_err(runtime_surface_query_error_to_api)?;
    let project_id = runtime_surface.surface.project_id;
    let target = terminal_launch_target_from_current_surface(&runtime_surface)
        .map_err(terminal_launch_target_error_to_api)?;
    Ok(ApiTerminalLaunchTarget { project_id, target })
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
        .ok_or_else(|| ApiError::NotFound(format!("runtime trace {session_id} 不存在")))?;
    Ok(())
}

fn terminal_launch_target_error_to_api(error: AgentRunTerminalLaunchTargetError) -> ApiError {
    ApiError::BadRequest(error.to_string())
}

fn runtime_surface_query_error_to_api(error: AgentRunRuntimeSurfaceQueryError) -> ApiError {
    if let Some(anchor_error) = error.as_runtime_backend_anchor_error() {
        return ApiError::Conflict(anchor_error.to_string());
    }
    match error {
        AgentRunRuntimeSurfaceQueryError::MissingAnchor {
            runtime_session_id, ..
        } => ApiError::NotFound(format!(
            "runtime trace 缺少 RuntimeSessionExecutionAnchor: {runtime_session_id}"
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
