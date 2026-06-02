//! RuntimeLaunchRequest 及 FrameLaunchEnvelope — Session 启动的类型体系。
//!
//! ## 类型层次
//!
//! ```text
//! FrameRuntimeSurface  ← 只来自 AgentFrame 持久化 surface
//! FrameLaunchIntent    ← 只来自 command/prompt intent
//! FrameLaunchEnvelope  ← Frame construction 输出，字段 non-optional
//!   ↓ .into_launch_request()
//! RuntimeLaunchRequest ← 向下兼容的 mutable bag（渐进 deprecate）
//! ```
//!
//! `FrameLaunchEnvelope` 是 session construction 到 planner 的唯一传递形式，
//! 让"缺字段"在构造边界暴露而不是到 planner 才兜底检查。

use std::collections::HashMap;
use std::path::PathBuf;

use agentdash_domain::workflow::{AgentFrame, AgentProcedureRef, RuntimeSessionSelectionPolicy};
use agentdash_spi::hooks::ContextFrame;
use agentdash_spi::{
    AgentConfig, AuthIdentity, CapabilityState, DiscoveredGuideline, SessionContextBundle,
    SessionMcpServer, Vfs,
};
use uuid::Uuid;

use crate::extension_runtime::ExtensionRuntimeProjection;
use crate::session::post_turn_handler::TerminalHookEffectBinding;

// ─── FrameRuntimeSurface: 只来自 AgentFrame 持久化 surface ───

/// 从 `AgentFrame` 投影的纯 surface 数据，不可被 command/extras 修改。
#[derive(Debug, Clone)]
pub struct FrameRuntimeSurface {
    pub agent_id: Uuid,
    pub frame_id: Uuid,
    pub frame_revision: i32,
    pub procedure_ref: Option<AgentProcedureRef>,
    pub capability_surface: serde_json::Value,
    pub context_slice: serde_json::Value,
    pub vfs_surface: serde_json::Value,
    pub mcp_surface: serde_json::Value,
    pub runtime_session_id: Option<String>,
    pub graph_instance_id: Option<Uuid>,
    pub activity_key: Option<String>,
}

impl FrameRuntimeSurface {
    pub fn from_frame(frame: &AgentFrame, runtime_policy: RuntimeSessionSelectionPolicy) -> Self {
        Self {
            agent_id: frame.agent_id,
            frame_id: frame.id,
            frame_revision: frame.revision,
            procedure_ref: frame.procedure_id.map(AgentProcedureRef::ById),
            capability_surface: frame
                .effective_capability_json
                .clone()
                .unwrap_or(serde_json::Value::Null),
            context_slice: frame
                .context_slice_json
                .clone()
                .unwrap_or(serde_json::Value::Null),
            vfs_surface: frame
                .vfs_surface_json
                .clone()
                .unwrap_or(serde_json::Value::Null),
            mcp_surface: frame
                .mcp_surface_json
                .clone()
                .unwrap_or(serde_json::Value::Null),
            runtime_session_id: frame.select_runtime_session_id(runtime_policy),
            graph_instance_id: frame.graph_instance_id,
            activity_key: frame.activity_key.clone(),
        }
    }
}

// ─── FrameLaunchIntent: 只来自 command/prompt intent ───

/// 来自 `LaunchCommand` / `AssemblyLaunchExtras` 的请求意图，
/// 不含任何 frame surface 数据。
#[derive(Debug, Clone, Default)]
pub struct FrameLaunchIntent {
    pub prompt_blocks: Option<Vec<serde_json::Value>>,
    pub environment_variables: HashMap<String, String>,
    pub identity: Option<AuthIdentity>,
    pub terminal_hook_effect_binding: Option<TerminalHookEffectBinding>,
    pub discovered_guidelines: Vec<DiscoveredGuideline>,
    pub extension_runtime: Option<ExtensionRuntimeProjection>,
}

// ─── FrameLaunchEnvelope: construction 输出，字段 non-optional ───

