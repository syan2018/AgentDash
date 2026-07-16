use agentdash_agent_protocol::ContextFrameSection;

use super::ProjectedSurfaceDimension;
use crate::context_projection::surface_state::NormalizedContextSurfaceDelta;

pub(super) fn project(
    delta: &NormalizedContextSurfaceDelta,
    phase_node: Option<&str>,
) -> Option<ProjectedSurfaceDimension> {
    if delta.vfs.mounts.added.is_empty()
        && delta.vfs.mounts.removed.is_empty()
        && delta.vfs.default_mount_before == delta.vfs.default_mount_after
    {
        return None;
    }
    let mut lines = vec![super::surface_update_heading("VFS Changes", phase_node)];
    if !delta.vfs.mounts.added.is_empty() {
        lines.push("- Added VFS mounts:".to_string());
        lines.extend(
            delta
                .vfs
                .mounts
                .added
                .iter()
                .map(|mount| format!("  - `{mount}` — 已挂载")),
        );
    }
    if !delta.vfs.mounts.removed.is_empty() {
        lines.push("- Removed VFS mounts:".to_string());
        lines.extend(
            delta
                .vfs
                .mounts
                .removed
                .iter()
                .map(|mount| format!("  - `{mount}` — 已移除")),
        );
    }
    if delta.vfs.default_mount_before != delta.vfs.default_mount_after {
        lines.push(format!(
            "- Default VFS mount: `{}` -> `{}`",
            delta.vfs.default_mount_before.as_deref().unwrap_or("none"),
            delta.vfs.default_mount_after.as_deref().unwrap_or("none"),
        ));
    }
    Some(ProjectedSurfaceDimension {
        section: ContextFrameSection::VfsDelta {
            vfs_mounts_added: delta.vfs.mounts.added.clone(),
            vfs_mounts_removed: delta.vfs.mounts.removed.clone(),
            default_mount_before: delta.vfs.default_mount_before.clone(),
            default_mount_after: delta.vfs.default_mount_after.clone(),
        },
        rendered_text: lines.join("\n"),
    })
}
