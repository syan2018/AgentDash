use std::sync::Arc;

use agentdash_application::execution_authority::{
    ExecutionAuthorityRequest, ExecutionResourceGrants,
};
use agentdash_application_ports::agent_run_surface::RuntimeSurfaceQueryPurpose;
use agentdash_domain::agent_run_target::AgentRunTarget;
use agentdash_domain::backend::{RuntimeBackendAnchor, RuntimeBackendAnchorSource};
use agentdash_integration_api::AuthIdentity;
use agentdash_platform_spi::Vfs;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::auth::{ProjectPermission, load_project_with_permission};
use crate::rpc::ApiError;

#[derive(Debug, Clone)]
pub(crate) struct ApiCurrentRuntimeSurface {
    pub vfs: Vfs,
}

#[derive(Debug, Clone)]
pub(crate) struct ApiAgentRunCurrentRuntimeSurfaceWithBackend {
    pub project_id: Uuid,
    pub runtime_thread_id: String,
    pub surface: ApiCurrentRuntimeSurface,
    pub runtime_backend_anchor: RuntimeBackendAnchor,
}

pub(crate) async fn resolve_current_runtime_surface_with_backend_for_agent_run_for_api(
    state: &Arc<AppState>,
    current_user: &AuthIdentity,
    run_id: &str,
    agent_id: &str,
    permission: ProjectPermission,
    _purpose: RuntimeSurfaceQueryPurpose,
    subject: &str,
) -> Result<ApiAgentRunCurrentRuntimeSurfaceWithBackend, ApiError> {
    let target = AgentRunTarget {
        run_id: parse_uuid(run_id, "run_id")?,
        agent_id: parse_uuid(agent_id, "agent_id")?,
    };
    let run = state
        .repos
        .lifecycle_run_repo
        .get_by_id(target.run_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("LifecycleRun 不存在: {}", target.run_id)))?;
    let agent = state
        .repos
        .lifecycle_agent_repo
        .get(target.agent_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("LifecycleAgent 不存在: {}", target.agent_id)))?;
    if agent.run_id != run.id || agent.project_id != run.project_id {
        return Err(ApiError::Conflict(format!(
            "LifecycleAgent {} 不属于 LifecycleRun {}",
            target.agent_id, target.run_id
        )));
    }
    load_project_with_permission(state.as_ref(), current_user, run.project_id, permission).await?;
    let binding = state
        .services
        .execution_authorities
        .resolve(ExecutionAuthorityRequest::for_target(target.clone()))
        .await
        .map_err(|error| ApiError::Conflict(format!("{}: {error}", error.code())))?;
    let authority = &binding;
    if authority.project_id() != run.project_id {
        return Err(ApiError::Conflict(format!(
            "{subject} Project 与 ExecutionAuthority 不一致"
        )));
    }
    let mount = selected_backend_mount(authority.resources())?;
    let anchor = RuntimeBackendAnchor::new(
        mount.backend_id.clone(),
        RuntimeBackendAnchorSource::RestoredAgentRun,
    )
    .map_err(|error| ApiError::Conflict(error.to_string()))?
    .with_workspace_id(authority.resources().workspace_id())
    .with_root_ref(Some(mount.root_ref.clone()))
    .with_source_detail(Some(format!(
        "product-applied-resource-surface:{}:{}",
        target.run_id, target.agent_id
    )));
    Ok(ApiAgentRunCurrentRuntimeSurfaceWithBackend {
        project_id: authority.project_id(),
        runtime_thread_id: binding.runtime_thread_id().to_string(),
        surface: ApiCurrentRuntimeSurface {
            vfs: authority.resources().vfs(authority.project_id()),
        },
        runtime_backend_anchor: anchor,
    })
}

fn selected_backend_mount(
    resources: &ExecutionResourceGrants,
) -> Result<&agentdash_application_agentrun::agent_run::AppliedVfsMount, ApiError> {
    if let Some(default_mount_id) = resources.default_mount_id() {
        return resources
            .vfs_mounts()
            .iter()
            .find(|mount| mount.mount_id == default_mount_id)
            .ok_or_else(|| {
                ApiError::Conflict(format!(
                    "Product applied resource surface 的 default mount 不存在: {default_mount_id}"
                ))
            });
    }
    match resources.vfs_mounts() {
        [mount] => Ok(mount),
        [] => Err(ApiError::Conflict(
            "Product applied resource surface 没有可用 VFS mount".to_string(),
        )),
        _ => Err(ApiError::Conflict(
            "Product applied resource surface 有多个 mount 但没有 canonical default mount"
                .to_string(),
        )),
    }
}

fn parse_uuid(raw: &str, name: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(raw).map_err(|_| ApiError::BadRequest(format!("无效的 {name}")))
}