/// Frame construction 到 planner 的传递物。
/// `working_directory`、`executor_config`、`capability_state` 在此保证 non-optional,
/// planner 不需要做"半成品是否 ready"的兜底检查。
#[derive(Debug, Clone)]
pub struct FrameLaunchEnvelope {
    pub surface: FrameRuntimeSurface,
    pub intent: FrameLaunchIntent,
    pub working_directory: PathBuf,
    pub executor_config: AgentConfig,
    pub capability_state: CapabilityState,
    pub vfs: Vfs,
    pub mcp_servers: Vec<SessionMcpServer>,
    pub context_bundle: Option<SessionContextBundle>,
    pub continuation_context_frame: Option<ContextFrame>,
    pub base_capability_state: Option<CapabilityState>,
    pub resolution_trace: LaunchResolutionTrace,
}

impl FrameLaunchEnvelope {
    /// 从已完成 construction 的 `RuntimeLaunchRequest` 验证并提取 envelope。
    ///
    /// 三个 non-optional 字段（`working_directory`、`executor_config`、`capability_state`）
    /// 必须已被 construction provider 填充，否则返回 Err。
    pub fn try_from_launch_request(
        req: RuntimeLaunchRequest,
    ) -> Result<Self, String> {
        let working_directory = req.working_directory.ok_or(
            "FrameLaunchEnvelope: working_directory 未在 construction 阶段解析",
        )?;
        let executor_config = req.executor_config.ok_or(
            "FrameLaunchEnvelope: executor_config 未在 construction 阶段解析",
        )?;
        let capability_state = req.typed_capability_state.ok_or(
            "FrameLaunchEnvelope: typed_capability_state 未在 construction 阶段解析",
        )?;
        let vfs = req.typed_vfs.unwrap_or_default();
        Ok(Self {
            surface: FrameRuntimeSurface {
                agent_id: req.agent_id,
                frame_id: req.frame_id,
                frame_revision: req.frame_revision,
                procedure_ref: req.procedure_ref,
                capability_surface: req.capability_surface,
                context_slice: req.context_slice,
                vfs_surface: req.vfs_surface,
                mcp_surface: req.mcp_surface,
                runtime_session_id: req.runtime_session_id,
                graph_instance_id: req.graph_instance_id,
                activity_key: req.activity_key,
            },
            intent: FrameLaunchIntent {
                prompt_blocks: req.prompt_blocks,
                environment_variables: req.environment_variables,
                identity: req.identity,
                terminal_hook_effect_binding: req.terminal_hook_effect_binding,
                discovered_guidelines: req.discovered_guidelines,
                extension_runtime: req.extension_runtime,
            },
            working_directory,
            executor_config,
            capability_state,
            vfs,
            mcp_servers: req.typed_mcp_servers,
            context_bundle: req.context_bundle,
            continuation_context_frame: req.continuation_context_frame,
            base_capability_state: req.base_capability_state,
            resolution_trace: req.resolution_trace,
        })
    }

    /// 向下兼容：把 envelope 转换为 RuntimeLaunchRequest，
    /// 供尚未迁移到 envelope 消费的代码路径使用。
    pub fn into_launch_request(self) -> RuntimeLaunchRequest {
        RuntimeLaunchRequest {
            agent_id: self.surface.agent_id,
            frame_id: self.surface.frame_id,
            frame_revision: self.surface.frame_revision,
            procedure_ref: self.surface.procedure_ref,
            capability_surface: self.surface.capability_surface,
            context_slice: self.surface.context_slice,
            vfs_surface: self.surface.vfs_surface,
            mcp_surface: self.surface.mcp_surface,
            runtime_session_id: self.surface.runtime_session_id,
            graph_instance_id: self.surface.graph_instance_id,
            activity_key: self.surface.activity_key,
            executor_config: Some(self.executor_config),
            working_directory: Some(self.working_directory),
            prompt_blocks: self.intent.prompt_blocks,
            environment_variables: self.intent.environment_variables,
            identity: self.intent.identity,
            terminal_hook_effect_binding: self.intent.terminal_hook_effect_binding,
            discovered_guidelines: self.intent.discovered_guidelines,
            extension_runtime: self.intent.extension_runtime,
            context_bundle: self.context_bundle,
            typed_capability_state: Some(self.capability_state),
            typed_vfs: Some(self.vfs),
            typed_mcp_servers: self.mcp_servers,
            continuation_context_frame: self.continuation_context_frame,
            base_capability_state: self.base_capability_state,
            resolution_trace: self.resolution_trace,
        }
    }
}

