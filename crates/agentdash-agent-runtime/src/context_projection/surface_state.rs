use std::collections::{BTreeMap, BTreeSet};

use agentdash_agent_protocol::{
    RuntimeCompanionAgentEntry, RuntimeContextFragmentEntry, RuntimeMemoryDiagnosticEntry,
    RuntimeMemorySourceEntry, RuntimeSkillEntry, RuntimeToolSchemaEntry,
};
use serde::{Deserialize, Serialize};

/// Runtime-owned, protocol-neutral snapshot used to project a surface transition.
///
/// Values are normalized by the Application source adapter once, persisted with the immutable
/// business surface, and never reconstructed from driver DTOs or bootstrap presentation JSON.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct NormalizedContextSurfaceState {
    #[serde(default)]
    pub capability_keys: BTreeSet<String>,
    #[serde(default)]
    pub excluded_tool_paths: BTreeSet<String>,
    #[serde(default)]
    pub included_tool_paths: BTreeSet<String>,
    #[serde(default)]
    pub mcp_servers: BTreeMap<String, NormalizedSurfaceEntity>,
    #[serde(default)]
    pub unavailable_mcp_servers: Vec<NormalizedMcpServerReadiness>,
    #[serde(default)]
    pub companion_agents: BTreeMap<String, RuntimeCompanionAgentEntry>,
    #[serde(default)]
    pub companion_agent_order: Vec<String>,
    #[serde(default)]
    pub vfs_mounts: BTreeMap<String, NormalizedSurfaceEntity>,
    #[serde(default)]
    pub vfs_links: BTreeMap<String, NormalizedSurfaceEntity>,
    #[serde(default)]
    pub default_vfs_mount: Option<String>,
    #[serde(default)]
    pub memory_sources: BTreeMap<String, RuntimeMemorySourceEntry>,
    #[serde(default)]
    pub memory_source_order: Vec<String>,
    #[serde(default)]
    pub memory_diagnostics: Vec<RuntimeMemoryDiagnosticEntry>,
    #[serde(default)]
    pub skills: BTreeMap<String, RuntimeSkillEntry>,
    #[serde(default)]
    pub skill_clusters: Vec<NormalizedSkillCluster>,
    #[serde(default)]
    pub tool_schemas: BTreeMap<String, RuntimeToolSchemaEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignment: Option<NormalizedAssignmentContext>,
}

