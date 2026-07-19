use std::sync::Arc;

use agentdash_application_agentrun::agent_run::{
    AgentRunAppliedResourceSurface, AgentRunAppliedResourceSurfaceQueryPort,
    AgentRunProductRuntimeBindingRepository, AppliedVfsMount, AppliedVfsOperation,
};
use agentdash_application_ports::agent_run_surface::RuntimeSurfaceQueryPurpose;
use agentdash_domain::agent_run_target::AgentRunTarget;
use agentdash_domain::backend::{RuntimeBackendAnchor, RuntimeBackendAnchorSource};
use agentdash_integration_api::AuthIdentity;
use agentdash_platform_spi::{Mount, MountCapability, Vfs};
use uuid::Uuid;

use crate::app_state::AppState;
use crate::auth::{ProjectPermission, load_project_with_permission};
use crate::rpc::ApiError;

#[derive(Debug, Clone)]
pub(crate) struct ApiCurrentRuntimeSurface {
    pub project_id: Uuid,
    pub vfs: Vfs,
}

#[derive(Debug, Clone)]
pub(crate) struct ApiAgentRunCurrentRuntimeSurfaceWithBackend {
    pub project_id: Uuid,
    pub runtime_thread_id: String,
    pub surface: ApiCurrentRuntimeSurface,
    pub runtime_backend_anchor: RuntimeBackendAnchor,
}

pub(crate) async fn resolve_current_runtime_surface_for_project_for_api(
    state: &Arc<AppState>,
    current_user: &AuthIdentity,
    runtime_thread_id: &str,
    expected_project_id: Uuid,
    purpose: RuntimeSurfaceQueryPurpose,
    subject: &str,
) -> Result<ApiCurrentRuntimeSurface, ApiError> {
    let binding = state
        .services
        .agent_run_product_runtime_bindings
        .load_product_binding_by_runtime_thread(
            &agentdash_agent_runtime_contract::RuntimeThreadId::new(runtime_thread_id)
                .map_err(|error| ApiError::BadRequest(error.to_string()))?,
        )
        .await
        .map_err(ApiError::Internal)?
        .ok_or_else(|| {
            ApiError::NotFound(format!(
                "{} 缺少 Product RuntimeThread binding: {}",
                purpose.component, runtime_thread_id
            ))
        })?;
    let surface = load_applied_surface(state, &binding.target).await?;
    if surface.project_id != expected_project_id {
        return Err(ApiError::Conflict(format!(
            "{subject} Project 与 Product applied resource surface 不一致: expected {expected_project_id}, actual {}",
            surface.project_id
        )));
    }
    load_project_with_permission(
        state.as_ref(),
        current_user,
        surface.project_id,
        ProjectPermission::Use,
    )
    .await?;
    Ok(ApiCurrentRuntimeSurface {
        project_id: surface.project_id,
        vfs: applied_vfs(&surface),
    })
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
        .ok_or_else(|| {
            ApiError::NotFound(format!("LifecycleAgent 不存在: {}", target.agent_id))
        })?;
    if agent.run_id != run.id || agent.project_id != run.project_id {
        return Err(ApiError::Conflict(format!(
            "LifecycleAgent {} 不属于 LifecycleRun {}",
            target.agent_id, target.run_id
        )));
    }
    load_project_with_permission(state.as_ref(), current_user, run.project_id, permission).await?;
    let binding = state
        .services
        .agent_run_product_runtime_bindings
        .load_product_binding(&target)
        .await
        .map_err(ApiError::Internal)?
        .ok_or_else(|| {
            ApiError::NotFound(format!(
                "{} 缺少 Product RuntimeThread binding: run={}, agent={}",
                purpose.component, target.run_id, target.agent_id
            ))
        })?;
    let applied = load_applied_surface(state, &target).await?;
    if applied.project_id != run.project_id {
        return Err(ApiError::Conflict(format!(
            "{subject} Project 与 Product applied resource surface 不一致"
        )));
    }
    let mount = selected_backend_mount(&applied)?;
    let anchor = RuntimeBackendAnchor::new(
        mount.backend_id.clone(),
        RuntimeBackendAnchorSource::RestoredAgentRun,
    )
    .map_err(|error| ApiError::Conflict(error.to_string()))?
    .with_workspace_id(applied.workspace_id)
    .with_root_ref(Some(mount.root_ref.clone()))
    .with_source_detail(Some(format!(
        "product-applied-resource-surface:{}:{}",
        target.run_id, target.agent_id
    )));
    Ok(ApiAgentRunCurrentRuntimeSurfaceWithBackend {
        project_id: applied.project_id,
        runtime_thread_id: binding.runtime_thread_id.to_string(),
        surface: ApiCurrentRuntimeSurface {
            project_id: applied.project_id,
            vfs: applied_vfs(&applied),
        },
        runtime_backend_anchor: anchor,
    })
}

async fn load_applied_surface(
    state: &AppState,
    target: &AgentRunTarget,
) -> Result<AgentRunAppliedResourceSurface, ApiError> {
    state
        .services
        .agent_run_product_persistence_composition
        .applied_resource_surfaces
        .applied_resource_surface(target, None)
        .await
        .map(|snapshot| snapshot.surface)
        .map_err(|error| ApiError::Conflict(error.to_string()))
}

fn selected_backend_mount(
    surface: &AgentRunAppliedResourceSurface,
) -> Result<&AppliedVfsMount, ApiError> {
    if let Some(default_mount_id) = surface.default_mount_id.as_deref() {
        return surface
            .vfs_mounts
            .iter()
            .find(|mount| mount.mount_id == default_mount_id)
            .ok_or_else(|| {
                ApiError::Conflict(format!(
                    "Product applied resource surface 的 default mount 不存在: {default_mount_id}"
                ))
            });
    }
    match surface.vfs_mounts.as_slice() {
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

fn applied_vfs(surface: &AgentRunAppliedResourceSurface) -> Vfs {
    Vfs {
        mounts: surface
            .vfs_mounts
            .iter()
            .cloned()
            .map(applied_mount)
            .collect(),
        default_mount_id: surface.default_mount_id.clone(),
        source_project_id: Some(surface.project_id.to_string()),
        source_story_id: None,
        links: Vec::new(),
    }
}

fn applied_mount(mount: AppliedVfsMount) -> Mount {
    Mount {
        id: mount.mount_id,
        provider: mount.provider,
        backend_id: mount.backend_id,
        root_ref: mount.root_ref,
        capabilities: mount
            .capabilities
            .into_iter()
            .map(|operation| match operation {
                AppliedVfsOperation::Read => MountCapability::Read,
                AppliedVfsOperation::List => MountCapability::List,
                AppliedVfsOperation::Search => MountCapability::Search,
                AppliedVfsOperation::Write => MountCapability::Write,
                AppliedVfsOperation::Exec => MountCapability::Exec,
            })
            .collect(),
        default_write: mount.default_write,
        display_name: mount.display_name,
        metadata: mount.metadata,
    }
}

fn parse_uuid(raw: &str, name: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(raw).map_err(|_| ApiError::BadRequest(format!("无效的 {name}")))
}