/// Launch 过程中 resolution 来源的 trace 数据（仅用于诊断/可观测性）。
#[derive(Debug, Clone, Default)]
pub struct LaunchResolutionTrace {
    pub vfs_source: Option<String>,
    pub mcp_source: Option<String>,
    pub capability_source: Option<String>,
    pub pending_overlay_applied: bool,
}

/// 从 AgentFrame 投影出的 runtime adapter 请求。
///
/// connector 通过此结构获取启动所需的全部 surface 数据，
/// 不再从 session / business owner 反查。
#[derive(Debug, Clone)]
pub struct RuntimeLaunchRequest {
    // ── frame 投影 ──
    pub agent_id: Uuid,
    pub frame_id: Uuid,
    pub frame_revision: i32,
    pub procedure_ref: Option<AgentProcedureRef>,
    pub capability_surface: serde_json::Value,
    pub context_slice: serde_json::Value,
    pub vfs_surface: serde_json::Value,
    pub mcp_surface: serde_json::Value,
    pub runtime_session_id: Option<String>,
    pub graph_instance_id: Option<Uuid>,
    pub activity_key: Option<String>,

    // ── session 创建 / connector 启动所需 ──
    pub executor_config: Option<AgentConfig>,
    pub working_directory: Option<PathBuf>,
    pub prompt_blocks: Option<Vec<serde_json::Value>>,
    pub environment_variables: HashMap<String, String>,
    pub identity: Option<AuthIdentity>,
    pub terminal_hook_effect_binding: Option<TerminalHookEffectBinding>,
    pub discovered_guidelines: Vec<DiscoveredGuideline>,
    pub extension_runtime: Option<ExtensionRuntimeProjection>,
    pub context_bundle: Option<SessionContextBundle>,

    // ── 结构化投影（从 frame JSON 反序列化的类型安全版本）──
    pub typed_capability_state: Option<CapabilityState>,
    pub typed_vfs: Option<Vfs>,
    pub typed_mcp_servers: Vec<SessionMcpServer>,

    // ── launch pipeline 所需附加字段 ──
    pub continuation_context_frame: Option<ContextFrame>,
    pub base_capability_state: Option<CapabilityState>,
    pub resolution_trace: LaunchResolutionTrace,
}

impl RuntimeLaunchRequest {
    /// 从一个 AgentFrame revision 投影出 launch request。
    ///
    /// JSON 字段 fallback 到 `serde_json::Value::Null`，
    /// connector 侧按需做 nullable 检查。
    pub fn from_frame(frame: &AgentFrame, runtime_policy: RuntimeSessionSelectionPolicy) -> Self {
        let runtime_session_id = frame.select_runtime_session_id(runtime_policy);

        let procedure_ref = frame.procedure_id.map(AgentProcedureRef::ById);

        let executor_config = frame
            .execution_profile_json
            .as_ref()
            .and_then(|v| serde_json::from_value::<AgentConfig>(v.clone()).ok());

        let typed_capability_state = frame
            .effective_capability_json
            .as_ref()
            .and_then(|v| serde_json::from_value::<CapabilityState>(v.clone()).ok());

        let typed_vfs = frame
            .vfs_surface_json
            .as_ref()
            .and_then(|v| serde_json::from_value::<Vfs>(v.clone()).ok());

        let typed_mcp_servers = frame
            .mcp_surface_json
            .as_ref()
            .and_then(|v| serde_json::from_value::<Vec<SessionMcpServer>>(v.clone()).ok())
            .unwrap_or_default();

        let working_directory = typed_vfs
            .as_ref()
            .and_then(|vfs| vfs.default_mount())
            .map(|mount| PathBuf::from(mount.root_ref.trim()))
            .filter(|path| !path.as_os_str().is_empty());
        Self {
            agent_id: frame.agent_id,
            frame_id: frame.id,
            frame_revision: frame.revision,
            procedure_ref,
            capability_surface: frame
                .effective_capability_json
                .clone()
                .unwrap_or(serde_json::Value::Null),
            context_slice: frame
                .context_slice_json
                .clone()
                .unwrap_or(serde_json::Value::Null),
            vfs_surface: frame
                .vfs_surface_json
                .clone()
                .unwrap_or(serde_json::Value::Null),
            mcp_surface: frame
                .mcp_surface_json
                .clone()
                .unwrap_or(serde_json::Value::Null),
            runtime_session_id,
            graph_instance_id: frame.graph_instance_id,
            activity_key: frame.activity_key.clone(),
            executor_config,
            working_directory,
            prompt_blocks: None,
            environment_variables: HashMap::new(),
            identity: None,
            terminal_hook_effect_binding: None,
            discovered_guidelines: Vec::new(),
            extension_runtime: None,
            context_bundle: None,
            typed_capability_state,
            typed_vfs,
            typed_mcp_servers,
            continuation_context_frame: None,
            base_capability_state: None,
            resolution_trace: LaunchResolutionTrace::default(),
        }
    }

