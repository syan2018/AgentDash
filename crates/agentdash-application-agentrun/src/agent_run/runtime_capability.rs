use agentdash_domain::common::Vfs;
use agentdash_domain::workflow::{AgentFrame, MountDirective};
use agentdash_platform_spi::{CapabilityState, RuntimeMcpServer};

pub use agentdash_platform_spi::{
    CapabilityStateDelta, DefaultMountDelta, NamedEntityDelta, SetDelta, VfsSurfaceDelta,
    compute_capability_state_delta,
};

/// AgentFrame revision 拆解后的 capability surface JSON 三元组。
#[derive(Debug, Clone)]
pub struct FrameCapabilitySurfaces {
    pub effective_capability_json: Option<serde_json::Value>,
    pub vfs_surface_json: Option<serde_json::Value>,
    pub mcp_surface_json: Option<serde_json::Value>,
}

/// 从 immutable AgentFrame revision 投影只读 CapabilityState。
pub fn project_capability_state_from_frame(frame: &AgentFrame) -> CapabilityState {
    let mut state: CapabilityState = frame
        .effective_capability_json
        .as_ref()
        .and_then(|json| serde_json::from_value(json.clone()).ok())
        .unwrap_or_default();

    if state.vfs.active.is_none()
        && let Some(vfs) = frame
            .vfs_surface_json
            .as_ref()
            .and_then(|json| serde_json::from_value::<Vfs>(json.clone()).ok())
    {
        state.vfs.active = Some(vfs);
    }

    if state.tool.mcp_servers.is_empty()
        && let Some(servers) = frame
            .mcp_surface_json
            .as_ref()
            .and_then(|json| serde_json::from_value::<Vec<RuntimeMcpServer>>(json.clone()).ok())
    {
        state.tool.mcp_servers = servers;
    }

    state
}

/// 将 CapabilityState 写成 AgentFrame canonical surface 与同 revision split projections。
pub fn capability_state_to_frame_surfaces(state: &CapabilityState) -> FrameCapabilitySurfaces {
    FrameCapabilitySurfaces {
        effective_capability_json: serde_json::to_value(state).ok(),
        vfs_surface_json: state
            .vfs
            .active
            .as_ref()
            .and_then(|vfs| serde_json::to_value(vfs).ok()),
        mcp_surface_json: serde_json::to_value(&state.tool.mcp_servers).ok(),
    }
}

/// 将 ProjectAgent 声明的 workspace module allowlist 投影进当前 Frame revision。
pub fn project_workspace_module_dimension(
    refs: Option<&[String]>,
) -> agentdash_platform_spi::WorkspaceModuleDimension {
    match refs {
        Some(ids) if !ids.is_empty() => agentdash_platform_spi::WorkspaceModuleDimension {
            mode: agentdash_platform_spi::WorkspaceModuleVisibilityMode::Allowlist,
            allowed_module_ids: ids.to_vec(),
        },
        _ => agentdash_platform_spi::WorkspaceModuleDimension::all(),
    }
}

pub fn compose_vfs_with_overlay_and_directives(
    base_vfs: Option<&Vfs>,
    overlay_vfs: &Vfs,
    mount_directives: &[MountDirective],
) -> Vfs {
    let mut vfs = base_vfs.cloned().unwrap_or_default();
    merge_vfs_overlay_into(&mut vfs, overlay_vfs);
    apply_mount_directives(&mut vfs, mount_directives);
    vfs
}

pub fn merge_vfs_overlay(mut base: Vfs, overlay: &Vfs) -> Vfs {
    merge_vfs_overlay_into(&mut base, overlay);
    base
}

fn merge_vfs_overlay_into(base: &mut Vfs, overlay: &Vfs) {
    for mount in &overlay.mounts {
        base.mounts.retain(|existing| existing.id != mount.id);
        base.mounts.push(mount.clone());
    }
    for link in &overlay.links {
        base.links.retain(|existing| {
            existing.from_mount_id != link.from_mount_id || existing.from_path != link.from_path
        });
        base.links.push(link.clone());
    }
    if overlay.default_mount_id.is_some() {
        base.default_mount_id = overlay.default_mount_id.clone();
    }
    if overlay.source_project_id.is_some() {
        base.source_project_id = overlay.source_project_id.clone();
    }
    if overlay.source_story_id.is_some() {
        base.source_story_id = overlay.source_story_id.clone();
    }
}

fn apply_mount_directives(vfs: &mut Vfs, directives: &[MountDirective]) {
    for directive in directives {
        match directive {
            MountDirective::AddMount { mount } | MountDirective::ReplaceMount { mount } => {
                vfs.mounts.retain(|existing| existing.id != mount.id);
                vfs.mounts.push(mount.clone());
            }
            MountDirective::RemoveMount { mount_id } => {
                vfs.mounts.retain(|existing| existing.id != *mount_id);
                vfs.links.retain(|link| {
                    link.from_mount_id != *mount_id && link.to_mount_id != *mount_id
                });
                if vfs.default_mount_id.as_deref() == Some(mount_id.as_str()) {
                    vfs.default_mount_id = None;
                }
            }
            MountDirective::AddLink { link } => {
                vfs.links.retain(|existing| {
                    existing.from_mount_id != link.from_mount_id
                        || existing.from_path != link.from_path
                });
                vfs.links.push(link.clone());
            }
            MountDirective::RemoveLink {
                from_mount_id,
                from_path,
            } => {
                vfs.links.retain(|existing| {
                    existing.from_mount_id != *from_mount_id || existing.from_path != *from_path
                });
            }
            MountDirective::SetDefaultMount { mount_id } => {
                vfs.default_mount_id = mount_id.clone();
            }
        }
    }
}
