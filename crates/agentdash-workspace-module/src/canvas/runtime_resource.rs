use agentdash_domain::canvas::Canvas;
use agentdash_spi::Vfs;

use agentdash_application_vfs::{ResolvedVfsSurfaceSource, VfsService, parse_mount_uri};

use super::{
    CanvasResolvedBindingFile, CanvasRuntimeSnapshot, build_runtime_snapshot,
    unresolved_canvas_binding_files,
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
        let mut files = unresolved_canvas_binding_files(canvas);
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
