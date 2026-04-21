//! `SessionRequestAssembler` — 统一 session 启动请求组装。
//!
//! ## 设计
//!
//! 代码库里一共有 5 条 session 启动路径,此前各自手写 bootstrap 逻辑:
//!
//! | 路径 | 实现入口 |
//! |---|---|
//! | ACP Story/Project | `api::routes::acp_sessions` → `SessionRequestAssembler::compose_owner_bootstrap` |
//! | Task runtime | `task::gateway::turn_context` → `SessionRequestAssembler::compose_task_runtime` |
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
use agentdash_domain::workflow::{
    LifecycleDefinition, LifecycleRun, LifecycleStepDefinition,
};
use agentdash_domain::workspace::Workspace;
use agentdash_spi::{FlowCapabilities, Vfs};
use uuid::Uuid;

use crate::canvas::append_visible_canvas_mounts;
use crate::capability::{
    AgentMcpServerEntry, AvailableMcpPresets, CapabilityResolver, CapabilityResolverInput,
    SessionWorkflowContext,
};
use crate::companion::tools::CompanionSliceMode;
use crate::context::{
    BuiltTaskAgentContext, ContextContributor, ContextContributorRegistry, McpContextContributor,
    StaticFragmentsContributor, TaskAgentBuildInput, TaskExecutionPhase,
    WorkflowContextBindingsContributor, build_declared_source_warning_fragment,
    build_task_agent_context, resolve_workspace_declared_sources,
};
use crate::platform_config::PlatformConfig;
use crate::project::context_builder::{ProjectContextBuildInput, build_project_context_markdown};
use crate::repository_set::RepositorySet;
use crate::runtime::RuntimeMcpServer;
use crate::session::types::{PromptSessionRequest, SessionBootstrapAction};
use crate::session::context::apply_workspace_defaults;
use crate::story::context_builder::{StoryContextBuildInput, build_story_context_markdown};
use crate::task::execution::TaskExecutionError;
use crate::vfs::{
    RelayVfsService, ResolveBindingsOutput, SessionMountTarget, build_lifecycle_mount_with_ports,
    resolve_context_bindings,
};
use crate::workflow::{
    ActiveWorkflowProjection, StepActivationInput, activate_step_with_platform,
    load_port_output_map, resolve_active_workflow_projection_for_session,
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
}

