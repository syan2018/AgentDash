//! 能力状态 delta 的纯数据模型与计算。
//!
//! delta 描述两份 `CapabilityState` 之间的结构化差异（工具能力 / 工具路径 /
//! MCP server / VFS / skill），是运行期能力切换通知与前端投影的共同基准。
//! 类型与计算都只依赖 spi `CapabilityState` 与 domain `Vfs`/`MountLink`，
//! 因此放在 spi 层，供 application 的 transition / projection / 渲染各阶段消费。

use std::collections::{BTreeMap, BTreeSet};

use agentdash_domain::common::{MountLink, Vfs};
use serde::{Deserialize, Serialize};

use super::CapabilityState;

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
    pub skills: NamedEntityDelta,
}

impl CapabilityStateDelta {
    pub fn is_empty(&self) -> bool {
        self.tool_capabilities.is_empty()
            && self.tool_clusters.is_empty()
            && self.excluded_tool_paths.is_empty()
            && self.included_tool_paths.is_empty()
            && self.mcp_servers.is_empty()
            && self.vfs.is_empty()
            && self.skills.is_empty()
    }
}

/// 计算两份 `CapabilityState` 之间的结构化 delta。
///
/// `after_capability_keys` 是调用方已展开的目标能力 key 集合（运行期 hook
/// 维护的有效能力视图），用于 tool capability 维度的 diff 基准。
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
                .tool
                .enabled_clusters
                .iter()
                .map(|cluster| format!("{cluster:?}"))
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    let after_clusters = after
        .tool
        .enabled_clusters
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
                .map(|surface| surface.tool.mcp_servers.as_slice())
                .unwrap_or(&[]),
            after.tool.mcp_servers.as_slice(),
            |server| server.name.clone(),
        ),
        vfs: vfs_surface_delta(
            before.and_then(|surface| surface.vfs.active.as_ref()),
            after.vfs.active.as_ref(),
        ),
        skills: named_entity_delta(
            before
                .map(|surface| surface.skill.skills.as_slice())
                .unwrap_or(&[]),
            after.skill.skills.as_slice(),
            |skill| skill.name.clone(),
        ),
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
