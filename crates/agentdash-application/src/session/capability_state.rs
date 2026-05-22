use std::collections::{BTreeMap, BTreeSet};

use agentdash_domain::common::{MountLink, Vfs};
use agentdash_domain::workflow::MountDirective;
use agentdash_spi::SessionMcpServer;
use agentdash_spi::hooks::CapabilityDelta;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use super::types::{
    ApplyMountOperationsEffect, ApplyVfsOverlayEffect, CAPABILITY_DIMENSION_COMPANION,
    CAPABILITY_DIMENSION_MCP, CAPABILITY_DIMENSION_TOOL, CAPABILITY_DIMENSION_VFS, CapabilityState,
    EFFECT_TYPE_APPLY_MOUNT_OPERATIONS, EFFECT_TYPE_APPLY_VFS_OVERLAY,
    EFFECT_TYPE_SET_COMPANION_AGENT_ROSTER, EFFECT_TYPE_SET_MCP_SERVER_SET,
    EFFECT_TYPE_SET_TOOL_ACCESS, PendingCapabilityStateTransition, RuntimeCapabilityEffectRecord,
    RuntimeCapabilityTransition, SetCompanionAgentRosterEffect, SetMcpServerSetEffect,
    SetToolAccessEffect,
};

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
        let after_vfs = self.after_state.vfs.active.as_ref();
        let current_clusters = self
            .after_state
            .tool
            .enabled_clusters
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
            .tool
            .mcp_servers
            .iter()
            .map(|server| server.name.clone())
            .collect::<Vec<_>>();
        let skill_names = self
            .after_state
            .skill
            .skills
            .iter()
            .map(|skill| skill.name.clone())
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
                "server_count": self.after_state.tool.mcp_servers.len(),
                "servers": mcp_servers,
            },
            "skills": {
                "count": self.after_state.skill.skills.len(),
                "items": skill_names,
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
        transition: RuntimeCapabilityTransition,
        source_turn_id: Option<String>,
        created_at: i64,
    ) -> Option<PendingCapabilityStateTransition> {
        Some(PendingCapabilityStateTransition {
            id,
            run_id: self.run_id?,
            lifecycle_key: self.lifecycle_key?.to_string(),
            phase_node: self.phase_node.to_string(),
            capability_keys: self.capability_keys.clone(),
            transition,
            created_at,
            source_turn_id,
        })
    }
}

pub fn apply_runtime_capability_transition(
    base_state: &CapabilityState,
    transition: &RuntimeCapabilityTransition,
) -> Result<CapabilityState, String> {
    replay_runtime_capability_transition(base_state, transition)
        .map(|replay| replay.capability_state)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeCapabilityReplay {
    pub capability_state: CapabilityState,
    pub effective_vfs: Option<Vfs>,
    pub effective_mcp_servers: Option<Vec<SessionMcpServer>>,
}

#[derive(Debug, Default)]
pub struct RuntimeCapabilityReplayContext {
    pub effective_vfs: Option<Vfs>,
    pub effective_mcp_servers: Option<Vec<SessionMcpServer>>,
}

pub trait CapabilityDimensionModule {
    fn key(&self) -> &'static str;

    fn replay_effect(
        &self,
        state: &mut CapabilityState,
        context: &mut RuntimeCapabilityReplayContext,
        record: &RuntimeCapabilityEffectRecord,
    ) -> Result<(), String>;
}

pub struct CapabilityDimensionRegistry {
    modules: Vec<Box<dyn CapabilityDimensionModule>>,
}

impl CapabilityDimensionRegistry {
    pub fn built_in() -> Self {
        Self {
            modules: vec![
                Box::new(VfsCapabilityDimensionModule),
                Box::new(ToolCapabilityDimensionModule),
                Box::new(McpCapabilityDimensionModule),
                Box::new(CompanionCapabilityDimensionModule),
            ],
        }
    }

    fn module_for(&self, key: &str) -> Option<&dyn CapabilityDimensionModule> {
        self.modules
            .iter()
            .find(|module| module.key() == key)
            .map(|module| module.as_ref())
    }

    fn replay_transition(
        &self,
        state: &mut CapabilityState,
        context: &mut RuntimeCapabilityReplayContext,
        transition: &RuntimeCapabilityTransition,
    ) -> Result<(), String> {
        for module in &self.modules {
            for record in transition
                .effects
                .iter()
                .filter(|record| record.dimension.as_str() == module.key())
            {
                module.replay_effect(state, context, record)?;
            }
        }
        for record in &transition.effects {
            if self.module_for(record.dimension.as_str()).is_none() {
                return Err(format!(
                    "未注册 capability dimension `{}`，无法 replay `{}` effect",
                    record.dimension.as_str(),
                    record.effect_type
                ));
            }
        }
        Ok(())
    }
}

struct ToolCapabilityDimensionModule;
struct McpCapabilityDimensionModule;
struct CompanionCapabilityDimensionModule;
struct VfsCapabilityDimensionModule;

impl CapabilityDimensionModule for ToolCapabilityDimensionModule {
    fn key(&self) -> &'static str {
        CAPABILITY_DIMENSION_TOOL
    }

    fn replay_effect(
        &self,
        state: &mut CapabilityState,
        _context: &mut RuntimeCapabilityReplayContext,
        record: &RuntimeCapabilityEffectRecord,
    ) -> Result<(), String> {
        ensure_effect_type(record, EFFECT_TYPE_SET_TOOL_ACCESS)?;
        let payload: SetToolAccessEffect = decode_effect_payload(record)?;
        state.tool.capabilities = payload.capabilities;
        state.tool.enabled_clusters = payload.enabled_clusters;
        state.tool.tool_policy = payload.tool_policy;
        Ok(())
    }
}