    /// 设置 continuation context frame（跨 turn 延续上下文）。
    pub fn with_continuation_context(mut self, frame: Option<ContextFrame>) -> Self {
        self.continuation_context_frame = frame;
        self
    }

    /// 设置 base capability state（用于 pending transition replay）。
    pub fn with_base_capability_state(mut self, state: Option<CapabilityState>) -> Self {
        self.base_capability_state = state;
        self
    }

    /// 设置 resolution trace（诊断用途）。
    pub fn with_resolution_trace(mut self, trace: LaunchResolutionTrace) -> Self {
        self.resolution_trace = trace;
        self
    }

    /// 设置用户 prompt blocks 和环境变量。
    pub fn with_prompt(
        mut self,
        prompt_blocks: Option<Vec<serde_json::Value>>,
        environment_variables: HashMap<String, String>,
    ) -> Self {
        self.prompt_blocks = prompt_blocks;
        self.environment_variables = environment_variables;
        self
    }

    /// 设置调用者身份。
    pub fn with_identity(mut self, identity: Option<AuthIdentity>) -> Self {
        self.identity = identity;
        self
    }

    /// 设置终端 hook effect binding。
    pub fn with_terminal_effects(mut self, binding: Option<TerminalHookEffectBinding>) -> Self {
        self.terminal_hook_effect_binding = binding;
        self
    }

    /// 设置 discovered guidelines（技能/指南发现结果）。
    pub fn with_discovered_guidelines(mut self, guidelines: Vec<DiscoveredGuideline>) -> Self {
        self.discovered_guidelines = guidelines;
        self
    }

    /// 设置扩展运行时投影。
    pub fn with_extension_runtime(mut self, ext: Option<ExtensionRuntimeProjection>) -> Self {
        self.extension_runtime = ext;
        self
    }

    /// 设置 context bundle。
    pub fn with_context_bundle(mut self, bundle: Option<SessionContextBundle>) -> Self {
        self.context_bundle = bundle;
        self
    }

    /// 覆盖执行器配置（当 compose 逻辑额外解析 executor 时使用）。
    pub fn with_executor_config(mut self, config: AgentConfig) -> Self {
        self.executor_config = Some(config);
        self
    }