/// Stable identity plus a content fingerprint for an entity whose payload is not rendered.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct NormalizedSurfaceEntity {
    pub fingerprint: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct NormalizedMcpServerReadiness {
    pub name: String,
    pub reason_code: String,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct NormalizedSkillCluster {
    pub provider_key: String,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_summary: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct NormalizedAssignmentContext {
    pub revision: u64,
    #[serde(default)]
    pub fragments: Vec<RuntimeContextFragmentEntry>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SurfaceSetDelta {
    pub added: Vec<String>,
    pub removed: Vec<String>,
}

impl SurfaceSetDelta {
    #[must_use]
    pub fn between(previous: &BTreeSet<String>, target: &BTreeSet<String>) -> Self {
        Self {
            added: target.difference(previous).cloned().collect(),
            removed: previous.difference(target).cloned().collect(),
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SurfaceEntityDelta {
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub changed: Vec<String>,
}

impl SurfaceEntityDelta {
    #[must_use]
    pub fn between<T: PartialEq>(
        previous: &BTreeMap<String, T>,
        target: &BTreeMap<String, T>,
    ) -> Self {
        let previous_keys = previous.keys().cloned().collect::<BTreeSet<_>>();
        let target_keys = target.keys().cloned().collect::<BTreeSet<_>>();
        let changed = previous_keys
            .intersection(&target_keys)
            .filter(|key| previous.get(*key) != target.get(*key))
            .cloned()
            .collect();
        Self {
            added: target_keys.difference(&previous_keys).cloned().collect(),
            removed: previous_keys.difference(&target_keys).cloned().collect(),
            changed,
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.changed.is_empty()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NormalizedVfsSurfaceDelta {
    pub mounts: SurfaceEntityDelta,
    pub links: SurfaceEntityDelta,
    pub default_mount_before: Option<String>,
    pub default_mount_after: Option<String>,
}

impl NormalizedVfsSurfaceDelta {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.mounts.is_empty()
            && self.links.is_empty()
            && self.default_mount_before == self.default_mount_after
    }

    /// Whether the main presentation vocabulary can represent this VFS change.
    ///
    /// The normalized state retains link and mount fingerprint changes for later consumers, while
    /// the owned `VfsDelta` section only exposes mount membership and default-mount transitions.
    #[must_use]
    pub fn presentation_is_empty(&self) -> bool {
        self.mounts.added.is_empty()
            && self.mounts.removed.is_empty()
            && self.default_mount_before == self.default_mount_after
    }
}

/// Single deterministic diff consumed by all live dimension projectors.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NormalizedContextSurfaceDelta {
    pub capability_keys: SurfaceSetDelta,
    pub excluded_tool_paths: SurfaceSetDelta,
    pub included_tool_paths: SurfaceSetDelta,
    pub mcp_servers: SurfaceEntityDelta,
    pub unavailable_mcp_servers: Vec<NormalizedMcpServerReadiness>,
    pub mcp_server_readiness_changed: bool,
    pub companion_agents: SurfaceEntityDelta,
    pub vfs: NormalizedVfsSurfaceDelta,
    pub memory_sources: SurfaceEntityDelta,
    pub skills: SurfaceEntityDelta,
    pub tool_schemas: SurfaceEntityDelta,
    pub assignment_changed: bool,
}

impl NormalizedContextSurfaceDelta {
    #[must_use]
    pub fn between(
        previous: &NormalizedContextSurfaceState,
        target: &NormalizedContextSurfaceState,
    ) -> Self {
        Self {
            capability_keys: SurfaceSetDelta::between(
                &previous.capability_keys,
                &target.capability_keys,
            ),
            excluded_tool_paths: SurfaceSetDelta::between(
                &previous.excluded_tool_paths,
                &target.excluded_tool_paths,
            ),
            included_tool_paths: SurfaceSetDelta::between(
                &previous.included_tool_paths,
                &target.included_tool_paths,
            ),
            mcp_servers: SurfaceEntityDelta::between(&previous.mcp_servers, &target.mcp_servers),
            unavailable_mcp_servers: target.unavailable_mcp_servers.clone(),
            mcp_server_readiness_changed: previous.unavailable_mcp_servers
                != target.unavailable_mcp_servers,
            companion_agents: SurfaceEntityDelta::between(
                &previous.companion_agents,
                &target.companion_agents,
            ),
            vfs: NormalizedVfsSurfaceDelta {
                mounts: SurfaceEntityDelta::between(&previous.vfs_mounts, &target.vfs_mounts),
                links: SurfaceEntityDelta::between(&previous.vfs_links, &target.vfs_links),
                default_mount_before: previous.default_vfs_mount.clone(),
                default_mount_after: target.default_vfs_mount.clone(),
            },
            memory_sources: SurfaceEntityDelta::between(
                &previous.memory_sources,
                &target.memory_sources,
            ),
            skills: SurfaceEntityDelta::between(&previous.skills, &target.skills),
            tool_schemas: SurfaceEntityDelta::between(&previous.tool_schemas, &target.tool_schemas),
            assignment_changed: previous.assignment != target.assignment,
        }
    }

    #[must_use]
    pub fn capability_dimensions_are_empty(&self) -> bool {
        self.capability_keys.is_empty()
            && self.excluded_tool_paths.is_empty()
            && self.included_tool_paths.is_empty()
            && self.mcp_servers.is_empty()
            && !self.mcp_server_readiness_changed
            && self.companion_agents.is_empty()
            && self.vfs.presentation_is_empty()
            && self.memory_sources.is_empty()
            && self.skills.is_empty()
            && self.tool_schemas.added.is_empty()
            && self.tool_schemas.changed.is_empty()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.capability_dimensions_are_empty() && !self.assignment_changed
    }
}