impl CapabilityDimensionModule for McpCapabilityDimensionModule {
    fn key(&self) -> &'static str {
        CAPABILITY_DIMENSION_MCP
    }

    fn replay_effect(
        &self,
        state: &mut CapabilityState,
        context: &mut RuntimeCapabilityReplayContext,
        record: &RuntimeCapabilityEffectRecord,
    ) -> Result<(), String> {
        ensure_effect_type(record, EFFECT_TYPE_SET_MCP_SERVER_SET)?;
        let payload: SetMcpServerSetEffect = decode_effect_payload(record)?;
        state.tool.mcp_servers = payload.servers.clone();
        context.effective_mcp_servers = Some(payload.servers);
        Ok(())
    }
}

impl CapabilityDimensionModule for CompanionCapabilityDimensionModule {
    fn key(&self) -> &'static str {
        CAPABILITY_DIMENSION_COMPANION
    }

    fn replay_effect(
        &self,
        state: &mut CapabilityState,
        _context: &mut RuntimeCapabilityReplayContext,
        record: &RuntimeCapabilityEffectRecord,
    ) -> Result<(), String> {
        ensure_effect_type(record, EFFECT_TYPE_SET_COMPANION_AGENT_ROSTER)?;
        let payload: SetCompanionAgentRosterEffect = decode_effect_payload(record)?;
        state.companion.agents = payload.agents;
        Ok(())
    }
}

impl CapabilityDimensionModule for VfsCapabilityDimensionModule {
    fn key(&self) -> &'static str {
        CAPABILITY_DIMENSION_VFS
    }

    fn replay_effect(
        &self,
        state: &mut CapabilityState,
        context: &mut RuntimeCapabilityReplayContext,
        record: &RuntimeCapabilityEffectRecord,
    ) -> Result<(), String> {
        match record.effect_type.as_str() {
            EFFECT_TYPE_APPLY_VFS_OVERLAY => {
                let payload: ApplyVfsOverlayEffect = decode_effect_payload(record)?;
                state.vfs.active = Some(match state.vfs.active.take() {
                    Some(base_vfs) => merge_vfs_overlay(base_vfs, &payload.overlay),
                    None => payload.overlay,
                });
            }
            EFFECT_TYPE_APPLY_MOUNT_OPERATIONS => {
                let payload: ApplyMountOperationsEffect = decode_effect_payload(record)?;
                let mut vfs = state.vfs.active.take().unwrap_or_default();
                apply_mount_directives(&mut vfs, &payload.operations);
                state.vfs.active = Some(vfs);
            }
            other => {
                return Err(format!(
                    "dimension `{}` 不支持 effect type `{other}`",
                    record.dimension.as_str()
                ));
            }
        }
        context.effective_vfs = state.vfs.active.clone();
        Ok(())
    }
}

