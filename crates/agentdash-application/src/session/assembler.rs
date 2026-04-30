//! `SessionRequestAssembler` — 统一 session 启动请求组装。
//!
//! ## 设计
//!
//! 代码库里一共有 5 条 session 启动路径,此前各自手写 bootstrap 逻辑:
//!
//! | 路径 | 实现入口 |
//! |---|---|
//! | ACP Story/Project | `api::routes::acp_sessions` → `SessionRequestAssembler::compose_owner_bootstrap` |
//! | Story step activation | `task::service::StoryStepActivationService::activate_story_step` → `SessionRequestAssembler::compose_story_step` |
//! | Routine | `routine::executor::build_project_agent_prompt_request` → `SessionRequestAssembler::compose_owner_bootstrap`(带 trigger tag) |
//! | Workflow AgentNode | `workflow::orchestrator::start_agent_node_prompt` → `compose_lifecycle_node` |
//! | Companion | `companion::tools` → `compose_companion` |
//!
//! 5 条路径共享 4 个"策略轴":owner scope mount / context bundle 生成 /
//! prompt 来源 / 能力裁剪 / 父 session 继承。但字段形状不相交(Task 有
//! `ActiveWorkflowProjection`,Companion 有 parent 继承,AgentNode 有 step),
//! 因此设计上采用**组合器+平坦末端**而非 sum type:
//!
//! ```text
//! 4 个 compose fn(各自 Spec) → PreparedSessionInputs(平坦) → finalize_request → PromptSessionRequest
//! ```
//!
//! compose 函数内部共享 building blocks(`load_available_presets` /
//! `build_owner_context` / `activate_step_with_platform` 等),不再重复散落。

use std::collections::{BTreeSet, HashMap, HashSet};

use agentdash_domain::canvas::CanvasRepository;
use agentdash_domain::common::AgentConfig;
use agentdash_domain::project::Project;
use agentdash_domain::session_binding::SessionOwnerCtx;
use agentdash_domain::story::Story;
use agentdash_domain::task::Task;
use agentdash_domain::workflow::CapabilityDirective;
use agentdash_domain::workflow::{LifecycleDefinition, LifecycleRun, LifecycleStepDefinition};
use agentdash_domain::workspace::Workspace;
use agentdash_spi::auth::AuthIdentity;
use agentdash_spi::{FlowCapabilities, SessionContextBundle, Vfs};
use uuid::Uuid;

use crate::canvas::append_visible_canvas_mounts;
use crate::capability::{
    AgentMcpServerEntry, AvailableMcpPresets, CapabilityResolver, CapabilityResolverInput,
    SessionWorkflowContext, capability_directives_from_active_workflow,
};
use crate::companion::tools::CompanionSliceMode;
use crate::context::{
    AuditTrigger, ContextBuildPhase, Contribution, SessionContextConfig, SharedContextAuditBus,
    TaskExecutionPhase, build_declared_source_warning_fragment, build_session_context_bundle,
    contribute_binding_initial_context, contribute_core_context, contribute_declared_sources,
    contribute_instruction, contribute_mcp, contribute_workflow_binding,
    contribute_workspace_static_sources, emit_bundle_fragments, resolve_workspace_declared_sources,
};
use crate::platform_config::PlatformConfig;
use crate::project::context_builder::{ProjectContextBuildInput, contribute_project_context};
use crate::repository_set::RepositorySet;
use crate::runtime::RuntimeMcpServer;
use crate::session::context::apply_workspace_defaults;
use crate::session::types::{PromptSessionRequest, HookSnapshotReloadTrigger, UserPromptInput};
use crate::story::context_builder::{StoryContextBuildInput, contribute_story_context};
use crate::task::execution::TaskExecutionError;
use crate::vfs::{
    RelayVfsService, SessionMountTarget, build_lifecycle_mount_with_ports, resolve_context_bindings,
};
use crate::workflow::{
    ActiveWorkflowProjection, StepActivationInput, activate_step_with_platform,
    load_port_output_map,
};
use crate::workspace::BackendAvailability;

// ═══════════════════════════════════════════════════════════════════
// SECTION 1:末端统一结构 PreparedSessionInputs + finalize_request
// ═══════════════════════════════════════════════════════════════════

/// compose 函数对 `PromptSessionRequest` 的全部后端注入字段的平坦表示。
///
/// ## 字段分组
///
/// - **用户输入侧字段**（`env` / `working_dir` / `prompt_blocks` / `executor_config`）：
///   可由 entry 通过 `SessionAssemblyBuilder::with_user_input` 一次性注入，
///   也可由 compose 逐项覆盖。最终 `finalize_request` 按"prepared 非空则覆盖 base"语义合并。
/// - **运行时注入字段**（`mcp_servers` / `vfs` / `flow_capabilities` 等）：compose 产出。
/// - **身份与回调字段**（`identity` / `post_turn_handler`）：entry 通过 builder first-class
///   方法注入；`finalize_request` 合入 req。2026-04-30 PR 1 Phase 1c 起由 builder 承载，
///   不再由调用方在 `finalize_request` 之后手工 `req.xxx = ...` 赋值。
///
/// 不派生 `Debug` 因为 `DynPostTurnHandler = Arc<dyn PostTurnHandler>`
/// 而 trait 不要求 Debug。
#[derive(Clone, Default)]
pub struct PreparedSessionInputs {
    pub prompt_blocks: Option<Vec<serde_json::Value>>,
    pub executor_config: Option<AgentConfig>,
    pub mcp_servers: Vec<agent_client_protocol::McpServer>,
    pub relay_mcp_server_names: HashSet<String>,
    pub vfs: Option<Vfs>,
    pub flow_capabilities: Option<FlowCapabilities>,
    pub effective_capability_keys: Option<BTreeSet<String>>,
    /// 结构化 session 上下文 Bundle —— 所有 connector 的主数据源。
    pub context_bundle: Option<SessionContextBundle>,
    pub hook_snapshot_reload: HookSnapshotReloadTrigger,
    /// workspace 默认值注入的数据源(session 中 vfs/working_dir 回填用)。
    pub workspace_defaults: Option<Workspace>,
    /// Story step(task 启动)场景下 compose 产出的 working_dir 覆盖值；
    /// 仅在需要绕过 workspace_defaults 的 task 启动路径使用。
    pub working_dir: Option<String>,
    /// 用户输入的环境变量（由 entry 通过 `with_user_input` 或 `with_env` 注入）。
    pub env: HashMap<String, String>,
    /// 发起本次 prompt 的用户身份（由 entry 通过 `with_identity` 注入）。
    pub identity: Option<AuthIdentity>,
    /// Turn 事件回调（由 task / routine 等 entry 通过 `with_post_turn_handler` 注入）。
    pub post_turn_handler: Option<crate::session::post_turn_handler::DynPostTurnHandler>,
    /// Context contributor pipeline 的诊断摘要（供启动响应展示）。
    pub source_summary: Vec<String>,
}

/// 把 `PreparedSessionInputs` 合并进一个 base `PromptSessionRequest`。
///
/// ## 合并语义（2026-04-30 对称化后）
///
/// | 字段 | 策略 |
/// |---|---|
/// | `prompt_blocks` | `Option`：prepared 非空覆盖；否则保留 base |
/// | `executor_config` | `Option`：prepared 非空覆盖；否则保留 base |
/// | `context_bundle` / `hook_snapshot_reload` / `flow_capabilities` / `effective_capability_keys` | 整体替换为 prepared 值 |
/// | `working_dir` | prepared 非空覆盖；否则 `apply_workspace_defaults` 按需从 workspace 回填 |
/// | `vfs` | prepared 非空覆盖；否则 `apply_workspace_defaults` 按需从 workspace 回填 |
/// | `mcp_servers` | **整体替换** 为 prepared 值（compose 内部已汇总 request + platform + custom + preset） |
/// | `relay_mcp_server_names` | **整体替换** 为 prepared 值（与 `mcp_servers` 对称） |
/// | `env` | prepared 非空（`!is_empty()`）时整体替换；否则保留 base 的 env |
/// | `identity` | prepared 非空时覆盖；否则保留 base |
/// | `post_turn_handler` | prepared 非空时覆盖；否则保留 base |
///
/// **对称化背景**：`mcp_servers` 与 `relay_mcp_server_names` 在 compose 内部都已经把
/// base 侧透传值合并进去（见 `compose_owner_bootstrap` 中 `effective_mcp_servers` /
/// `relay_mcp_server_names` 的汇总逻辑）。finalize 阶段 base 里这两个字段实际都是
/// `PromptSessionRequest::from_user_input` 的 default，统一整体替换既正确又对称。
///
/// **identity / post_turn_handler 下沉**（2026-04-30 PR 1 Phase 1c）：过去这两个字段
/// 由调用方在 `finalize_request` 之后手工 `req.identity = ...` 赋值，容易漏填
/// （如 routine 路径）。现在 entry 通过 `SessionAssemblyBuilder::with_identity` /
/// `with_post_turn_handler` 注入，`finalize_request` 统一合入，单一装配节拍保证不漏。
pub fn finalize_request(
    base: PromptSessionRequest,
    prepared: PreparedSessionInputs,
) -> PromptSessionRequest {
    let mut req = base;
    if let Some(blocks) = prepared.prompt_blocks {
        req.user_input.prompt_blocks = Some(blocks);
    }
    if let Some(cfg) = prepared.executor_config {
        req.user_input.executor_config = Some(cfg);
    }
    req.context_bundle = prepared.context_bundle;
    req.hook_snapshot_reload = prepared.hook_snapshot_reload;

    apply_workspace_defaults(
        &mut req.user_input.working_dir,
        &mut req.vfs,
        prepared.workspace_defaults.as_ref(),
    );
    if let Some(wd) = prepared.working_dir {
        req.user_input.working_dir = Some(wd);
    }
    // vfs 覆盖规则：prepared 非空则覆盖，否则保留（含 workspace_defaults 回填结果）。
    // 语义等价于旧的三重分支，但表达更直接；compose 产出的 workspace/canvas/lifecycle
    // mount 组合会覆盖前端透传的 vfs，是刻意为之。
    if prepared.vfs.is_some() {
        req.vfs = prepared.vfs;
    }
    req.mcp_servers = prepared.mcp_servers;
    req.relay_mcp_server_names = prepared.relay_mcp_server_names;
    req.flow_capabilities = prepared.flow_capabilities;
    req.effective_capability_keys = prepared.effective_capability_keys;
    if !prepared.env.is_empty() {
        req.user_input.env = prepared.env;
    }
    if prepared.identity.is_some() {
        req.identity = prepared.identity;
    }
    if prepared.post_turn_handler.is_some() {
        req.post_turn_handler = prepared.post_turn_handler;
    }
    req
}

// ═══════════════════════════════════════════════════════════════════
// SECTION 1.5:SessionAssemblyBuilder — 组合式 session 装配
// ═══════════════════════════════════════════════════════════════════

/// 声明式 session 装配 builder。
///
/// 将 session 启动拆为 6 个正交关注点（VFS / 能力 / MCP / 系统上下文 / Prompt / 工作流），
/// 每个关注点通过独立的 `with_*` 方法注入，`build()` 统一产出 `PreparedSessionInputs`。
///
/// ## 设计原则
///
/// - **每个层独立**：`with_*` 方法只写入自己关注的字段，不覆盖其他层
/// - **追加友好**：MCP / relay 等集合字段支持多次 `append`
/// - **复合便利**：`apply_companion_slice` / `apply_lifecycle_activation` 封装常见组合
/// - **新组合无需新函数**：companion + workflow 只需叠加对应层
///
/// 不派生 `Debug`（post_turn_handler trait 不要求 Debug）；Clone 保留。
#[derive(Clone, Default)]
pub struct SessionAssemblyBuilder {
    // ── VFS 层 ──
    vfs: Option<Vfs>,

