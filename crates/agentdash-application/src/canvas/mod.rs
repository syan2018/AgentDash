mod identity;
mod management;
mod promotion;
mod runtime;
mod runtime_resource;
mod tools;
mod visibility;

pub use identity::{
    CANVAS_MODULE_ID_PREFIX, CANVAS_MOUNT_ID_PREFIX, CANVAS_PRESENTATION_SCHEME,
    CANVAS_PROVIDER_ROOT_SCHEME, canvas_module_id, canvas_presentation_uri,
    canvas_provider_root_ref, canvas_vfs_mount_id, canvas_vfs_uri, derive_canvas_mount_id,
    normalize_canvas_mount_id, parse_canvas_module_id,
};
pub use management::{
    CanvasMutationInput, CreateCanvasInput, apply_canvas_mutation, build_canvas,
    create_project_canvas, delete_canvas_record, list_project_canvases, load_canvas_by_id,
    load_canvas_by_project_mount_id, update_canvas_record, upsert_canvas_binding,
    validate_canvas_contract,
};
pub use promotion::{
    CANVAS_EXTENSION_SNAPSHOT_ENTRY, CanvasExtensionPackage, CanvasExtensionPackageInput,
    build_canvas_extension_package,
};
pub use runtime::{
    CanvasResolvedBindingFile, CanvasRuntimeBinding, CanvasRuntimeBridgeSnapshot,
    CanvasRuntimeFile, CanvasRuntimeSnapshot, build_runtime_snapshot,
    build_runtime_snapshot_with_bindings, resolve_canvas_binding_files,
    unresolved_canvas_binding_files,
};
pub use runtime_resource::CanvasRuntimeResourceService;
pub use tools::expose_existing_canvas_for_session;
pub(crate) use tools::{
    BindCanvasDataParams, StartCanvasParams, bind_canvas_data_for_project,
    create_or_attach_canvas_for_session,
};
pub use visibility::append_visible_canvas_mounts;
