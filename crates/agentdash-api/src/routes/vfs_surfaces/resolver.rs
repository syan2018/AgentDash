use std::sync::Arc;

use agentdash_application_ports::vfs_surface_runtime::{
    ResolvedVfsSurface, ResolvedVfsSurfaceSource,
};
use agentdash_spi::Vfs;
use uuid::Uuid;

use crate::{
    app_state::AppState,
    auth::{ProjectPermission, load_project_with_permission},
    routes::sessions::ensure_session_permission,
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
    let bundle = state
        .services
        .vfs_surface_resolver
        .resolve_surface_bundle(&runtime_projection, source)
        .await?;
    ensure_surface_permission(state, current_user, source, bundle.project_id, permission).await?;
    Ok((bundle.surface, bundle.vfs))
}

async fn ensure_surface_permission(
    state: &Arc<AppState>,
    current_user: &agentdash_integration_api::AuthIdentity,
    source: &ResolvedVfsSurfaceSource,
    project_id: Uuid,
    permission: ProjectPermission,
) -> Result<(), ApiError> {
    if let ResolvedVfsSurfaceSource::SessionRuntime { session_id } = source {
        ensure_session_permission(state.as_ref(), current_user, session_id, permission).await?;
        return Ok(());
    }

    load_project_with_permission(state.as_ref(), current_user, project_id, permission).await?;
    Ok(())
}