    // ── 能力层 ──
    flow_capabilities: Option<FlowCapabilities>,
    effective_capability_keys: Option<BTreeSet<String>>,

    // ── MCP 层 ──
    mcp_servers: Vec<agent_client_protocol::McpServer>,
    relay_mcp_server_names: HashSet<String>,

    // ── 系统上下文层 ──
    context_bundle: Option<SessionContextBundle>,

    // ── Prompt 层 ──
    prompt_blocks: Option<Vec<serde_json::Value>>,
    executor_config: Option<AgentConfig>,
    hook_snapshot_reload: HookSnapshotReloadTrigger,

    // ── 元信息层 ──
    workspace_defaults: Option<Workspace>,

    // ── 用户输入侧 ──
    env: HashMap<String, String>,

    // ── 身份 & 回调（2026-04-30 PR 1 Phase 1c 新增） ──
    identity: Option<AuthIdentity>,
    post_turn_handler: Option<crate::session::post_turn_handler::DynPostTurnHandler>,

    // ── 其它平坦字段 ──
    working_dir: Option<String>,
    source_summary: Vec<String>,
}

impl SessionAssemblyBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    // ── VFS 层方法 ────────────────────────────────────────────────

    /// 直接设置完整 VFS（owner 构建 / lifecycle 激活产出等场景）。
    pub fn with_vfs(mut self, vfs: Vfs) -> Self {
        self.vfs = Some(vfs);
        self
    }

    /// 从父 session 切片生成 companion VFS。
    pub fn with_companion_vfs(
        mut self,
        parent_vfs: Option<&Vfs>,
        mode: CompanionSliceMode,
    ) -> Self {
        use crate::companion::tools::build_companion_execution_slice;
        let slice = build_companion_execution_slice(parent_vfs, &[], mode);
        self.vfs = slice.vfs;
        self
    }

    /// 在已有 VFS 上追加 lifecycle mount（story step activation 场景）。
    pub fn append_lifecycle_mount(
        mut self,
        run_id: Uuid,
        lifecycle_key: &str,
        writable_port_keys: &[String],
    ) -> Self {
        if let Some(space) = self.vfs.as_mut() {
            space.mounts.push(build_lifecycle_mount_with_ports(
                run_id,
                lifecycle_key,
                writable_port_keys,
            ));
        }
        self
    }

    /// 在已有 VFS 上追加 canvas mount。
    pub async fn append_canvas_mounts(
        mut self,
        canvas_repo: &dyn CanvasRepository,
        project_id: Uuid,
        mount_ids: &[String],
    ) -> Result<Self, String> {
        if let Some(space) = self.vfs.as_mut() {
            append_visible_canvas_mounts(canvas_repo, project_id, space, mount_ids)
                .await
                .map_err(|e| e.to_string())?;
        }
        Ok(self)
    }

    // ── 能力层方法 ────────────────────────────────────────────────

    /// 设置已解析的能力输出（由外部 CapabilityResolver 产出）。
    pub fn with_resolved_capabilities(
        mut self,
        flow_capabilities: FlowCapabilities,
        effective_keys: BTreeSet<String>,
    ) -> Self {
        self.flow_capabilities = Some(flow_capabilities);
        self.effective_capability_keys = Some(effective_keys);
        self
    }

    /// 使用 companion 专属能力裁剪。
    pub fn with_companion_capabilities(mut self, mode: CompanionSliceMode) -> Self {
        let mapped = map_slice_mode(mode);
        let flow_caps = CapabilityResolver::resolve_companion_caps(mapped);
        self.flow_capabilities = Some(flow_caps);
        self
    }

    // ── MCP 层方法 ────────────────────────────────────────────────

    /// 设置 MCP server 列表（覆盖）。
    pub fn with_mcp_servers(mut self, servers: Vec<agent_client_protocol::McpServer>) -> Self {
        self.mcp_servers = servers;
        self
    }

    /// 追加 MCP server 到列表。
    pub fn append_mcp_servers(
        mut self,
        servers: impl IntoIterator<Item = agent_client_protocol::McpServer>,
    ) -> Self {
        self.mcp_servers.extend(servers);
        self
    }

    /// 追加 relay MCP server name 集合。
    pub fn append_relay_mcp_names(mut self, names: impl IntoIterator<Item = String>) -> Self {
        self.relay_mcp_server_names.extend(names);
        self
    }

    // ── 系统上下文层方法 ──────────────────────────────────────────

    /// 设置结构化上下文 Bundle —— 所有 connector 的主数据源。
    pub fn with_context_bundle(mut self, bundle: SessionContextBundle) -> Self {
        self.context_bundle = Some(bundle);
        self
    }

    /// 可选设置 Bundle；为 `None` 时不覆盖已有值（用于 continuation 路径按条件注入）。
    pub fn with_optional_context_bundle(mut self, bundle: Option<SessionContextBundle>) -> Self {
        if bundle.is_some() {
            self.context_bundle = bundle;
        }
        self
    }

    // ── Prompt 层方法 ─────────────────────────────────────────────

    /// 设置 prompt blocks。
    pub fn with_prompt_blocks(mut self, blocks: Vec<serde_json::Value>) -> Self {
        self.prompt_blocks = Some(blocks);
        self
    }

    /// 设置执行器配置。
    pub fn with_executor_config(mut self, config: AgentConfig) -> Self {
        self.executor_config = Some(config);
        self
    }

    /// 设置 bootstrap action。
    pub fn with_hook_snapshot_reload(mut self, action: HookSnapshotReloadTrigger) -> Self {
        self.hook_snapshot_reload = action;
        self
    }

    // ── 元信息层方法 ──────────────────────────────────────────────

    /// 设置 workspace 默认值（用于 VFS/working_dir 回填）。
    pub fn with_workspace_defaults(mut self, workspace: Workspace) -> Self {
        self.workspace_defaults = Some(workspace);
        self
    }

    /// 可选设置 workspace 默认值。
    pub fn with_optional_workspace_defaults(mut self, workspace: Option<Workspace>) -> Self {
        self.workspace_defaults = workspace;
        self
    }

    /// 设置 compose 产出的 working_dir（task 启动场景覆盖 base working_dir）。
    pub fn with_working_dir(mut self, working_dir: Option<String>) -> Self {
        self.working_dir = working_dir;
        self
    }

    /// 设置 source summary（context contributor pipeline 诊断）。
    pub fn with_source_summary(mut self, summary: Vec<String>) -> Self {
        self.source_summary = summary;
        self
    }

    // ── 用户输入层方法（2026-04-30 PR 1 Phase 1c 新增） ─────────────

    /// 设置环境变量 map（entry 注入用户侧 env）。
    pub fn with_env(mut self, env: HashMap<String, String>) -> Self {
        self.env = env;
        self
    }

    /// 发起本次 prompt 的用户身份（含 `AuthIdentity::system_routine(id)` 等）。
    pub fn with_identity(mut self, identity: AuthIdentity) -> Self {
        self.identity = Some(identity);
        self
    }

    /// 可选设置身份（None 时不覆盖已有值）。
    pub fn with_optional_identity(mut self, identity: Option<AuthIdentity>) -> Self {
        if identity.is_some() {
            self.identity = identity;
        }
        self
    }

    /// Turn 事件回调（task / routine 等 entry 注入）。
    pub fn with_post_turn_handler(
        mut self,
        handler: crate::session::post_turn_handler::DynPostTurnHandler,
    ) -> Self {
        self.post_turn_handler = Some(handler);
        self
    }

    /// 可选 Turn 事件回调（None 时不覆盖）。
    pub fn with_optional_post_turn_handler(
        mut self,
        handler: Option<crate::session::post_turn_handler::DynPostTurnHandler>,
    ) -> Self {
        if handler.is_some() {
            self.post_turn_handler = handler;
        }
        self
    }

    /// 一次性吸收 `UserPromptInput` 的所有字段。
    ///
    /// 等价于依次调用 `with_prompt_blocks` / `with_executor_config` / `with_working_dir` /
    /// `with_env`；便于 entry 把"用户原始输入"集中交给 builder，compose 阶段如需要再
    /// 通过独立 `with_*` 方法覆盖个别字段（compose 产出优先）。
    pub fn with_user_input(mut self, input: UserPromptInput) -> Self {
        if let Some(blocks) = input.prompt_blocks {
            self.prompt_blocks = Some(blocks);
        }
        if let Some(cfg) = input.executor_config {
            self.executor_config = Some(cfg);
        }
        if input.working_dir.is_some() {
            self.working_dir = input.working_dir;
        }
        self.env = input.env;
        self
    }

    // ── 复合便利方法 ──────────────────────────────────────────────

    /// 一步完成 companion slice 装配（VFS + MCP + 能力 + prompt + bootstrap）。
    ///
    /// 保留 `self` 上预先设置的 `identity` / `post_turn_handler` / `env` 等字段
    /// （用 `..self` 叠加语法），只覆盖 companion slice 涉及的关注点。
    pub fn apply_companion_slice(
        self,
        parent_vfs: Option<&Vfs>,
        parent_mcp_servers: &[agent_client_protocol::McpServer],
        parent_context_bundle: Option<&SessionContextBundle>,
        mode: CompanionSliceMode,
        executor_config: AgentConfig,
        dispatch_prompt: String,
    ) -> Self {
        use crate::companion::tools::build_companion_execution_slice;

        let slice = build_companion_execution_slice(parent_vfs, parent_mcp_servers, mode);
        let mapped = map_slice_mode(mode);
        let flow_caps = CapabilityResolver::resolve_companion_caps(mapped);

        let prompt_blocks = vec![serde_json::json!({
            "type": "text",
            "text": dispatch_prompt,
        })];

        Self {
            vfs: slice.vfs,
            flow_capabilities: Some(flow_caps),
            effective_capability_keys: None,
            mcp_servers: slice.mcp_servers,
            relay_mcp_server_names: HashSet::new(),
            context_bundle: parent_context_bundle.cloned(),
            prompt_blocks: Some(prompt_blocks),
            executor_config: Some(executor_config),
            hook_snapshot_reload: HookSnapshotReloadTrigger::Reload,
            workspace_defaults: None,
            working_dir: None,
            source_summary: Vec::new(),
            // 保留调用方已注入的身份 / 回调 / env 不被 companion slice 清空
            env: self.env,
            identity: self.identity,
            post_turn_handler: self.post_turn_handler,
        }
    }

    /// 一步完成 lifecycle node 装配（VFS + 能力 + MCP + prompt）。
    pub fn apply_lifecycle_activation(
        mut self,
        activation: &crate::workflow::StepActivation,
        inherited_executor_config: Option<AgentConfig>,
    ) -> Self {
        self.vfs = Some(activation.lifecycle_vfs.clone());
        self.flow_capabilities = Some(activation.flow_capabilities.clone());
        self.effective_capability_keys = Some(activation.capability_keys.clone());
        self.mcp_servers = activation.mcp_servers.clone();
        self.prompt_blocks = Some(vec![serde_json::json!({
            "type": "text",
            "text": "请执行当前 lifecycle 节点。",
        })]);
        self.executor_config = inherited_executor_config;
        self
    }

    // ── 构建 ──────────────────────────────────────────────────────

    /// 将累积的声明合并为 `PreparedSessionInputs`。
    pub fn build(self) -> PreparedSessionInputs {
        PreparedSessionInputs {
            prompt_blocks: self.prompt_blocks,
            executor_config: self.executor_config,
            mcp_servers: self.mcp_servers,
            relay_mcp_server_names: self.relay_mcp_server_names,
            vfs: self.vfs,
            flow_capabilities: self.flow_capabilities,
            effective_capability_keys: self.effective_capability_keys,
            context_bundle: self.context_bundle,
            hook_snapshot_reload: self.hook_snapshot_reload,
            workspace_defaults: self.workspace_defaults,
            working_dir: self.working_dir,
            env: self.env,
            identity: self.identity,
            post_turn_handler: self.post_turn_handler,
            source_summary: self.source_summary,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// SECTION 2:Assembler 共享服务容器
// ═══════════════════════════════════════════════════════════════════

/// `SessionRequestAssembler` 依赖的基础设施引用集合。
///
/// 由 `AppState` / 各 handler 构造后传入各 compose 函数,避免每个 compose
/// 签名都携带 6-7 个 service 参数。
pub struct SessionRequestAssembler<'a> {
    pub vfs_service: &'a RelayVfsService,
    pub canvas_repo: &'a dyn CanvasRepository,
    pub availability: &'a dyn BackendAvailability,
    pub repos: &'a RepositorySet,
    pub platform_config: &'a PlatformConfig,
    /// 可选审计总线 —— 每次 compose 产出 Bundle 后批量 emit。
    ///
    /// 为 `None` 时（例如单元测试 / routine 内部降级路径）跳过 emit；
    /// 生产路径由 `AppState` 注入 `InMemoryContextAuditBus` 共享实例。
    pub audit_bus: Option<SharedContextAuditBus>,
}

// ═══════════════════════════════════════════════════════════════════
// SECTION 3:共享 building blocks
// ═══════════════════════════════════════════════════════════════════

/// 加载 project 级 MCP Preset 并展开为 resolver 消费的 map。查询失败降级为空。
pub async fn load_available_presets(
    repos: &RepositorySet,
    project_id: Uuid,
) -> AvailableMcpPresets {
    match repos.mcp_preset_repo.list_by_project(project_id).await {
        Ok(presets) => presets.into_iter().map(|p| (p.key.clone(), p)).collect(),
        Err(error) => {
            tracing::warn!(
                project_id = %project_id,
                error = %error,
                "加载 project MCP Preset 列表失败,mcp:<X> 能力将退化到 inline agent_mcp_servers"
            );
            Default::default()
        }
    }
}

/// 从 agent-level `preset_mcp_servers` 抽出 `AgentMcpServerEntry`(供 resolver 解析 `mcp:<name>`)。
pub fn extract_agent_mcp_entries(
    preset_mcp_servers: &[agent_client_protocol::McpServer],
    relay_mcp_server_names: &HashSet<String>,
) -> Vec<AgentMcpServerEntry> {
    preset_mcp_servers
        .iter()
        .filter_map(|s| {
            let name = match s {
                agent_client_protocol::McpServer::Http(h) => h.name.clone(),
                agent_client_protocol::McpServer::Sse(h) => h.name.clone(),
                agent_client_protocol::McpServer::Stdio(h) => h.name.clone(),
                _ => return None,
            };
            Some(AgentMcpServerEntry {
                uses_relay: relay_mcp_server_names.contains(&name),
                name,
                server: s.clone(),
            })
        })
        .collect()
}

/// 把 ACP MCP server 列表转为 RuntimeMcpServer(供 context_builder 消费)。
pub fn acp_mcp_servers_to_runtime(
    servers: &[agent_client_protocol::McpServer],
) -> Vec<RuntimeMcpServer> {
    servers
        .iter()
        .filter_map(|server| match server {
            agent_client_protocol::McpServer::Http(http) => Some(RuntimeMcpServer::Http {
                name: http.name.clone(),
                url: http.url.clone(),
            }),
            agent_client_protocol::McpServer::Sse(sse) => Some(RuntimeMcpServer::Http {
                name: sse.name.clone(),
                url: sse.url.clone(),
            }),
            _ => None,
        })
        .collect()
}

// ═══════════════════════════════════════════════════════════════════
// SECTION 4:Owner Bootstrap(Story / Project / Routine 共用)
// ═══════════════════════════════════════════════════════════════════

/// Owner 级 session bootstrap 的 owner scope 描述。
pub enum OwnerScope<'a> {
    Story {
        story: &'a Story,
        project: &'a Project,
        workspace: Option<&'a Workspace>,
    },
    Project {
        project: &'a Project,
        workspace: Option<&'a Workspace>,
        agent_id: Option<Uuid>,
        agent_display_name: String,
        preset_name: Option<String>,
    },
}

