mod diagnostics;
mod promotion;

use uuid::Uuid;

use agentdash_domain::canvas::{Canvas, CanvasAccessAction};
use agentdash_domain::project::ProjectAuthorizationContext;
use agentdash_workspace_module::canvas::CanvasRepositorySet;

use crate::ApplicationError;

pub use agentdash_domain::canvas::canvas_access_projection;
pub use agentdash_workspace_module::canvas::{
    CANVAS_BIND_DATA_OPERATION_KEY, CANVAS_BIND_DATA_ORIGIN,
    CANVAS_GET_INTERACTION_STATE_OPERATION_KEY, CANVAS_INSPECT_OPERATION_KEY,
    CANVAS_MODULE_ID_PREFIX, CANVAS_MOUNT_ID_PREFIX, CANVAS_PRESENTATION_SCHEME,
    CANVAS_PREVIEW_VIEW_KEY, CANVAS_PROVIDER_ROOT_SCHEME, CANVAS_RENDERER_KIND, canvas_module_id,
    canvas_presentation_uri, canvas_provider_root_ref, canvas_vfs_mount_id, canvas_vfs_uri,
    derive_canvas_mount_id, normalize_canvas_mount_id, parse_canvas_module_id,
};
pub use agentdash_workspace_module::canvas::{
    CanvasFsMountProvider, CanvasListScopeFilter, CanvasMountAccess, CanvasMutationInput,
    CanvasResolvedBindingFile, CanvasRuntimeBinding, CanvasRuntimeBridgeSnapshot,
    CanvasRuntimeFile, CanvasRuntimeResourceService, CanvasRuntimeSnapshot, CanvasWithAccess,
    CopyCanvasInput, CreateCanvasInput, CreatePersonalCanvasInput, PublishCanvasInput,
    UnpublishCanvasResult, append_canvas_mount, append_canvas_mounts, append_visible_canvas_mounts,
    apply_canvas_mutation, build_canvas, build_canvas_mount, build_canvas_mount_id,
    build_personal_canvas, build_runtime_snapshot, build_runtime_snapshot_with_bindings,
    canvas_runtime_mount_access, refresh_canvas_mount_binding_files, resolve_canvas_binding_files,
    unresolved_canvas_binding_files, validate_canvas_contract, validate_canvas_data_bindings,
};
pub use diagnostics::{
    CanvasAgentRunContext, CanvasInteractionSnapshotInput, CanvasRuntimeObservationInput,
    latest_interaction_snapshot, latest_runtime_observation, resolve_agent_run_canvas_context,
    upsert_interaction_snapshot, upsert_runtime_observation,
};
pub use promotion::{
    CANVAS_EXTENSION_SNAPSHOT_ENTRY, CanvasExtensionPackage, CanvasExtensionPackageInput,
    build_canvas_extension_package,
};

pub async fn list_project_canvases(
    repos: &dyn CanvasRepositorySet,
    project_id: Uuid,
) -> Result<Vec<Canvas>, ApplicationError> {
    agentdash_workspace_module::canvas::list_project_canvases(repos, project_id)
        .await
        .map_err(ApplicationError::from)
}

pub async fn create_personal_canvas(
    repos: &dyn CanvasRepositorySet,
    current_user: &ProjectAuthorizationContext,
    input: CreatePersonalCanvasInput,
) -> Result<CanvasWithAccess, ApplicationError> {
    agentdash_workspace_module::canvas::create_personal_canvas(repos, current_user, input)
        .await
        .map_err(ApplicationError::from)
}

pub async fn create_project_canvas(
    repos: &dyn CanvasRepositorySet,
    current_user: &ProjectAuthorizationContext,
    input: CreateCanvasInput,
) -> Result<CanvasWithAccess, ApplicationError> {
    let canvas = agentdash_workspace_module::canvas::create_project_canvas(repos, input)
        .await
        .map_err(ApplicationError::from)?;
    agentdash_workspace_module::canvas::load_canvas_with_access(
        repos,
        current_user,
        canvas.id,
        CanvasAccessAction::View,
    )
    .await
    .map_err(ApplicationError::from)
}

