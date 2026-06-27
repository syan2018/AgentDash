mod access;
mod entity;
mod repository;
mod runtime_state;
mod value_objects;

pub use access::canvas_access_projection;
pub use entity::Canvas;
pub use repository::CanvasRepository;
pub use runtime_state::{
    CanvasInteractionEvent, CanvasInteractionSnapshot, CanvasRuntimeDiagnostic,
    CanvasRuntimeDocumentState, CanvasRuntimeObservation, CanvasRuntimeObservationStatus,
    CanvasRuntimeStateRepository, CanvasRuntimeViewport,
};
pub use value_objects::{
    CANVAS_SYSTEM_BUNDLE, CANVAS_SYSTEM_SKILL_NAME, CanvasAccessAction, CanvasAccessProjection,
    CanvasDataBinding, CanvasFile, CanvasImportMap, CanvasSandboxConfig, CanvasScope,
    canvas_binding_data_path, infer_binding_content_type, is_text_compatible_binding_content_type,
    normalize_binding_content_type,
};