impl<'a> OwnerScope<'a> {
    fn project_id(&self) -> Uuid {
        match self {
            Self::Story { project, .. } | Self::Project { project, .. } => project.id,
        }
    }

    fn owner_ctx(&self) -> SessionOwnerCtx {
        match self {
            Self::Story { project, story, .. } => SessionOwnerCtx::Story {
                project_id: project.id,
                story_id: story.id,
            },
            Self::Project { project, .. } => SessionOwnerCtx::Project {
                project_id: project.id,
            },
        }
    }

    fn mount_target(&self) -> SessionMountTarget {
        match self {
            Self::Story { .. } => SessionMountTarget::Story,
            Self::Project { .. } => SessionMountTarget::Project,
        }
    }

    fn agent_id(&self) -> Option<Uuid> {
        match self {
            Self::Project { agent_id, .. } => *agent_id,
            _ => None,
        }
    }
}

/// agent 级 MCP 配置(来自 project_agent / routine agent context)。
#[derive(Default, Clone)]
pub struct AgentLevelMcp {
    pub preset_mcp_servers: Vec<agent_client_protocol::McpServer>,
    pub relay_mcp_server_names: HashSet<String>,
}

/// Owner bootstrap compose 的完整输入。
pub struct OwnerBootstrapSpec<'a> {
    pub owner: OwnerScope<'a>,
    pub executor_config: AgentConfig,
    /// user 层 prompt blocks(外部传入或 Routine 模板)。
    pub user_prompt_blocks: Vec<serde_json::Value>,
    pub agent_mcp: AgentLevelMcp,
    /// 前端/request 已携带的 MCP server(透传)。
    pub request_mcp_servers: Vec<agent_client_protocol::McpServer>,
    /// 前端已携带的 VFS(None 时 assembler 自行构建)。
    pub existing_vfs: Option<Vfs>,
    pub visible_canvas_mount_ids: Vec<String>,
    pub agent_declared_capabilities: Option<Vec<String>>,
    /// Session lifecycle 三态判定结果,决定 context bundle / prompt_blocks 组装方式。
    pub lifecycle: OwnerPromptLifecycle,
    /// 审计总线用于索引的 session key（SessionHub 分配的 `sess-<ms>-<short>`）。
    ///
    /// 为 `None` 时跳过审计 emit（例如 session 尚未创建的 bootstrap 路径）。
    pub audit_session_key: Option<String>,
}

/// Owner bootstrap 阶段 session_hub 判定出的 prompt lifecycle 模式,决定 compose
/// 如何组装 context bundle + prompt_blocks + hook_snapshot_reload。
///
/// 与 `SessionPromptLifecycle` 结构等价,但这里只暴露 compose 所需的 3 个分支,
/// continuation bundle(来自 SessionHub)由调用方在 Spec 里预先算好传入。
pub enum OwnerPromptLifecycle {
    /// owner 首次启动,需要把 owner 上下文 Bundle 注入并包到 prompt blocks。
    OwnerBootstrap,
    /// 已有 repository，compose 使用预构建的 continuation bundle（当 connector
    /// 不支持原生 repository restore 时）或直接复用 owner context bundle
    /// （当 connector 支持原生消息历史恢复时）。
    RepositoryRehydrate {
        /// 由 SessionHub 预先把历史事件渲染成 continuation Bundle，用于不支持
        /// `supports_repository_restore` 的 connector。
        prebuilt_continuation_bundle: Option<SessionContextBundle>,
        /// 是否把 owner context bundle 也一并附加（true = 继续用 owner bundle；
        /// false = 只用 prebuilt_continuation_bundle）。
        include_owner_bundle: bool,
    },
    /// 普通 turn,无 owner bootstrap。
    Plain,
}

/// Owner 级 session 的上下文 Contribution 组装 —— Story 与 Project 各走自己的 contribute_*。
fn build_owner_context_contribution(
    owner: &OwnerScope<'_>,
    vfs: Option<&Vfs>,
    mcp_servers: &[RuntimeMcpServer],
    effective_agent_type: &str,
    workspace_source_fragments: Vec<agentdash_spi::ContextFragment>,
    workspace_source_warnings: Vec<String>,
) -> Contribution {
    match owner {
        OwnerScope::Story {
            story,
            project,
            workspace,
        } => contribute_story_context(StoryContextBuildInput {
            story,
            project,
            workspace: *workspace,
            vfs,
            mcp_servers,
            effective_agent_type: Some(effective_agent_type),
            workspace_source_fragments,
            workspace_source_warnings,
        }),
        OwnerScope::Project {
            project,
            workspace,
            agent_display_name,
            preset_name,
            ..
        } => contribute_project_context(ProjectContextBuildInput {
            project,
            workspace: workspace.as_deref(),
            vfs,
            mcp_servers,
            effective_agent_type: Some(effective_agent_type),
            preset_name: preset_name.as_deref(),
            agent_display_name,
        }),
    }
}

/// Owner bootstrap 场景下把 `ContextBuildPhase` 映射到 Session 级的 phase 标签。
fn owner_scope_phase(owner: &OwnerScope<'_>) -> ContextBuildPhase {
    match owner {
        OwnerScope::Story { .. } => ContextBuildPhase::StoryOwner,
        OwnerScope::Project { .. } => ContextBuildPhase::ProjectAgent,
    }
}

impl<'a> SessionRequestAssembler<'a> {
    pub fn new(
        vfs_service: &'a RelayVfsService,
        canvas_repo: &'a dyn CanvasRepository,
        availability: &'a dyn BackendAvailability,
        repos: &'a RepositorySet,
        platform_config: &'a PlatformConfig,
    ) -> Self {
        Self {
            vfs_service,
            canvas_repo,
            availability,
            repos,
            platform_config,
            audit_bus: None,
        }
    }

    /// 配置审计总线（生产路径由 `AppState` 注入）。
    pub fn with_audit_bus(mut self, bus: SharedContextAuditBus) -> Self {
        self.audit_bus = Some(bus);
        self
    }

    /// 若存在审计总线且 session_key 可用，则把 bundle 的所有 fragment 批量 emit。
    ///
    /// `session_key` 应由调用方（spec.audit_session_key）提供，对应 SessionHub 分配的
    /// `sess-<ms>-<short>` 字符串 ID。若为 `None`（例如 owner bootstrap 创建新 session 时
    /// 尚未分配 ID 的场景），跳过 emit。
    fn audit_bundle(
        &self,
        bundle: &agentdash_spi::SessionContextBundle,
        session_key: Option<&str>,
        trigger: AuditTrigger,
    ) {
        let (Some(bus), Some(session_key)) = (self.audit_bus.as_deref(), session_key) else {
            return;
        };
        emit_bundle_fragments(bus, bundle, session_key, trigger);
    }

