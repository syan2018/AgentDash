use agentdash_domain::DomainError;
use agentdash_domain::canvas::Canvas;

pub use agentdash_canvas::{
    CANVAS_BIND_DATA_OPERATION_KEY, CANVAS_BIND_DATA_ORIGIN, CANVAS_MODULE_ID_PREFIX,
    CANVAS_MOUNT_ID_PREFIX, CANVAS_PRESENTATION_SCHEME, CANVAS_PREVIEW_VIEW_KEY,
    CANVAS_PROVIDER_ROOT_SCHEME, CANVAS_RENDERER_KIND, canvas_module_id, canvas_presentation_uri,
    canvas_vfs_uri, derive_canvas_mount_id, parse_canvas_module_id,
};

pub fn normalize_canvas_mount_id(raw: &str) -> Result<String, DomainError> {
    agentdash_canvas::normalize_canvas_mount_id(raw)
        .map_err(|error| DomainError::InvalidConfig(error.to_string()))
}

pub fn canvas_vfs_mount_id(canvas: &Canvas) -> String {
    let canvas_mount_id = normalize_canvas_mount_id(&canvas.mount_id)
        .expect("Canvas canvas_mount_id must be normalized before VFS projection");
    agentdash_canvas::canvas_vfs_mount_id(&canvas_mount_id)
}

pub fn canvas_provider_root_ref(canvas: &Canvas) -> String {
    agentdash_canvas::canvas_provider_root_ref(canvas.id)
}
