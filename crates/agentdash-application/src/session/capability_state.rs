use std::collections::{BTreeMap, BTreeSet};

use agentdash_domain::common::{MountLink, Vfs};
use agentdash_domain::workflow::MountDirective;
use agentdash_spi::hooks::CapabilityDelta;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use super::types::{CapabilityState, PendingCapabilityStateTransition};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SetDelta {
    pub added: Vec<String>,
    pub removed: Vec<String>,
}

impl SetDelta {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct NamedEntityDelta {
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub changed: Vec<String>,
}

impl NamedEntityDelta {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.changed.is_empty()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DefaultMountDelta {
    pub before: Option<String>,
    pub after: Option<String>,
    pub changed: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct VfsSurfaceDelta {
    pub mounts: NamedEntityDelta,
    pub links: NamedEntityDelta,
    pub default_mount: DefaultMountDelta,
}

impl VfsSurfaceDelta {
    pub fn is_empty(&self) -> bool {
        self.mounts.is_empty() && self.links.is_empty() && !self.default_mount.changed
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CapabilityStateDelta {
    pub tool_capabilities: SetDelta,
    pub tool_clusters: SetDelta,
    pub excluded_tool_paths: SetDelta,
    pub included_tool_paths: SetDelta,
    pub mcp_servers: NamedEntityDelta,
    pub vfs: VfsSurfaceDelta,
}

impl CapabilityStateDelta {
    pub fn is_empty(&self) -> bool {
        self.tool_capabilities.is_empty()
            && self.tool_clusters.is_empty()
            && self.excluded_tool_paths.is_empty()
            && self.included_tool_paths.is_empty()
            && self.mcp_servers.is_empty()
            && self.vfs.is_empty()
    }
}

/// 一次 workflow/runtime 上下文切换的结构化描述。
///
/// 它把 phase 切换带来的 active workflow、能力状态、
/// hook/event payload 和 pending metadata 统一放进同一个事务值对象。live apply、
/// pending next turn、next-turn apply 都应从这里派生事件，避免多个入口各自拼 JSON。
pub struct RuntimeContextTransition<'a> {
    pub phase_node: &'a str,
    pub run_id: Option<Uuid>,
    pub lifecycle_key: Option<&'a str>,
    pub apply_mode: &'a str,
    pub before_state: Option<&'a CapabilityState>,
    pub after_state: &'a CapabilityState,
    pub capability_keys: &'a BTreeSet<String>,
    pub steering_delivery: Value,
    pub state_changed_override: Option<bool>,
    pub steering_capability_delta: Option<&'a CapabilityDelta>,
}

impl<'a> RuntimeContextTransition<'a> {
    pub fn event_payload(&self) -> Value {
        let delta = compute_capability_state_delta(
            self.before_state,
            self.after_state,
            self.capability_keys,
        );
        let state_changed = self
            .state_changed_override
            .unwrap_or(self.before_state != Some(self.after_state));
        let after_vfs = self.after_state.vfs.as_ref();
        let current_clusters = self
            .after_state
            .tool_clusters
            .iter()
            .map(|cluster| format!("{cluster:?}"))
            .collect::<Vec<_>>();
        let current_excluded_paths = self
            .after_state
            .excluded_tool_paths()
            .into_iter()
            .collect::<Vec<_>>();
        let current_included_paths = self
            .after_state
            .included_tool_paths()
            .into_iter()
            .collect::<Vec<_>>();
        let mcp_servers = self
            .after_state
            .mcp_servers
            .iter()
            .map(|server| server.name.clone())
            .collect::<Vec<_>>();
        let mount_ids: Vec<String> = after_vfs
            .map(|vfs| vfs.mounts.iter().map(|mount| mount.id.clone()).collect())
            .unwrap_or_default();
        let mut payload = serde_json::json!({
            "phase_node": self.phase_node,
            "run_id": self.run_id.map(|id| id.to_string()),
            "lifecycle_key": self.lifecycle_key,
            "apply_mode": self.apply_mode,
            "state_changed": state_changed,
            "delta": delta,
            "tool_capabilities": {
                "current": self.capability_keys.iter().cloned().collect::<Vec<_>>(),
            },
            "tool_state": {
                "tool_clusters": current_clusters,
                "excluded_tool_paths": current_excluded_paths,
                "included_tool_paths": current_included_paths,
            },
            "mcp": {
                "server_count": self.after_state.mcp_servers.len(),
                "servers": mcp_servers,
            },
            "vfs": {
                "mounts": mount_ids,
                "default_mount_id": after_vfs.and_then(|vfs| vfs.default_mount_id.clone()),
                "links": after_vfs.map(|vfs| vfs.links.iter().map(link_key).collect::<Vec<_>>()).unwrap_or_default(),
            },
            "steering_delivery": self.steering_delivery.clone(),
        });
        if let (Some(object), Some(delta)) =
            (payload.as_object_mut(), self.steering_capability_delta)
        {
            object.insert(
                "steering_capability_delta".to_string(),
                serde_json::json!({
                    "added": delta.added.clone(),
                    "removed": delta.removed.clone(),
                }),
            );
        }
        payload
    }