fn ensure_effect_type(
    record: &RuntimeCapabilityEffectRecord,
    expected: &'static str,
) -> Result<(), String> {
    if record.effect_type == expected {
        return Ok(());
    }
    Err(format!(
        "dimension `{}` 不支持 effect type `{}`，期望 `{expected}`",
        record.dimension.as_str(),
        record.effect_type
    ))
}

fn decode_effect_payload<T: DeserializeOwned>(
    record: &RuntimeCapabilityEffectRecord,
) -> Result<T, String> {
    serde_json::from_value(record.payload.clone()).map_err(|error| {
        format!(
            "dimension `{}` effect `{}` payload decode failed: {error}",
            record.dimension.as_str(),
            record.effect_type
        )
    })
}

pub fn replay_runtime_capability_transition(
    base_state: &CapabilityState,
    transition: &RuntimeCapabilityTransition,
) -> Result<RuntimeCapabilityReplay, String> {
    let mut state = base_state.clone();
    let mut context = RuntimeCapabilityReplayContext::default();
    CapabilityDimensionRegistry::built_in().replay_transition(
        &mut state,
        &mut context,
        transition,
    )?;
    let effective_vfs = state.vfs.active.clone();
    Ok(RuntimeCapabilityReplay {
        capability_state: state,
        effective_vfs,
        effective_mcp_servers: context.effective_mcp_servers,
    })
}

