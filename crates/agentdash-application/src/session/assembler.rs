//! `SessionRequestAssembler` — 统一 session 启动请求组装。
//!
//! ## 设计
//!
//! 代码库里一共有 5 条 session 启动路径,此前各自手写 bootstrap 逻辑:
//!
//! | 路径 | 实现入口 |
//! |---|---|
//! | ACP Story/Project | `api::routes::acp_sessions` → `SessionRequestAssembler::compose_owner_bootstrap` |
//! | Task runtime | `task::service::TaskLifecycleService::activate_story_step` → `SessionRequestAssembler::compose_story_step` |
//! | Routine | `routine::executor::build_project_agent_prompt_request` → `SessionRequestAssembler::compose_owner_bootstrap`(带 trigger tag) |
//! | Workflow AgentNode | `workflow::orchestrator::start_agent_node_prompt` → `compose_lifecycle_node` |
//! | Companion | `companion::tools` → `compose_companion` |
//!
//! 5 条路径共享 4 个"策略轴":owner scope mount / system_context 生成 /
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

use std::collections::{BTreeSet, HashSet};

use agentdash_domain::canvas::CanvasRepository;
use agentdash_domain::common::AgentConfig;
use agentdash_domain::project::Project;
use agentdash_domain::session_binding::SessionOwnerCtx;
use agentdash_domain::story::Story;
use agentdash_domain::task::Task;
use agentdash_domain::workflow::CapabilityDirective;
use agentdash_domain::workflow::{LifecycleDefinition, LifecycleRun, LifecycleStepDefinition};
use agentdash_domain::workspace::Workspace;
use agentdash_spi::{FlowCapabilities, Vfs};
use uuid::Uuid;