/// 把 `PreparedSessionInputs` 合并进一个 base `PromptSessionRequest`。
///
/// - user_input.prompt_blocks 若 compose 有产出则覆盖,否则保留原值
/// - vfs 若 compose 有产出则使用,否则用 workspace_defaults 填充
/// - 其余字段直接覆盖
pub fn finalize_request(base: PromptSessionRequest, prepared: PreparedSessionInputs) -> PromptSessionRequest {
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
    if req.vfs.is_none() {
        req.vfs = prepared.vfs;
    } else if prepared.vfs.is_some() {
        // base.vfs 已有(前端透传场景)但 compose 也产出了 —— 以 compose 为准
        // 保证 compose 的 workspace/canvas/lifecycle mount 组合不被覆盖
        req.vfs = prepared.vfs;
    }
    req.mcp_servers = prepared.mcp_servers;
    req.relay_mcp_server_names.extend(prepared.relay_mcp_server_names);
    req.flow_capabilities = prepared.flow_capabilities;
    req.effective_capability_keys = prepared.effective_capability_keys;
    req
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
        Ok(presets) => presets
            .into_iter()
            .map(|p| (p.name, p.server_decl))
            .collect(),
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
        let workflow_caps = resolve_owner_workflow_capabilities(self.repos, &spec.owner).await;
        let workflow_ctx = match workflow_caps {
            Some(caps) => SessionWorkflowContext {
                has_active_workflow: true,
                workflow_capabilities: Some(caps),
            },
            None => SessionWorkflowContext::NONE,
        };

        // ── 3. CapabilityResolver ──
        let cap_input = CapabilityResolverInput {
            owner_ctx,
            agent_declared_capabilities: spec.agent_declared_capabilities,
            workflow_ctx,
            agent_mcp_servers: extract_agent_mcp_entries(&spec.agent_mcp.preset_mcp_servers),
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

        // ── 5. Context markdown 生成 ──
        let runtime_mcp_servers = acp_mcp_servers_to_runtime(&effective_mcp_servers);
        let runtime_vfs = vfs.clone();

        let (workspace_fragments, workspace_warnings) = match &spec.owner {
            OwnerScope::Story { story, workspace, .. } => {
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
                wrap_owner_bootstrap_blocks(&spec.owner, &context_markdown, spec.user_prompt_blocks),
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
                (sys_ctx, spec.user_prompt_blocks, SessionBootstrapAction::None)
            }
            OwnerPromptLifecycle::Plain => (
                None,
                spec.user_prompt_blocks,
                SessionBootstrapAction::None,
            ),
        };

        let effective_capability_keys: BTreeSet<String> = cap_output
            .effective_capabilities
            .iter()
            .map(|c| c.key().to_string())
            .collect();

        Ok(PreparedSessionInputs {
            prompt_blocks: Some(prompt_blocks),
            executor_config: Some(spec.executor_config),
            mcp_servers: effective_mcp_servers,
            relay_mcp_server_names: spec.agent_mcp.relay_mcp_server_names,
            vfs,
            flow_capabilities: Some(cap_output.flow_capabilities),
            effective_capability_keys: Some(effective_capability_keys),
            system_context,
            bootstrap_action,
            workspace_defaults: match &spec.owner {
                OwnerScope::Story { workspace, .. } => workspace.cloned(),
                OwnerScope::Project { workspace, .. } => workspace.as_deref().cloned(),
            },
        })
    }

    /// Task 运行时(lazy bootstrap),合并 turn_context + session_runtime_inputs 两处重复。
    pub async fn compose_task_runtime(
        &self,
        spec: TaskRuntimeSpec<'_>,
    ) -> Result<TaskRuntimeOutput, TaskExecutionError> {
        // ── 1. 解析 executor config ──
        use crate::session::ExecutorResolution;
        use crate::task::config::resolve_task_executor_config;
        use crate::task::session_runtime_inputs::resolve_task_executor_source;

        let executor_source = resolve_task_executor_source(
            spec.task,
            spec.project,
            spec.explicit_executor_config.as_ref(),
        );
        let (resolved_config, executor_resolution) =
            match resolve_task_executor_config(spec.explicit_executor_config, spec.task, spec.project) {
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

        // ── 2. 解析 active workflow projection(从 task session bindings 反查) ──
        let workflow = resolve_workflow_via_task_sessions(self.repos, spec.task).await?;

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
        let workflow_caps = workflow.as_ref().and_then(|p| {
            p.primary_workflow
                .as_ref()
                .map(|w| w.contract.capabilities.clone())
        });
        let workflow_ctx = SessionWorkflowContext {
            has_active_workflow: workflow.is_some(),
            workflow_capabilities: workflow_caps,
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
        let mcp_servers_for_context: Vec<RuntimeMcpServer> = platform_mcp_configs
            .iter()
            .map(|c| crate::runtime_bridge::acp_mcp_server_to_runtime(&c.to_acp_mcp_server()))
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
                wf, bindings_out,
            )));
        }

        let task_phase = match spec.phase {
            TaskRuntimePhase::Start => TaskExecutionPhase::Start,
            TaskRuntimePhase::Continue => TaskExecutionPhase::Continue,
        };
        let _ = mcp_servers_for_context; // 保留变量用于未来扩展
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

        Ok(TaskRuntimeOutput {
            built,
            vfs,
            resolved_config,
            executor_resolution,
            use_cloud_native_agent: use_cloud_native,
            workspace: workspace_ref.cloned(),
            flow_capabilities,
            effective_capability_keys,
            workflow,
            resolved_bindings,
        })
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
    pub fn compose_companion(
        &self,
        spec: CompanionSpec<'_>,
    ) -> PreparedSessionInputs {
        compose_companion(spec)
    }
}

/// Workflow AgentNode session 激活(脱离 `SessionRequestAssembler`,方便 orchestrator
/// 等不持有完整 service 集合的调用方使用)。
pub async fn compose_lifecycle_node(
    repos: &RepositorySet,
    platform_config: &PlatformConfig,
    spec: LifecycleNodeSpec<'_>,
) -> Result<PreparedSessionInputs, String> {
    // owner_ctx 推导:orchestrator 场景下父 session 是 Project 级 agent session,
    // 新 AgentNode session 同样是 Project 级。未来 parent_owner_ctx 由上游显式传入。
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

    let kickoff_prompt = activation.kickoff_prompt.to_default_prompt();
    let prompt_blocks = vec![serde_json::json!({
        "type": "text",
        "text": kickoff_prompt,
    })];

    Ok(PreparedSessionInputs {
        prompt_blocks: Some(prompt_blocks),
        executor_config: spec.inherited_executor_config,
        mcp_servers: activation.mcp_servers,
        relay_mcp_server_names: HashSet::new(),
        vfs: Some(activation.lifecycle_vfs),
        flow_capabilities: Some(activation.flow_capabilities),
        effective_capability_keys: Some(activation.capability_keys),
        system_context: None,
        bootstrap_action: SessionBootstrapAction::None,
        workspace_defaults: None,
    })
}

