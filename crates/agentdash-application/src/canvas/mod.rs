mod management;
mod promotion;
mod runtime;
mod tools;
mod visibility;

pub use management::{
    CanvasMutationInput, CreateCanvasInput, apply_canvas_mutation, build_canvas,
    create_project_canvas, delete_canvas_record, list_project_canvases, load_canvas_by_ref,
    update_canvas_record, upsert_canvas_binding, validate_canvas_contract,
};
pub use promotion::{
    CANVAS_EXTENSION_SNAPSHOT_ENTRY, CanvasExtensionPackage, CanvasExtensionPackageInput,
    build_canvas_extension_package,
};
pub use runtime::{
    CanvasRuntimeBinding, CanvasRuntimeBridgeSnapshot, CanvasRuntimeFile, CanvasRuntimeSnapshot,
    build_runtime_snapshot, build_runtime_snapshot_with_bindings,
};
pub use tools::expose_existing_canvas_for_session;
pub(crate) use tools::{
    BindCanvasDataParams, StartCanvasParams, bind_canvas_data_for_project,
    create_or_attach_canvas_for_session,
};
pub use visibility::append_visible_canvas_mounts;
