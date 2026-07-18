use std::sync::Arc;

use agentdash_application_ports::agent_run_runtime::AgentRunRuntimeTarget;
use agentdash_application_ports::agent_run_surface::{
    AgentRunRuntimeSurface, AgentRunRuntimeSurfaceQueryError, AgentRunRuntimeSurfaceWithBackend,
    AgentRunTerminalLaunchTarget, AgentRunTerminalLaunchTargetError, RuntimeSurfaceQueryPurpose,
};
use agentdash_application_vfs::PROVIDER_RELAY_FS;
use agentdash_integration_api::AuthIdentity;
use agentdash_platform_spi::{RuntimeBackendAnchor, Vfs};
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

#[derive(Debug, Clone)]
pub(crate) struct ApiAgentRunCurrentRuntimeSurfaceWithBackend {
    pub project_id: Uuid,
    pub runtime_session_id: String,
    pub surface: ApiCurrentRuntimeSurface,
    pub runtime_backend_anchor: RuntimeBackendAnchor,
}

#[derive(Debug, Clone)]
pub(crate) struct ApiTerminalLaunchTarget {
    pub project_id: Uuid,
    pub target: AgentRunTerminalLaunchTarget,
}

pub(crate) async fn resolve_current_runtime_surface_for_api(
    state: &Arc<AppState>,
    current_user: &AuthIdentity,
    session_id: &str,
    purpose: RuntimeSurfaceQueryPurpose,
) -> Result<ApiCurrentRuntimeSurface, ApiError> {
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
        ProjectPermission::Use,
    )
    .await?;
    Ok(ApiCurrentRuntimeSurface::from(surface))
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

pub(crate) async fn resolve_current_runtime_surface_with_backend_for_agent_run_for_api(
    state: &Arc<AppState>,
    current_user: &AuthIdentity,
    run_id: &str,
    agent_id: &str,
    permission: ProjectPermission,
    purpose: RuntimeSurfaceQueryPurpose,
    subject: &str,
) -> Result<ApiAgentRunCurrentRuntimeSurfaceWithBackend, ApiError> {
    let run_id = parse_uuid_param(run_id, "run_id")?;
    let agent_id = parse_uuid_param(agent_id, "agent_id")?;
    let run = state
        .repos
        .lifecycle_run_repo
        .get_by_id(run_id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::NotFound(format!("LifecycleRun 不存在: {run_id}")))?;
    load_project_with_permission(state.as_ref(), current_user, run.project_id, permission).await?;
    let agent = state
        .repos
        .lifecycle_agent_repo
        .get(agent_id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::NotFound(format!("LifecycleAgent 不存在: {agent_id}")))?;
    if agent.run_id != run.id || agent.project_id != run.project_id {
        return Err(ApiError::Conflict(format!(
            "LifecycleAgent {agent_id} 不属于 LifecycleRun {run_id}"
        )));
    }

    let binding = state
        .repos
        .agent_run_runtime_binding_repo
        .load(&AgentRunRuntimeTarget {
            run_id: run.id,
            agent_id: agent.id,
        })
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?
        .ok_or_else(|| {
            ApiError::NotFound(format!(
                "AgentRun Runtime binding 不存在: run={}, agent={}",
                run.id, agent.id
            ))
        })?;
    let runtime_thread_id = binding.thread_id.to_string();
    let runtime_surface = state
        .services
        .runtime_surface_query
        .current_runtime_surface_with_backend(&runtime_thread_id, purpose)
        .await
        .map_err(runtime_surface_query_error_to_api)?;
    let surface = ApiCurrentRuntimeSurfaceWithBackend::from(runtime_surface);
    ensure_current_runtime_surface_project_matches(&surface.surface, run.project_id, subject)?;
    Ok(ApiAgentRunCurrentRuntimeSurfaceWithBackend {
        project_id: run.project_id,
        runtime_session_id: runtime_thread_id,
        surface: surface.surface,
        runtime_backend_anchor: surface.runtime_backend_anchor,
    })
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

fn parse_uuid_param(raw: &str, name: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(raw).map_err(|_| ApiError::BadRequest(format!("无效的 {name}")))
}

pub(crate) async fn resolve_terminal_launch_target_for_runtime_session(
    state: &Arc<AppState>,
    session_id: &str,
) -> Result<ApiTerminalLaunchTarget, ApiError> {
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

fn terminal_launch_target_error_to_api(error: AgentRunTerminalLaunchTargetError) -> ApiError {
    ApiError::BadRequest(error.to_string())
}

fn terminal_launch_target_from_current_surface(
    surface: &AgentRunRuntimeSurfaceWithBackend,
) -> Result<AgentRunTerminalLaunchTarget, AgentRunTerminalLaunchTargetError> {
    let backend_anchor = &surface.runtime_backend_anchor;
    let mount = if let Some(root_ref) = backend_anchor
        .root_ref
        .as_deref()
        .map(str::trim)
        .filter(|root_ref| !root_ref.is_empty())
    {
        surface
            .surface
            .vfs
            .mounts
            .iter()
            .find(|mount| mount.root_ref.trim() == root_ref)
            .ok_or_else(|| AgentRunTerminalLaunchTargetError::MissingAnchorMount {
                root_ref: root_ref.to_string(),
            })?
    } else {
        surface
            .surface
            .vfs
            .default_mount()
            .ok_or(AgentRunTerminalLaunchTargetError::MissingMount)?
    };
    if mount.provider != PROVIDER_RELAY_FS {
        return Err(
            AgentRunTerminalLaunchTargetError::UnsupportedMountProvider {
                mount_id: mount.id.clone(),
                provider: mount.provider.clone(),
            },
        );
    }
    let backend_id = backend_anchor.backend_id();
    if backend_id.is_empty() {
        return Err(AgentRunTerminalLaunchTargetError::MissingBackendId);
    }
    let mount_root_ref = mount.root_ref.trim();
    if mount_root_ref.is_empty() {
        return Err(AgentRunTerminalLaunchTargetError::MissingMountRootRef {
            mount_id: mount.id.clone(),
        });
    }
    Ok(AgentRunTerminalLaunchTarget {
        backend_id: backend_id.to_string(),
        mount_root_ref: mount_root_ref.to_string(),
    })
}

fn runtime_surface_query_error_to_api(error: AgentRunRuntimeSurfaceQueryError) -> ApiError {
    match error {
        AgentRunRuntimeSurfaceQueryError::MissingAnchor {
            runtime_session_id, ..
        } => ApiError::NotFound(format!(
            "runtime trace 缺少 AgentRun Runtime binding: {runtime_session_id}"
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
        AgentRunRuntimeSurfaceQueryError::Repository { message, .. } => ApiError::Internal(message),
        AgentRunRuntimeSurfaceQueryError::MissingSurfaceClosure { .. }
        | AgentRunRuntimeSurfaceQueryError::RuntimeBackendAnchor { .. }
        | AgentRunRuntimeSurfaceQueryError::Projection { .. } => {
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
