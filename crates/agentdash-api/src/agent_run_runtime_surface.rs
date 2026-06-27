use std::sync::Arc;

use agentdash_application_agentrun::agent_run::{
    AgentRunRuntimeSurface, AgentRunRuntimeSurfaceQueryError, AgentRunRuntimeSurfaceWithBackend,
    AgentRunTerminalLaunchTarget, AgentRunTerminalLaunchTargetError, RuntimeSurfaceQueryPurpose,
    terminal_launch_target_from_current_surface,
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
    pub project_id: Uuid,
    pub vfs: Vfs,
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
    let surface = state
        .services
        .runtime_surface_query
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
    let surface = state
        .services
        .runtime_surface_query
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

pub(crate) async fn resolve_current_runtime_surface_for_project_for_api(
    state: &Arc<AppState>,
    current_user: &AuthIdentity,
    session_id: &str,
    expected_project_id: Uuid,
    purpose: RuntimeSurfaceQueryPurpose,
    subject: &str,
) -> Result<ApiCurrentRuntimeSurface, ApiError> {
    let surface =
        resolve_current_runtime_surface_for_api(state, current_user, session_id, purpose).await?;
    ensure_current_runtime_surface_project_matches(&surface, expected_project_id, subject)?;
    Ok(surface)
}

pub(crate) async fn resolve_current_runtime_surface_with_backend_for_project_for_api(
    state: &Arc<AppState>,
    current_user: &AuthIdentity,
    session_id: &str,
    expected_project_id: Uuid,
    purpose: RuntimeSurfaceQueryPurpose,
    subject: &str,
) -> Result<ApiCurrentRuntimeSurfaceWithBackend, ApiError> {
    let surface = resolve_current_runtime_surface_with_backend_for_api(
        state,
        current_user,
        session_id,
        purpose,
    )
    .await?;
    ensure_current_runtime_surface_project_matches(&surface.surface, expected_project_id, subject)?;
    Ok(surface)
}

pub(crate) fn ensure_current_runtime_surface_project_matches(
    surface: &ApiCurrentRuntimeSurface,
    expected_project_id: Uuid,
    subject: &str,
) -> Result<(), ApiError> {
    if surface.project_id != expected_project_id {
        return Err(ApiError::Conflict(format!(
            "{subject} Project 与 runtime session current surface Project 不一致: expected {expected_project_id}, actual {}",
            surface.project_id
        )));
    }
    Ok(())
}

pub(crate) async fn resolve_terminal_launch_target_for_api(
    state: &Arc<AppState>,
    current_user: &AuthIdentity,
    session_id: &str,
) -> Result<AgentRunTerminalLaunchTarget, ApiError> {
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
    load_project_with_permission(
        state.as_ref(),
        current_user,
        runtime_surface.surface.project_id,
        ProjectPermission::View,
    )
    .await?;
    terminal_launch_target_from_current_surface(&runtime_surface)
        .map_err(terminal_launch_target_error_to_api)
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
            project_id: surface.project_id,
            vfs: surface.vfs,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn runtime_surface(project_id: Uuid) -> ApiCurrentRuntimeSurface {
        ApiCurrentRuntimeSurface {
            project_id,
            vfs: Vfs::default(),
        }
    }

    #[test]
    fn current_surface_project_guard_accepts_matching_project() {
        let project_id = Uuid::new_v4();
        let surface = runtime_surface(project_id);

        ensure_current_runtime_surface_project_matches(&surface, project_id, "Canvas runtime")
            .expect("matching project");
    }

    #[test]
    fn current_surface_project_guard_rejects_mismatch_before_runtime_invocation() {
        let expected_project_id = Uuid::new_v4();
        let actual_project_id = Uuid::new_v4();
        let surface = runtime_surface(actual_project_id);

        let error = ensure_current_runtime_surface_project_matches(
            &surface,
            expected_project_id,
            "Extension runtime",
        )
        .expect_err("project mismatch");

        match error {
            ApiError::Conflict(message) => {
                assert!(message.contains("Extension runtime"));
                assert!(message.contains("current surface Project 不一致"));
                assert!(message.contains(&expected_project_id.to_string()));
                assert!(message.contains(&actual_project_id.to_string()));
            }
            other => panic!("expected conflict, got {other:?}"),
        }
    }
}
