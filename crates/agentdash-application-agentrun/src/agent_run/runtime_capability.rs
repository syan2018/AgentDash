use std::collections::BTreeSet;

use agentdash_domain::channel::{
    ChannelCapabilityRef, ChannelDirective, ChannelEgressPolicy, ChannelIngressPolicy,
    ChannelReadiness,
};
use agentdash_domain::common::{MountLink, Vfs};
use agentdash_domain::workflow::{AgentFrame, MountDirective, ToolCapabilityDirective};
use agentdash_spi::{CapabilityState, RuntimeMcpServer};
use serde::de::DeserializeOwned;
use serde_json::Value;
use uuid::Uuid;

pub use agentdash_spi::{
    CapabilityStateDelta, DefaultMountDelta, NamedEntityDelta, SetDelta, VfsSurfaceDelta,
    compute_capability_state_delta,
};

pub use agentdash_spi::AccumulationPolicy;

use agentdash_spi::session_persistence::{
    ApplyChannelDirectivesEffect, ApplyMountOperationsEffect, ApplyVfsOverlayEffect,
    CAPABILITY_DIMENSION_CHANNEL, CAPABILITY_DIMENSION_COMPANION, CAPABILITY_DIMENSION_MCP,
    CAPABILITY_DIMENSION_TOOL, CAPABILITY_DIMENSION_VFS, CapabilityArtifactSource,
    CapabilityContributionRecord, CapabilityDeclarationRecord,
    DECLARATION_TYPE_CAPABILITY_DIRECTIVE, DECLARATION_TYPE_MOUNT_OPERATION,
    EFFECT_TYPE_APPLY_CHANNEL_DIRECTIVES, EFFECT_TYPE_APPLY_MOUNT_OPERATIONS,
    EFFECT_TYPE_APPLY_VFS_OVERLAY, EFFECT_TYPE_SET_COMPANION_AGENT_ROSTER,
    EFFECT_TYPE_SET_MCP_SERVER_SET, EFFECT_TYPE_SET_TOOL_ACCESS, PendingCapabilityStateTransition,
    RuntimeCapabilityEffectRecord, RuntimeCapabilityTransition, SetCompanionAgentRosterEffect,
    SetMcpServerSetEffect, SetToolAccessEffect,
};

// ── AgentFrame ↔ CapabilityState 投影 ─────────────────────────────

/// AgentFrame revision 拆解后的 capability surface JSON 三元组。
///
/// 对应 `AgentFrame` 的三个 JSON 列；`AgentFrameBuilder` 在写入时
/// 使用此结构统一填充，避免调用方手动拆分维度。
#[derive(Debug, Clone)]
pub struct FrameCapabilitySurfaces {
    pub effective_capability_json: Option<serde_json::Value>,
    pub vfs_surface_json: Option<serde_json::Value>,
    pub mcp_surface_json: Option<serde_json::Value>,
}

/// 从 `AgentFrame` revision 投影出只读 `CapabilityState`。
///
/// `effective_capability_json` 是 canonical surface；split VFS/MCP 列只在
/// canonical 维度缺失时作为同一 revision 的投影补全，不能覆盖 canonical state。
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

/// 将 `CapabilityState` 拆分为 frame 的三个 JSON surface 列。
///
/// 这是 `project_capability_state_from_frame` 的逆操作：
/// - `effective_capability_json`: 完整 `CapabilityState` 序列化
/// - `vfs_surface_json`: `state.vfs.active` 单独提取
/// - `mcp_surface_json`: 从 capability/draft 投影中提取本次 MCP executable surface
pub fn capability_state_to_frame_surfaces(state: &CapabilityState) -> FrameCapabilitySurfaces {
    FrameCapabilitySurfaces {
        effective_capability_json: serde_json::to_value(state).ok(),
        vfs_surface_json: state
            .vfs
            .active
            .as_ref()
            .and_then(|vfs| serde_json::to_value(vfs).ok()),
        mcp_surface_json: if state.tool.mcp_servers.is_empty() {
            None
        } else {
            serde_json::to_value(&state.tool.mcp_servers).ok()
        },
    }
}