pub fn replay_runtime_capability_transitions(
    base_state: &CapabilityState,
    transitions: &[PendingCapabilityStateTransition],
) -> Result<RuntimeCapabilityReplay, String> {
    let mut state = base_state.clone();
    let mut context = RuntimeCapabilityReplayContext::default();
    let registry = CapabilityDimensionRegistry::built_in();
    for transition in transitions {
        registry.replay_transition(&mut state, &mut context, &transition.transition)?;
    }
    let effective_vfs = state.vfs.active.clone();
    Ok(RuntimeCapabilityReplay {
        capability_state: state,
        effective_vfs,
        effective_mcp_servers: context.effective_mcp_servers,
    })
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
            vfs: agentdash_spi::VfsDimension {
                active: Some(Vfs {
                    mounts: vec![mount("workspace", "relay_fs")],
                    default_mount_id: Some("workspace".to_string()),
                    source_project_id: None,
                    source_story_id: None,
                    links: Vec::new(),
                }),
            },
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

    #[test]
    fn runtime_capability_transition_replays_vfs_overlay_without_persisting_full_state() {
        let mut base = CapabilityState {
            vfs: agentdash_spi::VfsDimension {
                active: Some(Vfs {
                    mounts: vec![mount("workspace", "relay_fs")],
                    default_mount_id: Some("workspace".to_string()),
                    source_project_id: None,
                    source_story_id: None,
                    links: Vec::new(),
                }),
            },
            ..Default::default()
        };
        base.tool
            .enabled_clusters
            .insert(agentdash_spi::ToolCluster::Read);

        let mut target = CapabilityState::from_clusters([agentdash_spi::ToolCluster::Write]);
        target.vfs.active = Some(Vfs {
            mounts: vec![mount("review", "inline_fs")],
            default_mount_id: None,
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        });
        let transition = RuntimeCapabilityTransition::from_runtime_projection_parts(
            &target,
            target.vfs.active.clone(),
            vec![MountDirective::SetDefaultMount {
                mount_id: Some("review".to_string()),
            }],
            Vec::new(),
        );
        let transition = transition.expect("transition builds");

        let replayed = apply_runtime_capability_transition(&base, &transition).expect("replay");
        let replayed_vfs = replayed.vfs.active.as_ref().expect("active vfs");
        let mount_ids = replayed_vfs
            .mounts
            .iter()
            .map(|mount| mount.id.as_str())
            .collect::<BTreeSet<_>>();
        assert_eq!(
            mount_ids,
            BTreeSet::from(["review", "workspace"]),
            "patch replay 应把 pending VFS 作为 overlay 合并到 construction base VFS"
        );
        assert_eq!(replayed_vfs.default_mount_id.as_deref(), Some("review"));
        assert!(
            replayed
                .tool
                .enabled_clusters
                .contains(&agentdash_spi::ToolCluster::Write)
        );
        assert!(
            !replayed
                .tool
                .enabled_clusters
                .contains(&agentdash_spi::ToolCluster::Read)
        );
        assert_eq!(
            apply_runtime_capability_transition(&base, &transition).expect("replay"),
            replayed
        );

        let serialized = serde_json::to_value(PendingCapabilityStateTransition {
            id: "pending-transition".to_string(),
            run_id: Uuid::new_v4(),
            lifecycle_key: "dev".to_string(),
            phase_node: "review".to_string(),
            capability_keys: BTreeSet::new(),
            transition,
            created_at: 1,
            source_turn_id: None,
        })
        .expect("transition serializes");
        assert!(serialized.get("transition").is_some());
        assert!(serialized.get("state").is_none());
        assert!(serialized["transition"].get("tool").is_none());
        assert!(serialized["transition"].get("companion").is_none());
        assert!(serialized["transition"].get("declarations").is_some());
        assert!(serialized["transition"].get("effects").is_some());
    }

    #[test]
    fn runtime_capability_transition_fold_replays_multiple_vfs_effects_in_order() {
        let base = CapabilityState {
            vfs: agentdash_spi::VfsDimension {
                active: Some(Vfs {
                    mounts: vec![mount("workspace", "relay_fs")],
                    default_mount_id: Some("workspace".to_string()),
                    source_project_id: None,
                    source_story_id: None,
                    links: Vec::new(),
                }),
            },
            ..Default::default()
        };
        let overlay = Vfs {
            mounts: vec![mount("review", "inline_fs")],
            default_mount_id: None,
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        };
        let add_review = RuntimeCapabilityTransition::from_runtime_projection_parts(
            &CapabilityState::default(),
            Some(overlay),
            Vec::new(),
            Vec::new(),
        )
        .expect("transition builds");
        let set_default = RuntimeCapabilityTransition::from_runtime_projection_parts(
            &CapabilityState::default(),
            None,
            vec![MountDirective::SetDefaultMount {
                mount_id: Some("review".to_string()),
            }],
            Vec::new(),
        )
        .expect("transition builds");
        let transitions = vec![
            PendingCapabilityStateTransition {
                id: "pending-a".to_string(),
                run_id: Uuid::new_v4(),
                lifecycle_key: "dev".to_string(),
                phase_node: "review-a".to_string(),
                capability_keys: BTreeSet::new(),
                transition: add_review,
                created_at: 1,
                source_turn_id: None,
            },
            PendingCapabilityStateTransition {
                id: "pending-b".to_string(),
                run_id: Uuid::new_v4(),
                lifecycle_key: "dev".to_string(),
                phase_node: "review-b".to_string(),
                capability_keys: BTreeSet::new(),
                transition: set_default,
                created_at: 2,
                source_turn_id: None,
            },
        ];

        let replay =
            replay_runtime_capability_transitions(&base, &transitions).expect("fold replay");
        let vfs = replay
            .capability_state
            .vfs
            .active
            .expect("active vfs after replay");
        let mount_ids = vfs
            .mounts
            .iter()
            .map(|mount| mount.id.as_str())
            .collect::<BTreeSet<_>>();
        assert_eq!(mount_ids, BTreeSet::from(["review", "workspace"]));
        assert_eq!(vfs.default_mount_id.as_deref(), Some("review"));
    }

    #[test]
    fn runtime_capability_transition_rejects_invalid_module_payload() {
        let transition = RuntimeCapabilityTransition {
            declarations: Vec::new(),
            effects: vec![RuntimeCapabilityEffectRecord {
                dimension: crate::session::CapabilityDimensionKey::new(CAPABILITY_DIMENSION_TOOL),
                effect_type: EFFECT_TYPE_SET_TOOL_ACCESS.to_string(),
                payload: serde_json::json!({
                    "capabilities": "not-a-set",
                    "enabledClusters": [],
                    "toolPolicy": {}
                }),
            }],
        };

        let error = replay_runtime_capability_transition(&CapabilityState::default(), &transition)
            .expect_err("invalid payload should fail at module boundary");
        assert!(error.contains("payload decode failed"));
    }
}