    /// 覆盖工作目录（当 compose 逻辑额外解析 working_dir 时使用）。
    pub fn with_working_directory(mut self, dir: PathBuf) -> Self {
        self.working_directory = Some(dir);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::workflow::AgentFrame;

    #[test]
    fn from_frame_projects_all_fields() {
        let agent_id = Uuid::new_v4();
        let proc_id = Uuid::new_v4();
        let gi_id = Uuid::new_v4();
        let session_id = Uuid::new_v4();

        let mut frame = AgentFrame::new_revision(agent_id, 3, "test");
        frame.procedure_id = Some(proc_id);
        frame.graph_instance_id = Some(gi_id);
        frame.activity_key = Some("implement".to_string());
        frame.effective_capability_json = Some(serde_json::json!({"file_read": true}));
        frame.context_slice_json = Some(serde_json::json!({"project": "demo"}));
        frame.vfs_surface_json = Some(serde_json::json!({"mounts": []}));
        frame.mcp_surface_json = Some(serde_json::json!({"servers": []}));
        frame.runtime_session_refs_json =
            AgentFrame::runtime_session_refs_json([session_id.to_string()]);

        let request =
            RuntimeLaunchRequest::from_frame(&frame, RuntimeSessionSelectionPolicy::LaunchPrimary);

        assert_eq!(request.agent_id, agent_id);
        assert_eq!(request.frame_id, frame.id);
        assert_eq!(request.frame_revision, 3);
        assert_eq!(request.graph_instance_id, Some(gi_id));
        assert_eq!(request.activity_key.as_deref(), Some("implement"));
        assert_eq!(request.runtime_session_id, Some(session_id.to_string()));
        assert!(matches!(
            request.procedure_ref,
            Some(AgentProcedureRef::ById(id)) if id == proc_id
        ));
        assert_eq!(
            request.capability_surface,
            serde_json::json!({"file_read": true})
        );
        assert_eq!(
            request.context_slice,
            serde_json::json!({"project": "demo"})
        );
        assert!(request.executor_config.is_none());
        assert!(request.prompt_blocks.is_none());
        assert!(request.identity.is_none());
    }

    #[test]
    fn from_frame_handles_empty_fields() {
        let agent_id = Uuid::new_v4();
        let frame = AgentFrame::new_initial(agent_id, None);

        let request =
            RuntimeLaunchRequest::from_frame(&frame, RuntimeSessionSelectionPolicy::LaunchPrimary);

        assert_eq!(request.agent_id, agent_id);
        assert_eq!(request.frame_revision, 1);
        assert!(request.procedure_ref.is_none());
        assert!(request.runtime_session_id.is_none());
        assert!(request.graph_instance_id.is_none());
        assert!(request.activity_key.is_none());
        assert!(request.capability_surface.is_null());
        assert!(request.context_slice.is_null());
        assert!(request.vfs_surface.is_null());
        assert!(request.mcp_surface.is_null());
        assert!(request.executor_config.is_none());
        assert!(request.working_directory.is_none());
        assert!(request.typed_capability_state.is_none());
        assert!(request.typed_vfs.is_none());
        assert!(request.typed_mcp_servers.is_empty());
    }

    #[test]
    fn from_frame_uses_explicit_runtime_session_policy() {
        let agent_id = Uuid::new_v4();
        let s1 = Uuid::new_v4();
        let s2 = Uuid::new_v4();
        let mut frame = AgentFrame::new_revision(agent_id, 2, "test");
        frame.runtime_session_refs_json =
            AgentFrame::runtime_session_refs_json([s1.to_string(), s2.to_string()]);

        let primary_request =
            RuntimeLaunchRequest::from_frame(&frame, RuntimeSessionSelectionPolicy::LaunchPrimary);
        let latest_request =
            RuntimeLaunchRequest::from_frame(&frame, RuntimeSessionSelectionPolicy::LatestAttached);
        assert_eq!(primary_request.runtime_session_id, Some(s1.to_string()));
        assert_eq!(latest_request.runtime_session_id, Some(s2.to_string()));
    }

    #[test]
    fn from_frame_projects_execution_profile() {
        let agent_id = Uuid::new_v4();
        let config = AgentConfig::new("PI_AGENT");
        let mut frame = AgentFrame::new_revision(agent_id, 1, "test");
        frame.execution_profile_json = serde_json::to_value(&config).ok();

        let request =
            RuntimeLaunchRequest::from_frame(&frame, RuntimeSessionSelectionPolicy::LaunchPrimary);
        assert_eq!(
            request
                .executor_config
                .as_ref()
                .map(|c| c.executor.as_str()),
            Some("PI_AGENT")
        );
    }

    #[test]
    fn builder_chain_sets_all_launch_fields() {
        let agent_id = Uuid::new_v4();
        let frame = AgentFrame::new_revision(agent_id, 1, "test");

        let request =
            RuntimeLaunchRequest::from_frame(&frame, RuntimeSessionSelectionPolicy::LaunchPrimary)
                .with_executor_config(AgentConfig::new("PI_AGENT"))
                .with_working_directory(PathBuf::from("/workspace"))
                .with_prompt(
                    Some(vec![serde_json::json!({"type": "text", "text": "hello"})]),
                    HashMap::from([("A".to_string(), "B".to_string())]),
                )
                .with_discovered_guidelines(vec![]);

        assert_eq!(request.executor_config.unwrap().executor, "PI_AGENT");
        assert_eq!(request.working_directory, Some(PathBuf::from("/workspace")));
        assert!(request.prompt_blocks.is_some());
        assert_eq!(request.environment_variables["A"], "B");
    }
}
