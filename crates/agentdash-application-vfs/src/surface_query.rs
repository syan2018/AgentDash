use agentdash_application_ports::vfs_surface_runtime::VfsSurfaceRuntimeProjection;
use agentdash_domain::inline_file::InlineFileRepository;

use super::{
    PROVIDER_INLINE_FS, ResolvedMountSummary, ResolvedVfsSurface, ResolvedVfsSurfaceSource,
    inline_storage_key_from_mount, mount_purpose,
};

pub async fn build_surface_summary(
    inline_file_repo: &dyn InlineFileRepository,
    runtime: &dyn VfsSurfaceRuntimeProjection,
    source: &ResolvedVfsSurfaceSource,
    vfs: &agentdash_spi::Vfs,
) -> ResolvedVfsSurface {
    let mut mounts = Vec::with_capacity(vfs.mounts.len());

    for mount in &vfs.mounts {
        let backend_online = if !mount.backend_id.is_empty() {
            Some(runtime.is_backend_online(&mount.backend_id).await)
        } else {
            None
        };

        let file_count = if mount.provider == PROVIDER_INLINE_FS {
            if let Ok(storage_key) = inline_storage_key_from_mount(mount) {
                inline_file_repo
                    .count_files(
                        storage_key.owner_kind,
                        storage_key.owner_id,
                        &storage_key.container_id,
                    )
                    .await
                    .ok()
                    .map(|count| count as usize)
            } else {
                None
            }
        } else {
            None
        };

        mounts.push(ResolvedMountSummary {
            id: mount.id.clone(),
            display_name: mount.display_name.clone(),
            provider: mount.provider.clone(),
            backend_id: mount.backend_id.clone(),
            capabilities: mount
                .capabilities
                .iter()
                .map(|capability| format!("{capability:?}").to_lowercase())
                .collect(),
            default_write: mount.default_write,
            purpose: mount_purpose(mount),
            backend_online,
            file_count,
            edit_capabilities: runtime.edit_capabilities(mount),
        });
    }

    ResolvedVfsSurface {
        surface_ref: source.surface_ref(),
        source: source.clone(),
        mounts,
        default_mount_id: vfs.default_mount_id.clone(),
    }
}