/// Companion 子 session 组装(脱离 `SessionRequestAssembler`,companion tool
/// 在父 session 作用域内即可完成,不需要 assembler 的完整服务依赖)。
pub fn compose_companion(spec: CompanionSpec<'_>) -> PreparedSessionInputs {
    use crate::companion::tools::build_companion_execution_slice;

    let slice = build_companion_execution_slice(
        spec.parent_vfs,
        spec.parent_mcp_servers,
        spec.slice_mode,
    );
    let flow_caps = CapabilityResolver::resolve_companion_caps(map_slice_mode(spec.slice_mode));

    PreparedSessionInputs {
        prompt_blocks: Some(vec![serde_json::json!({
            "type": "text",
            "text": spec.dispatch_prompt,
        })]),
        executor_config: Some(spec.companion_executor_config),
        mcp_servers: slice.mcp_servers.clone(),
        relay_mcp_server_names: HashSet::new(),
        vfs: slice.vfs.clone(),
        flow_capabilities: Some(flow_caps),
        effective_capability_keys: None,
        system_context: spec.parent_system_context.map(|s| s.to_string()),
        bootstrap_action: SessionBootstrapAction::OwnerContext,
        workspace_defaults: None,
    }
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
        CompanionSliceMode::ConstraintsOnly => crate::capability::CompanionSliceMode::ConstraintsOnly,
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

/// Task runtime compose 输入。
pub struct TaskRuntimeSpec<'a> {
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
}

/// Task runtime compose 输出 —— 比 `PreparedSessionInputs` 多携带 task 路径
/// 特有的辅助字段(turn dispatcher 需要这些)。
pub struct TaskRuntimeOutput {
    pub built: BuiltTaskAgentContext,
    pub vfs: Option<Vfs>,
    pub resolved_config: Option<AgentConfig>,
    pub executor_resolution: crate::session::ExecutorResolution,
    pub use_cloud_native_agent: bool,
    pub workspace: Option<Workspace>,
    pub flow_capabilities: FlowCapabilities,
    pub effective_capability_keys: BTreeSet<String>,
    pub workflow: Option<ActiveWorkflowProjection>,
    pub resolved_bindings: Option<ResolveBindingsOutput>,
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

// ═══════════════════════════════════════════════════════════════════
// SECTION 6:内部 helper
// ═══════════════════════════════════════════════════════════════════

/// Owner bootstrap 阶段解析 workflow capabilities(来自默认 agent_link → lifecycle → entry step workflow)。
///
/// Story owner 找 project 内 `is_default_for_story=true` 的 agent_link;
/// Project owner 用 (project_id, agent_id) 直接查 agent_link。
/// 找不到任何绑定返回 None。
async fn resolve_owner_workflow_capabilities(
    repos: &RepositorySet,
    owner: &OwnerScope<'_>,
) -> Option<Vec<String>> {
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

    // 3. 查 workflow 定义 → contract.capabilities
    let workflow = repos
        .workflow_definition_repo
        .get_by_project_and_key(project_id, workflow_key)
        .await
        .ok()
        .flatten()?;

    Some(workflow.contract.capabilities.clone())
}

/// 通过 task 的 session binding 查找是否有 session 关联了活跃的 lifecycle run。
async fn resolve_workflow_via_task_sessions(
    repos: &RepositorySet,
    task: &Task,
) -> Result<Option<ActiveWorkflowProjection>, TaskExecutionError> {
    use agentdash_domain::session_binding::SessionOwnerType;

    let bindings = repos
        .session_binding_repo
        .list_by_owner(SessionOwnerType::Task, task.id)
        .await
        .map_err(|e| TaskExecutionError::Internal(e.to_string()))?;

    for binding in &bindings {
        if let Some(projection) = resolve_active_workflow_projection_for_session(
            &binding.session_id,
            repos.session_binding_repo.as_ref(),
            repos.workflow_definition_repo.as_ref(),
            repos.lifecycle_definition_repo.as_ref(),
            repos.lifecycle_run_repo.as_ref(),
        )
        .await
        .map_err(TaskExecutionError::Internal)?
        {
            return Ok(Some(projection));
        }
    }
    Ok(None)
}

// 允许未使用的 re-export,为 PR4-B..F 保留扩展点
#[allow(unused_imports)]
use crate::workflow::KickoffPromptFragment as _Kickoff;
#[allow(unused_imports)]
use crate::workflow::StepActivation as _StepActivation;