pub async fn list_canvases_for_user(
    repos: &dyn CanvasRepositorySet,
    current_user: &ProjectAuthorizationContext,
    project_id: Uuid,
    scope_filter: CanvasListScopeFilter,
) -> Result<Vec<CanvasWithAccess>, ApplicationError> {
    agentdash_workspace_module::canvas::list_canvases_for_user(
        repos,
        current_user,
        project_id,
        scope_filter,
    )
    .await
    .map_err(ApplicationError::from)
}

pub async fn load_canvas_by_id(
    repos: &dyn CanvasRepositorySet,
    canvas_id: Uuid,
) -> Result<Canvas, ApplicationError> {
    agentdash_workspace_module::canvas::load_canvas_by_id(repos, canvas_id)
        .await
        .map_err(ApplicationError::from)
}

pub async fn load_canvas_with_access(
    repos: &dyn CanvasRepositorySet,
    current_user: &ProjectAuthorizationContext,
    canvas_id: Uuid,
    action: CanvasAccessAction,
) -> Result<CanvasWithAccess, ApplicationError> {
    agentdash_workspace_module::canvas::load_canvas_with_access(
        repos,
        current_user,
        canvas_id,
        action,
    )
    .await
    .map_err(ApplicationError::from)
}

pub async fn load_canvas_by_project_mount_id(
    repos: &dyn CanvasRepositorySet,
    project_id: Uuid,
    raw_canvas_mount_id: &str,
) -> Result<Canvas, ApplicationError> {
    agentdash_workspace_module::canvas::load_canvas_by_project_mount_id(
        repos,
        project_id,
        raw_canvas_mount_id,
    )
    .await
    .map_err(ApplicationError::from)
}

pub async fn publish_canvas_to_project(
    repos: &dyn CanvasRepositorySet,
    current_user: &ProjectAuthorizationContext,
    source_canvas_id: Uuid,
    input: PublishCanvasInput,
) -> Result<CanvasWithAccess, ApplicationError> {
    agentdash_workspace_module::canvas::publish_canvas_to_project(
        repos,
        current_user,
        source_canvas_id,
        input,
    )
    .await
    .map_err(ApplicationError::from)
}

pub async fn copy_canvas_to_personal(
    repos: &dyn CanvasRepositorySet,
    current_user: &ProjectAuthorizationContext,
    source_canvas_id: Uuid,
    input: CopyCanvasInput,
) -> Result<CanvasWithAccess, ApplicationError> {
    agentdash_workspace_module::canvas::copy_canvas_to_personal(
        repos,
        current_user,
        source_canvas_id,
        input,
    )
    .await
    .map_err(ApplicationError::from)
}

pub async fn unpublish_project_canvas(
    repos: &dyn CanvasRepositorySet,
    current_user: &ProjectAuthorizationContext,
    published_canvas_id: Uuid,
) -> Result<UnpublishCanvasResult, ApplicationError> {
    agentdash_workspace_module::canvas::unpublish_project_canvas(
        repos,
        current_user,
        published_canvas_id,
    )
    .await
    .map_err(ApplicationError::from)
}

pub async fn update_canvas_record(
    repos: &dyn CanvasRepositorySet,
    canvas: Canvas,
    input: CanvasMutationInput,
) -> Result<Canvas, ApplicationError> {
    let canvas = agentdash_workspace_module::canvas::update_canvas_record(repos, canvas, input)
        .await
        .map_err(ApplicationError::from)?;
    Ok(canvas)
}

pub async fn delete_canvas_record(
    repos: &dyn CanvasRepositorySet,
    canvas: &Canvas,
) -> Result<(), ApplicationError> {
    agentdash_workspace_module::canvas::delete_canvas_record(repos, canvas)
        .await
        .map_err(ApplicationError::from)
}
