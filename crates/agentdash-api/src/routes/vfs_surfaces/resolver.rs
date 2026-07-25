use std::sync::Arc;

use agentdash_agent_runtime_contract::RuntimeThreadId;
use agentdash_application_ports::vfs_surface_runtime::{
    ResolvedVfsSurface, ResolvedVfsSurfaceSource,
};
use agentdash_platform_spi::Vfs;
use uuid::Uuid;

use crate::{
    app_state::AppState,
    auth::{ProjectPermission, load_project_with_permission},
    rpc::ApiError,
    vfs_surface_runtime::ApiVfsSurfaceRuntimeProjection,
};

pub(crate) async fn resolve_surface_from_source(
    state: &Arc<AppState>,
    current_user: &agentdash_integration_api::AuthIdentity,
    source: &ResolvedVfsSurfaceSource,
) -> Result<ResolvedVfsSurface, ApiError> {
    let (surface, _vfs) =
        resolve_surface_bundle(state, current_user, source, ProjectPermission::Use).await?;
    Ok(surface)
}

pub(crate) async fn resolve_surface_bundle(
    state: &Arc<AppState>,
    current_user: &agentdash_integration_api::AuthIdentity,
    source: &ResolvedVfsSurfaceSource,
    permission: ProjectPermission,
) -> Result<(ResolvedVfsSurface, Vfs), ApiError> {
    let runtime_projection = ApiVfsSurfaceRuntimeProjection::new(
        state.services.backend_registry.clone(),
        state.services.mount_provider_registry.clone(),
    );
    let bundle = if let ResolvedVfsSurfaceSource::RuntimeThread { runtime_thread_id } = source {
        let thread_id = RuntimeThreadId::new(runtime_thread_id.clone())
            .map_err(|error| ApiError::BadRequest(format!("runtime thread id 非法: {error}")))?;
        let binding = state
            .services
            .agent_run_product_runtime_bindings
            .load_product_binding_by_runtime_thread(&thread_id)
            .await
            .map_err(ApiError::Internal)?
            .ok_or_else(|| {
                ApiError::Conflict(format!(
                    "runtime thread 缺少 canonical Product binding: {runtime_thread_id}"
                ))
            })?;
        state
            .services
            .vfs_surface_resolver
            .resolve_agent_run_surface_bundle(&runtime_projection, source, &binding.target)
            .await?
    } else {
        state
            .services
            .vfs_surface_resolver
            .resolve_surface_bundle(&runtime_projection, source)
            .await?
    };
    ensure_surface_permission(state, current_user, bundle.project_id, permission).await?;
    Ok((bundle.surface, bundle.vfs))
}

async fn ensure_surface_permission(
    state: &Arc<AppState>,
    current_user: &agentdash_integration_api::AuthIdentity,
    project_id: Uuid,
    permission: ProjectPermission,
) -> Result<(), ApiError> {
    load_project_with_permission(state.as_ref(), current_user, project_id, permission).await?;
    Ok(())
}