use crate::canvas::append_visible_canvas_mounts;
use crate::capability::{
    AgentMcpServerEntry, AvailableMcpPresets, CapabilityResolver, CapabilityResolverInput,
    SessionWorkflowContext, capability_directives_from_active_workflow,
};
use crate::companion::tools::CompanionSliceMode;
use crate::context::{
    ContextContributor, ContextContributorRegistry, McpContextContributor,
    StaticFragmentsContributor, TaskAgentBuildInput, TaskExecutionPhase,
    WorkflowContextBindingsContributor, build_declared_source_warning_fragment,
    build_task_agent_context, resolve_workspace_declared_sources,
};
use crate::platform_config::PlatformConfig;
use crate::project::context_builder::{ProjectContextBuildInput, build_project_context_markdown};
use crate::repository_set::RepositorySet;
use crate::runtime::RuntimeMcpServer;
use crate::session::context::apply_workspace_defaults;
use crate::session::types::{PromptSessionRequest, SessionBootstrapAction};
use crate::story::context_builder::{StoryContextBuildInput, build_story_context_markdown};
use crate::task::execution::TaskExecutionError;
use crate::vfs::{
    RelayVfsService, SessionMountTarget, build_lifecycle_mount_with_ports,
    resolve_context_bindings,
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
/// 用户输入(prompt_blocks / working_dir / env)不放这里——它沿着
/// 原始 `PromptSessionRequest` 透传,compose 仅改写 `prompt_blocks` 与 `executor_config`。
#[derive(Debug, Clone, Default)]
pub struct PreparedSessionInputs {
    pub prompt_blocks: Option<Vec<serde_json::Value>>,
    pub executor_config: Option<AgentConfig>,
    pub mcp_servers: Vec<agent_client_protocol::McpServer>,
    pub relay_mcp_server_names: HashSet<String>,
    pub vfs: Option<Vfs>,
    pub flow_capabilities: Option<FlowCapabilities>,
    pub effective_capability_keys: Option<BTreeSet<String>>,
    pub system_context: Option<String>,
    pub bootstrap_action: SessionBootstrapAction,
    /// workspace 默认值注入的数据源(session 中 vfs/working_dir 回填用)。
    pub workspace_defaults: Option<Workspace>,
    /// Story step(task 启动)场景下 compose 产出的 working_dir 覆盖值；
    /// 仅在需要绕过 workspace_defaults 的 task 启动路径使用。
    pub working_dir: Option<String>,
    /// Context contributor pipeline 的诊断摘要（供 StartedTurn.context_sources）。
    pub source_summary: Vec<String>,
}

/// 把 `PreparedSessionInputs` 合并进一个 base `PromptSessionRequest`。
///
/// - user_input.prompt_blocks 若 compose 有产出则覆盖,否则保留原值
/// - vfs 若 compose 有产出则使用,否则用 workspace_defaults 填充
/// - 其余字段直接覆盖
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
    req.system_context = prepared.system_context;
    req.bootstrap_action = prepared.bootstrap_action;

    apply_workspace_defaults(
        &mut req.user_input.working_dir,
        &mut req.vfs,
        prepared.workspace_defaults.as_ref(),
    );
    if let Some(wd) = prepared.working_dir {
        req.user_input.working_dir = Some(wd);
    }
    if req.vfs.is_none() {
        req.vfs = prepared.vfs;
    } else if prepared.vfs.is_some() {
        // base.vfs 已有(前端透传场景)但 compose 也产出了 —— 以 compose 为准
        // 保证 compose 的 workspace/canvas/lifecycle mount 组合不被覆盖
        req.vfs = prepared.vfs;
    }
    req.mcp_servers = prepared.mcp_servers;
    req.relay_mcp_server_names
        .extend(prepared.relay_mcp_server_names);
    req.flow_capabilities = prepared.flow_capabilities;
    req.effective_capability_keys = prepared.effective_capability_keys;
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
#[derive(Debug, Clone, Default)]
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
    system_context: Option<String>,

    // ── Prompt 层 ──
    prompt_blocks: Option<Vec<serde_json::Value>>,
    executor_config: Option<AgentConfig>,
    bootstrap_action: SessionBootstrapAction,

    // ── 元信息层 ──
    workspace_defaults: Option<Workspace>,

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
    pub fn with_companion_vfs(mut self, parent_vfs: Option<&Vfs>, mode: CompanionSliceMode) -> Self {
        use crate::companion::tools::build_companion_execution_slice;
        let slice = build_companion_execution_slice(parent_vfs, &[], mode);
        self.vfs = slice.vfs;
        self
    }

    /// 在已有 VFS 上追加 lifecycle mount（task runtime 场景）。
    pub fn append_lifecycle_mount(
        mut self,
        run_id: Uuid,
        lifecycle_key: &str,
        writable_port_keys: &[String],
    ) -> Self {
        if let Some(space) = self.vfs.as_mut() {
            space
                .mounts
                .push(build_lifecycle_mount_with_ports(run_id, lifecycle_key, writable_port_keys));
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
    pub fn append_mcp_servers(mut self, servers: impl IntoIterator<Item = agent_client_protocol::McpServer>) -> Self {
        self.mcp_servers.extend(servers);
        self
    }

    /// 追加 relay MCP server name 集合。
    pub fn append_relay_mcp_names(mut self, names: impl IntoIterator<Item = String>) -> Self {
        self.relay_mcp_server_names.extend(names);
        self
    }

    // ── 系统上下文层方法 ──────────────────────────────────────────

    /// 设置 system_context 字符串。
    pub fn with_system_context(mut self, context: String) -> Self {
        self.system_context = Some(context);
        self
    }

    /// 可选设置 system_context。
    pub fn with_optional_system_context(mut self, context: Option<String>) -> Self {
        self.system_context = context;
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
    pub fn with_bootstrap_action(mut self, action: SessionBootstrapAction) -> Self {
        self.bootstrap_action = action;
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

    // ── 复合便利方法 ──────────────────────────────────────────────

    /// 一步完成 companion slice 装配（VFS + MCP + 能力 + prompt + bootstrap）。
    pub fn apply_companion_slice(
        self,
        parent_vfs: Option<&Vfs>,
        parent_mcp_servers: &[agent_client_protocol::McpServer],
        parent_system_context: Option<&str>,
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
            system_context: parent_system_context.map(|s| s.to_string()),
            prompt_blocks: Some(prompt_blocks),
            executor_config: Some(executor_config),
            bootstrap_action: SessionBootstrapAction::OwnerContext,
            workspace_defaults: None,
            working_dir: None,
            source_summary: Vec::new(),
        }
    }

    /// 一步完成 lifecycle node 装配（VFS + 能力 + MCP + prompt）。
    pub fn apply_lifecycle_activation(
        mut self,
        activation: &crate::workflow::StepActivation,
        inherited_executor_config: Option<AgentConfig>,
    ) -> Self {
        let kickoff_prompt = activation.kickoff_prompt.to_default_prompt();
        self.vfs = Some(activation.lifecycle_vfs.clone());
        self.flow_capabilities = Some(activation.flow_capabilities.clone());
        self.effective_capability_keys = Some(activation.capability_keys.clone());
        self.mcp_servers = activation.mcp_servers.clone();
        self.prompt_blocks = Some(vec![serde_json::json!({
            "type": "text",
            "text": kickoff_prompt,
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
            system_context: self.system_context,
            bootstrap_action: self.bootstrap_action,
            workspace_defaults: self.workspace_defaults,
            working_dir: self.working_dir,
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
    pub contributor_registry: &'a ContextContributorRegistry,
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
    /// Session lifecycle 三态判定结果,决定 system_context / prompt_blocks 组装方式。
    pub lifecycle: OwnerPromptLifecycle,
}

/// Owner bootstrap 阶段 session_hub 判定出的 prompt lifecycle 模式,决定 compose
/// 如何组装 system_context + prompt_blocks + bootstrap_action。
///
/// 与 `SessionPromptLifecycle` 结构等价,但这里只暴露 compose 所需的 3 个分支,
/// continuation system_context(来自 SessionHub)由调用方在 Spec 里预先算好传入。
pub enum OwnerPromptLifecycle {
    /// owner 首次启动,需要把 context_markdown 注入 system_context 并包到 prompt blocks。
    OwnerBootstrap,
    /// 已有 repository,compose 直接返回 continuation system_context 或 context_markdown。
    RepositoryRehydrate {
        prebuilt_continuation_system_context: Option<String>,
        include_markdown_as_system_context: bool,
    },
    /// 普通 turn,无 owner bootstrap。
    Plain,
}

/// Owner context markdown 的装配方式——Story 与 Project 各走自己的 builder。
fn build_owner_context_markdown_sync(
    owner: &OwnerScope<'_>,
    vfs: Option<&Vfs>,
    mcp_servers: &[RuntimeMcpServer],
    effective_agent_type: &str,
    workspace_source_fragments: Vec<agentdash_spi::ContextFragment>,
    workspace_source_warnings: Vec<String>,
) -> String {
    match owner {
        OwnerScope::Story {
            story,
            project,
            workspace,
        } => {
            let (md, _) = build_story_context_markdown(StoryContextBuildInput {
                story,
                project,
                workspace: *workspace,
                vfs,
                mcp_servers,
                effective_agent_type: Some(effective_agent_type),
                workspace_source_fragments,
                workspace_source_warnings,
            });
            md
        }
        OwnerScope::Project {
            project,
            workspace,
            agent_display_name,
            preset_name,
            ..
        } => {
            let (md, _) = build_project_context_markdown(ProjectContextBuildInput {
                project,
                workspace: workspace.as_deref(),
                vfs,
                mcp_servers,
                effective_agent_type: Some(effective_agent_type),
                preset_name: preset_name.as_deref(),
                agent_display_name,
            });
            md
        }
    }
}

/// 把 Story/Project 各自的 owner-bootstrap prompt 包裹(外层系统语境 + 用户 blocks)。
fn wrap_owner_bootstrap_blocks(
    owner: &OwnerScope<'_>,
    system_markdown: &str,
    user_blocks: Vec<serde_json::Value>,
) -> Vec<serde_json::Value> {
    match owner {
        OwnerScope::Story { story, .. } => {
            crate::story::context_builder::build_story_owner_prompt_blocks(
                story.id,
                system_markdown.to_string(),
                user_blocks,
            )
        }
        OwnerScope::Project { project, .. } => {
            crate::project::context_builder::build_project_owner_prompt_blocks(
                project.id,
                system_markdown.to_string(),
                user_blocks,
            )
        }
    }
}

impl<'a> SessionRequestAssembler<'a> {
    pub fn new(
        vfs_service: &'a RelayVfsService,
        canvas_repo: &'a dyn CanvasRepository,
        availability: &'a dyn BackendAvailability,
        repos: &'a RepositorySet,
        platform_config: &'a PlatformConfig,
        contributor_registry: &'a ContextContributorRegistry,
    ) -> Self {
        Self {
            vfs_service,
            canvas_repo,
            availability,
            repos,
            platform_config,
            contributor_registry,
        }
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

        let context_markdown = build_owner_context_markdown_sync(
            &spec.owner,
            runtime_vfs.as_ref(),
            &runtime_mcp_servers,
            spec.executor_config.executor.as_str(),
            workspace_fragments,
            workspace_warnings,
        );

        // ── 6. Prompt lifecycle 三态 → system_context + prompt_blocks + bootstrap_action ──
        let (system_context, prompt_blocks, bootstrap_action) = match spec.lifecycle {
            OwnerPromptLifecycle::OwnerBootstrap => (
                Some(context_markdown.clone()),
                wrap_owner_bootstrap_blocks(
                    &spec.owner,
                    &context_markdown,
                    spec.user_prompt_blocks,
                ),
                SessionBootstrapAction::OwnerContext,
            ),
            OwnerPromptLifecycle::RepositoryRehydrate {
                prebuilt_continuation_system_context,
                include_markdown_as_system_context,
            } => {
                let sys_ctx = if prebuilt_continuation_system_context.is_some() {
                    prebuilt_continuation_system_context
                } else if include_markdown_as_system_context {
                    Some(context_markdown.clone())
                } else {
                    None
                };
                (
                    sys_ctx,
                    spec.user_prompt_blocks,
                    SessionBootstrapAction::None,
                )
            }
            OwnerPromptLifecycle::Plain => {
                (None, spec.user_prompt_blocks, SessionBootstrapAction::None)
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
            .with_optional_system_context(system_context)
            .with_bootstrap_action(bootstrap_action)
            .with_optional_workspace_defaults(workspace_defaults);

        if let Some(vfs) = vfs {
            builder = builder.with_vfs(vfs);
        }

        Ok(builder.build())
    }

    /// Story step(task 启动)场景下组装 session — 合并原 `compose_task_runtime`
    /// 与 `build_task_session_runtime_inputs` 两处重复。
    ///
    /// 内部走 6 个阶段:
    /// 1. 解析 executor config（来源诊断保留给 tracing/metadata）
    /// 2. 查找活跃 lifecycle run 对应的 `ActiveWorkflowProjection`（由调用方传入）
    /// 3. 构建 VFS（workspace mount + lifecycle mount，cloud-native 场景）
    /// 4. 解析 context bindings（需要 VFS 已就绪）
    /// 5. CapabilityResolver（以 workflow baseline 或空集为输入）
    /// 6. build_task_agent_context（走 contributor pipeline 产出 prompt / system context）
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

        // ── 6. 构造 task agent context(走 contributor pipeline) ──
        let (story_ref, project_ref, workspace_ref) = (spec.story, spec.project, spec.workspace);
        let mut extra_contributors: Vec<Box<dyn ContextContributor>> = Vec::new();

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

        if !resolved_workspace_sources.fragments.is_empty() {
            extra_contributors.push(Box::new(StaticFragmentsContributor::new(
                resolved_workspace_sources.fragments,
            )));
        }
        if !resolved_workspace_sources.warnings.is_empty() {
            extra_contributors.push(Box::new(StaticFragmentsContributor::new(vec![
                build_declared_source_warning_fragment(
                    "declared_source_warnings",
                    96,
                    &resolved_workspace_sources.warnings,
                ),
            ])));
        }

        for mcp_config in &platform_mcp_configs {
            extra_contributors.push(Box::new(McpContextContributor::new(mcp_config.clone())));
        }

        if let (Some(wf), Some(bindings_out)) = (workflow.clone(), resolved_bindings.clone()) {
            extra_contributors.push(Box::new(WorkflowContextBindingsContributor::new(
                wf,
                bindings_out,
            )));
        }

        let task_phase = match spec.phase {
            TaskRuntimePhase::Start => TaskExecutionPhase::Start,
            TaskRuntimePhase::Continue => TaskExecutionPhase::Continue,
        };
        let built = build_task_agent_context(
            TaskAgentBuildInput {
                task: spec.task,
                story: story_ref,
                project: project_ref,
                workspace: workspace_ref,
                vfs: vfs.as_ref(),
                effective_agent_type,
                phase: task_phase,
                override_prompt: spec.override_prompt,
                additional_prompt: spec.additional_prompt,
                extra_contributors,
            },
            self.contributor_registry,
        )
        .map_err(TaskExecutionError::UnprocessableEntity)?;

        // ── 汇总 MCP 列表：platform + custom + contributor 产出 ──
        let mut effective_mcp_servers: Vec<agent_client_protocol::McpServer> = platform_mcp_configs
            .iter()
            .map(|c| c.to_acp_mcp_server())
            .collect();
        effective_mcp_servers.extend(cap_output.custom_mcp_servers.iter().cloned());
        effective_mcp_servers.extend(
            crate::runtime_bridge::runtime_mcp_servers_to_acp(&built.mcp_servers),
        );

        let mut builder = SessionAssemblyBuilder::new()
            .with_prompt_blocks(built.prompt_blocks)
            .with_mcp_servers(effective_mcp_servers)
            .append_relay_mcp_names(relay_mcp_server_names)
            .with_resolved_capabilities(flow_capabilities, effective_capability_keys)
            .with_optional_system_context(built.system_context)
            .with_working_dir(built.working_dir)
            .with_source_summary(built.source_summary)
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
        compose_lifecycle_node(self.repos, self.platform_config, spec).await
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
            ready_port_keys,
        },
        platform_config,
    );

    Ok(SessionAssemblyBuilder::new()
        .apply_lifecycle_activation(&activation, spec.inherited_executor_config)
        .build())
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
            spec.parent_system_context,
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

// ═══════════════════════════════════════════════════════════════════
// SECTION 5:其余 Spec 结构 + 辅助函数
// ═══════════════════════════════════════════════════════════════════

/// Task runtime 的 phase(与 `crate::task::execution::ExecutionPhase` 映射)。
#[derive(Debug, Clone, Copy)]
pub enum TaskRuntimePhase {
    Start,
    Continue,
}

/// Story step 场景下 compose 所需的完整上下文。
///
/// 用于 `TaskLifecycleService` facade 的 task 启动路径
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
    pub phase: TaskRuntimePhase,
    pub override_prompt: Option<&'a str>,
    pub additional_prompt: Option<&'a str>,
    pub explicit_executor_config: Option<AgentConfig>,
    /// 若为 true,executor 解析失败时直接返回 Err;否则返回 failed 状态继续。
    pub strict_config_resolution: bool,
    /// 对应活跃 lifecycle run 的投影（由 facade 通过 SessionBinding 两跳定位后传入）。
    pub active_workflow: Option<ActiveWorkflowProjection>,
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
    pub parent_system_context: Option<&'a str>,
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
    let slice = build_companion_execution_slice(
        comp.parent_vfs,
        comp.parent_mcp_servers,
        comp.slice_mode,
    );

    // ── 2. Workflow step activation（产出 lifecycle mount + 能力 + MCP） ──
    let owner_ctx = SessionOwnerCtx::Project { project_id };
    let port_output_map =
        load_port_output_map(repos.inline_file_repo.as_ref(), spec.run.id).await;
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

    let workflow_injection = spec.workflow.map(|w| {
        let inj = &w.contract.injection;
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
        parts.join("\n\n")
    }).filter(|s| !s.is_empty());
    let system_context = match (comp.parent_system_context, workflow_injection) {
        (Some(parent), Some(wf_inject)) => Some(format!("{parent}\n\n{wf_inject}")),
        (Some(parent), None) => Some(parent.to_string()),
        (None, Some(wf_inject)) => Some(wf_inject),
        (None, None) => None,
    };

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
        .with_optional_system_context(system_context)
        .with_prompt_blocks(prompt_blocks)
        .with_executor_config(comp.companion_executor_config.clone())
        .with_bootstrap_action(SessionBootstrapAction::OwnerContext)
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