    /// Owner 级 session bootstrap(Story / Project / Routine)。
    pub async fn compose_owner_bootstrap(
        &self,
        spec: OwnerBootstrapSpec<'_>,
    ) -> Result<PreparedSessionInputs, String> {
        let project_id = spec.owner.project_id();
        let owner_ctx = spec.owner.owner_ctx();

        // ── 1. VFS 构建 + canvas 挂载 ──
        let mut vfs = match spec.existing_vfs {
            Some(vfs) => Some(vfs),
            None => {
                let target = spec.owner.mount_target();
                let built = match &spec.owner {
                    OwnerScope::Story {
                        story,
                        project,
                        workspace,
                    } => self.vfs_service.build_vfs(
                        project,
                        Some(*story),
                        *workspace,
                        target,
                        Some(spec.executor_config.executor.as_str()),
                    )?,
                    OwnerScope::Project {
                        project, workspace, ..
                    } => self.vfs_service.build_vfs(
                        project,
                        None,
                        *workspace,
                        target,
                        Some(spec.executor_config.executor.as_str()),
                    )?,
                };
                Some(built)
            }
        };
        if let Some(space) = vfs.as_mut() {
            append_visible_canvas_mounts(
                self.canvas_repo,
                project_id,
                space,
                &spec.visible_canvas_mount_ids,
            )
            .await
            .map_err(|e| e.to_string())?;
        }

        // ── 2. workflow 上下文解析 → 能力集 ──
        let workflow_directives =
            resolve_owner_workflow_capability_directives(self.repos, &spec.owner).await;
        let workflow_ctx = match workflow_directives {
            Some(directives) => SessionWorkflowContext {
                has_active_workflow: true,
                workflow_capability_directives: Some(directives),
            },
            None => SessionWorkflowContext::NONE,
        };

        // ── 3. CapabilityResolver ──
        let cap_input = CapabilityResolverInput {
            owner_ctx,
            agent_declared_capabilities: spec.agent_declared_capabilities,
            workflow_ctx,
            agent_mcp_servers: extract_agent_mcp_entries(
                &spec.agent_mcp.preset_mcp_servers,
                &spec.agent_mcp.relay_mcp_server_names,
            ),
            available_presets: load_available_presets(self.repos, project_id).await,
            companion_slice_mode: None,
        };
        let cap_output = CapabilityResolver::resolve(&cap_input, self.platform_config);

        // ── 4. MCP server 列表汇总(request + platform + custom + preset) ──
        let mut effective_mcp_servers = spec.request_mcp_servers;
        for config in &cap_output.platform_mcp_configs {
            effective_mcp_servers.push(config.to_acp_mcp_server());
        }
        effective_mcp_servers.extend(cap_output.custom_mcp_servers.iter().cloned());
        effective_mcp_servers.extend(spec.agent_mcp.preset_mcp_servers.iter().cloned());
        let mut relay_mcp_server_names = spec.agent_mcp.relay_mcp_server_names.clone();
        relay_mcp_server_names.extend(cap_output.custom_relay_mcp_server_names.iter().cloned());

        // ── 5. Context markdown 生成 ──
        let runtime_mcp_servers = acp_mcp_servers_to_runtime(&effective_mcp_servers);
        let runtime_vfs = vfs.clone();

        let (workspace_fragments, workspace_warnings) = match &spec.owner {
            OwnerScope::Story {
                story, workspace, ..
            } => {
                let resolved = resolve_workspace_declared_sources(
                    self.availability,
                    self.vfs_service,
                    &story.context.source_refs,
                    *workspace,
                    60,
                )
                .await?;
                (resolved.fragments, resolved.warnings)
            }
            OwnerScope::Project { .. } => (Vec::new(), Vec::new()),
        };

        let owner_contribution = build_owner_context_contribution(
            &spec.owner,
            runtime_vfs.as_ref(),
            &runtime_mcp_servers,
            spec.executor_config.executor.as_str(),
            workspace_fragments,
            workspace_warnings,
        );

        // ── 5b. 聚合 Contribution → Bundle ──
        let bundle_session_id = Uuid::new_v4();
        let bundle_phase = owner_scope_phase(&spec.owner);
        let context_bundle = build_session_context_bundle(
            SessionContextConfig {
                session_id: bundle_session_id,
                phase: bundle_phase,
                default_scope: agentdash_spi::ContextFragment::default_scope(),
            },
            vec![owner_contribution],
        );
        self.audit_bundle(
            &context_bundle,
            spec.audit_session_key.as_deref(),
            AuditTrigger::SessionBootstrap,
        );

        // ── 6. Prompt lifecycle 三态 → bundle / prompt_blocks / hook_snapshot_reload ──
        //
        // - OwnerBootstrap：使用新建的 owner context bundle
        // - RepositoryRehydrate：根据 connector 能力，使用 continuation bundle 或 owner bundle
        // - Plain：不附加 bundle
        let (prompt_blocks, hook_snapshot_reload, effective_bundle) = match spec.lifecycle {
            OwnerPromptLifecycle::OwnerBootstrap => (
                spec.user_prompt_blocks,
                HookSnapshotReloadTrigger::Reload,
                Some(context_bundle),
            ),
            OwnerPromptLifecycle::RepositoryRehydrate {
                prebuilt_continuation_bundle,
                include_owner_bundle,
            } => {
                let chosen_bundle = prebuilt_continuation_bundle.or_else(|| {
                    if include_owner_bundle {
                        Some(context_bundle)
                    } else {
                        None
                    }
                });
                (
                    spec.user_prompt_blocks,
                    HookSnapshotReloadTrigger::None,
                    chosen_bundle,
                )
            }
            OwnerPromptLifecycle::Plain => {
                (spec.user_prompt_blocks, HookSnapshotReloadTrigger::None, None)
            }
        };

        let effective_capability_keys: BTreeSet<String> = cap_output
            .effective_capabilities
            .iter()
            .map(|c| c.key().to_string())
            .collect();

        let workspace_defaults = match &spec.owner {
            OwnerScope::Story { workspace, .. } => workspace.cloned(),
            OwnerScope::Project { workspace, .. } => workspace.as_deref().cloned(),
        };

        let mut builder = SessionAssemblyBuilder::new()
            .with_prompt_blocks(prompt_blocks)
            .with_executor_config(spec.executor_config)
            .with_mcp_servers(effective_mcp_servers)
            .append_relay_mcp_names(relay_mcp_server_names)
            .with_resolved_capabilities(cap_output.flow_capabilities, effective_capability_keys)
            .with_hook_snapshot_reload(hook_snapshot_reload)
            .with_optional_workspace_defaults(workspace_defaults)
            .with_optional_context_bundle(effective_bundle);

        if let Some(vfs) = vfs {
            builder = builder.with_vfs(vfs);
        }

        Ok(builder.build())
    }

