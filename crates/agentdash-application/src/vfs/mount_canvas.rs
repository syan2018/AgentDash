use agentdash_domain::canvas::Canvas;

use crate::canvas::{CanvasResolvedBindingFile, canvas_provider_root_ref, canvas_vfs_mount_id};
use crate::runtime::{Mount, MountCapability, Vfs};

use super::mount::PROVIDER_CANVAS_FS;

pub fn build_canvas_mount_id(canvas: &Canvas) -> String {
    canvas_vfs_mount_id(canvas)
}

pub fn build_canvas_mount(canvas: &Canvas) -> Mount {
    Mount {
        id: build_canvas_mount_id(canvas),
        provider: PROVIDER_CANVAS_FS.to_string(),
        backend_id: String::new(),
        root_ref: canvas_provider_root_ref(canvas),
        capabilities: vec![
            MountCapability::Read,
            MountCapability::Write,
            MountCapability::List,
            MountCapability::Search,
        ],
        default_write: false,
        display_name: if canvas.title.trim().is_empty() {
            format!("Canvas {}", canvas.id)
        } else {
            canvas.title.clone()
        },
        metadata: serde_json::json!({
            "canvas_id": canvas.id.to_string(),
            "canvas_mount_id": canvas.mount_id,
            "vfs_mount_id": canvas_vfs_mount_id(canvas),
            "project_id": canvas.project_id.to_string(),
            "entry_file": canvas.entry_file,
        }),
    }
}

pub fn append_canvas_mounts(vfs: &mut Vfs, canvases: &[Canvas]) {
    for canvas in canvases {
        let mount = build_canvas_mount(canvas);
        vfs.mounts.retain(|existing| existing.id != mount.id);
        vfs.mounts.push(mount);
    }
}

pub fn refresh_canvas_mount_binding_files(
    vfs: &mut Vfs,
    canvas: &Canvas,
    binding_files: &[CanvasResolvedBindingFile],
) {
    let mount_id = build_canvas_mount_id(canvas);
    let Some(mount) = vfs.mounts.iter_mut().find(|mount| mount.id == mount_id) else {
        return;
    };
    let Some(metadata) = mount.metadata.as_object_mut() else {
        return;
    };
    metadata.insert(
        "binding_files".to_string(),
        serde_json::to_value(binding_files).unwrap_or_else(|_| serde_json::json!([])),
    );
}
