use std::collections::BTreeMap;

use agentdash_domain::workflow::MountDirective;
use agentdash_spi::{
    ApplyMountOperationsEffect, ApplyVfsOverlayEffect, CAPABILITY_DIMENSION_COMPANION,
    CAPABILITY_DIMENSION_MCP, CAPABILITY_DIMENSION_TOOL, CAPABILITY_DIMENSION_VFS,
    EFFECT_TYPE_APPLY_MOUNT_OPERATIONS, EFFECT_TYPE_APPLY_VFS_OVERLAY,
    EFFECT_TYPE_SET_COMPANION_AGENT_ROSTER, EFFECT_TYPE_SET_MCP_SERVER_SET,
    EFFECT_TYPE_SET_TOOL_ACCESS, RuntimeCapabilityTransition, SetCompanionAgentRosterEffect,
    SetMcpServerSetEffect, SetToolAccessEffect, Vfs,
};

use super::types::CapabilityState;

pub(crate) fn apply_runtime_capability_transition(
    base_state: &CapabilityState,
    transition: &RuntimeCapabilityTransition,
) -> Result<CapabilityState, String> {
    let mut state = base_state.clone();
    for effect in &transition.effects {
        match (effect.dimension.as_str(), effect.effect_type.as_str()) {
            (CAPABILITY_DIMENSION_TOOL, EFFECT_TYPE_SET_TOOL_ACCESS) => {
                let payload = decode_effect::<SetToolAccessEffect>(&effect.payload)?;
                state.tool.capabilities = payload.capabilities;
                state.tool.enabled_clusters = payload.enabled_clusters;
                state.tool.tool_policy = payload.tool_policy;
            }
            (CAPABILITY_DIMENSION_MCP, EFFECT_TYPE_SET_MCP_SERVER_SET) => {
                let payload = decode_effect::<SetMcpServerSetEffect>(&effect.payload)?;
                state.tool.mcp_servers = payload.servers;
            }
            (CAPABILITY_DIMENSION_COMPANION, EFFECT_TYPE_SET_COMPANION_AGENT_ROSTER) => {
                let payload = decode_effect::<SetCompanionAgentRosterEffect>(&effect.payload)?;
                state.companion.agents = payload.agents;
            }
            (CAPABILITY_DIMENSION_VFS, EFFECT_TYPE_APPLY_VFS_OVERLAY) => {
                let payload = decode_effect::<ApplyVfsOverlayEffect>(&effect.payload)?;
                state.vfs.active = Some(merge_vfs_overlay(
                    state.vfs.active.take().unwrap_or_default(),
                    payload.overlay,
                ));
            }
            (CAPABILITY_DIMENSION_VFS, EFFECT_TYPE_APPLY_MOUNT_OPERATIONS) => {
                let payload = decode_effect::<ApplyMountOperationsEffect>(&effect.payload)?;
                let mut active = state.vfs.active.take().unwrap_or_default();
                apply_mount_operations(&mut active, payload.operations);
                state.vfs.active = Some(active);
            }
            _ => {}
        }
    }
    Ok(state)
}

fn decode_effect<T>(payload: &serde_json::Value) -> Result<T, String>
where
    T: serde::de::DeserializeOwned,
{
    serde_json::from_value(payload.clone())
        .map_err(|error| format!("runtime capability effect payload decode failed: {error}"))
}

fn merge_vfs_overlay(mut base: Vfs, overlay: Vfs) -> Vfs {
    let mut mounts = base
        .mounts
        .into_iter()
        .map(|mount| (mount.id.clone(), mount))
        .collect::<BTreeMap<_, _>>();
    for mount in overlay.mounts {
        mounts.insert(mount.id.clone(), mount);
    }

    let mut links = base
        .links
        .into_iter()
        .map(|link| ((link.from_mount_id.clone(), link.from_path.clone()), link))
        .collect::<BTreeMap<_, _>>();
    for link in overlay.links {
        links.insert((link.from_mount_id.clone(), link.from_path.clone()), link);
    }

    base.mounts = mounts.into_values().collect();
    base.links = links.into_values().collect();
    if overlay.default_mount_id.is_some() {
        base.default_mount_id = overlay.default_mount_id;
    }
    if overlay.source_project_id.is_some() {
        base.source_project_id = overlay.source_project_id;
    }
    if overlay.source_story_id.is_some() {
        base.source_story_id = overlay.source_story_id;
    }
    base
}

fn apply_mount_operations(vfs: &mut Vfs, operations: Vec<MountDirective>) {
    for operation in operations {
        match operation {
            MountDirective::AddMount { mount } | MountDirective::ReplaceMount { mount } => {
                vfs.mounts.retain(|existing| existing.id != mount.id);
                vfs.mounts.push(mount);
            }
            MountDirective::RemoveMount { mount_id } => {
                vfs.mounts.retain(|mount| mount.id != mount_id);
                vfs.links
                    .retain(|link| link.from_mount_id != mount_id && link.to_mount_id != mount_id);
                if vfs.default_mount_id.as_deref() == Some(mount_id.as_str()) {
                    vfs.default_mount_id = None;
                }
            }
            MountDirective::AddLink { link } => {
                vfs.links.retain(|existing| {
                    existing.from_mount_id != link.from_mount_id
                        || existing.from_path != link.from_path
                });
                vfs.links.push(link);
            }
            MountDirective::RemoveLink {
                from_mount_id,
                from_path,
            } => {
                vfs.links.retain(|link| {
                    link.from_mount_id != from_mount_id || link.from_path != from_path
                });
            }
            MountDirective::SetDefaultMount { mount_id } => {
                vfs.default_mount_id = mount_id;
            }
        }
    }
}
