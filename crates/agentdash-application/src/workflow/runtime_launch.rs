//! RuntimeLaunchRequest — 从 AgentFrame 投影出的 runtime adapter 请求。
//!
//! ## 设计定位
//!
//! `RuntimeLaunchRequest` 是 connector launch 的唯一输入来源：
//!
//! ```text
//! AgentFrame revision
//!   → RuntimeLaunchRequest::from_frame()
//!   → connector ExecutionContext
//!   → RuntimeSession events
//! ```
//!
//! launch 路径从 `SessionConstructionPlan → LaunchPlan → ConnectorInputPlan → ExecutionContext`
//! 迁移到 `AgentFrame → RuntimeLaunchRequest → ExecutionContext`。
//!
//! ### Session 创建 + Connector 启动
//!
//! `RuntimeLaunchRequest` 同时承载 surface 投影和 session 创建所需的执行器配置。
//! compose 函数产出 `(AgentFrameBuilder, RuntimeLaunchRequest)`：
//! - `AgentFrameBuilder.build()` 持久化 frame revision
//! - `RuntimeLaunchRequest` 驱动 runtime session 创建 + connector 启动

use std::collections::HashMap;
use std::path::PathBuf;

use agentdash_domain::workflow::{AgentFrame, AgentProcedureRef};
use agentdash_spi::{
    AgentConfig, AuthIdentity, CapabilityState, DiscoveredGuideline, SessionContextBundle,
    SessionMcpServer, Vfs,
};
use uuid::Uuid;

use crate::extension_runtime::ExtensionRuntimeProjection;
use crate::session::post_turn_handler::TerminalHookEffectBinding;

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
    pub runtime_session_id: Option<Uuid>,
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
}

impl RuntimeLaunchRequest {
    /// 从一个 AgentFrame revision 投影出 launch request。
    ///
    /// JSON 字段 fallback 到 `serde_json::Value::Null`，
    /// connector 侧按需做 nullable 检查。
    pub fn from_frame(frame: &AgentFrame) -> Self {
        let runtime_session_id = frame
            .runtime_session_refs_json
            .as_ref()
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first())
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok());

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
        }
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
        frame.runtime_session_refs_json = Some(serde_json::json!([session_id.to_string()]));

        let request = RuntimeLaunchRequest::from_frame(&frame);

        assert_eq!(request.agent_id, agent_id);
        assert_eq!(request.frame_id, frame.id);
        assert_eq!(request.frame_revision, 3);
        assert_eq!(request.graph_instance_id, Some(gi_id));
        assert_eq!(request.activity_key.as_deref(), Some("implement"));
        assert_eq!(request.runtime_session_id, Some(session_id));
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

        let request = RuntimeLaunchRequest::from_frame(&frame);

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
    fn from_frame_picks_first_session_ref() {
        let agent_id = Uuid::new_v4();
        let s1 = Uuid::new_v4();
        let s2 = Uuid::new_v4();
        let mut frame = AgentFrame::new_revision(agent_id, 2, "test");
        frame.runtime_session_refs_json =
            Some(serde_json::json!([s1.to_string(), s2.to_string()]));

        let request = RuntimeLaunchRequest::from_frame(&frame);
        assert_eq!(request.runtime_session_id, Some(s1));
    }

    #[test]
    fn from_frame_projects_execution_profile() {
        let agent_id = Uuid::new_v4();
        let config = AgentConfig::new("PI_AGENT");
        let mut frame = AgentFrame::new_revision(agent_id, 1, "test");
        frame.execution_profile_json = serde_json::to_value(&config).ok();

        let request = RuntimeLaunchRequest::from_frame(&frame);
        assert_eq!(
            request.executor_config.as_ref().map(|c| c.executor.as_str()),
            Some("PI_AGENT")
        );
    }

    #[test]
    fn builder_chain_sets_all_launch_fields() {
        let agent_id = Uuid::new_v4();
        let frame = AgentFrame::new_revision(agent_id, 1, "test");

        let request = RuntimeLaunchRequest::from_frame(&frame)
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