    /// Story step activation 场景下组装 child session。
    ///
    /// 内部走 6 个阶段:
    /// 1. 解析 executor config（来源诊断保留给 tracing/metadata）
    /// 2. 查找活跃 lifecycle run 对应的 `ActiveWorkflowProjection`（由调用方传入）
    /// 3. 构建 VFS（workspace mount + lifecycle mount，cloud-native 场景）
    /// 4. 解析 context bindings（需要 VFS 已就绪）
    /// 5. CapabilityResolver（以 workflow baseline 或空集为输入）
    /// 6. 组装 `Vec<Contribution>` → `build_session_context_bundle` 产出 bundle 与 prompt resource block
    ///
    /// 输出统一为 `PreparedSessionInputs`；调用方通过 `finalize_request` 合入 base
    /// `PromptSessionRequest` 后交 `session_hub.start_prompt` 派发。
    pub async fn compose_story_step(
        &self,
        spec: StoryStepSpec<'_>,
    ) -> Result<PreparedSessionInputs, TaskExecutionError> {
        // ── 1. 解析 executor config ──
        use crate::session::ExecutorResolution;
        use crate::task::config::{resolve_task_executor_config, resolve_task_executor_source};

        let executor_source = resolve_task_executor_source(
            spec.task,
            spec.project,
            spec.explicit_executor_config.as_ref(),
        );
        let (resolved_config, _executor_resolution) = match resolve_task_executor_config(
            spec.explicit_executor_config.clone(),
            spec.task,
            spec.project,
        ) {
            Ok(config) => (config, ExecutorResolution::resolved(executor_source)),
            Err(err) if spec.strict_config_resolution => return Err(err),
            Err(err) => (
                None,
                ExecutorResolution::failed(executor_source, err.to_string()),
            ),
        };

        let effective_agent_type = resolved_config.as_ref().map(|c| c.executor.as_str());
        let use_cloud_native = resolved_config
            .as_ref()
            .is_some_and(|c| c.is_cloud_native());

        let workflow = spec.active_workflow.clone();

        // ── 3. VFS(workspace + lifecycle mount) ──
        let vfs = if use_cloud_native {
            let mut space = self
                .vfs_service
                .build_vfs(
                    spec.project,
                    Some(spec.story),
                    spec.workspace,
                    SessionMountTarget::Task,
                    effective_agent_type,
                )
                .map_err(|error| TaskExecutionError::Internal(error.to_string()))?;

            if let Some(active_workflow) = workflow.as_ref() {
                let writable_port_keys: Vec<String> = active_workflow
                    .active_step
                    .output_ports
                    .iter()
                    .map(|p| p.key.clone())
                    .collect();
                space.mounts.push(build_lifecycle_mount_with_ports(
                    active_workflow.run.id,
                    &active_workflow.lifecycle.key,
                    &writable_port_keys,
                ));
            }
            Some(space)
        } else {
            None
        };

        // ── 4. 解析 context bindings(需要 vfs 已就绪) ──
        let resolved_bindings = match (&vfs, &workflow) {
            (Some(space), Some(wf)) => {
                let bindings = wf
                    .active_contract()
                    .map(|c| c.injection.context_bindings.as_slice())
                    .unwrap_or(&[]);
                if bindings.is_empty() {
                    None
                } else {
                    Some(
                        resolve_context_bindings(bindings, space, self.vfs_service)
                            .await
                            .map_err(TaskExecutionError::UnprocessableEntity)?,
                    )
                }
            }
            _ => None,
        };

        // ── 5. CapabilityResolver(走 workflow baseline 或空集) ──
        let workflow_directives = workflow.as_ref().and_then(|p| {
            p.primary_workflow
                .as_ref()
                .map(capability_directives_from_active_workflow)
        });
        let workflow_ctx = SessionWorkflowContext {
            has_active_workflow: workflow.is_some(),
            workflow_capability_directives: workflow_directives,
        };
        let cap_input = CapabilityResolverInput {
            owner_ctx: SessionOwnerCtx::Task {
                project_id: spec.task.project_id,
                story_id: spec.task.story_id,
                task_id: spec.task.id,
            },
            agent_declared_capabilities: None,
            workflow_ctx,
            agent_mcp_servers: vec![],
            available_presets: load_available_presets(self.repos, spec.task.project_id).await,
            companion_slice_mode: None,
        };
        let cap_output = CapabilityResolver::resolve(&cap_input, self.platform_config);

        let platform_mcp_configs = cap_output.platform_mcp_configs.clone();
        let relay_mcp_server_names: HashSet<String> = cap_output
            .custom_relay_mcp_server_names
            .iter()
            .cloned()
            .collect();
        let flow_capabilities = cap_output.flow_capabilities.clone();
        let effective_capability_keys: BTreeSet<String> = cap_output
            .effective_capabilities
            .iter()
            .map(|c| c.key().to_string())
            .collect();

        // ── 6. 构造 task agent context（Bundle 路径） ──
        let (story_ref, project_ref, workspace_ref) = (spec.story, spec.project, spec.workspace);

        let mut declared_sources = story_ref.context.source_refs.clone();
        declared_sources.extend(spec.task.agent_binding.context_sources.clone());
        let resolved_workspace_sources = resolve_workspace_declared_sources(
            self.availability,
            self.vfs_service,
            &declared_sources,
            workspace_ref,
            86,
        )
        .await
        .map_err(TaskExecutionError::UnprocessableEntity)?;

        let task_phase = match spec.phase {
            StoryStepPhase::Start => TaskExecutionPhase::Start,
            StoryStepPhase::Continue => TaskExecutionPhase::Continue,
        };

        // 按依赖倒置：调用方聚合 Vec<Contribution>，builder 只做合并。
        let mut contributions: Vec<Contribution> = Vec::new();
        contributions.push(contribute_core_context(
            spec.task,
            story_ref,
            project_ref,
            workspace_ref,
        ));
        contributions.push(contribute_binding_initial_context(spec.task));
        contributions.push(contribute_declared_sources(spec.task, story_ref));
        if !resolved_workspace_sources.fragments.is_empty() {
            contributions.push(contribute_workspace_static_sources(
                resolved_workspace_sources.fragments.clone(),
            ));
        }
        if !resolved_workspace_sources.warnings.is_empty() {
            contributions.push(Contribution::fragments_only(vec![
                build_declared_source_warning_fragment(
                    "declared_source_warnings",
                    96,
                    &resolved_workspace_sources.warnings,
                ),
            ]));
        }
        let mut task_mcp_servers: Vec<crate::runtime::RuntimeMcpServer> = Vec::new();
        for mcp_config in &platform_mcp_configs {
            let contrib = contribute_mcp(mcp_config);
            task_mcp_servers.extend(contrib.mcp_servers.iter().cloned());
            contributions.push(contrib);
        }
        if let (Some(wf), Some(bindings_out)) = (workflow.clone(), resolved_bindings.clone()) {
            contributions.push(contribute_workflow_binding(&wf, &bindings_out));
        }
        contributions.push(contribute_instruction(
            spec.task,
            story_ref,
            workspace_ref,
            task_phase,
            spec.override_prompt,
            spec.additional_prompt,
        ));

        // session plan fragments（vfs / tools / persona / workflow / runtime_policy）
        let effective_session_composition =
            crate::session::plan::resolve_story_session_composition(Some(story_ref));
        let session_plan = crate::session::plan::build_session_plan_fragments(
            crate::session::plan::SessionPlanInput {
                owner_ctx: SessionOwnerCtx::Task {
                    project_id: project_ref.id,
                    story_id: story_ref.id,
                    task_id: spec.task.id,
                },
                phase: match task_phase {
                    TaskExecutionPhase::Start => crate::session::plan::SessionPlanPhase::TaskStart,
                    TaskExecutionPhase::Continue => {
                        crate::session::plan::SessionPlanPhase::TaskContinue
                    }
                },
                vfs: vfs.as_ref(),
                mcp_servers: &task_mcp_servers,
                session_composition: effective_session_composition.as_ref(),
                agent_type: effective_agent_type,
                preset_name: spec.task.agent_binding.preset_name.as_deref(),
                has_custom_prompt_template: spec
                    .task
                    .agent_binding
                    .prompt_template
                    .as_deref()
                    .is_some_and(|value| !value.trim().is_empty()),
                has_initial_context: spec
                    .task
                    .agent_binding
                    .initial_context
                    .as_deref()
                    .is_some_and(|value| !value.trim().is_empty()),
                workspace_attached: vfs.is_some(),
            },
        );
        contributions.push(Contribution::fragments_only(session_plan.fragments));

        let context_bundle = build_session_context_bundle(
            SessionContextConfig {
                session_id: Uuid::new_v4(),
                phase: match task_phase {
                    TaskExecutionPhase::Start => ContextBuildPhase::TaskStart,
                    TaskExecutionPhase::Continue => ContextBuildPhase::TaskContinue,
                },
                default_scope: agentdash_spi::ContextFragment::default_scope(),
            },
            contributions,
        );
        self.audit_bundle(
            &context_bundle,
            spec.audit_session_key.as_deref(),
            AuditTrigger::ComposerRebuild,
        );

        // Task 的业务上下文只进入 context_bundle/system prompt。这里保留一个非空
        // turn trigger，避免把完整 owner context 再渲染进用户消息和标题生成输入。
        let prompt_blocks = build_story_step_trigger_prompt_blocks(task_phase);

        let source_summary: Vec<String> = context_bundle
            .iter_fragments()
            .map(|f| format!("{}({})", f.label, f.slot))
            .collect();

        let working_dir = workspace_ref.map(|_| ".".to_string());

        // ── 汇总 MCP 列表：platform + custom + contribution 产出 ──
        let mut effective_mcp_servers: Vec<agent_client_protocol::McpServer> = platform_mcp_configs
            .iter()
            .map(|c| c.to_acp_mcp_server())
            .collect();
        effective_mcp_servers.extend(cap_output.custom_mcp_servers.iter().cloned());
        effective_mcp_servers.extend(crate::runtime_bridge::runtime_mcp_servers_to_acp(
            &task_mcp_servers,
        ));

        let mut builder = SessionAssemblyBuilder::new()
            .with_prompt_blocks(prompt_blocks)
            .with_mcp_servers(effective_mcp_servers)
            .append_relay_mcp_names(relay_mcp_server_names)
            .with_resolved_capabilities(flow_capabilities, effective_capability_keys)
            .with_context_bundle(context_bundle)
            .with_working_dir(working_dir)
            .with_source_summary(source_summary)
            .with_optional_workspace_defaults(workspace_ref.cloned());

        if let Some(vfs) = vfs {
            builder = builder.with_vfs(vfs);
        }
        if let Some(cfg) = resolved_config {
            builder = builder.with_executor_config(cfg);
        }

        Ok(builder.build())
    }

    /// Workflow AgentNode session 激活(orchestrator 创建子 session 的场景)。
    ///
    /// 内部调 `activate_step_with_platform` → kickoff prompt + VFS mount + caps + MCP。
    pub async fn compose_lifecycle_node(
        &self,
        spec: LifecycleNodeSpec<'_>,
    ) -> Result<PreparedSessionInputs, String> {
        compose_lifecycle_node_with_audit(
            self.repos,
            self.platform_config,
            spec,
            self.audit_bus.clone(),
            None,
        )
        .await
    }

    /// Companion 子 session(父 session slice 继承场景)。
    ///
    /// 纯同步函数 —— 不做 IO,只做父 session 的 vfs/mcp 子集 + 能力裁剪。
    pub fn compose_companion(&self, spec: CompanionSpec<'_>) -> PreparedSessionInputs {
        compose_companion(spec)
    }
}

/// Workflow AgentNode session 激活(脱离 `SessionRequestAssembler`,方便 orchestrator
/// 等不持有完整 service 集合的调用方使用)。
///
/// 内部委托给 `SessionAssemblyBuilder::apply_lifecycle_activation`。
pub async fn compose_lifecycle_node(
    repos: &RepositorySet,
    platform_config: &PlatformConfig,
    spec: LifecycleNodeSpec<'_>,
) -> Result<PreparedSessionInputs, String> {
    compose_lifecycle_node_with_audit(repos, platform_config, spec, None, None).await
}

pub async fn compose_lifecycle_node_with_audit(
    repos: &RepositorySet,
    platform_config: &PlatformConfig,
    spec: LifecycleNodeSpec<'_>,
    audit_bus: Option<SharedContextAuditBus>,
    audit_session_key: Option<&str>,
) -> Result<PreparedSessionInputs, String> {
    let owner_ctx = SessionOwnerCtx::Project {
        project_id: spec.run.project_id,
    };

    let port_output_map = load_port_output_map(repos.inline_file_repo.as_ref(), spec.run.id).await;
    let ready_port_keys: BTreeSet<String> = port_output_map.keys().cloned().collect();

    let activation = activate_step_with_platform(
        &StepActivationInput {
            owner_ctx,
            active_step: spec.step,
            workflow: spec.workflow,
            run_id: spec.run.id,
            lifecycle_key: &spec.lifecycle.key,
            edges: &spec.lifecycle.edges,
            agent_declared_capabilities: None,
            agent_mcp_servers: vec![],
            available_presets: load_available_presets(repos, spec.run.project_id).await,
            companion_slice_mode: None,
            baseline_override: None,
            capability_directives: &[],
            ready_port_keys: ready_port_keys.clone(),
        },
        platform_config,
    );

    let context_bundle = build_session_context_bundle(
        SessionContextConfig {
            session_id: Uuid::new_v4(),
            phase: ContextBuildPhase::LifecycleNode,
            default_scope: agentdash_spi::ContextFragment::default_scope(),
        },
        vec![contribute_lifecycle_context(
            &spec,
            &activation,
            &ready_port_keys,
        )],
    );
    if let (Some(bus), Some(session_key)) = (audit_bus.as_ref(), audit_session_key) {
        emit_bundle_fragments(
            bus.as_ref(),
            &context_bundle,
            session_key,
            AuditTrigger::ComposerRebuild,
        );
    }
    let source_summary = context_bundle
        .iter_fragments()
        .map(|fragment| format!("{}({})", fragment.label, fragment.slot))
        .collect::<Vec<_>>();

    Ok(SessionAssemblyBuilder::new()
        .apply_lifecycle_activation(&activation, spec.inherited_executor_config)
        .with_context_bundle(context_bundle)
        .with_source_summary(source_summary)
        .build())
}

