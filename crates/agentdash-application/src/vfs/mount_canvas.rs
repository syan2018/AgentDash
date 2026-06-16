use agentdash_domain::canvas::Canvas;

use crate::runtime::{Mount, MountCapability, Vfs};

use super::mount::PROVIDER_CANVAS_FS;

pub fn build_canvas_mount_id(canvas: &Canvas) -> String {
    format!("cvs-{}", canvas.mount_id)
}

pub fn build_canvas_mount(canvas: &Canvas) -> Mount {
    Mount {
        id: build_canvas_mount_id(canvas),
        provider: PROVIDER_CANVAS_FS.to_string(),
        backend_id: String::new(),
        root_ref: format!("canvas://{}", canvas.id),
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
            "mount_id": canvas.mount_id,
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
