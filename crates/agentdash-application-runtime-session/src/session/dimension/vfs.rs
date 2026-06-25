//! VFS 维度 — 追踪虚拟文件系统挂载点的增删与默认挂载变化。

use agentdash_spi::hooks::ContextFrameSection;

use super::DimensionDelta;
use agentdash_spi::CapabilityStateDelta;

#[derive(Debug, Clone)]
pub(crate) struct VfsDimensionDelta {
    pub mounts_added: Vec<String>,
    pub mounts_removed: Vec<String>,
    pub default_mount_before: Option<String>,
    pub default_mount_after: Option<String>,
}

impl VfsDimensionDelta {
    pub fn from_state_delta(
        state_delta: Option<&CapabilityStateDelta>,
    ) -> Option<Box<dyn DimensionDelta>> {
        let state_delta = state_delta?;
        let delta = Self {
            mounts_added: state_delta.vfs.mounts.added.clone(),
            mounts_removed: state_delta.vfs.mounts.removed.clone(),
            default_mount_before: state_delta.vfs.default_mount.before.clone(),
            default_mount_after: state_delta.vfs.default_mount.after.clone(),
        };
        if !delta.has_changes() {
            return None;
        }
        Some(Box::new(delta))
    }
}

impl DimensionDelta for VfsDimensionDelta {
    fn has_changes(&self) -> bool {
        !self.mounts_added.is_empty()
            || !self.mounts_removed.is_empty()
            || self.default_mount_before != self.default_mount_after
    }

    fn to_section(&self) -> ContextFrameSection {
        ContextFrameSection::VfsDelta {
            vfs_mounts_added: self.mounts_added.clone(),
            vfs_mounts_removed: self.mounts_removed.clone(),
            default_mount_before: self.default_mount_before.clone(),
            default_mount_after: self.default_mount_after.clone(),
        }
    }

    fn render_text(&self, phase_node: Option<&str>) -> String {
        let mut lines = vec![match phase_node {
            Some(node) => format!("## VFS Changes — Step Transition: {node}"),
            None => "## VFS Changes".to_string(),
        }];

        if !self.mounts_added.is_empty() {
            lines.push("- Added VFS mounts:".to_string());
            for mount in &self.mounts_added {
                lines.push(format!("  - `{mount}` — 已挂载"));
            }
        }
        if !self.mounts_removed.is_empty() {
            lines.push("- Removed VFS mounts:".to_string());
            for mount in &self.mounts_removed {
                lines.push(format!("  - `{mount}` — 已移除"));
            }
        }
        if self.default_mount_before != self.default_mount_after {
            lines.push(format!(
                "- Default VFS mount: `{}` -> `{}`",
                self.default_mount_before.as_deref().unwrap_or("none"),
                self.default_mount_after.as_deref().unwrap_or("none"),
            ));
        }

        lines.join("\n")
    }
}
