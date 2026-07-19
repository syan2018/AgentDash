use std::collections::BTreeSet;

use agentdash_domain::canvas::Canvas;
use agentdash_platform_spi::Vfs;

use agentdash_application_vfs::{ResolvedVfsSurfaceSource, VfsService, parse_mount_uri};

use super::runtime::runtime_binding_from_canvas_binding;
use super::{
    CanvasResolvedBindingFile, CanvasRuntimeFile, CanvasRuntimeSnapshot, build_runtime_snapshot,
    canvas_mount_runtime_data_bindings, canvas_vfs_mount_id, unresolved_canvas_binding_files,
};

pub struct CanvasRuntimeResourceService<'a> {
    vfs_service: &'a VfsService,
}

impl<'a> CanvasRuntimeResourceService<'a> {
    pub fn new(vfs_service: &'a VfsService) -> Self {
        Self { vfs_service }
    }

    pub async fn build_snapshot_with_bindings(
        &self,
        canvas: &Canvas,
        session_id: Option<String>,
        vfs: Option<&Vfs>,
    ) -> CanvasRuntimeSnapshot {
        let mut snapshot = build_runtime_snapshot(canvas, session_id);
        let Some(vfs) = vfs else {
            return snapshot;
        };
        if let Some(session_id) = snapshot.session_id.as_deref() {
            snapshot.resource_surface_ref = Some(
                ResolvedVfsSurfaceSource::SessionRuntime {
                    session_id: session_id.to_string(),
                }
                .surface_ref(),
            );
        }
        let runtime_bindings = canvas_runtime_mount(vfs, canvas)
            .map(canvas_mount_runtime_data_bindings)
            .unwrap_or_default();
        snapshot.bindings = runtime_bindings
            .iter()
            .map(runtime_binding_from_canvas_binding)
            .collect();
        let mut existing_paths = snapshot
            .files
            .iter()
            .map(|file| file.path.clone())
            .collect::<BTreeSet<_>>();
        for binding in &runtime_bindings {
            let path = binding.data_path();
            if existing_paths.insert(path.clone()) {
                snapshot.files.push(CanvasRuntimeFile {
                    path,
                    content: binding.placeholder_content().to_string(),
                    file_type: "data".to_string(),
                });
            }
        }

        let resolved_files = self.resolve_binding_files(canvas, vfs).await;
        for resolved_file in resolved_files {
            let Some(binding) = snapshot
                .bindings
                .iter_mut()
                .find(|binding| binding.alias == resolved_file.alias)
            else {
                continue;
            };
            if let Some(file) = snapshot
                .files
                .iter_mut()
                .find(|file| file.path == binding.data_path)
            {
                file.content = resolved_file.content;
                file.file_type = "data".to_string();
                binding.resolved = resolved_file.resolved;
            }
        }

        snapshot
    }

    pub async fn resolve_binding_files(
        &self,
        canvas: &Canvas,
        vfs: &Vfs,
    ) -> Vec<CanvasResolvedBindingFile> {
        let runtime_bindings = canvas_runtime_mount(vfs, canvas)
            .map(canvas_mount_runtime_data_bindings)
            .unwrap_or_default();
        let mut files = unresolved_canvas_binding_files(&runtime_bindings);
        for file in &mut files {
            let Ok(resource_ref) = parse_mount_uri(&file.source_uri, vfs) else {
                continue;
            };
            let Ok(result) = self
                .vfs_service
                .read_text(vfs, &resource_ref, None, None)
                .await
            else {
                continue;
            };
            file.content = result.content;
            file.resolved = true;
        }
        files
    }
}

fn canvas_runtime_mount<'a>(
    vfs: &'a Vfs,
    canvas: &Canvas,
) -> Option<&'a agentdash_domain::common::Mount> {
    let mount_id = canvas_vfs_mount_id(&canvas.mount_id);
    vfs.mounts.iter().find(|mount| mount.id == mount_id)
}
