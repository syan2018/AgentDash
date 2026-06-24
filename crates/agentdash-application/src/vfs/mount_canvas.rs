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
        if let Some(existing) = vfs
            .mounts
            .iter_mut()
            .find(|existing| existing.id == mount.id)
        {
            *existing = mount;
        } else {
            vfs.mounts.push(mount);
        }
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
    if binding_files.is_empty() {
        metadata.remove("binding_files");
        return;
    }
    metadata.insert(
        "binding_files".to_string(),
        serde_json::to_value(binding_files).unwrap_or_else(|_| serde_json::json!([])),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    use uuid::Uuid;

    fn canvas(mount_id: &str) -> Canvas {
        Canvas::new(
            Uuid::new_v4(),
            mount_id.to_string(),
            "Dashboard".to_string(),
            String::new(),
        )
    }

    fn relay_mount(id: &str) -> Mount {
        Mount {
            id: id.to_string(),
            provider: "relay_fs".to_string(),
            backend_id: "backend-a".to_string(),
            root_ref: format!("D:/{id}"),
            capabilities: vec![MountCapability::Read],
            default_write: false,
            display_name: id.to_string(),
            metadata: serde_json::Value::Null,
        }
    }

    #[test]
    fn append_canvas_mounts_replaces_existing_mount_without_reordering() {
        let canvas = canvas("cvs-dashboard-a");
        let mut vfs = Vfs {
            mounts: vec![relay_mount("workspace")],
            default_mount_id: Some("workspace".to_string()),
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        };
        append_canvas_mounts(&mut vfs, std::slice::from_ref(&canvas));
        vfs.mounts.push(relay_mount("tail"));
        let before_order = vfs
            .mounts
            .iter()
            .map(|mount| mount.id.clone())
            .collect::<Vec<_>>();

        append_canvas_mounts(&mut vfs, std::slice::from_ref(&canvas));

        let after_order = vfs
            .mounts
            .iter()
            .map(|mount| mount.id.clone())
            .collect::<Vec<_>>();
        assert_eq!(after_order, before_order);
    }

    #[test]
    fn refresh_canvas_mount_binding_files_omits_empty_binding_metadata() {
        let canvas = canvas("cvs-dashboard-a");
        let mut vfs = Vfs::default();
        append_canvas_mounts(&mut vfs, std::slice::from_ref(&canvas));

        refresh_canvas_mount_binding_files(&mut vfs, &canvas, &[]);

        let mount = vfs
            .mounts
            .iter()
            .find(|mount| mount.id == canvas.mount_id)
            .expect("canvas mount should exist");
        assert!(
            mount
                .metadata
                .get("binding_files")
                .is_none_or(serde_json::Value::is_null),
            "empty Canvas binding projection should not create binding_files metadata"
        );
    }
}