    pub fn to_pending_capability_state_transition(
        &self,
        id: String,
        source_turn_id: Option<String>,
        created_at: i64,
    ) -> Option<PendingCapabilityStateTransition> {
        Some(PendingCapabilityStateTransition {
            id,
            run_id: self.run_id?,
            lifecycle_key: self.lifecycle_key?.to_string(),
            phase_node: self.phase_node.to_string(),
            capability_keys: self.capability_keys.clone(),
            state: self.after_state.clone(),
            created_at,
            source_turn_id,
        })
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

pub fn compute_capability_state_delta(
    before: Option<&CapabilityState>,
    after: &CapabilityState,
    after_capability_keys: &BTreeSet<String>,
) -> CapabilityStateDelta {
    let before_capabilities = before
        .map(|state| state.capability_keys())
        .unwrap_or_default();
    let before_clusters = before
        .map(|state| {
            state
                .tool_clusters
                .iter()
                .map(|cluster| format!("{cluster:?}"))
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    let after_clusters = after
        .tool_clusters
        .iter()
        .map(|cluster| format!("{cluster:?}"))
        .collect::<BTreeSet<_>>();
    let before_excluded_paths = before
        .map(|state| state.excluded_tool_paths())
        .unwrap_or_default();
    let before_included_paths = before
        .map(|state| state.included_tool_paths())
        .unwrap_or_default();

    CapabilityStateDelta {
        tool_capabilities: set_delta(&before_capabilities, after_capability_keys),
        tool_clusters: set_delta(&before_clusters, &after_clusters),
        excluded_tool_paths: set_delta(&before_excluded_paths, &after.excluded_tool_paths()),
        included_tool_paths: set_delta(&before_included_paths, &after.included_tool_paths()),
        mcp_servers: named_entity_delta(
            before
                .map(|surface| surface.mcp_servers.as_slice())
                .unwrap_or(&[]),
            after.mcp_servers.as_slice(),
            |server| server.name.clone(),
        ),
        vfs: vfs_surface_delta(
            before.and_then(|surface| surface.vfs.as_ref()),
            after.vfs.as_ref(),
        ),
    }
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

fn set_delta(before: &BTreeSet<String>, after: &BTreeSet<String>) -> SetDelta {
    SetDelta {
        added: after.difference(before).cloned().collect(),
        removed: before.difference(after).cloned().collect(),
    }
}

fn named_entity_delta<T, F>(before: &[T], after: &[T], key: F) -> NamedEntityDelta
where
    T: PartialEq,
    F: Fn(&T) -> String,
{
    let before_map = before
        .iter()
        .map(|item| (key(item), item))
        .collect::<BTreeMap<_, _>>();
    let after_map = after
        .iter()
        .map(|item| (key(item), item))
        .collect::<BTreeMap<_, _>>();
    let before_keys = before_map.keys().cloned().collect::<BTreeSet<_>>();
    let after_keys = after_map.keys().cloned().collect::<BTreeSet<_>>();
    let mut changed = Vec::new();
    for name in before_keys.intersection(&after_keys) {
        if before_map.get(name) != after_map.get(name) {
            changed.push(name.clone());
        }
    }

    NamedEntityDelta {
        added: after_keys.difference(&before_keys).cloned().collect(),
        removed: before_keys.difference(&after_keys).cloned().collect(),
        changed,
    }
}

fn vfs_surface_delta(before: Option<&Vfs>, after: Option<&Vfs>) -> VfsSurfaceDelta {
    let before_mounts = before.map(|vfs| vfs.mounts.as_slice()).unwrap_or(&[]);
    let after_mounts = after.map(|vfs| vfs.mounts.as_slice()).unwrap_or(&[]);
    let before_links = before.map(|vfs| vfs.links.as_slice()).unwrap_or(&[]);
    let after_links = after.map(|vfs| vfs.links.as_slice()).unwrap_or(&[]);
    let before_default = before.and_then(|vfs| vfs.default_mount_id.clone());
    let after_default = after.and_then(|vfs| vfs.default_mount_id.clone());
    VfsSurfaceDelta {
        mounts: named_entity_delta(before_mounts, after_mounts, |mount| mount.id.clone()),
        links: named_entity_delta(before_links, after_links, link_key),
        default_mount: DefaultMountDelta {
            changed: before_default != after_default,
            before: before_default,
            after: after_default,
        },
    }
}

fn link_key(link: &MountLink) -> String {
    format!("{}:{}", link.from_mount_id, link.from_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::common::{Mount, MountCapability};
    use agentdash_spi::CapabilityState;

    fn mount(id: &str, provider: &str) -> Mount {
        Mount {
            id: id.to_string(),
            provider: provider.to_string(),
            backend_id: "backend".to_string(),
            root_ref: format!("{provider}://{id}"),
            capabilities: vec![MountCapability::Read],
            default_write: false,
            display_name: id.to_string(),
            metadata: serde_json::Value::Null,
        }
    }

    #[test]
    fn mount_directives_can_add_remove_link_and_switch_default() {
        let base = Vfs {
            mounts: vec![mount("workspace", "relay_fs"), mount("secret", "inline_fs")],
            default_mount_id: Some("workspace".to_string()),
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        };
        let overlay = Vfs::default();
        let result = compose_vfs_with_overlay_and_directives(
            Some(&base),
            &overlay,
            &[
                MountDirective::RemoveMount {
                    mount_id: "secret".to_string(),
                },
                MountDirective::AddMount {
                    mount: mount("review", "inline_fs"),
                },
                MountDirective::AddLink {
                    link: MountLink {
                        from_mount_id: "workspace".to_string(),
                        from_path: "review".to_string(),
                        to_mount_id: "review".to_string(),
                        to_path: String::new(),
                    },
                },
                MountDirective::SetDefaultMount {
                    mount_id: Some("review".to_string()),
                },
            ],
        );

        let ids = result
            .mounts
            .iter()
            .map(|mount| mount.id.as_str())
            .collect::<BTreeSet<_>>();
        assert!(ids.contains("workspace"));
        assert!(ids.contains("review"));
        assert!(!ids.contains("secret"));
        assert_eq!(result.default_mount_id.as_deref(), Some("review"));
        assert_eq!(result.links.len(), 1);
    }

    #[test]
    fn event_payload_uses_structured_capability_state_shape() {
        let mut capability_keys = BTreeSet::new();
        capability_keys.insert("file_read".to_string());
        let after_state = CapabilityState {
            mcp_servers: Vec::new(),
            vfs: Some(Vfs {
                mounts: vec![mount("workspace", "relay_fs")],
                default_mount_id: Some("workspace".to_string()),
                source_project_id: None,
                source_story_id: None,
                links: Vec::new(),
            }),
            ..Default::default()
        };

        let payload = RuntimeContextTransition {
            phase_node: "review",
            run_id: Some(Uuid::new_v4()),
            lifecycle_key: Some("lc"),
            apply_mode: "live",
            before_state: None,
            after_state: &after_state,
            capability_keys: &capability_keys,
            steering_delivery: serde_json::json!({"status": "not_required"}),
            state_changed_override: None,
            steering_capability_delta: None,
        }
        .event_payload();

        assert!(payload.get("tool_capabilities").is_some());
        assert!(payload.get("tool_state").is_some());
        assert!(payload.get("mcp").is_some());
        assert!(payload.get("vfs").is_some());
        assert!(
            payload
                .get("delta")
                .and_then(|value| value.get("tool_capabilities"))
                .is_some(),
            "delta 字段应使用 snake_case，便于前端直接读取规范字段"
        );
        assert!(payload.get("added").is_none());
        assert!(payload.get("removed").is_none());
        assert!(payload.get("capabilities").is_none());
        assert!(payload.get("tool_clusters").is_none());
        assert!(payload.get("mcp_servers").is_none());
        assert!(payload.get("mounts").is_none());
    }
}