/// 将 ProjectAgent preset 声明的 workspace module 可见性白名单投影进 base
/// `CapabilityState.workspace_module` 维度（三态直达）。
///
/// 语义（workspace_module 属 `Replace` 策略）：
/// - `None`（Unspecified，未声明）   → 显式投影 `mode = All`
/// - `Some([])`（Cleared，显式清空） → 显式投影 `mode = All`
/// - `Some([..非空])`（Allowlist）   → `mode = Allowlist` + allowed_module_ids
///
/// base 每 revision 由当前 config 重新投影，不存在"继承上一版白名单"，
/// 因此清空（空集）自然回到 All，而非把上一版名单捞回。
pub fn project_workspace_module_dimension(
    refs: Option<&[String]>,
) -> agentdash_spi::WorkspaceModuleDimension {
    match refs {
        Some(ids) if !ids.is_empty() => agentdash_spi::WorkspaceModuleDimension {
            mode: agentdash_spi::WorkspaceModuleVisibilityMode::Allowlist,
            allowed_module_ids: ids.to_vec(),
        },
        _ => agentdash_spi::WorkspaceModuleDimension::all(),
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
    pub steering_capability_delta: Option<&'a SetDelta>,
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
            .map(|skill| skill.capability_key_or_name().to_string())
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

/// 纯函数：将单次 transition diff 应用到 base state，返回新 state。
///
/// 调用方应将返回值写入 AgentFrame revision（通过 `AgentFrameBuilder::with_capability_state`），
/// 而非直接存入 session 内存。内存中的 `CapabilityState` 仅作为 frame 投影缓存。
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
    pub effective_mcp_servers: Option<Vec<RuntimeMcpServer>>,
}

#[derive(Debug, Default)]
pub struct RuntimeCapabilityReplayContext {
    pub effective_vfs: Option<Vfs>,
    pub effective_mcp_servers: Option<Vec<RuntimeMcpServer>>,
}

#[derive(Debug, Default)]
pub struct RuntimeCapabilityProjectionContext;

/// Capability 维度模块——将结构化 diff（`RuntimeCapabilityEffectRecord`）
/// 应用到 `CapabilityState` 投影上。
///
/// 所有维度共享同一个"只读投影 + diff 应用"语义：
/// - diff 由 workflow/hook 产出为 `RuntimeCapabilityEffectRecord`
/// - `replay_effect` 将 diff 应用到 state 副本（调用方预先 clone）
/// - 应用后的新 state 由调用方通过 `AgentFrameBuilder::with_capability_state`
///   写入 AgentFrame revision，CapabilityState 始终是 frame 的投影缓存
pub trait CapabilityDimensionModule {
    fn key(&self) -> &'static str;

    /// 本维度的累积策略——声明跨 revision 更新如何合并前后声明。
    ///
    /// 无默认实现，强制每个维度显式声明自己属于 `Replace` / `Accumulate` / `Ephemeral`。
    fn policy(&self) -> AccumulationPolicy;

    fn validate_declaration(&self, record: &CapabilityDeclarationRecord) -> Result<(), String>;

    fn compile_declaration(
        &self,
        record: &CapabilityDeclarationRecord,
    ) -> Result<Option<CapabilityContributionRecord>, String> {
        self.validate_declaration(record)?;
        Ok(None)
    }

    fn validate_effect(&self, record: &RuntimeCapabilityEffectRecord) -> Result<(), String>;

    /// 将单条 effect diff 应用到可变 state 上。
    ///
    /// **调用者约束**：调用方应在 clone 的 state 副本上调用此方法，
    /// 将结果写入 AgentFrame revision（通过 `AgentFrameBuilder`），
    /// 不要将可变引用暴露给 session 的长期状态。
    fn replay_effect(
        &self,
        state: &mut CapabilityState,
        context: &mut RuntimeCapabilityReplayContext,
        record: &RuntimeCapabilityEffectRecord,
    ) -> Result<(), String>;

    fn normalize_projection(
        &self,
        _state: &mut CapabilityState,
        _context: &RuntimeCapabilityProjectionContext,
    ) -> Result<(), String> {
        Ok(())
    }
}

pub struct CapabilityDimensionRegistry {
    modules: Vec<Box<dyn CapabilityDimensionModule>>,
}

impl CapabilityDimensionRegistry {
    pub fn new() -> Self {
        Self {
            modules: Vec::new(),
        }
    }

    pub fn built_in() -> Self {
        let mut registry = Self::new();
        registry
            .register_module(VfsCapabilityDimensionModule)
            .expect("built-in vfs dimension should register");
        registry
            .register_module(ToolCapabilityDimensionModule)
            .expect("built-in tool dimension should register");
        registry
            .register_module(McpCapabilityDimensionModule)
            .expect("built-in mcp dimension should register");
        registry
            .register_module(CompanionCapabilityDimensionModule)
            .expect("built-in companion dimension should register");
        registry
            .register_module(ChannelCapabilityDimensionModule)
            .expect("built-in channel dimension should register");
        registry
    }

    pub fn register_module<M>(&mut self, module: M) -> Result<(), String>
    where
        M: CapabilityDimensionModule + 'static,
    {
        if self.module_for(module.key()).is_some() {
            return Err(format!("capability dimension `{}` 已注册", module.key()));
        }
        self.modules.push(Box::new(module));
        Ok(())
    }

    fn module_for(&self, key: &str) -> Option<&dyn CapabilityDimensionModule> {
        self.modules
            .iter()
            .find(|module| module.key() == key)
            .map(|module| module.as_ref())
    }

    pub fn validate_transition(
        &self,
        transition: &RuntimeCapabilityTransition,
    ) -> Result<(), String> {
        for record in &transition.declarations {
            let module = self.module_for(record.dimension.as_str()).ok_or_else(|| {
                format!(
                    "未注册 capability dimension `{}`，无法验证 `{}` declaration",
                    record.dimension.as_str(),
                    record.declaration_type
                )
            })?;
            module.validate_declaration(record)?;
        }
        for record in &transition.effects {
            let module = self.module_for(record.dimension.as_str()).ok_or_else(|| {
                format!(
                    "未注册 capability dimension `{}`，无法验证 `{}` effect",
                    record.dimension.as_str(),
                    record.effect_type
                )
            })?;
            module.validate_effect(record)?;
        }
        Ok(())
    }

    pub fn replay_transition(
        &self,
        state: &mut CapabilityState,
        context: &mut RuntimeCapabilityReplayContext,
        transition: &RuntimeCapabilityTransition,
    ) -> Result<(), String> {
        self.validate_transition(transition)?;
        for module in &self.modules {
            for record in transition
                .effects
                .iter()
                .filter(|record| record.dimension.as_str() == module.key())
            {
                module.replay_effect(state, context, record)?;
            }
        }
        Ok(())
    }
}

impl Default for CapabilityDimensionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub struct ToolCapabilityDimensionModule;
pub struct McpCapabilityDimensionModule;
pub struct CompanionCapabilityDimensionModule;
pub struct ChannelCapabilityDimensionModule;
pub struct VfsCapabilityDimensionModule;

impl ToolCapabilityDimensionModule {
    pub fn capability_directive_declarations(
        source: CapabilityArtifactSource,
        directives: impl IntoIterator<Item = ToolCapabilityDirective>,
    ) -> Result<Vec<CapabilityDeclarationRecord>, String> {
        directives
            .into_iter()
            .map(|directive| {
                CapabilityDeclarationRecord::typed(
                    CAPABILITY_DIMENSION_TOOL,
                    DECLARATION_TYPE_CAPABILITY_DIRECTIVE,
                    source.clone(),
                    &directive,
                )
            })
            .collect()
    }

    pub fn set_tool_access_effect(
        payload: SetToolAccessEffect,
    ) -> Result<RuntimeCapabilityEffectRecord, String> {
        RuntimeCapabilityEffectRecord::typed(
            CAPABILITY_DIMENSION_TOOL,
            EFFECT_TYPE_SET_TOOL_ACCESS,
            &payload,
        )
    }
}

impl McpCapabilityDimensionModule {
    pub fn set_server_set_effect(
        servers: Vec<RuntimeMcpServer>,
    ) -> Result<RuntimeCapabilityEffectRecord, String> {
        RuntimeCapabilityEffectRecord::typed(
            CAPABILITY_DIMENSION_MCP,
            EFFECT_TYPE_SET_MCP_SERVER_SET,
            &SetMcpServerSetEffect { servers },
        )
    }
}

impl CompanionCapabilityDimensionModule {
    pub fn set_agent_roster_effect(
        agents: Vec<agentdash_spi::context::capability::CompanionAgentEntry>,
    ) -> Result<RuntimeCapabilityEffectRecord, String> {
        RuntimeCapabilityEffectRecord::typed(
            CAPABILITY_DIMENSION_COMPANION,
            EFFECT_TYPE_SET_COMPANION_AGENT_ROSTER,
            &SetCompanionAgentRosterEffect { agents },
        )
    }
}

impl ChannelCapabilityDimensionModule {
    pub fn apply_channel_directives_effect(
        directives: Vec<ChannelDirective>,
    ) -> Result<RuntimeCapabilityEffectRecord, String> {
        RuntimeCapabilityEffectRecord::typed(
            CAPABILITY_DIMENSION_CHANNEL,
            EFFECT_TYPE_APPLY_CHANNEL_DIRECTIVES,
            &ApplyChannelDirectivesEffect { directives },
        )
    }
}

impl VfsCapabilityDimensionModule {
    pub fn mount_operation_declarations(
        source: CapabilityArtifactSource,
        directives: impl IntoIterator<Item = MountDirective>,
    ) -> Result<Vec<CapabilityDeclarationRecord>, String> {
        directives
            .into_iter()
            .map(|directive| {
                CapabilityDeclarationRecord::typed(
                    CAPABILITY_DIMENSION_VFS,
                    DECLARATION_TYPE_MOUNT_OPERATION,
                    source.clone(),
                    &directive,
                )
            })
            .collect()
    }

    pub fn apply_vfs_overlay_effect(overlay: Vfs) -> Result<RuntimeCapabilityEffectRecord, String> {
        RuntimeCapabilityEffectRecord::typed(
            CAPABILITY_DIMENSION_VFS,
            EFFECT_TYPE_APPLY_VFS_OVERLAY,
            &ApplyVfsOverlayEffect { overlay },
        )
    }

    pub fn apply_mount_operations_effect(
        operations: Vec<MountDirective>,
    ) -> Result<RuntimeCapabilityEffectRecord, String> {
        RuntimeCapabilityEffectRecord::typed(
            CAPABILITY_DIMENSION_VFS,
            EFFECT_TYPE_APPLY_MOUNT_OPERATIONS,
            &ApplyMountOperationsEffect { operations },
        )
    }
}

impl CapabilityDimensionModule for ToolCapabilityDimensionModule {
    fn key(&self) -> &'static str {
        CAPABILITY_DIMENSION_TOOL
    }

    fn policy(&self) -> AccumulationPolicy {
        AccumulationPolicy::Replace
    }

    fn validate_declaration(&self, record: &CapabilityDeclarationRecord) -> Result<(), String> {
        ensure_declaration_type(record, DECLARATION_TYPE_CAPABILITY_DIRECTIVE)?;
        let _: ToolCapabilityDirective = decode_declaration_payload(record)?;
        Ok(())
    }

    fn validate_effect(&self, record: &RuntimeCapabilityEffectRecord) -> Result<(), String> {
        ensure_effect_type(record, EFFECT_TYPE_SET_TOOL_ACCESS)?;
        let _: SetToolAccessEffect = decode_effect_payload(record)?;
        Ok(())
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

    fn policy(&self) -> AccumulationPolicy {
        AccumulationPolicy::Replace
    }

    fn validate_declaration(&self, record: &CapabilityDeclarationRecord) -> Result<(), String> {
        Err(format!(
            "dimension `{}` 当前不支持 declaration type `{}`",
            record.dimension.as_str(),
            record.declaration_type
        ))
    }

    fn validate_effect(&self, record: &RuntimeCapabilityEffectRecord) -> Result<(), String> {
        ensure_effect_type(record, EFFECT_TYPE_SET_MCP_SERVER_SET)?;
        let _: SetMcpServerSetEffect = decode_effect_payload(record)?;
        Ok(())
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

    fn policy(&self) -> AccumulationPolicy {
        AccumulationPolicy::Replace
    }

    fn validate_declaration(&self, record: &CapabilityDeclarationRecord) -> Result<(), String> {
        Err(format!(
            "dimension `{}` 当前不支持 declaration type `{}`",
            record.dimension.as_str(),
            record.declaration_type
        ))
    }

    fn validate_effect(&self, record: &RuntimeCapabilityEffectRecord) -> Result<(), String> {
        ensure_effect_type(record, EFFECT_TYPE_SET_COMPANION_AGENT_ROSTER)?;
        let _: SetCompanionAgentRosterEffect = decode_effect_payload(record)?;
        Ok(())
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

impl CapabilityDimensionModule for ChannelCapabilityDimensionModule {
    fn key(&self) -> &'static str {
        CAPABILITY_DIMENSION_CHANNEL
    }

    fn policy(&self) -> AccumulationPolicy {
        AccumulationPolicy::Accumulate
    }

    fn validate_declaration(&self, record: &CapabilityDeclarationRecord) -> Result<(), String> {
        Err(format!(
            "dimension `{}` 当前不支持 declaration type `{}`",
            record.dimension.as_str(),
            record.declaration_type
        ))
    }

    fn validate_effect(&self, record: &RuntimeCapabilityEffectRecord) -> Result<(), String> {
        ensure_effect_type(record, EFFECT_TYPE_APPLY_CHANNEL_DIRECTIVES)?;
        let payload: ApplyChannelDirectivesEffect = decode_effect_payload(record)?;
        for directive in payload.directives {
            if let ChannelDirective::Expose { operations, .. } = directive
                && operations.is_empty()
            {
                return Err("channel expose directive operations must not be empty".to_string());
            }
        }
        Ok(())
    }

    fn replay_effect(
        &self,
        state: &mut CapabilityState,
        _context: &mut RuntimeCapabilityReplayContext,
        record: &RuntimeCapabilityEffectRecord,
    ) -> Result<(), String> {
        ensure_effect_type(record, EFFECT_TYPE_APPLY_CHANNEL_DIRECTIVES)?;
        let payload: ApplyChannelDirectivesEffect = decode_effect_payload(record)?;
        for directive in payload.directives {
            match directive {
                ChannelDirective::Expose {
                    channel_ref,
                    aliases,
                    operations,
                } => {
                    state
                        .channel
                        .visible_channels
                        .retain(|existing| existing.channel_ref != channel_ref);
                    state.channel.visible_channels.push(ChannelCapabilityRef {
                        channel_ref,
                        aliases,
                        operations,
                        ingress_policy: ChannelIngressPolicy::ParticipantsOnly,
                        egress_policy: ChannelEgressPolicy::ParticipantsOnly,
                        readiness: ChannelReadiness::Ready,
                    });
                }
                ChannelDirective::Revoke { channel_ref } => {
                    state
                        .channel
                        .visible_channels
                        .retain(|existing| existing.channel_ref != channel_ref);
                }
            }
        }
        normalize_channel_projection(&mut state.channel.visible_channels);
        Ok(())
    }

    fn normalize_projection(
        &self,
        state: &mut CapabilityState,
        _context: &RuntimeCapabilityProjectionContext,
    ) -> Result<(), String> {
        normalize_channel_projection(&mut state.channel.visible_channels);
        Ok(())
    }
}

impl CapabilityDimensionModule for VfsCapabilityDimensionModule {
    fn key(&self) -> &'static str {
        CAPABILITY_DIMENSION_VFS
    }

    fn policy(&self) -> AccumulationPolicy {
        AccumulationPolicy::Accumulate
    }

    fn validate_declaration(&self, record: &CapabilityDeclarationRecord) -> Result<(), String> {
        ensure_declaration_type(record, DECLARATION_TYPE_MOUNT_OPERATION)?;
        let _: MountDirective = decode_declaration_payload(record)?;
        Ok(())
    }

    fn validate_effect(&self, record: &RuntimeCapabilityEffectRecord) -> Result<(), String> {
        match record.effect_type.as_str() {
            EFFECT_TYPE_APPLY_VFS_OVERLAY => {
                let _: ApplyVfsOverlayEffect = decode_effect_payload(record)?;
                Ok(())
            }
            EFFECT_TYPE_APPLY_MOUNT_OPERATIONS => {
                let _: ApplyMountOperationsEffect = decode_effect_payload(record)?;
                Ok(())
            }
            other => Err(format!(
                "dimension `{}` 不支持 effect type `{other}`",
                record.dimension.as_str()
            )),
        }
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

fn ensure_declaration_type(
    record: &CapabilityDeclarationRecord,
    expected: &'static str,
) -> Result<(), String> {
    if record.declaration_type == expected {
        return Ok(());
    }
    Err(format!(
        "dimension `{}` 不支持 declaration type `{}`，期望 `{expected}`",
        record.dimension.as_str(),
        record.declaration_type
    ))
}

fn decode_declaration_payload<T: DeserializeOwned>(
    record: &CapabilityDeclarationRecord,
) -> Result<T, String> {
    serde_json::from_value(record.payload.clone()).map_err(|error| {
        format!(
            "dimension `{}` declaration `{}` payload decode failed: {error}",
            record.dimension.as_str(),
            record.declaration_type
        )
    })
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

fn normalize_channel_projection(channels: &mut [ChannelCapabilityRef]) {
    channels.sort_by(|left, right| {
        (
            left.channel_ref.owner.stable_key(),
            left.channel_ref.channel_id,
        )
            .cmp(&(
                right.channel_ref.owner.stable_key(),
                right.channel_ref.channel_id,
            ))
    });
}

/// 纯函数：将单次 transition diff 应用到 base state 副本，返回完整 replay 结果。
///
/// 不修改 base_state；内部 clone 后应用 effects。
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

/// 纯函数：将多条 pending transitions 依次应用到 base state 副本。
///
/// 调用方将最终 `capability_state` 写入 AgentFrame revision 作为权威存储。
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

fn link_key(link: &MountLink) -> String {
    format!("{}:{}", link.from_mount_id, link.from_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::channel::{ChannelOperation, ChannelOwner, ChannelRef};
    use agentdash_domain::common::{Mount, MountCapability};
    use agentdash_spi::{CapabilityDimensionKey, CapabilityState, McpTransportConfig};

    // ── AgentFrame 投影 round-trip 测试 ──────────────────────────────

    #[test]
    fn project_from_empty_frame_returns_default_state() {
        let frame = AgentFrame::new_initial(Uuid::new_v4());
        let state = project_capability_state_from_frame(&frame);
        assert_eq!(state, CapabilityState::default());
    }

    #[test]
    fn capability_state_deserializes_missing_channel_as_empty_dimension() {
        let mut value = serde_json::to_value(CapabilityState::default()).expect("default json");
        value.as_object_mut().expect("object").remove("channel");
        let state: CapabilityState = serde_json::from_value(value).expect("capability state");

        assert!(state.channel.visible_channels.is_empty());
    }

    #[test]
    fn project_round_trip_preserves_capability_state() {
        let mut state = CapabilityState::from_clusters([agentdash_spi::ToolCluster::Read]);
        state.vfs.active = Some(Vfs {
            mounts: vec![mount("workspace", "relay_fs")],
            default_mount_id: Some("workspace".to_string()),
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        });
        state.tool.mcp_servers = vec![agentdash_spi::RuntimeMcpServer {
            name: "test-server".to_string(),
            transport: agentdash_spi::McpTransportConfig::Http {
                url: "http://localhost:3000".to_string(),
                headers: vec![],
            },
            uses_relay: false,
            readiness: Default::default(),
        }];
        state.companion.agents = vec![agentdash_spi::context::capability::CompanionAgentEntry {
            name: "helper".to_string(),
            executor: "codex".to_string(),
            display_name: "Helper".to_string(),
        }];

        let surfaces = capability_state_to_frame_surfaces(&state);
        let mut frame = AgentFrame::new_initial(Uuid::new_v4());
        frame.effective_capability_json = surfaces.effective_capability_json;
        frame.vfs_surface_json = surfaces.vfs_surface_json;
        frame.mcp_surface_json = surfaces.mcp_surface_json;

        let projected = project_capability_state_from_frame(&frame);
        assert_eq!(projected.tool.enabled_clusters, state.tool.enabled_clusters);
        assert_eq!(projected.vfs.active, state.vfs.active);
        assert_eq!(projected.tool.mcp_servers.len(), 1);
        assert_eq!(projected.tool.mcp_servers[0].name, "test-server");
        assert_eq!(projected.companion.agents.len(), 1);
        assert_eq!(projected.companion.agents[0].name, "helper");
    }

    #[test]
    fn effective_capability_json_remains_canonical_when_split_vfs_differs() {
        let canonical_vfs = Vfs {
            mounts: vec![mount("canonical", "relay_fs")],
            default_mount_id: Some("canonical".to_string()),
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        };
        let mut base = CapabilityState::default();
        base.vfs.active = Some(canonical_vfs);
        let surfaces = capability_state_to_frame_surfaces(&base);

        let stale_split_vfs = Vfs {
            mounts: vec![mount("stale-split", "inline_fs")],
            default_mount_id: Some("stale-split".to_string()),
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        };

        let mut frame = AgentFrame::new_initial(Uuid::new_v4());
        frame.effective_capability_json = surfaces.effective_capability_json;
        frame.vfs_surface_json = serde_json::to_value(&stale_split_vfs).ok();

        let projected = project_capability_state_from_frame(&frame);
        assert_eq!(
            projected
                .vfs
                .active
                .as_ref()
                .and_then(|v| v.default_mount_id.as_deref()),
            Some("canonical"),
            "split VFS projection must not overwrite canonical capability state"
        );
    }

    #[test]
    fn effective_capability_json_remains_canonical_when_split_mcp_differs() {
        let mut base = CapabilityState::default();
        base.tool.mcp_servers = vec![RuntimeMcpServer {
            name: "canonical-server".to_string(),
            transport: McpTransportConfig::Http {
                url: "http://localhost/canonical".to_string(),
                headers: Vec::new(),
            },
            uses_relay: false,
            readiness: Default::default(),
        }];
        let surfaces = capability_state_to_frame_surfaces(&base);
        let stale_split_servers = vec![RuntimeMcpServer {
            name: "stale-split-server".to_string(),
            transport: McpTransportConfig::Http {
                url: "http://localhost/stale".to_string(),
                headers: Vec::new(),
            },
            uses_relay: false,
            readiness: Default::default(),
        }];

        let mut frame = AgentFrame::new_initial(Uuid::new_v4());
        frame.effective_capability_json = surfaces.effective_capability_json;
        frame.mcp_surface_json = serde_json::to_value(&stale_split_servers).ok();

        let projected = project_capability_state_from_frame(&frame);
        assert_eq!(projected.tool.mcp_servers.len(), 1);
        assert_eq!(
            projected.tool.mcp_servers[0].name, "canonical-server",
            "split MCP projection must not overwrite canonical capability state"
        );
    }

    #[test]
    fn channel_directives_expose_and_revoke_visible_channel_refs() {
        let channel_ref = ChannelRef {
            owner: ChannelOwner::LifecycleRun {
                run_id: Uuid::new_v4(),
            },
            channel_id: Uuid::new_v4(),
        };
        let operations = [ChannelOperation::Read, ChannelOperation::Reply]
            .into_iter()
            .collect();
        let expose = ChannelCapabilityDimensionModule::apply_channel_directives_effect(vec![
            ChannelDirective::Expose {
                channel_ref: channel_ref.clone(),
                aliases: vec!["review".to_string()],
                operations,
            },
        ])
        .expect("channel effect");
        let transition = RuntimeCapabilityTransition::from_records(vec![], vec![expose]);

        let replay = replay_runtime_capability_transition(&CapabilityState::default(), &transition)
            .expect("replay expose");
        assert_eq!(replay.capability_state.channel.visible_channels.len(), 1);
        assert_eq!(
            replay.capability_state.channel.visible_channels[0].aliases,
            vec!["review"]
        );

        let revoke = ChannelCapabilityDimensionModule::apply_channel_directives_effect(vec![
            ChannelDirective::Revoke { channel_ref },
        ])
        .expect("channel revoke effect");
        let transition = RuntimeCapabilityTransition::from_records(vec![], vec![revoke]);
        let replay = replay_runtime_capability_transition(&replay.capability_state, &transition)
            .expect("replay revoke");

        assert!(replay.capability_state.channel.visible_channels.is_empty());
    }

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

    fn transition_from_parts(
        state: &CapabilityState,
        vfs_overlay: Option<Vfs>,
        mount_directives: Vec<MountDirective>,
        tool_directives: Vec<ToolCapabilityDirective>,
    ) -> RuntimeCapabilityTransition {
        let mut declarations = ToolCapabilityDimensionModule::capability_directive_declarations(
            CapabilityArtifactSource::workflow(),
            tool_directives,
        )
        .expect("tool declarations build");
        declarations.extend(
            VfsCapabilityDimensionModule::mount_operation_declarations(
                CapabilityArtifactSource::workflow(),
                mount_directives.clone(),
            )
            .expect("vfs declarations build"),
        );

        let mut effects = vec![
            ToolCapabilityDimensionModule::set_tool_access_effect(SetToolAccessEffect {
                capabilities: state.tool.capabilities.clone(),
                enabled_clusters: state.tool.enabled_clusters.clone(),
                tool_policy: state.tool.tool_policy.clone(),
            })
            .expect("tool effect builds"),
            McpCapabilityDimensionModule::set_server_set_effect(state.tool.mcp_servers.clone())
                .expect("mcp effect builds"),
            CompanionCapabilityDimensionModule::set_agent_roster_effect(
                state.companion.agents.clone(),
            )
            .expect("companion effect builds"),
        ];
        if let Some(overlay) = vfs_overlay {
            effects.push(
                VfsCapabilityDimensionModule::apply_vfs_overlay_effect(overlay)
                    .expect("vfs overlay effect builds"),
            );
        }
        if !mount_directives.is_empty() {
            effects.push(
                VfsCapabilityDimensionModule::apply_mount_operations_effect(mount_directives)
                    .expect("mount operation effect builds"),
            );
        }

        let transition = RuntimeCapabilityTransition::from_records(declarations, effects);
        CapabilityDimensionRegistry::built_in()
            .validate_transition(&transition)
            .expect("transition validates");
        transition
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
        let transition = transition_from_parts(
            &target,
            target.vfs.active.clone(),
            vec![MountDirective::SetDefaultMount {
                mount_id: Some("review".to_string()),
            }],
            Vec::new(),
        );

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
        let add_review = transition_from_parts(
            &CapabilityState::default(),
            Some(overlay),
            Vec::new(),
            Vec::new(),
        );
        let set_default = transition_from_parts(
            &CapabilityState::default(),
            None,
            vec![MountDirective::SetDefaultMount {
                mount_id: Some("review".to_string()),
            }],
            Vec::new(),
        );
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
                dimension: CapabilityDimensionKey::new(CAPABILITY_DIMENSION_TOOL),
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

    #[test]
    fn dimension_modules_declare_expected_accumulation_policy() {
        assert_eq!(
            ToolCapabilityDimensionModule.policy(),
            AccumulationPolicy::Replace
        );
        assert_eq!(
            McpCapabilityDimensionModule.policy(),
            AccumulationPolicy::Replace
        );
        assert_eq!(
            CompanionCapabilityDimensionModule.policy(),
            AccumulationPolicy::Replace
        );
        assert_eq!(
            VfsCapabilityDimensionModule.policy(),
            AccumulationPolicy::Accumulate
        );
    }

    #[test]
    fn project_to_base_workspace_module_all_when_none() {
        let dim = project_workspace_module_dimension(None);
        assert_eq!(dim.mode, agentdash_spi::WorkspaceModuleVisibilityMode::All);
        assert!(dim.allowed_module_ids.is_empty());
    }

    #[test]
    fn project_to_base_workspace_module_all_when_empty() {
        // Cleared（显式空集）→ All —— carry-forward bug 回归锁。
        let dim = project_workspace_module_dimension(Some(&[]));
        assert_eq!(dim.mode, agentdash_spi::WorkspaceModuleVisibilityMode::All);
        assert!(dim.allowed_module_ids.is_empty());
    }

    #[test]
    fn project_to_base_workspace_module_allowlist_when_set() {
        let refs = vec!["ext:demo".to_string(), "canvas:cvs-dashboard-a".to_string()];
        let dim = project_workspace_module_dimension(Some(&refs));
        assert_eq!(
            dim.mode,
            agentdash_spi::WorkspaceModuleVisibilityMode::Allowlist
        );
        assert_eq!(dim.allowed_module_ids, refs);
    }

    #[test]
    fn workspace_module_round_trips_through_effective_capability_json() {
        // set allowlist → effective_capability_json → 还原 → Allowlist 保真
        let state = CapabilityState {
            workspace_module: project_workspace_module_dimension(Some(&["ext:demo".to_string()])),
            ..Default::default()
        };
        let surfaces = capability_state_to_frame_surfaces(&state);
        let mut frame = AgentFrame::new_initial(Uuid::new_v4());
        frame.effective_capability_json = surfaces.effective_capability_json;
        let projected = project_capability_state_from_frame(&frame);
        assert_eq!(
            projected.workspace_module.mode,
            agentdash_spi::WorkspaceModuleVisibilityMode::Allowlist
        );
        assert_eq!(
            projected.workspace_module.allowed_module_ids,
            vec!["ext:demo".to_string()]
        );

        // clear（空集）→ 下一 revision 重新投影 → All（不继承上一版名单）
        let cleared = CapabilityState {
            workspace_module: project_workspace_module_dimension(Some(&[])),
            ..Default::default()
        };
        let cleared_surfaces = capability_state_to_frame_surfaces(&cleared);
        let mut next_frame = AgentFrame::new_initial(Uuid::new_v4());
        next_frame.effective_capability_json = cleared_surfaces.effective_capability_json;
        let cleared_projected = project_capability_state_from_frame(&next_frame);
        assert_eq!(
            cleared_projected.workspace_module.mode,
            agentdash_spi::WorkspaceModuleVisibilityMode::All
        );
    }

    #[test]
    fn capability_state_json_requires_workspace_module_dimension() {
        let json = serde_json::json!({
            "tool": {},
            "companion": {},
            "vfs": {},
            "skill": {}
        });
        serde_json::from_value::<CapabilityState>(json).expect_err("workspace_module 必须显式存在");

        let empty_dimension = serde_json::json!({
            "tool": {},
            "companion": {},
            "vfs": {},
            "skill": {},
            "workspace_module": {}
        });
        serde_json::from_value::<CapabilityState>(empty_dimension)
            .expect_err("workspace_module.mode 必须显式存在");
    }

    #[test]
    fn workspace_module_dimension_default_is_empty_allowlist() {
        let dim = agentdash_spi::WorkspaceModuleDimension::default();
        assert_eq!(
            dim.mode,
            agentdash_spi::WorkspaceModuleVisibilityMode::Allowlist
        );
        assert!(dim.allowed_module_ids.is_empty());
        assert!(!dim.allows("ext:demo"));
    }

    #[test]
    fn runtime_capability_transition_validates_declarations_at_module_boundary() {
        let transition = transition_from_parts(
            &CapabilityState::default(),
            None,
            vec![MountDirective::SetDefaultMount {
                mount_id: Some("workspace".to_string()),
            }],
            vec![ToolCapabilityDirective::add_simple("file_read")],
        );

        assert!(
            transition.declarations.iter().any(|record| {
                record.dimension.as_str() == CAPABILITY_DIMENSION_TOOL
                    && record.declaration_type == DECLARATION_TYPE_CAPABILITY_DIRECTIVE
            }),
            "tool directives must stay visible as declaration records"
        );
        assert!(
            transition.declarations.iter().any(|record| {
                record.dimension.as_str() == CAPABILITY_DIMENSION_VFS
                    && record.declaration_type == DECLARATION_TYPE_MOUNT_OPERATION
            }),
            "mount operations must stay visible as VFS declaration records"
        );
        CapabilityDimensionRegistry::built_in()
            .validate_transition(&transition)
            .expect("declarations should validate");
    }
}