fn contribute_lifecycle_context(
    spec: &LifecycleNodeSpec<'_>,
    activation: &crate::workflow::StepActivation,
    ready_port_keys: &BTreeSet<String>,
) -> Contribution {
    let mut fragments = Vec::new();

    let step_desc = spec.step.description.trim();
    let workflow_label = spec
        .workflow
        .map(|workflow| format!("`{}` ({})", workflow.key, workflow.name))
        .unwrap_or_else(|| "未绑定 workflow".to_string());
    let mut lifecycle_lines = vec![
        format!("- Lifecycle: `{}`", spec.lifecycle.key),
        format!("- Run: `{}`", spec.run.id),
        format!("- Step: `{}`", spec.step.key),
        format!("- Node type: `{:?}`", spec.step.node_type),
        format!("- Workflow: {workflow_label}"),
    ];
    if !step_desc.is_empty() {
        lifecycle_lines.push(format!("- Step description: {step_desc}"));
    }
    if ready_port_keys.is_empty() {
        lifecycle_lines.push("- Ready input ports: 无".to_string());
    } else {
        lifecycle_lines.push(format!(
            "- Ready input ports: {}",
            ready_port_keys
                .iter()
                .map(|key| format!("`{key}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    fragments.push(agentdash_spi::ContextFragment {
        slot: "workflow_context".to_string(),
        label: "lifecycle_node_context".to_string(),
        order: 80,
        strategy: agentdash_spi::MergeStrategy::Append,
        scope: agentdash_spi::ContextFragment::default_scope(),
        source: "lifecycle:activation".to_string(),
        content: format!("## Lifecycle Node\n{}", lifecycle_lines.join("\n")),
    });

    if let Some(workflow) = spec.workflow {
        let injection = &workflow.contract.injection;
        let mut parts = Vec::new();
        if let Some(goal) = injection
            .goal
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            parts.push(format!("## Workflow Goal\n{goal}"));
        }
        if !injection.instructions.is_empty() {
            parts.push(format!(
                "## Workflow Instructions\n{}",
                injection
                    .instructions
                    .iter()
                    .filter_map(|item| {
                        let trimmed = item.trim();
                        (!trimmed.is_empty()).then(|| format!("- {trimmed}"))
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            ));
        }
        if !injection.context_bindings.is_empty() {
            parts.push(format!(
                "## Workflow Context Bindings\n{}",
                injection
                    .context_bindings
                    .iter()
                    .map(|binding| {
                        let title = binding
                            .title
                            .as_deref()
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                            .unwrap_or(binding.locator.as_str());
                        let required = if binding.required {
                            "required"
                        } else {
                            "optional"
                        };
                        format!(
                            "- `{}` ({required}) — {}: {}",
                            binding.locator, title, binding.reason
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            ));
        }
        let content = parts.join("\n\n");
        if !content.trim().is_empty() {
            fragments.push(agentdash_spi::ContextFragment {
                slot: "workflow_context".to_string(),
                label: "lifecycle_workflow_injection".to_string(),
                order: 83,
                strategy: agentdash_spi::MergeStrategy::Append,
                scope: agentdash_spi::ContextFragment::default_scope(),
                source: "lifecycle:workflow_injection".to_string(),
                content,
            });
        }
    }

    let mut runtime_parts = vec![format!(
        "## Lifecycle Runtime Policy\n{}\n\n完成当前节点后调用 `complete_lifecycle_node` 提交总结与产物。",
        activation.kickoff_prompt.title_line
    )];
    if !activation.kickoff_prompt.output_section.trim().is_empty() {
        runtime_parts.push(activation.kickoff_prompt.output_section.trim().to_string());
    }
    if !activation.kickoff_prompt.input_section.trim().is_empty() {
        runtime_parts.push(activation.kickoff_prompt.input_section.trim().to_string());
    }
    if !activation.capability_keys.is_empty() {
        runtime_parts.push(format!(
            "## Effective Capabilities\n{}",
            activation
                .capability_keys
                .iter()
                .map(|key| format!("- `{key}`"))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    fragments.push(agentdash_spi::ContextFragment {
        slot: "runtime_policy".to_string(),
        label: "lifecycle_runtime_policy".to_string(),
        order: 84,
        strategy: agentdash_spi::MergeStrategy::Append,
        scope: agentdash_spi::ContextFragment::default_scope(),
        source: "lifecycle:runtime_policy".to_string(),
        content: runtime_parts.join("\n\n"),
    });

    Contribution::fragments_only(fragments)
}

/// Companion 子 session 组装(脱离 `SessionRequestAssembler`,companion tool
/// 在父 session 作用域内即可完成,不需要 assembler 的完整服务依赖)。
///
/// 内部委托给 `SessionAssemblyBuilder::apply_companion_slice`。
pub fn compose_companion(spec: CompanionSpec<'_>) -> PreparedSessionInputs {
    SessionAssemblyBuilder::new()
        .apply_companion_slice(
            spec.parent_vfs,
            spec.parent_mcp_servers,
            spec.parent_context_bundle,
            spec.slice_mode,
            spec.companion_executor_config,
            spec.dispatch_prompt,
        )
        .build()
}

/// `companion::tools::CompanionSliceMode` → `capability::resolver::CompanionSliceMode`。
///
/// 两个 enum 在字段上等价(Full / Compact / WorkflowOnly / ConstraintsOnly),
/// 历史上为避免循环依赖分裂成两处。此 mapper 是桥接。后续清理可统一到单一定义。
fn map_slice_mode(mode: CompanionSliceMode) -> crate::capability::CompanionSliceMode {
    match mode {
        CompanionSliceMode::Full => crate::capability::CompanionSliceMode::Full,
        CompanionSliceMode::Compact => crate::capability::CompanionSliceMode::Compact,
        CompanionSliceMode::WorkflowOnly => crate::capability::CompanionSliceMode::WorkflowOnly,
        CompanionSliceMode::ConstraintsOnly => {
            crate::capability::CompanionSliceMode::ConstraintsOnly
        }
    }
}

fn build_story_step_trigger_prompt_blocks(phase: TaskExecutionPhase) -> Vec<serde_json::Value> {
    let text = match phase {
        TaskExecutionPhase::Start => "请开始执行当前任务。",
        TaskExecutionPhase::Continue => "请继续推进当前任务。",
    };
    vec![serde_json::json!({
        "type": "text",
        "text": text,
    })]
}

// ═══════════════════════════════════════════════════════════════════
// SECTION 5:其余 Spec 结构 + 辅助函数
// ═══════════════════════════════════════════════════════════════════

/// Story step activation 的 phase(与 `crate::task::execution::ExecutionPhase` 映射)。
#[derive(Debug, Clone, Copy)]
pub enum StoryStepPhase {
    Start,
    Continue,
}

/// Story step 场景下 compose 所需的完整上下文。
///
/// 用于 `StoryStepActivationService` facade 的 step activation 路径
/// （`start_task` / `continue_task` 内部先定位 task 对应 step，再调 compose）。
///
/// 与 `LifecycleNodeSpec`（orchestrator 的 phase node 使用）不同：
/// - `StoryStepSpec` 持有 task/story/project/workspace 完整 entity 引用
/// - 承载 user prompt 注入（`override_prompt` / `additional_prompt`）
/// - 承载 explicit executor config（HTTP 请求透传）
/// - 承载 `ActiveWorkflowProjection`（由 facade 通过 SessionBinding 两跳定位后传入）
pub struct StoryStepSpec<'a> {
    pub run: &'a LifecycleRun,
    pub lifecycle: &'a LifecycleDefinition,
    pub step: &'a LifecycleStepDefinition,
    pub task: &'a Task,
    pub story: &'a Story,
    pub project: &'a Project,
    pub workspace: Option<&'a Workspace>,
    pub phase: StoryStepPhase,
    pub override_prompt: Option<&'a str>,
    pub additional_prompt: Option<&'a str>,
    pub explicit_executor_config: Option<AgentConfig>,
    /// 若为 true,executor 解析失败时直接返回 Err;否则返回 failed 状态继续。
    pub strict_config_resolution: bool,
    /// 对应活跃 lifecycle run 的投影（由 facade 通过 SessionBinding 两跳定位后传入）。
    pub active_workflow: Option<ActiveWorkflowProjection>,
    /// 审计总线用于索引的 session key。
    pub audit_session_key: Option<String>,
}

/// Lifecycle AgentNode compose 输入。
pub struct LifecycleNodeSpec<'a> {
    pub run: &'a LifecycleRun,
    pub lifecycle: &'a LifecycleDefinition,
    pub step: &'a LifecycleStepDefinition,
    pub workflow: Option<&'a agentdash_domain::workflow::WorkflowDefinition>,
    pub inherited_executor_config: Option<AgentConfig>,
}

/// Companion compose 输入。
pub struct CompanionSpec<'a> {
    pub parent_vfs: Option<&'a Vfs>,
    pub parent_mcp_servers: &'a [agent_client_protocol::McpServer],
    /// 父 session 的结构化上下文 Bundle，companion 直接继承（按 slice_mode 过滤）。
    pub parent_context_bundle: Option<&'a SessionContextBundle>,
    pub slice_mode: CompanionSliceMode,
    pub companion_executor_config: AgentConfig,
    pub dispatch_prompt: String,
}

/// Companion + Workflow 组合 compose 输入。
pub struct CompanionWorkflowSpec<'a> {
    pub companion: CompanionSpec<'a>,
    /// 已创建的 lifecycle run。
    pub run: &'a LifecycleRun,
    pub lifecycle: &'a LifecycleDefinition,
    pub step: &'a LifecycleStepDefinition,
    pub workflow: Option<&'a agentdash_domain::workflow::WorkflowDefinition>,
}

/// Companion + Workflow 返回值。
pub struct CompanionWorkflowOutput {
    pub prepared: PreparedSessionInputs,
    pub activation: crate::workflow::StepActivation,
}

/// Companion + Workflow 组合组装。
///
/// 基于 companion VFS slice 叠加 lifecycle mount 和 workflow 能力/MCP，
/// 通过 `SessionAssemblyBuilder` 声明式组合两个关注点。
pub async fn compose_companion_with_workflow(
    repos: &RepositorySet,
    platform_config: &PlatformConfig,
    spec: CompanionWorkflowSpec<'_>,
) -> Result<CompanionWorkflowOutput, String> {
    use crate::companion::tools::build_companion_execution_slice;

    let project_id = spec.run.project_id;
    let comp = &spec.companion;

    // ── 1. Companion VFS slice 作为基础 ──
    let slice =
        build_companion_execution_slice(comp.parent_vfs, comp.parent_mcp_servers, comp.slice_mode);

    // ── 2. Workflow step activation（产出 lifecycle mount + 能力 + MCP） ──
    let owner_ctx = SessionOwnerCtx::Project { project_id };
    let port_output_map = load_port_output_map(repos.inline_file_repo.as_ref(), spec.run.id).await;
    let ready_port_keys: BTreeSet<String> = port_output_map.keys().cloned().collect();

    let activation = activate_step_with_platform(
        &StepActivationInput {
            owner_ctx,
            active_step: spec.step,
            workflow: spec.workflow,
            run_id: spec.run.id,
            lifecycle_key: &spec.lifecycle.key,
            edges: &spec.lifecycle.edges,
            agent_declared_capabilities: None,
            agent_mcp_servers: vec![],
            available_presets: load_available_presets(repos, project_id).await,
            companion_slice_mode: Some(map_slice_mode(comp.slice_mode)),
            baseline_override: None,
            capability_directives: &[],
            ready_port_keys,
        },
        platform_config,
    );

    // ── 3. 用 builder 组合 companion + workflow 两个层 ──
    let mut vfs = slice.vfs.unwrap_or_default();
    vfs.mounts.push(activation.lifecycle_mount.clone());

    // 继承父 bundle 并叠加 workflow injection 片段。workflow injection 作为独立
    // fragment 注入 Bundle，替代旧的字符串拼接路径。
    let mut merged_bundle = comp.parent_context_bundle.cloned();
    if let Some(workflow) = spec.workflow {
        let inj = &workflow.contract.injection;
        let mut parts: Vec<String> = Vec::new();
        if let Some(goal) = &inj.goal {
            if !goal.trim().is_empty() {
                parts.push(format!("## Workflow Goal\n{goal}"));
            }
        }
        if !inj.instructions.is_empty() {
            parts.push(format!(
                "## Workflow Instructions\n{}",
                inj.instructions
                    .iter()
                    .map(|i| format!("- {i}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            ));
        }
        let workflow_content = parts.join("\n\n");
        if !workflow_content.is_empty() {
            let workflow_fragment = agentdash_spi::ContextFragment {
                slot: "workflow_context".to_string(),
                label: "companion_workflow_injection".to_string(),
                order: 83,
                strategy: agentdash_spi::MergeStrategy::Append,
                scope: agentdash_spi::ContextFragment::default_scope(),
                source: "companion:workflow_injection".to_string(),
                content: workflow_content,
            };
            match merged_bundle.as_mut() {
                Some(bundle) => bundle.upsert_by_slot(workflow_fragment),
                None => {
                    let mut bundle = agentdash_spi::SessionContextBundle::new(
                        Uuid::new_v4(),
                        ContextBuildPhase::Companion.as_tag(),
                    );
                    bundle.upsert_by_slot(workflow_fragment);
                    merged_bundle = Some(bundle);
                }
            }
        }
    }

    let prompt_blocks = vec![serde_json::json!({
        "type": "text",
        "text": comp.dispatch_prompt,
    })];

    let prepared = SessionAssemblyBuilder::new()
        .with_vfs(vfs)
        .with_resolved_capabilities(
            activation.flow_capabilities.clone(),
            activation.capability_keys.clone(),
        )
        .with_mcp_servers(slice.mcp_servers)
        .append_mcp_servers(activation.mcp_servers.clone())
        .with_optional_context_bundle(merged_bundle)
        .with_prompt_blocks(prompt_blocks)
        .with_executor_config(comp.companion_executor_config.clone())
        .with_hook_snapshot_reload(HookSnapshotReloadTrigger::Reload)
        .build();

    Ok(CompanionWorkflowOutput {
        prepared,
        activation,
    })
}

// ═══════════════════════════════════════════════════════════════════
// SECTION 6:内部 helper
// ═══════════════════════════════════════════════════════════════════

/// Owner bootstrap 阶段解析 workflow capability directives(来自默认 agent_link → lifecycle → entry step workflow)。
///
/// Story owner 找 project 内 `is_default_for_story=true` 的 agent_link;
/// Project owner 用 (project_id, agent_id) 直接查 agent_link。
/// 找不到任何绑定返回 None。
async fn resolve_owner_workflow_capability_directives(
    repos: &RepositorySet,
    owner: &OwnerScope<'_>,
) -> Option<Vec<CapabilityDirective>> {
    let project_id = owner.project_id();

    // 1. 找到关联的 agent_link
    let link_opt = match owner {
        OwnerScope::Project { .. } => {
            let agent_id = owner.agent_id()?;
            repos
                .agent_link_repo
                .find_by_project_and_agent(project_id, agent_id)
                .await
                .ok()
                .flatten()
        }
        OwnerScope::Story { .. } => repos
            .agent_link_repo
            .list_by_project(project_id)
            .await
            .ok()
            .and_then(|links| links.into_iter().find(|l| l.is_default_for_story)),
    };
    let link = link_opt?;
    let lifecycle_key = link
        .default_lifecycle_key
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())?;

    // 2. 查 lifecycle 定义 → entry step → workflow_key
    let lifecycle = repos
        .lifecycle_definition_repo
        .get_by_project_and_key(project_id, lifecycle_key)
        .await
        .ok()
        .flatten()?;
    let entry_step = lifecycle
        .steps
        .iter()
        .find(|s| s.key == lifecycle.entry_step_key)?;
    let workflow_key = entry_step.effective_workflow_key()?;

    // 3. 查 workflow 定义 → contract.capability_directives
    let workflow = repos
        .workflow_definition_repo
        .get_by_project_and_key(project_id, workflow_key)
        .await
        .ok()
        .flatten()?;

    Some(capability_directives_from_active_workflow(&workflow))
}

// 允许未使用的 re-export,为 PR4-B..F 保留扩展点
#[allow(unused_imports)]
use crate::workflow::KickoffPromptFragment as _Kickoff;
#[allow(unused_imports)]
use crate::workflow::StepActivation as _StepActivation;

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::workflow::{
        InputPortDefinition, LifecycleDefinition, LifecycleStepDefinition, OutputPortDefinition,
        WorkflowBindingKind, WorkflowContract, WorkflowDefinition, WorkflowDefinitionSource,
        WorkflowInjectionSpec,
    };
    use std::collections::BTreeSet;

    #[test]
    fn story_step_trigger_prompt_does_not_embed_owner_context() {
        for phase in [TaskExecutionPhase::Start, TaskExecutionPhase::Continue] {
            let blocks = build_story_step_trigger_prompt_blocks(phase);
            let text = blocks
                .iter()
                .filter_map(|block| block.get("text").and_then(serde_json::Value::as_str))
                .collect::<Vec<_>>()
                .join("\n");

            assert!(!text.trim().is_empty());
            assert!(!text.contains("## Task"));
            assert!(!text.contains("## Story"));
            assert!(!text.contains("## Project"));
            assert!(!text.contains("## Instruction"));
            assert!(!text.contains("agentdash://task-context"));
        }
    }

    #[test]
    fn lifecycle_context_contribution_contains_workflow_and_runtime_fragments() {
        let project_id = Uuid::new_v4();
        let step = LifecycleStepDefinition {
            key: "implement".to_string(),
            description: "实现功能".to_string(),
            workflow_key: Some("wf_impl".to_string()),
            node_type: Default::default(),
            output_ports: vec![OutputPortDefinition {
                key: "summary".to_string(),
                description: "实现摘要".to_string(),
                gate_strategy: Default::default(),
                gate_params: None,
            }],
            input_ports: vec![InputPortDefinition {
                key: "design".to_string(),
                description: "设计方案".to_string(),
                context_strategy: Default::default(),
                context_template: None,
                standalone_fulfillment: Default::default(),
            }],
        };
        let lifecycle = LifecycleDefinition::new(
            project_id,
            "dev",
            "Dev",
            "dev lifecycle",
            WorkflowBindingKind::Story,
            WorkflowDefinitionSource::BuiltinSeed,
            "implement",
            vec![step.clone()],
            vec![],
        )
        .expect("lifecycle");
        let run = agentdash_domain::workflow::LifecycleRun::new(
            project_id,
            lifecycle.id,
            "sess-story",
            &lifecycle.steps,
            &lifecycle.entry_step_key,
            &lifecycle.edges,
        )
        .expect("run");
        let workflow = WorkflowDefinition::new(
            project_id,
            "wf_impl",
            "Implementation",
            "实现工作流",
            WorkflowBindingKind::Story,
            WorkflowDefinitionSource::BuiltinSeed,
            WorkflowContract {
                injection: WorkflowInjectionSpec {
                    goal: Some("交付可验证实现".to_string()),
                    instructions: vec!["保持上下文收口".to_string()],
                    context_bindings: vec![],
                },
                ..WorkflowContract::default()
            },
        )
        .expect("workflow");
        let mount = crate::vfs::build_lifecycle_mount_with_ports(
            run.id,
            &lifecycle.key,
            &["summary".into()],
        );
        let activation = crate::workflow::StepActivation {
            flow_capabilities: Default::default(),
            mcp_servers: vec![],
            capability_keys: BTreeSet::from(["workflow_management".to_string()]),
            kickoff_prompt: crate::workflow::KickoffPromptFragment {
                title_line: "你正在执行 lifecycle `dev` 的 node `implement`。".to_string(),
                output_section: "## 必须交付的产出\n- `summary`".to_string(),
                input_section: "## 输入上下文\n- `design`".to_string(),
            },
            lifecycle_mount: mount.clone(),
            lifecycle_vfs: Vfs {
                mounts: vec![mount],
                default_mount_id: None,
                source_project_id: None,
                source_story_id: None,
                links: Vec::new(),
            },
        };

        let spec = LifecycleNodeSpec {
            run: &run,
            lifecycle: &lifecycle,
            step: &step,
            workflow: Some(&workflow),
            inherited_executor_config: None,
        };
        let contribution =
            contribute_lifecycle_context(&spec, &activation, &BTreeSet::from(["design".into()]));
        let bundle = build_session_context_bundle(
            SessionContextConfig {
                session_id: Uuid::new_v4(),
                phase: ContextBuildPhase::LifecycleNode,
                default_scope: agentdash_spi::ContextFragment::default_scope(),
            },
            vec![contribution],
        );
        let rendered = bundle.render_section(
            agentdash_spi::FragmentScope::RuntimeAgent,
            agentdash_spi::RUNTIME_AGENT_CONTEXT_SLOTS,
        );

        assert!(rendered.contains("## Lifecycle Node"));
        assert!(rendered.contains("交付可验证实现"));
        assert!(rendered.contains("complete_lifecycle_node"));
        assert!(rendered.contains("workflow_management"));
    }

    // ═══════════════════════════════════════════════════════════
    // finalize_request 合并语义回归测试
    // ═══════════════════════════════════════════════════════════
    //
    // 这些测试锁定 `finalize_request` 对称化后的行为（2026-04-30）：
    // - mcp_servers / relay_mcp_server_names 统一整体替换；
    // - vfs 语义三分支等价于"prepared 非空则覆盖"；
    // - workspace_defaults 顺序保持"先回填、再被 prepared.vfs 覆盖"。

    mod finalize_request_tests {
        use super::super::*;
        use crate::session::UserPromptInput;
        use agent_client_protocol::{McpServer as AcpMcpServer, McpServerHttp};
        use agentdash_domain::workspace::{
            Workspace, WorkspaceIdentityKind, WorkspaceResolutionPolicy,
        };
        use agentdash_spi::Vfs;
        use std::collections::HashSet;

        fn minimal_workspace() -> Workspace {
            // 没有 binding 的最小 Workspace —— `build_workspace_vfs` 会失败被 `.ok()`
            // 吞掉，保证 vfs 保持 None；足够验证 working_dir 默认回填那一支。
            Workspace::new(
                uuid::Uuid::nil(),
                "test-ws".to_string(),
                WorkspaceIdentityKind::LocalDir,
                serde_json::json!({}),
                WorkspaceResolutionPolicy::PreferDefaultBinding,
            )
        }

        fn base_req() -> PromptSessionRequest {
            PromptSessionRequest::from_user_input(UserPromptInput::from_text("ping"))
        }

        fn http_server(name: &str, url: &str) -> AcpMcpServer {
            AcpMcpServer::Http(McpServerHttp::new(name.to_string(), url.to_string()))
        }

        fn server_name(server: &AcpMcpServer) -> Option<&str> {
            match server {
                AcpMcpServer::Http(h) => Some(h.name.as_str()),
                AcpMcpServer::Sse(s) => Some(s.name.as_str()),
                AcpMcpServer::Stdio(s) => Some(s.name.as_str()),
                _ => None,
            }
        }

        #[test]
        fn mcp_servers_prepared_overrides_base() {
            // base 和 prepared 都有值时，prepared 整体替换 base —— compose 内部已汇总。
            let mut base = base_req();
            base.mcp_servers = vec![http_server("base_only", "http://base")];

            let prepared = PreparedSessionInputs {
                mcp_servers: vec![
                    http_server("compose_a", "http://a"),
                    http_server("compose_b", "http://b"),
                ],
                ..Default::default()
            };

            let result = finalize_request(base, prepared);
            let names: Vec<&str> = result.mcp_servers.iter().filter_map(server_name).collect();
            assert_eq!(names, vec!["compose_a", "compose_b"]);
        }

        #[test]
        fn mcp_servers_prepared_empty_still_replaces() {
            // 对称化后 prepared 为空时也会整体替换（compose 显式产出空集视为"没有 MCP"）。
            let mut base = base_req();
            base.mcp_servers = vec![http_server("base_only", "http://base")];
            let prepared = PreparedSessionInputs::default();

            let result = finalize_request(base, prepared);
            assert!(result.mcp_servers.is_empty());
        }

        #[test]
        fn relay_mcp_server_names_prepared_overrides_base() {
            // 对称化关键点：relay_mcp_server_names 不再 extend，而是替换，与 mcp_servers 一致。
            let mut base = base_req();
            base.relay_mcp_server_names = HashSet::from(["base_only".to_string()]);

            let prepared = PreparedSessionInputs {
                relay_mcp_server_names: HashSet::from([
                    "compose_a".to_string(),
                    "compose_b".to_string(),
                ]),
                ..Default::default()
            };

            let result = finalize_request(base, prepared);
            assert_eq!(
                result.relay_mcp_server_names,
                HashSet::from(["compose_a".to_string(), "compose_b".to_string()]),
                "base 的 relay name 应被 prepared 替换而非叠加"
            );
        }

        #[test]
        fn relay_mcp_server_names_empty_prepared_clears_base() {
            // 与 mcp_servers 行为对称：空 prepared 也会清空。
            let mut base = base_req();
            base.relay_mcp_server_names = HashSet::from(["base_only".to_string()]);
            let prepared = PreparedSessionInputs::default();

            let result = finalize_request(base, prepared);
            assert!(result.relay_mcp_server_names.is_empty());
        }

        #[test]
        fn vfs_prepared_some_overrides_base() {
            // base 已有 vfs、prepared 也有 vfs → 以 prepared 为准（保留 compose 的 mount 组合）。
            let mut base = base_req();
            base.vfs = Some(Vfs {
                mounts: Vec::new(),
                default_mount_id: Some("base-mount".to_string()),
                source_project_id: None,
                source_story_id: None,
                links: Vec::new(),
            });
            let prepared = PreparedSessionInputs {
                vfs: Some(Vfs {
                    mounts: Vec::new(),
                    default_mount_id: Some("prepared-mount".to_string()),
                    source_project_id: None,
                    source_story_id: None,
                    links: Vec::new(),
                }),
                ..Default::default()
            };

            let result = finalize_request(base, prepared);
            assert_eq!(
                result.vfs.and_then(|v| v.default_mount_id),
                Some("prepared-mount".to_string()),
            );
        }

        #[test]
        fn vfs_prepared_none_preserves_base() {
            // base 有 vfs、prepared 没有 → 保留 base（不强制清空）。
            let mut base = base_req();
            base.vfs = Some(Vfs {
                mounts: Vec::new(),
                default_mount_id: Some("base-mount".to_string()),
                source_project_id: None,
                source_story_id: None,
                links: Vec::new(),
            });
            let prepared = PreparedSessionInputs::default();

            let result = finalize_request(base, prepared);
            assert_eq!(
                result.vfs.and_then(|v| v.default_mount_id),
                Some("base-mount".to_string()),
            );
        }

        #[test]
        fn workspace_defaults_fills_working_dir_when_absent() {
            // base 与 prepared 都无 working_dir，workspace_defaults 非空 → 回填 "."。
            // （vfs 由于 minimal workspace 没 binding 会保持 None，不是本测试关注点。）
            let base = base_req();
            let prepared = PreparedSessionInputs {
                workspace_defaults: Some(minimal_workspace()),
                ..Default::default()
            };

            let result = finalize_request(base, prepared);
            assert_eq!(result.user_input.working_dir.as_deref(), Some("."));
        }

        #[test]
        fn prepared_working_dir_overrides_workspace_default() {
            // prepared.working_dir = Some(X) 覆盖 apply_workspace_defaults 回填的 "."。
            let base = base_req();
            let prepared = PreparedSessionInputs {
                workspace_defaults: Some(minimal_workspace()),
                working_dir: Some("packages/foo".to_string()),
                ..Default::default()
            };

            let result = finalize_request(base, prepared);
            assert_eq!(
                result.user_input.working_dir.as_deref(),
                Some("packages/foo"),
                "prepared.working_dir 应覆盖 workspace default 的 \".\""
            );
        }

        #[test]
        fn prompt_blocks_prepared_overrides_base() {
            let mut base = base_req();
            base.user_input.prompt_blocks =
                Some(vec![serde_json::json!({ "type": "text", "text": "base" })]);
            let prepared = PreparedSessionInputs {
                prompt_blocks: Some(vec![
                    serde_json::json!({ "type": "text", "text": "compose" }),
                ]),
                ..Default::default()
            };

            let result = finalize_request(base, prepared);
            let texts: Vec<&str> = result
                .user_input
                .prompt_blocks
                .as_ref()
                .unwrap()
                .iter()
                .filter_map(|b| b.get("text").and_then(serde_json::Value::as_str))
                .collect();
            assert_eq!(texts, vec!["compose"]);
        }

        #[test]
        fn prompt_blocks_prepared_none_preserves_base() {
            let mut base = base_req();
            base.user_input.prompt_blocks =
                Some(vec![serde_json::json!({ "type": "text", "text": "base" })]);
            let prepared = PreparedSessionInputs::default();

            let result = finalize_request(base, prepared);
            let texts: Vec<&str> = result
                .user_input
                .prompt_blocks
                .as_ref()
                .unwrap()
                .iter()
                .filter_map(|b| b.get("text").and_then(serde_json::Value::as_str))
                .collect();
            assert_eq!(texts, vec!["base"]);
        }

        #[test]
        fn context_bundle_prepared_overrides_base() {
            // Bundle 为 Option 整体替换语义：prepared = None 也会清掉 base。
            use agentdash_spi::SessionContextBundle;

            let mut base = base_req();
            base.context_bundle = Some(SessionContextBundle::new(
                uuid::Uuid::new_v4(),
                "test-base",
            ));
            // prepared 为 None 时整体替换：base bundle 被清除
            let prepared = PreparedSessionInputs::default();

            let result = finalize_request(base, prepared);
            assert!(
                result.context_bundle.is_none(),
                "context_bundle 为整体替换字段，prepared=None 会清除 base"
            );
        }

        // ═══════════════════════════════════════════════════════════
        // PR 1 Phase 1c 新字段测试：identity / post_turn_handler / env
        // ═══════════════════════════════════════════════════════════

        #[test]
        fn identity_prepared_overrides_base() {
            // prepared.identity = Some → 覆盖 base.identity。
            use agentdash_spi::auth::{AuthIdentity, AuthMode};

            let mut base = base_req();
            base.identity = Some(AuthIdentity {
                auth_mode: AuthMode::Personal,
                user_id: "base-user".to_string(),
                subject: "base-user".to_string(),
                display_name: None,
                email: None,
                groups: Vec::new(),
                is_admin: false,
                provider: None,
                extra: serde_json::Value::Null,
            });
            let prepared = PreparedSessionInputs {
                identity: Some(AuthIdentity::system_routine("r-1234")),
                ..Default::default()
            };

            let result = finalize_request(base, prepared);
            let id = result.identity.expect("identity exists");
            assert_eq!(id.user_id, "system:routine:r-1234");
            assert_eq!(id.provider.as_deref(), Some("system.routine"));
            assert!(!id.is_admin);
        }

        #[test]
        fn identity_prepared_none_preserves_base() {
            // prepared.identity = None → 保留 base.identity（不会清空）。
            use agentdash_spi::auth::{AuthIdentity, AuthMode};

            let mut base = base_req();
            let base_id = AuthIdentity {
                auth_mode: AuthMode::Enterprise,
                user_id: "alice".to_string(),
                subject: "alice".to_string(),
                display_name: Some("Alice".to_string()),
                email: None,
                groups: Vec::new(),
                is_admin: false,
                provider: None,
                extra: serde_json::Value::Null,
            };
            base.identity = Some(base_id);

            let prepared = PreparedSessionInputs::default();
            let result = finalize_request(base, prepared);
            assert_eq!(
                result.identity.as_ref().map(|i| i.user_id.as_str()),
                Some("alice"),
                "prepared.identity=None 时 base.identity 应被保留"
            );
        }

        #[test]
        fn env_prepared_overrides_base_when_nonempty() {
            // prepared.env 非空 → 整体替换。
            let mut base = base_req();
            base.user_input.env.insert("FOO".to_string(), "base".to_string());

            let mut prepared_env = HashMap::new();
            prepared_env.insert("BAR".to_string(), "prepared".to_string());
            let prepared = PreparedSessionInputs {
                env: prepared_env,
                ..Default::default()
            };

            let result = finalize_request(base, prepared);
            assert!(!result.user_input.env.contains_key("FOO"));
            assert_eq!(
                result.user_input.env.get("BAR").map(String::as_str),
                Some("prepared")
            );
        }

        #[test]
        fn env_prepared_empty_preserves_base() {
            // prepared.env 为空 → 保留 base.env。
            let mut base = base_req();
            base.user_input.env.insert("FOO".to_string(), "base".to_string());

            let prepared = PreparedSessionInputs::default();
            let result = finalize_request(base, prepared);
            assert_eq!(
                result.user_input.env.get("FOO").map(String::as_str),
                Some("base"),
                "prepared.env 为空时 base.env 应被保留"
            );
        }

        #[test]
        fn system_routine_identity_shape() {
            // 固化 AuthIdentity::system_routine 产出形状（E1 契约）。
            let id = agentdash_spi::auth::AuthIdentity::system_routine("r-abc");
            assert_eq!(id.user_id, "system:routine:r-abc");
            assert_eq!(id.subject, "system:routine:r-abc");
            assert_eq!(id.provider.as_deref(), Some("system.routine"));
            assert!(!id.is_admin);
            assert!(id.groups.is_empty());
            assert_eq!(id.display_name.as_deref(), Some("System Routine"));
            // auth_mode = Personal 避免匹配企业级 admin 策略
            assert!(matches!(
                id.auth_mode,
                agentdash_spi::auth::AuthMode::Personal
            ));
        }

        #[test]
        fn builder_with_identity_method_propagates_to_prepared() {
            // 验证 SessionAssemblyBuilder.with_identity() 的值能顺利进入 PreparedSessionInputs.identity。
            let id = agentdash_spi::auth::AuthIdentity::system_routine("r-zzz");
            let prepared = SessionAssemblyBuilder::new().with_identity(id.clone()).build();
            assert_eq!(
                prepared.identity.as_ref().map(|i| i.user_id.as_str()),
                Some("system:routine:r-zzz"),
            );
        }

        #[test]
        fn builder_with_user_input_unpacks_fields() {
            // 验证 with_user_input 一次性吸收 UserPromptInput 的四字段。
            use crate::session::UserPromptInput;
            let mut env = HashMap::new();
            env.insert("PATH".to_string(), "/usr/bin".to_string());

            let input = UserPromptInput {
                prompt_blocks: Some(vec![serde_json::json!({ "type": "text", "text": "hi" })]),
                working_dir: Some("subdir".to_string()),
                env,
                executor_config: None,
            };
            let prepared = SessionAssemblyBuilder::new().with_user_input(input).build();
            assert_eq!(
                prepared.working_dir.as_deref(),
                Some("subdir"),
                "with_user_input 应把 UserPromptInput.working_dir 写入 builder.working_dir"
            );
            assert!(
                prepared.prompt_blocks.is_some(),
                "with_user_input 应把 prompt_blocks 写入 builder"
            );
            assert_eq!(
                prepared.env.get("PATH").map(String::as_str),
                Some("/usr/bin"),
                "with_user_input 应把 env 写入 builder"
            );
        }
    }
}
