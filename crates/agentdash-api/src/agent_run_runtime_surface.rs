use std::sync::Arc;

use agentdash_application_ports::agent_run_surface::{
    AgentRunRuntimeSurfaceQueryError, AgentRunRuntimeSurfaceWithBackend,
    AgentRunTerminalLaunchTarget, AgentRunTerminalLaunchTargetError, RuntimeSurfaceQueryPurpose,
};
use agentdash_application_vfs::PROVIDER_RELAY_FS;
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
        AgentRunRuntimeSurfaceQueryError::RuntimeBackendAnchor { .. }
        | AgentRunRuntimeSurfaceQueryError::Projection { .. } => {
            ApiError::Conflict(error.to_string())
        }
    }
}
