mod identity;
mod management;
mod runtime;
mod runtime_resource;
mod vfs_mount;
mod vfs_provider;
mod visibility;

pub use identity::{
    CANVAS_BIND_DATA_OPERATION_KEY, CANVAS_BIND_DATA_ORIGIN,
    CANVAS_GET_INTERACTION_STATE_OPERATION_KEY, CANVAS_INSPECT_OPERATION_KEY,
    CANVAS_MODULE_ID_PREFIX, CANVAS_MOUNT_ID_PREFIX, CANVAS_PRESENTATION_SCHEME,
    CANVAS_PREVIEW_VIEW_KEY, CANVAS_PROVIDER_ROOT_SCHEME, CANVAS_RENDERER_KIND,
    CanvasIdentityError, canvas_module_id, canvas_presentation_uri, canvas_provider_root_ref,
    canvas_vfs_mount_id, canvas_vfs_uri, derive_canvas_mount_id, normalize_canvas_mount_id,
    parse_canvas_module_id,
};
pub use management::{
    CanvasListScopeFilter, CanvasMutationInput, CanvasRepositorySet, CanvasWithAccess,
    CopyCanvasInput, CreateCanvasInput, CreatePersonalCanvasInput, PublishCanvasInput,
    UnpublishCanvasResult, apply_canvas_mutation, build_canvas, build_personal_canvas,
    copy_canvas_to_personal, create_personal_canvas, create_project_canvas, delete_canvas_record,
    list_canvases_for_user, list_project_canvases, load_canvas_by_id,
    load_canvas_by_project_mount_id, load_canvas_with_access, publish_canvas_to_project,
    unpublish_project_canvas, update_canvas_record, upsert_canvas_data_binding,
    validate_canvas_contract, validate_canvas_data_bindings,
};
pub use runtime::{
    CanvasResolvedBindingFile, CanvasRuntimeBinding, CanvasRuntimeBridgeSnapshot,
    CanvasRuntimeFile, CanvasRuntimeSnapshot, build_runtime_snapshot,
    build_runtime_snapshot_with_bindings, resolve_canvas_binding_files,
    unresolved_canvas_binding_files,
};
pub use runtime_resource::CanvasRuntimeResourceService;
pub use vfs_mount::{
    CANVAS_RUNTIME_DATA_BINDINGS_METADATA_KEY, CanvasMountAccess, append_canvas_mount,
    append_canvas_mounts, build_canvas_mount, build_canvas_mount_id,
    canvas_mount_runtime_data_bindings, refresh_canvas_mount_binding_files,
    upsert_canvas_runtime_data_binding,
};
pub use vfs_provider::CanvasFsMountProvider;
pub use visibility::{canvas_runtime_mount_access, canvas_runtime_mount_access_for_user};
