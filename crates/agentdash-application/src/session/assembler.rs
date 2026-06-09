//! `SessionRequestAssembler` — 统一 session 启动请求组装。
//!
//! ## 设计
//!
//! 代码库里一共有 5 条 session 启动路径,此前各自手写 bootstrap 逻辑:
//!
//! | 路径 | 实现入口 |
//! |---|---|
//! | Story/Project | lifecycle dispatch → `SessionRequestAssembler::compose_owner_bootstrap` |
//! | Story activity activation | `task::service::StoryActivityActivationService::activate_story_activity` → `SessionRequestAssembler::compose_story_step` |
//! | Routine | `routine::executor::build_project_agent_prompt_request` → `SessionRequestAssembler::compose_owner_bootstrap`(带 trigger tag) |
//! | Workflow AgentNode | `workflow::orchestrator::start_agent_node_prompt` → `compose_lifecycle_node` |
//! | Companion | `companion::tools` → `compose_companion` |
//!
//! 5 条路径共享 4 个"策略轴":owner scope mount / context bundle 生成 /
//! prompt 来源 / 能力裁剪 / 父 session 继承。但字段形状不相交(Task 有
//! `ActiveWorkflowProjection`,Companion 有 parent 继承,AgentNode 有 step),
//! 因此设计上采用**组合器内部草稿**收束各轴字段，公共入口合入当前 construction
//! provider handoff:
//!
//! ```text
//! 4 个 compose fn(各自 Spec) → SessionAssemblyBuilder → construction facts
//! ```
//!
//! compose 函数内部共享 building blocks(`load_available_presets` /
//! `build_owner_context` / `activate_activity_with_platform` 等),不再重复散落。
//! 后续必须继续把 task effect / hook 迁移字段拆入 `LaunchPlan` / outbox。

use std::collections::BTreeSet;

use agentdash_domain::canvas::CanvasRepository;
use agentdash_domain::common::{AgentConfig, AgentVfsAccessGrant};
use agentdash_domain::project::Project;
use agentdash_domain::story::Story;
use agentdash_domain::task::Task;
use agentdash_domain::workflow::ToolCapabilityDirective;
use agentdash_domain::workflow::{
    ActivityDefinition, AgentProcedureContract, LifecycleRun, WorkflowGraph,
};
use agentdash_domain::workspace::Workspace;
use agentdash_spi::{CapabilityScope, CapabilityScopeCtx};
use agentdash_spi::{CapabilityState, SessionContextBundle, Vfs};
use async_trait::async_trait;
use uuid::Uuid;

use crate::canvas::append_visible_canvas_mounts;
use crate::capability::{
    AgentMcpServerEntry, AvailableMcpPresets, CapabilityResolver, CapabilityResolverInput,
    CompanionContribution, ContextContributionSource, ContextContributions, McpCandidates,
    ToolContribution, tool_directives_from_active_workflow,
    tool_directives_from_active_workflow_projection,
};
use crate::companion::{
    skill_projection::{
        append_companion_system_skill_key, ensure_companion_system_skill_asset,
        has_lifecycle_mount, project_companion_system_skill_to_activation,
    },
    tools::CompanionSliceMode,
};
use crate::context::{
    AuditTrigger, ContextBuildPhase, Contribution, SessionContextConfig, SharedContextAuditBus,
    TaskExecutionPhase, build_declared_source_warning_fragment, build_session_context_bundle,
    contribute_binding_initial_context, contribute_core_context, contribute_declared_sources,
    contribute_instruction, contribute_workflow_binding, contribute_workspace_static_sources,
    emit_bundle_fragments, resolve_workspace_declared_sources,
};
use crate::platform_config::PlatformConfig;
use crate::project::context_builder::{ProjectContextBuildInput, contribute_project_context};
use crate::repository_set::RepositorySet;
use crate::runtime::RuntimeMcpServer;
use crate::runtime_bridge::session_mcp_servers_to_runtime;
use crate::session::assembly_builder::SessionAssemblyBuilder;
#[cfg(test)]
use crate::session::assembly_builder::slice_companion_bundle;
use crate::session::post_turn_handler::TerminalHookEffectBinding;
use crate::story::context_builder::{StoryContextBuildInput, contribute_story_context};
use crate::task::execution::TaskExecutionError;
use crate::task::gateway::{effect_executor::TaskHookEffectExecutor, resolve_task_backend_id};
use crate::vfs::{
    ResolveBindingsOutput, SessionMountTarget, VfsService, apply_agent_vfs_access_grants,
    resolve_context_bindings,
};
use crate::workflow::{
    ActiveWorkflowProjection, ActivityActivationInput, RuntimeNodeArtifactScope,
    activate_activity_with_platform, ensure_active_workflow_lifecycle_mount,
    load_scoped_port_output_map,
};
use crate::workspace::BackendAvailability;

// ═══════════════════════════════════════════════════════════════════
// SECTION 1:内部 builder prompt 投影
// ═══════════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════════
// SECTION 2:Assembler 共享服务容器
// ═══════════════════════════════════════════════════════════════════

/// `SessionRequestAssembler` 依赖的基础设施引用集合。
///
/// 由 `AppState` / 各 handler 构造后传入各 compose 函数,避免每个 compose
/// 签名都携带 6-7 个 service 参数。
pub struct SessionRequestAssembler<'a> {
    pub vfs_service: &'a VfsService,
    pub canvas_repo: &'a dyn CanvasRepository,
    pub availability: &'a dyn BackendAvailability,
    pub repos: &'a RepositorySet,
    pub platform_config: &'a PlatformConfig,
    /// 可选审计总线 —— 每次 compose 产出 Bundle 后批量 emit。
    ///
    /// 为 `None` 时（例如单元测试 / routine 内部降级路径）跳过 emit；
    /// 生产路径由 `AppState` 注入 `InMemoryContextAuditBus` 共享实例。
    pub audit_bus: Option<SharedContextAuditBus>,
    pub companion_parent_facts_provider: Option<&'a dyn CompanionParentFactsProvider>,
}

#[async_trait]
pub trait CompanionParentFactsProvider: Send + Sync {
    async fn latest_companion_parent_capability_state(
        &self,
        parent_session_id: &str,
    ) -> Option<CapabilityState>;
}

#[async_trait]
impl CompanionParentFactsProvider for crate::session::SessionCapabilityService {
    async fn latest_companion_parent_capability_state(
        &self,
        parent_session_id: &str,
    ) -> Option<CapabilityState> {
        self.get_latest_capability_state(parent_session_id).await
    }
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

/// 查询当前 project 可用的 companion agent 候选列表。
///
/// 1. 拉取 project 下所有 ProjectAgent
/// 2. 读取每个 ProjectAgent 信息(name / agent_type / display_name)
/// 3. 如果 caller_agent_id 存在，按其 config 中 `allowed_companions` 过滤
async fn load_companion_candidates(
    repos: &RepositorySet,
    project_id: Uuid,
    caller_agent_id: Option<Uuid>,
) -> Result<Vec<agentdash_spi::context::capability::CompanionAgentEntry>, String> {
    let agents = match repos.project_agent_repo.list_by_project(project_id).await {
        Ok(agents) => agents,
        Err(_) => return Ok(Vec::new()),
    };
    if agents.is_empty() {
        return Ok(Vec::new());
    }

    // 解析 caller 的 allowed_companions 过滤列表
    let caller_allowed: Option<Vec<String>> = if let Some(caller_id) = caller_agent_id {
        if let Some(agent) = agents.iter().find(|item| item.id == caller_id) {
            let preset = agent.preset_config().map_err(|error| error.to_string())?;
            preset.allowed_companions.filter(|v| !v.is_empty())
        } else {
            None
        }
    } else {
        None
    };

    let mut entries = Vec::new();
    for agent in agents {
        if let Some(ref allowed) = caller_allowed {
            if !allowed.iter().any(|a| a.eq_ignore_ascii_case(&agent.name)) {
                continue;
            }
        }
        let preset = agent.preset_config().map_err(|error| error.to_string())?;
        let display = preset
            .display_name
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
            .unwrap_or_else(|| agent.name.clone());
        entries.push(agentdash_spi::context::capability::CompanionAgentEntry {
            name: agent.name,
            executor: agent.agent_type,
            display_name: display,
        });
    }
    Ok(entries)
}

/// 从 agent-level `preset_mcp_servers` 抽出 `AgentMcpServerEntry`(供 resolver 解析 `mcp:<name>`)。
pub fn extract_agent_mcp_entries(
    preset_mcp_servers: &[agentdash_spi::SessionMcpServer],
) -> Vec<AgentMcpServerEntry> {
    preset_mcp_servers
        .iter()
        .map(|s| AgentMcpServerEntry {
            name: s.name.clone(),
            server: s.clone(),
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

    fn owner_ctx(&self) -> CapabilityScopeCtx {
        match self {
            Self::Story { project, story, .. } => CapabilityScopeCtx::Story {
                project_id: project.id,
                story_id: story.id,
            },
            Self::Project { project, .. } => CapabilityScopeCtx::Project {
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
    pub preset_mcp_servers: Vec<agentdash_spi::SessionMcpServer>,
}

/// Owner bootstrap compose 的完整输入。
pub struct OwnerBootstrapSpec<'a> {
    pub owner: OwnerScope<'a>,
    pub executor_config: AgentConfig,
    /// user 层 canonical 用户输入(外部传入或 Routine 模板)。
    pub user_input: Vec<agentdash_agent_protocol::UserInputBlock>,
    pub agent_mcp: AgentLevelMcp,
    /// Agent preset 中声明的能力指令，作为 agent 来源 contribution 输入 resolver。
    pub agent_tool_directives: Vec<ToolCapabilityDirective>,
    /// Agent preset 中选择装载的项目 SkillAsset key。
    pub agent_skill_asset_keys: Vec<String>,
    /// Agent preset 中声明的 Project VFS mount 访问授权。
    pub agent_vfs_access_grants: Vec<AgentVfsAccessGrant>,
    /// 前端/request 已携带的 MCP server(透传)。
    pub request_mcp_servers: Vec<agentdash_spi::SessionMcpServer>,
    /// 前端已携带的 VFS(None 时 assembler 自行构建)。
    pub existing_vfs: Option<Vfs>,
    pub visible_canvas_mount_ids: Vec<String>,
    /// Agent preset 声明的 workspace module 可见性白名单（`ext:{key}` / `canvas:{mount_id}`）。
    ///
    /// 三态保真（事实源是 ProjectAgent preset，投影进 base `CapabilityState.workspace_module`）：
    /// `None`（未声明）/`Some([])`（显式清空）→ 全集可见（`mode=All`）；
    /// `Some([..非空])` → 仅白名单（`mode=Allowlist`）。
    pub visible_workspace_module_refs: Option<Vec<String>>,
    /// 当前 session 已绑定的活跃 workflow run。Project/Story owner session 在
    /// bootstrap 或续跑时可通过它获得 lifecycle VFS 与 workflow 能力基线。
    pub active_workflow: Option<ActiveWorkflowProjection>,
    /// Session lifecycle 三态判定结果,决定 context bundle / prompt_blocks 组装方式。
    pub lifecycle: OwnerPromptLifecycle,
    /// 审计总线用于索引的 session key（SessionRuntimeInner 分配的 `sess-<ms>-<short>`）。
    ///
    /// 为 `None` 时跳过审计 emit（例如 session 尚未创建的 bootstrap 路径）。
    pub audit_session_key: Option<String>,
    /// 调用方 agent 的 UUID — 用于从 agent_link config 中读取 allowed_companions 过滤。
    pub caller_agent_id: Option<Uuid>,
}

/// Owner bootstrap 阶段 session_runtime_inner 判定出的 prompt lifecycle 模式,决定 compose
/// 如何组装 context bundle + prompt_blocks。
///
/// 与 `SessionPromptLifecycle` 结构等价,但这里只暴露 compose 所需的 3 个分支,
/// continuation bundle(来自 SessionRuntimeInner)由调用方在 Spec 里预先算好传入。
pub enum OwnerPromptLifecycle {
    /// owner 首次启动,需要把 owner 上下文 Bundle 注入并包到 prompt blocks。
    OwnerBootstrap,
    /// 已有 repository，compose 使用预构建的 continuation bundle（当 connector
    /// 不支持原生 repository restore 时）或直接复用 owner context bundle
    /// （当 connector 支持原生消息历史恢复时）。
    RepositoryRehydrate {
        /// 由 SessionRuntimeInner 预先把历史事件渲染成 continuation Bundle，用于不支持
        /// `supports_repository_restore` 的 connector。
        prebuilt_continuation_bundle: Option<SessionContextBundle>,
        /// 是否把 owner context bundle 也一并附加（true = 继续用 owner bundle；
        /// false = 只用 prebuilt_continuation_bundle）。
        include_owner_bundle: bool,
    },
    /// 普通 turn,无 owner bootstrap。
    Plain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OwnerAuditLifecycle {
    Bootstrap,
    Rehydrate,
    Plain,
}

fn owner_audit_lifecycle(lifecycle: &OwnerPromptLifecycle) -> OwnerAuditLifecycle {
    match lifecycle {
        OwnerPromptLifecycle::OwnerBootstrap => OwnerAuditLifecycle::Bootstrap,
        OwnerPromptLifecycle::RepositoryRehydrate { .. } => OwnerAuditLifecycle::Rehydrate,
        OwnerPromptLifecycle::Plain => OwnerAuditLifecycle::Plain,
    }
}

fn resolve_owner_audit_trigger(
    lifecycle: OwnerAuditLifecycle,
    has_effective_bundle: bool,
) -> Option<AuditTrigger> {
    if !has_effective_bundle {
        return None;
    }

    match lifecycle {
        OwnerAuditLifecycle::Bootstrap => Some(AuditTrigger::SessionBootstrap),
        // RepositoryRehydrate 也是一次 owner 上下文重建，归类为 compose_rebuild。
        OwnerAuditLifecycle::Rehydrate => Some(AuditTrigger::ComposerRebuild),
        OwnerAuditLifecycle::Plain => None,
    }
}

/// Owner 级 session 的上下文 Contribution 组装 —— Story 与 Project 各走自己的 contribute_*。
///
/// 不再内联 SessionPlan / VFS / MCP 这些"运行时画像"字段 —— 调用方在外层
/// （`compose_owner_bootstrap`）显式 push SessionPlan contribution，保证三条
/// compose 路径（owner / story_step / lifecycle_node）的 SessionPlan 产出
/// 节拍一致（PR 5b）。
fn build_owner_context_contribution(
    owner: &OwnerScope<'_>,
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
            preset_name: preset_name.as_deref(),
            agent_display_name,
        }),
    }
}

/// Owner 路径的 SessionPlan contribution 构建（外挂到 compose_owner_bootstrap 顶层）。
///
/// PR 5b 把 SessionPlan fragments 从 `contribute_story_context` / `contribute_project_context`
/// 内部迁出到此函数，与 task 路径（`compose_story_step` 内部 push）保持一致的外挂节拍。
fn build_owner_session_plan_contribution(
    owner: &OwnerScope<'_>,
    vfs: Option<&Vfs>,
    mcp_servers: &[RuntimeMcpServer],
    effective_agent_type: &str,
) -> Contribution {
    use crate::session::plan::{
        SessionPlanInput, SessionPlanPhase, build_session_plan_fragments,
        resolve_story_session_composition,
    };
    let (plan_phase, owner_ctx, session_composition, preset_name, workspace_attached) = match owner
    {
        OwnerScope::Story {
            story,
            project,
            workspace,
        } => (
            SessionPlanPhase::StoryOwner,
            CapabilityScopeCtx::Story {
                project_id: project.id,
                story_id: story.id,
            },
            resolve_story_session_composition(Some(*story)),
            None,
            workspace.is_some(),
        ),
        OwnerScope::Project {
            project,
            workspace,
            preset_name,
            ..
        } => (
            SessionPlanPhase::ProjectAgent,
            CapabilityScopeCtx::Project {
                project_id: project.id,
            },
            None,
            preset_name.as_deref(),
            workspace.is_some(),
        ),
    };

    let plan = build_session_plan_fragments(SessionPlanInput {
        scope: owner_ctx.owner_type(),
        phase: plan_phase,
        vfs,
        mcp_servers,
        session_composition: session_composition.as_ref(),
        agent_type: Some(effective_agent_type),
        preset_name,
        has_custom_prompt_template: false,
        has_initial_context: false,
        workspace_attached,
    });
    Contribution::fragments_only(plan.fragments)
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
        vfs_service: &'a VfsService,
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
            companion_parent_facts_provider: None,
        }
    }

    /// 配置审计总线（生产路径由 `AppState` 注入）。
    pub fn with_audit_bus(mut self, bus: SharedContextAuditBus) -> Self {
        self.audit_bus = Some(bus);
        self
    }

    pub fn with_companion_parent_facts_provider(
        mut self,
        provider: &'a dyn CompanionParentFactsProvider,
    ) -> Self {
        self.companion_parent_facts_provider = Some(provider);
        self
    }

    /// 若存在审计总线且 session_key 可用，则把 bundle 的所有 fragment 批量 emit。
    ///
    /// `session_key` 应由调用方（spec.audit_session_key）提供，对应 SessionRuntimeInner 分配的
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

    async fn prepare_owner_bootstrap_vfs(
        &self,
        spec: &OwnerBootstrapSpec<'_>,
        project_id: Uuid,
        active_workflow: Option<&ActiveWorkflowProjection>,
    ) -> Result<Option<Vfs>, String> {
        let project_vfs_mounts = self
            .repos
            .project_vfs_mount_repo
            .list_by_project(project_id)
            .await
            .map_err(|error| format!("读取 Project VFS Mount 失败: {error}"))?;

        let mut vfs = match spec.existing_vfs.clone() {
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
                        &project_vfs_mounts,
                        Some(*story),
                        *workspace,
                        target,
                        Some(spec.executor_config.executor.as_str()),
                    )?,
                    OwnerScope::Project {
                        project, workspace, ..
                    } => self.vfs_service.build_vfs(
                        project,
                        &project_vfs_mounts,
                        None,
                        *workspace,
                        target,
                        Some(spec.executor_config.executor.as_str()),
                    )?,
                };
                Some(built)
            }
        };

        if let Some(space) = vfs.as_mut()
            && matches!(spec.owner, OwnerScope::Project { .. })
        {
            apply_agent_vfs_access_grants(space, Some(&spec.agent_vfs_access_grants));
        }

        let mut vfs = ensure_active_workflow_lifecycle_mount(vfs, active_workflow);
        let mut skill_asset_keys = spec.agent_skill_asset_keys.clone();
        if let Some(space) = vfs.as_mut() {
            if has_lifecycle_mount(space) {
                ensure_companion_system_skill_asset(self.repos, project_id)
                    .await
                    .map_err(|error| error.to_string())?;
                append_companion_system_skill_key(&mut skill_asset_keys);
            }
            append_visible_canvas_mounts(
                self.canvas_repo,
                project_id,
                space,
                &spec.visible_canvas_mount_ids,
            )
            .await
            .map_err(|e| e.to_string())?;
            crate::vfs::append_skill_asset_projection(space, project_id, &skill_asset_keys);
        }

        Ok(vfs)
    }

    async fn resolve_owner_capabilities(
        &self,
        spec: &OwnerBootstrapSpec<'_>,
        project_id: Uuid,
        owner_ctx: CapabilityScopeCtx,
        active_workflow: Option<&ActiveWorkflowProjection>,
    ) -> Result<CapabilityState, String> {
        let workflow_tool: Option<ToolContribution> = if let Some(workflow) = active_workflow {
            let directives = tool_directives_from_active_workflow_projection(workflow);
            Some(ToolContribution {
                directives,
                has_active_workflow: true,
            })
        } else {
            let workflow_directives =
                resolve_owner_workflow_tool_directives(self.repos, &spec.owner).await;
            workflow_directives.map(|directives| ToolContribution {
                directives,
                has_active_workflow: true,
            })
        };

        let available_companions =
            load_companion_candidates(self.repos, project_id, spec.caller_agent_id).await?;
        let mut contributions = Vec::new();
        if !spec.agent_tool_directives.is_empty() {
            contributions.push(ContextContributions {
                source: ContextContributionSource::Agent,
                tool: Some(ToolContribution {
                    directives: spec.agent_tool_directives.clone(),
                    has_active_workflow: false,
                }),
                companion: None,
            });
        }
        contributions.push(ContextContributions {
            source: ContextContributionSource::Resource,
            tool: None,
            companion: Some(CompanionContribution {
                available: available_companions,
            }),
        });
        if let Some(wf_tool) = workflow_tool {
            contributions.push(ContextContributions {
                source: ContextContributionSource::Workflow,
                tool: Some(wf_tool),
                companion: None,
            });
        }

        let cap_input = CapabilityResolverInput {
            owner_ctx,
            contributions,
            mcp_candidates: McpCandidates {
                presets: load_available_presets(self.repos, project_id).await,
                agent_servers: extract_agent_mcp_entries(&spec.agent_mcp.preset_mcp_servers),
            },
            capability_context: None,
        };
        Ok(CapabilityResolver::resolve(
            &cap_input,
            self.platform_config,
        ))
    }

    async fn build_owner_context_bundle(
        &self,
        spec: &OwnerBootstrapSpec<'_>,
        vfs: Option<&Vfs>,
        session_mcp_servers: &[agentdash_spi::SessionMcpServer],
    ) -> Result<SessionContextBundle, String> {
        let runtime_mcp_servers = session_mcp_servers_to_runtime(session_mcp_servers);
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
                .await
                .map_err(|error| error.to_string())?;
                (resolved.fragments, resolved.warnings)
            }
            OwnerScope::Project { .. } => (Vec::new(), Vec::new()),
        };

        let owner_contribution =
            build_owner_context_contribution(&spec.owner, workspace_fragments, workspace_warnings);
        let session_plan_contribution = build_owner_session_plan_contribution(
            &spec.owner,
            vfs,
            &runtime_mcp_servers,
            spec.executor_config.executor.as_str(),
        );

        Ok(build_session_context_bundle(
            SessionContextConfig {
                session_id: Uuid::new_v4(),
                phase: owner_scope_phase(&spec.owner),
                default_scope: agentdash_spi::ContextFragment::default_scope(),
            },
            vec![owner_contribution, session_plan_contribution],
        ))
    }

    /// Owner 级 session bootstrap(Story / Project / Routine)。
    pub(in crate::session) async fn compose_owner_bootstrap(
        &self,
        spec: OwnerBootstrapSpec<'_>,
    ) -> Result<SessionAssemblyBuilder, String> {
        let project_id = spec.owner.project_id();
        let owner_ctx = spec.owner.owner_ctx();
        let active_workflow = spec.active_workflow.clone();
        let vfs = self
            .prepare_owner_bootstrap_vfs(&spec, project_id, active_workflow.as_ref())
            .await?;
        let mut cap_output = self
            .resolve_owner_capabilities(&spec, project_id, owner_ctx, active_workflow.as_ref())
            .await?;
        // Workspace module 声明式可见性收口到 base CapabilityState（取代旧的
        // visible_workspace_module_refs_json 旁路 + frame_construction 直接赋值）：
        // 三态直达，经 effective_capability_json 序列化/还原；空集回 All（修 carry-forward bug）。
        cap_output.workspace_module = crate::session::capability_state::project_workspace_module_dimension(
            spec.visible_workspace_module_refs.as_deref(),
        );
        let mut session_mcp_servers = spec.request_mcp_servers.clone();
        session_mcp_servers.extend(cap_output.tool.mcp_servers.iter().cloned());
        session_mcp_servers.extend(spec.agent_mcp.preset_mcp_servers.iter().cloned());
        let context_bundle = self
            .build_owner_context_bundle(&spec, vfs.as_ref(), &session_mcp_servers)
            .await?;
        let audit_lifecycle = owner_audit_lifecycle(&spec.lifecycle);
        let (user_input, effective_bundle) = match spec.lifecycle {
            OwnerPromptLifecycle::OwnerBootstrap => (spec.user_input.clone(), Some(context_bundle)),
            OwnerPromptLifecycle::RepositoryRehydrate {
                ref prebuilt_continuation_bundle,
                include_owner_bundle,
            } => {
                let chosen_bundle = prebuilt_continuation_bundle.clone().or({
                    if include_owner_bundle {
                        Some(context_bundle)
                    } else {
                        None
                    }
                });
                (spec.user_input.clone(), chosen_bundle)
            }
            OwnerPromptLifecycle::Plain => (spec.user_input.clone(), None),
        };
        if let (Some(bundle), Some(trigger)) = (
            effective_bundle.as_ref(),
            resolve_owner_audit_trigger(audit_lifecycle, effective_bundle.is_some()),
        ) {
            self.audit_bundle(bundle, spec.audit_session_key.as_deref(), trigger);
        }

        let workspace_defaults = match &spec.owner {
            OwnerScope::Story { workspace, .. } => workspace.cloned(),
            OwnerScope::Project { workspace, .. } => workspace.as_deref().cloned(),
        };

        let mut builder = SessionAssemblyBuilder::new()
            .with_input(user_input)
            .with_executor_config(spec.executor_config.clone())
            .with_mcp_servers(session_mcp_servers)
            .with_resolved_capabilities(cap_output)
            .with_optional_workspace_defaults(workspace_defaults)
            .with_optional_context_bundle(effective_bundle);

        if let Some(vfs) = vfs {
            builder = builder.with_vfs(vfs);
        }

        Ok(builder.build())
    }

    /// owner_bootstrap 的 frame builder 路径。
    ///
    /// 返回 `(AgentFrameBuilder, AssemblyLaunchExtras)`：
    /// - frame builder 携带所有 surface 数据，调用方 `.build()` 持久化
    /// - extras 包含 context bundle / prompt / executor config 等 launch 数据
    pub async fn compose_owner_bootstrap_to_frame(
        &self,
        frame_builder: crate::workflow::frame_builder::AgentFrameBuilder,
        spec: OwnerBootstrapSpec<'_>,
    ) -> Result<
        (
            crate::workflow::frame_builder::AgentFrameBuilder,
            crate::session::assembly_builder::AssemblyLaunchExtras,
        ),
        String,
    > {
        let prepared = self.compose_owner_bootstrap(spec).await?;
        Ok(crate::session::assembly_builder::project_assembly_to_frame(
            frame_builder,
            prepared,
        ))
    }

    fn resolve_story_step_executor_config(
        spec: &StoryStepSpec<'_>,
    ) -> Result<(Option<AgentConfig>, Option<String>), TaskExecutionError> {
        use crate::task::config::{resolve_task_executor_config, resolve_task_executor_source};

        let executor_source = resolve_task_executor_source(
            spec.task,
            spec.project,
            spec.explicit_executor_config.as_ref(),
        );
        match resolve_task_executor_config(
            spec.explicit_executor_config.clone(),
            spec.task,
            spec.project,
        ) {
            Ok(config) => Ok((config.clone(), config.as_ref().map(|c| c.executor.clone()))),
            Err(err) if spec.strict_config_resolution => Err(err),
            Err(err) => {
                let _resolution =
                    crate::session::ExecutorResolution::failed(executor_source, err.to_string());
                Ok((None, None))
            }
        }
    }

    async fn prepare_story_step_vfs(
        &self,
        spec: &StoryStepSpec<'_>,
        executor_name: Option<&str>,
        use_cloud_native: bool,
    ) -> Result<Option<Vfs>, TaskExecutionError> {
        let vfs = if use_cloud_native {
            Some(
                self.vfs_service
                    .build_vfs(
                        spec.project,
                        &self
                            .repos
                            .project_vfs_mount_repo
                            .list_by_project(spec.project.id)
                            .await
                            .map_err(|error| {
                                TaskExecutionError::Internal(format!(
                                    "读取 Project VFS Mount 失败: {error}"
                                ))
                            })?,
                        Some(spec.story),
                        spec.workspace,
                        SessionMountTarget::Task,
                        executor_name,
                    )
                    .map_err(|error| TaskExecutionError::Internal(error.to_string()))?,
            )
        } else {
            None
        };

        Ok(ensure_active_workflow_lifecycle_mount(
            vfs,
            spec.active_workflow.as_ref(),
        ))
    }

    async fn resolve_story_step_context_bindings(
        &self,
        vfs: Option<&Vfs>,
        workflow: Option<&ActiveWorkflowProjection>,
    ) -> Result<Option<ResolveBindingsOutput>, TaskExecutionError> {
        let (Some(space), Some(wf)) = (vfs, workflow) else {
            return Ok(None);
        };
        let bindings = wf
            .active_contract()
            .map(|c| c.injection.context_bindings.as_slice())
            .unwrap_or(&[]);
        if bindings.is_empty() {
            return Ok(None);
        }
        resolve_context_bindings(bindings, space, self.vfs_service)
            .await
            .map(Some)
            .map_err(TaskExecutionError::UnprocessableEntity)
    }

    async fn resolve_story_step_capabilities(
        &self,
        spec: &StoryStepSpec<'_>,
        workflow: Option<&ActiveWorkflowProjection>,
    ) -> (CapabilityState, Vec<agentdash_spi::SessionMcpServer>) {
        let mut contributions = Vec::new();
        if let Some(directives) = workflow.map(tool_directives_from_active_workflow_projection) {
            contributions.push(ContextContributions {
                source: ContextContributionSource::Workflow,
                tool: Some(ToolContribution {
                    directives,
                    has_active_workflow: true,
                }),
                companion: None,
            });
        }
        let cap_input = CapabilityResolverInput {
            owner_ctx: CapabilityScopeCtx::Task {
                project_id: spec.task.project_id,
                story_id: spec.task.story_id,
                task_id: spec.task.id,
            },
            contributions,
            mcp_candidates: McpCandidates {
                presets: load_available_presets(self.repos, spec.task.project_id).await,
                agent_servers: vec![],
            },
            capability_context: None,
        };
        let mut capability_state = CapabilityResolver::resolve(&cap_input, self.platform_config);
        let mut session_mcp_servers = spec.request_mcp_servers.to_vec();
        session_mcp_servers.extend(capability_state.tool.mcp_servers.iter().cloned());
        capability_state.tool.mcp_servers = session_mcp_servers.clone();
        (capability_state, session_mcp_servers)
    }

    async fn build_story_step_context_bundle(
        &self,
        spec: &StoryStepSpec<'_>,
        vfs: Option<&Vfs>,
        workflow: Option<&ActiveWorkflowProjection>,
        resolved_bindings: Option<&ResolveBindingsOutput>,
        capability_state: &CapabilityState,
        effective_agent_type: Option<&str>,
    ) -> Result<(SessionContextBundle, TaskExecutionPhase), TaskExecutionError> {
        let mut declared_sources = spec.story.context.source_refs.clone();
        declared_sources.extend(spec.task.dispatch_preference.context_sources.clone());
        let resolved_workspace_sources = resolve_workspace_declared_sources(
            self.availability,
            self.vfs_service,
            &declared_sources,
            spec.workspace,
            86,
        )
        .await
        .map_err(|error| TaskExecutionError::UnprocessableEntity(error.to_string()))?;
        let task_phase = match spec.phase {
            StoryStepPhase::Start => TaskExecutionPhase::Start,
            StoryStepPhase::Continue => TaskExecutionPhase::Continue,
        };

        let mut contributions: Vec<Contribution> = Vec::new();
        contributions.push(contribute_core_context(
            spec.task,
            spec.story,
            spec.project,
            spec.workspace,
        ));
        contributions.push(contribute_binding_initial_context(spec.task));
        contributions.push(contribute_declared_sources(spec.task, spec.story));
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

        let task_mcp_servers = session_mcp_servers_to_runtime(&capability_state.tool.mcp_servers);
        if let (Some(wf), Some(bindings_out)) = (workflow.cloned(), resolved_bindings.cloned()) {
            contributions.push(contribute_workflow_binding(&wf, &bindings_out));
        }
        contributions.push(contribute_instruction(
            spec.task,
            spec.story,
            spec.workspace,
            task_phase,
            spec.override_prompt,
            spec.additional_prompt,
        ));

        let effective_session_composition =
            crate::session::plan::resolve_story_session_composition(Some(spec.story));
        let session_plan = crate::session::plan::build_session_plan_fragments(
            crate::session::plan::SessionPlanInput {
                scope: CapabilityScope::Task,
                phase: match task_phase {
                    TaskExecutionPhase::Start => crate::session::plan::SessionPlanPhase::TaskStart,
                    TaskExecutionPhase::Continue => {
                        crate::session::plan::SessionPlanPhase::TaskContinue
                    }
                },
                vfs,
                mcp_servers: &task_mcp_servers,
                session_composition: effective_session_composition.as_ref(),
                agent_type: effective_agent_type,
                preset_name: spec.task.dispatch_preference.preset_name.as_deref(),
                has_custom_prompt_template: spec
                    .task
                    .dispatch_preference
                    .prompt_template
                    .as_deref()
                    .is_some_and(|value| !value.trim().is_empty()),
                has_initial_context: spec
                    .task
                    .dispatch_preference
                    .initial_context
                    .as_deref()
                    .is_some_and(|value| !value.trim().is_empty()),
                workspace_attached: vfs.is_some(),
            },
        );
        contributions.push(Contribution::fragments_only(session_plan.fragments));

        Ok((
            build_session_context_bundle(
                SessionContextConfig {
                    session_id: Uuid::new_v4(),
                    phase: match task_phase {
                        TaskExecutionPhase::Start => ContextBuildPhase::TaskStart,
                        TaskExecutionPhase::Continue => ContextBuildPhase::TaskContinue,
                    },
                    default_scope: agentdash_spi::ContextFragment::default_scope(),
                },
                contributions,
            ),
            task_phase,
        ))
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
    /// 输出统一为 `SessionAssemblyBuilder`；调用方投影到 `AgentFrameBuilder`，
    /// 再由 frame 生成 runtime launch request。
    pub(in crate::session) async fn compose_story_step(
        &self,
        spec: StoryStepSpec<'_>,
    ) -> Result<SessionAssemblyBuilder, TaskExecutionError> {
        let (resolved_config, effective_agent_type) =
            Self::resolve_story_step_executor_config(&spec)?;
        let use_cloud_native = resolved_config
            .as_ref()
            .is_some_and(|c| c.is_cloud_native());
        let vfs = self
            .prepare_story_step_vfs(&spec, effective_agent_type.as_deref(), use_cloud_native)
            .await?;
        let resolved_bindings = self
            .resolve_story_step_context_bindings(vfs.as_ref(), spec.active_workflow.as_ref())
            .await?;
        let (capability_state, session_mcp_servers) = self
            .resolve_story_step_capabilities(&spec, spec.active_workflow.as_ref())
            .await;
        let (context_bundle, task_phase) = self
            .build_story_step_context_bundle(
                &spec,
                vfs.as_ref(),
                spec.active_workflow.as_ref(),
                resolved_bindings.as_ref(),
                &capability_state,
                effective_agent_type.as_deref(),
            )
            .await?;
        self.audit_bundle(
            &context_bundle,
            spec.audit_session_key.as_deref(),
            AuditTrigger::ComposerRebuild,
        );
        let user_input = build_story_step_trigger_input(task_phase);
        let mut builder = SessionAssemblyBuilder::new()
            .with_input(user_input)
            .with_mcp_servers(session_mcp_servers)
            .with_resolved_capabilities(capability_state)
            .with_context_bundle(context_bundle)
            .with_optional_workspace_defaults(spec.workspace.cloned());

        if let Some(vfs) = vfs {
            builder = builder.with_vfs(vfs);
        }
        if let Some(cfg) = resolved_config {
            builder = builder.with_executor_config(cfg);
        }

        Ok(builder.build())
    }

    /// story_step 的 frame builder 路径。
    pub async fn compose_story_step_to_frame(
        &self,
        frame_builder: crate::workflow::frame_builder::AgentFrameBuilder,
        spec: StoryStepSpec<'_>,
    ) -> Result<
        (
            crate::workflow::frame_builder::AgentFrameBuilder,
            crate::session::assembly_builder::AssemblyLaunchExtras,
            Option<crate::session::post_turn_handler::TerminalHookEffectBinding>,
        ),
        TaskExecutionError,
    > {
        let task_id = spec.task.id;
        let backend_id = resolve_task_backend_id(self.repos, self.availability, spec.task).await?;
        let prepared = self.compose_story_step(spec).await?;
        let (fb, extras) =
            crate::session::assembly_builder::project_assembly_to_frame(frame_builder, prepared);
        let hook_binding = Some(TerminalHookEffectBinding {
            handler: serde_json::json!({
                "kind": "task",
                "task_id": task_id,
                "backend_id": backend_id,
            }),
            supported_effect_kinds: TaskHookEffectExecutor::SUPPORTED_KINDS
                .iter()
                .map(|kind| (*kind).to_string())
                .collect(),
        });
        Ok((fb, extras, hook_binding))
    }

    /// lifecycle_node 的 frame builder 路径。
    pub async fn compose_lifecycle_node_to_frame(
        &self,
        frame_builder: crate::workflow::frame_builder::AgentFrameBuilder,
        spec: LifecycleNodeSpec<'_>,
    ) -> Result<
        (
            crate::workflow::frame_builder::AgentFrameBuilder,
            crate::session::assembly_builder::AssemblyLaunchExtras,
        ),
        String,
    > {
        let prepared = compose_lifecycle_node_with_audit(
            self.repos,
            self.platform_config,
            spec,
            self.audit_bus.clone(),
            None,
        )
        .await?;
        Ok(crate::session::assembly_builder::project_assembly_to_frame(
            frame_builder,
            prepared,
        ))
    }

    /// companion 的 frame builder 路径。
    pub async fn compose_companion_to_frame(
        &self,
        frame_builder: crate::workflow::frame_builder::AgentFrameBuilder,
        spec: CompanionParentSpec<'_>,
    ) -> Result<
        (
            crate::workflow::frame_builder::AgentFrameBuilder,
            crate::session::assembly_builder::AssemblyLaunchExtras,
        ),
        String,
    > {
        let parent_facts = self
            .resolve_companion_parent_facts(spec.parent_session_id)
            .await?;
        let prepared = compose_companion(CompanionSpec {
            parent_vfs: parent_facts.parent_vfs.as_ref(),
            parent_mcp_servers: &parent_facts.parent_mcp_servers,
            parent_context_bundle: parent_facts.parent_context_bundle.as_ref(),
            slice_mode: spec.slice_mode,
            companion_executor_config: spec.companion_executor_config,
            dispatch_prompt: spec.dispatch_prompt,
        });
        Ok(crate::session::assembly_builder::project_assembly_to_frame(
            frame_builder,
            prepared,
        ))
    }

    /// companion + workflow 的 frame builder 路径。
    pub async fn compose_companion_with_workflow_to_frame(
        &self,
        frame_builder: crate::workflow::frame_builder::AgentFrameBuilder,
        spec: CompanionParentWorkflowSpec<'_>,
    ) -> Result<
        (
            crate::workflow::frame_builder::AgentFrameBuilder,
            crate::session::assembly_builder::AssemblyLaunchExtras,
        ),
        String,
    > {
        let parent_facts = self
            .resolve_companion_parent_facts(spec.companion.parent_session_id)
            .await?;
        let prepared = compose_companion_with_workflow(
            self.repos,
            self.platform_config,
            CompanionWorkflowSpec {
                companion: CompanionSpec {
                    parent_vfs: parent_facts.parent_vfs.as_ref(),
                    parent_mcp_servers: &parent_facts.parent_mcp_servers,
                    parent_context_bundle: parent_facts.parent_context_bundle.as_ref(),
                    slice_mode: spec.companion.slice_mode,
                    companion_executor_config: spec.companion.companion_executor_config,
                    dispatch_prompt: spec.companion.dispatch_prompt,
                },
                run: spec.run,
                orchestration_id: spec.orchestration_id,
                node_path: spec.node_path,
                attempt: spec.attempt,
                lifecycle: spec.lifecycle,
                activity: spec.activity,
                workflow: spec.workflow,
            },
        )
        .await?;
        Ok(crate::session::assembly_builder::project_assembly_to_frame(
            frame_builder,
            prepared,
        ))
    }

    pub(crate) async fn resolve_companion_parent_facts(
        &self,
        parent_session_id: &str,
    ) -> Result<CompanionParentFacts, String> {
        let Some(provider) = self.companion_parent_facts_provider else {
            return Err("companion parent facts provider 未注入".to_string());
        };
        let parent_capability_state = provider
            .latest_companion_parent_capability_state(parent_session_id)
            .await;
        Ok(CompanionParentFacts {
            parent_vfs: parent_capability_state
                .as_ref()
                .and_then(|state| state.vfs.active.clone()),
            parent_mcp_servers: parent_capability_state
                .as_ref()
                .map(|state| state.tool.mcp_servers.clone())
                .unwrap_or_default(),
            parent_context_bundle: None,
        })
    }
}

/// lifecycle_node 的 frame builder 路径（free-standing 版本）。
pub async fn compose_lifecycle_node_to_frame_with_audit(
    frame_builder: crate::workflow::frame_builder::AgentFrameBuilder,
    repos: &RepositorySet,
    platform_config: &PlatformConfig,
    spec: LifecycleNodeSpec<'_>,
    audit_bus: Option<SharedContextAuditBus>,
    audit_session_key: Option<&str>,
) -> Result<
    (
        crate::workflow::frame_builder::AgentFrameBuilder,
        crate::session::assembly_builder::AssemblyLaunchExtras,
    ),
    String,
> {
    let prepared = compose_lifecycle_node_with_audit(
        repos,
        platform_config,
        spec,
        audit_bus,
        audit_session_key,
    )
    .await?;
    Ok(crate::session::assembly_builder::project_assembly_to_frame(
        frame_builder,
        prepared,
    ))
}

pub(in crate::session) async fn compose_lifecycle_node_with_audit(
    repos: &RepositorySet,
    platform_config: &PlatformConfig,
    spec: LifecycleNodeSpec<'_>,
    audit_bus: Option<SharedContextAuditBus>,
    audit_session_key: Option<&str>,
) -> Result<SessionAssemblyBuilder, String> {
    let owner_ctx = CapabilityScopeCtx::Project {
        project_id: spec.run.project_id,
    };

    let artifact_scope = RuntimeNodeArtifactScope {
        run_id: spec.run.id,
        orchestration_id: spec.orchestration_id,
        node_path: spec.node_path.to_string(),
        attempt: spec.attempt,
    };
    let port_output_map =
        load_scoped_port_output_map(repos.inline_file_repo.as_ref(), &artifact_scope).await;
    let ready_port_keys: BTreeSet<String> = port_output_map.keys().cloned().collect();

    let mut activation = activate_activity_with_platform(
        &ActivityActivationInput {
            owner_ctx,
            active_activity: spec.activity,
            workflow_contract: spec.workflow_contract,
            run_id: spec.run.id,
            orchestration_id: spec.orchestration_id,
            node_path: spec.node_path,
            attempt: spec.attempt,
            lifecycle_key: spec.lifecycle_key,
            agent_mcp_servers: vec![],
            available_presets: load_available_presets(repos, spec.run.project_id).await,
            companion_slice_mode: None,
            baseline_override: None,
            tool_directives: &[],
            ready_port_keys: ready_port_keys.clone(),
            available_companions: Vec::new(),
        },
        platform_config,
    );
    project_companion_system_skill_to_activation(repos, spec.run.project_id, &mut activation)
        .await
        .map_err(|error| error.to_string())?;

    // SessionPlan 在 PR 5b 前 lifecycle node 路径完全不产出，导致 lifecycle agent
    // 的 bundle 相比 owner / task 路径最薄。此处补上 SessionPlan contribution，
    // 让 lifecycle node 与其余两路都有 vfs / tools / persona / workflow /
    // runtime_policy 的统一画像。
    let lifecycle_mcp_runtime: Vec<RuntimeMcpServer> = activation
        .mcp_servers
        .iter()
        .map(crate::runtime_bridge::session_mcp_server_to_runtime)
        .collect();
    let lifecycle_plan = crate::session::plan::build_session_plan_fragments(
        crate::session::plan::SessionPlanInput {
            scope: CapabilityScope::Project,
            phase: crate::session::plan::SessionPlanPhase::ProjectAgent,
            vfs: Some(&activation.lifecycle_vfs),
            mcp_servers: &lifecycle_mcp_runtime,
            session_composition: None,
            agent_type: None,
            preset_name: None,
            has_custom_prompt_template: false,
            has_initial_context: false,
            workspace_attached: true,
        },
    );

    let context_bundle = build_session_context_bundle(
        SessionContextConfig {
            session_id: Uuid::new_v4(),
            phase: ContextBuildPhase::LifecycleNode,
            default_scope: agentdash_spi::ContextFragment::default_scope(),
        },
        vec![
            contribute_lifecycle_context(&spec, &activation, &ready_port_keys),
            Contribution::fragments_only(lifecycle_plan.fragments),
        ],
    );
    if let (Some(bus), Some(session_key)) = (audit_bus.as_ref(), audit_session_key) {
        emit_bundle_fragments(
            bus.as_ref(),
            &context_bundle,
            session_key,
            AuditTrigger::ComposerRebuild,
        );
    }
    Ok(SessionAssemblyBuilder::new()
        .apply_lifecycle_activation(&activation, spec.inherited_executor_config)
        .with_context_bundle(context_bundle)
        .build())
}

/// 由 activity executor 推导展示用的 node 语义。
fn activity_node_type(
    activity: &ActivityDefinition,
) -> agentdash_domain::workflow::LifecycleNodeType {
    use agentdash_domain::workflow::{ActivityExecutorSpec, LifecycleNodeType};
    match &activity.executor {
        ActivityExecutorSpec::Agent(spec) if spec.continues_current_agent() => {
            LifecycleNodeType::PhaseNode
        }
        ActivityExecutorSpec::Agent(_) => LifecycleNodeType::AgentNode,
        _ => LifecycleNodeType::AgentNode,
    }
}

fn contribute_lifecycle_context(
    spec: &LifecycleNodeSpec<'_>,
    activation: &crate::workflow::ActivityActivation,
    ready_port_keys: &BTreeSet<String>,
) -> Contribution {
    let mut fragments = Vec::new();

    let step_desc = spec.activity.description.trim();
    let workflow_label = spec
        .workflow_label
        .map(str::to_string)
        .unwrap_or_else(|| "未绑定 workflow".to_string());
    let node_type = activity_node_type(spec.activity);
    let mut lifecycle_lines = vec![
        format!("- Lifecycle: `{}`", spec.lifecycle_key),
        format!("- Run: `{}`", spec.run.id),
        format!("- Step: `{}`", spec.activity.key),
        format!("- Node type: `{node_type:?}`"),
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

    if let Some(workflow_contract) = spec.workflow_contract {
        if let Some(content) = crate::context::rendering::render_workflow_injection(
            &workflow_contract.injection,
            crate::context::rendering::WorkflowInjectionMode::Declarative,
        ) {
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
pub(in crate::session) fn compose_companion(spec: CompanionSpec<'_>) -> SessionAssemblyBuilder {
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

fn build_story_step_trigger_input(
    phase: TaskExecutionPhase,
) -> Vec<agentdash_agent_protocol::UserInputBlock> {
    let text = match phase {
        TaskExecutionPhase::Start => "请开始执行当前任务。",
        TaskExecutionPhase::Continue => "请继续推进当前任务。",
    };
    agentdash_agent_protocol::text_user_input_blocks(text)
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

/// Task execution session 场景下 compose 所需的完整上下文。
///
/// 用于 `StoryActivityActivationService` 的 task 启动 / 续跑路径
/// （`start_task` / `continue_task` 直接以 task 为入口调 compose）。
///
/// 与 `LifecycleNodeSpec`（orchestrator 的 phase node 使用）不同：
/// - `StoryStepSpec` 持有 task/story/project/workspace 完整 entity 引用
/// - 承载 user prompt 注入（`override_prompt` / `additional_prompt`）
/// - 承载 explicit executor config（HTTP 请求透传）
/// - `active_workflow` 可选：task execution session 无 lifecycle binding 时为 `None`，
///   走纯 task 装配（不带 lifecycle workflow injection）
pub struct StoryStepSpec<'a> {
    pub task: &'a Task,
    pub story: &'a Story,
    pub project: &'a Project,
    pub workspace: Option<&'a Workspace>,
    pub phase: StoryStepPhase,
    pub override_prompt: Option<&'a str>,
    pub additional_prompt: Option<&'a str>,
    pub request_mcp_servers: &'a [agentdash_spi::SessionMcpServer],
    pub explicit_executor_config: Option<AgentConfig>,
    /// 若为 true,executor 解析失败时直接返回 Err;否则返回 failed 状态继续。
    pub strict_config_resolution: bool,
    /// 对应活跃 lifecycle run 的投影（由 facade 通过 subject association 定位后传入）。
    pub active_workflow: Option<ActiveWorkflowProjection>,
    /// 审计总线用于索引的 session key。
    pub audit_session_key: Option<String>,
}

/// Lifecycle AgentNode compose 输入。
pub struct LifecycleNodeSpec<'a> {
    pub run: &'a LifecycleRun,
    pub orchestration_id: Uuid,
    pub node_path: &'a str,
    pub attempt: u32,
    pub lifecycle_key: &'a str,
    pub activity: &'a ActivityDefinition,
    pub workflow_contract: Option<&'a AgentProcedureContract>,
    pub workflow_label: Option<&'a str>,
    pub inherited_executor_config: Option<AgentConfig>,
}

/// Companion compose 输入。
pub struct CompanionSpec<'a> {
    pub parent_vfs: Option<&'a Vfs>,
    pub parent_mcp_servers: &'a [agentdash_spi::SessionMcpServer],
    /// 父 session 的结构化上下文 Bundle，companion 直接继承（按 slice_mode 过滤）。
    pub parent_context_bundle: Option<&'a SessionContextBundle>,
    pub slice_mode: CompanionSliceMode,
    pub companion_executor_config: AgentConfig,
    pub dispatch_prompt: String,
}

pub struct CompanionParentSpec<'a> {
    pub parent_session_id: &'a str,
    pub slice_mode: CompanionSliceMode,
    pub companion_executor_config: AgentConfig,
    pub dispatch_prompt: String,
}

pub struct CompanionParentWorkflowSpec<'a> {
    pub companion: CompanionParentSpec<'a>,
    pub run: &'a LifecycleRun,
    pub orchestration_id: Uuid,
    pub node_path: &'a str,
    pub attempt: u32,
    pub lifecycle: &'a WorkflowGraph,
    pub activity: &'a ActivityDefinition,
    pub workflow: Option<&'a agentdash_domain::workflow::AgentProcedure>,
}

pub(crate) struct CompanionParentFacts {
    pub(crate) parent_vfs: Option<Vfs>,
    pub(crate) parent_mcp_servers: Vec<agentdash_spi::SessionMcpServer>,
    pub(crate) parent_context_bundle: Option<SessionContextBundle>,
}

/// Companion + Workflow 组合 compose 输入。
pub struct CompanionWorkflowSpec<'a> {
    pub companion: CompanionSpec<'a>,
    /// 已创建的 lifecycle run。
    pub run: &'a LifecycleRun,
    pub orchestration_id: Uuid,
    pub node_path: &'a str,
    pub attempt: u32,
    pub lifecycle: &'a WorkflowGraph,
    pub activity: &'a ActivityDefinition,
    pub workflow: Option<&'a agentdash_domain::workflow::AgentProcedure>,
}

/// Companion + Workflow 组合组装。
///
/// 基于 companion VFS slice 叠加 lifecycle mount 和 workflow 能力/MCP，
/// 通过 `SessionAssemblyBuilder` 声明式组合两个关注点。
pub(in crate::session) async fn compose_companion_with_workflow(
    repos: &RepositorySet,
    platform_config: &PlatformConfig,
    spec: CompanionWorkflowSpec<'_>,
) -> Result<SessionAssemblyBuilder, String> {
    use crate::companion::tools::build_companion_execution_slice;

    let project_id = spec.run.project_id;
    let comp = &spec.companion;

    // ── 1. Companion VFS slice 作为基础 ──
    let slice =
        build_companion_execution_slice(comp.parent_vfs, comp.parent_mcp_servers, comp.slice_mode);

    // ── 2. Workflow activity activation（产出 lifecycle mount + 能力 + MCP） ──
    let owner_ctx = CapabilityScopeCtx::Project { project_id };
    let artifact_scope = RuntimeNodeArtifactScope {
        run_id: spec.run.id,
        orchestration_id: spec.orchestration_id,
        node_path: spec.node_path.to_string(),
        attempt: spec.attempt,
    };
    let port_output_map =
        load_scoped_port_output_map(repos.inline_file_repo.as_ref(), &artifact_scope).await;
    let ready_port_keys: BTreeSet<String> = port_output_map.keys().cloned().collect();

    let activation = activate_activity_with_platform(
        &ActivityActivationInput {
            owner_ctx,
            active_activity: spec.activity,
            workflow_contract: spec.workflow.map(|workflow| &workflow.contract),
            run_id: spec.run.id,
            orchestration_id: spec.orchestration_id,
            node_path: spec.node_path,
            attempt: spec.attempt,
            lifecycle_key: &spec.lifecycle.key,
            agent_mcp_servers: vec![],
            available_presets: load_available_presets(repos, project_id).await,
            companion_slice_mode: Some(comp.slice_mode),
            baseline_override: None,
            tool_directives: &[],
            ready_port_keys,
            available_companions: Vec::new(),
        },
        platform_config,
    );

    // ── 3. 用 builder 组合 companion + workflow 两个层 ──
    //
    // 继承父 bundle 并叠加 workflow injection 片段。workflow injection 作为独立
    // fragment 注入 Bundle，替代旧的字符串拼接路径。
    // 渲染文本由共享 `render_workflow_injection` 产出（SummaryOnly 模式 —— companion
    // 不需要 declarative bindings 列表）；companion+workflow 路径若提供 audit_session_key
    // 会通过调用方在外层 emit 至审计总线。
    let mut merged_bundle = comp.parent_context_bundle.cloned();
    if let Some(workflow) = spec.workflow
        && let Some(workflow_content) = crate::context::rendering::render_workflow_injection(
            &workflow.contract.injection,
            crate::context::rendering::WorkflowInjectionMode::SummaryOnly,
        )
    {
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

    let user_input = agentdash_agent_protocol::text_user_input_blocks(comp.dispatch_prompt.clone());

    Ok(SessionAssemblyBuilder::new()
        .with_vfs(slice.vfs.unwrap_or_default())
        .apply_lifecycle_activation(&activation, Some(comp.companion_executor_config.clone()))
        .append_mcp_servers(slice.mcp_servers.into_iter())
        .with_optional_context_bundle(merged_bundle)
        .with_input(user_input)
        .build())
}

// ═══════════════════════════════════════════════════════════════════
// SECTION 6:内部 helper
// ═══════════════════════════════════════════════════════════════════

/// Owner bootstrap 阶段解析 workflow tool directives(来自 ProjectAgent → lifecycle → entry step workflow)。
///
/// Story owner 找 project 内 `is_default_for_story=true` 的 ProjectAgent;
/// Project owner 用 (project_id, project_agent_id) 直接查 ProjectAgent。
/// 找不到任何绑定返回 None。
async fn resolve_owner_workflow_tool_directives(
    repos: &RepositorySet,
    owner: &OwnerScope<'_>,
) -> Option<Vec<ToolCapabilityDirective>> {
    let project_id = owner.project_id();

    // 1. 找到 ProjectAgent
    let agent_opt = match owner {
        OwnerScope::Project { .. } => {
            let agent_id = owner.agent_id()?;
            repos
                .project_agent_repo
                .get_by_project_and_id(project_id, agent_id)
                .await
                .ok()
                .flatten()
        }
        OwnerScope::Story { .. } => repos
            .project_agent_repo
            .list_by_project(project_id)
            .await
            .ok()
            .and_then(|agents| agents.into_iter().find(|agent| agent.is_default_for_story)),
    };
    let agent = agent_opt?;
    let lifecycle_key = agent
        .default_lifecycle_key
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())?;

    // 2. 查 activity lifecycle 定义 → entry activity → procedure_key
    let lifecycle = repos
        .workflow_graph_repo
        .get_by_project_and_key(project_id, lifecycle_key)
        .await
        .ok()
        .flatten()?;
    let entry_activity = lifecycle
        .activities
        .iter()
        .find(|a| a.key == lifecycle.entry_activity_key)?;
    let procedure_key = match &entry_activity.executor {
        agentdash_domain::workflow::ActivityExecutorSpec::Agent(spec) => {
            spec.procedure_key.as_str()
        }
        _ => return None,
    };

    // 3. 查 workflow 定义 → contract.capability_config.tool_directives
    let workflow = repos
        .agent_procedure_repo
        .get_by_project_and_key(project_id, procedure_key)
        .await
        .ok()
        .flatten()?;

    Some(tool_directives_from_active_workflow(&workflow))
}

#[cfg(test)]
#[allow(deprecated)]
mod tests {
    use super::*;
    use crate::vfs::build_lifecycle_mount_with_ports;
    use agentdash_domain::workflow::{
        ActivityDefinition, ActivityExecutorSpec, AgentActivityExecutorSpec, AgentProcedure,
        AgentProcedureContract, DefinitionSource, InputPortDefinition, LifecycleNodeType,
        OutputPortDefinition, WorkflowGraph, WorkflowInjectionSpec,
    };
    use std::collections::BTreeSet;

    // ── companion bundle fragment 裁剪回归（PR 5d · E8①） ──

    fn activity_with_agent_executor(executor: AgentActivityExecutorSpec) -> ActivityDefinition {
        ActivityDefinition {
            key: "implement".to_string(),
            description: String::new(),
            executor: ActivityExecutorSpec::Agent(executor),
            input_ports: Vec::new(),
            output_ports: Vec::new(),
            completion_policy: Default::default(),
            iteration_policy: Default::default(),
            join_policy: Default::default(),
        }
    }

    #[test]
    fn activity_node_type_follows_agent_reuse_policy() {
        assert_eq!(
            activity_node_type(&activity_with_agent_executor(
                AgentActivityExecutorSpec::create_activity_agent("wf_impl")
            )),
            LifecycleNodeType::AgentNode
        );
        assert_eq!(
            activity_node_type(&activity_with_agent_executor(
                AgentActivityExecutorSpec::continue_current_agent("wf_impl")
            )),
            LifecycleNodeType::PhaseNode
        );
    }

    fn bundle_with_slots(slots: &[&str]) -> agentdash_spi::SessionContextBundle {
        let mut bundle = agentdash_spi::SessionContextBundle::new(
            Uuid::new_v4(),
            ContextBuildPhase::StoryOwner.as_tag(),
        );
        for (idx, slot) in slots.iter().enumerate() {
            bundle.upsert_by_slot(agentdash_spi::ContextFragment {
                slot: (*slot).to_string(),
                label: format!("label_{slot}"),
                order: 10 + idx as i32,
                strategy: agentdash_spi::MergeStrategy::Append,
                scope: agentdash_spi::ContextFragment::default_scope(),
                source: "test".to_string(),
                content: format!("body_{slot}"),
            });
        }
        bundle
    }

    fn slot_set(bundle: &agentdash_spi::SessionContextBundle) -> std::collections::HashSet<String> {
        bundle
            .bootstrap_fragments
            .iter()
            .map(|f| f.slot.clone())
            .collect()
    }

    #[test]
    fn slice_companion_bundle_full_retains_all_slots() {
        let parent = bundle_with_slots(&["story", "workflow_context", "vfs", "constraint"]);
        let sliced = slice_companion_bundle(&parent, CompanionSliceMode::Full);
        let slots = slot_set(&sliced);
        assert!(slots.contains("story"));
        assert!(slots.contains("workflow_context"));
        assert!(slots.contains("vfs"));
        assert!(slots.contains("constraint"));
    }

    #[test]
    fn slice_companion_bundle_compact_drops_runtime_slots() {
        let parent = bundle_with_slots(&[
            "story",
            "task",
            "workflow_context",
            "vfs",
            "tools",
            "persona",
            "required_context",
            "runtime_policy",
        ]);
        let sliced = slice_companion_bundle(&parent, CompanionSliceMode::Compact);
        let slots = slot_set(&sliced);
        // 保留业务上下文与 workflow 声明
        assert!(slots.contains("story"));
        assert!(slots.contains("task"));
        assert!(slots.contains("workflow_context"));
        // 剔除运行时画像
        assert!(!slots.contains("vfs"));
        assert!(!slots.contains("tools"));
        assert!(!slots.contains("persona"));
        assert!(!slots.contains("required_context"));
        assert!(!slots.contains("runtime_policy"));
    }

    #[test]
    fn slice_companion_bundle_workflow_only_keeps_workflow_slots() {
        let parent = bundle_with_slots(&["story", "workflow", "workflow_context", "constraint"]);
        let sliced = slice_companion_bundle(&parent, CompanionSliceMode::WorkflowOnly);
        let slots = slot_set(&sliced);
        assert!(slots.contains("workflow"));
        assert!(slots.contains("workflow_context"));
        assert!(!slots.contains("story"));
        assert!(!slots.contains("constraint"));
    }

    #[test]
    fn slice_companion_bundle_constraints_only_keeps_constraint_slots() {
        let parent = bundle_with_slots(&["story", "workflow_context", "constraint", "constraints"]);
        let sliced = slice_companion_bundle(&parent, CompanionSliceMode::ConstraintsOnly);
        let slots = slot_set(&sliced);
        assert!(slots.contains("constraint"));
        assert!(slots.contains("constraints"));
        assert!(!slots.contains("story"));
        assert!(!slots.contains("workflow_context"));
    }

    #[test]
    fn story_step_trigger_prompt_does_not_embed_owner_context() {
        for phase in [TaskExecutionPhase::Start, TaskExecutionPhase::Continue] {
            let blocks = build_story_step_trigger_input(phase);
            let text = blocks
                .iter()
                .filter_map(agentdash_agent_protocol::user_input_text)
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
    fn owner_bootstrap_audit_trigger_requires_effective_bundle() {
        assert_eq!(
            resolve_owner_audit_trigger(OwnerAuditLifecycle::Bootstrap, true),
            Some(AuditTrigger::SessionBootstrap),
        );
        assert_eq!(
            resolve_owner_audit_trigger(OwnerAuditLifecycle::Bootstrap, false),
            None,
        );
    }

    #[test]
    fn owner_rehydrate_audit_trigger_maps_to_composer_rebuild() {
        assert_eq!(
            resolve_owner_audit_trigger(OwnerAuditLifecycle::Rehydrate, true),
            Some(AuditTrigger::ComposerRebuild),
        );
        assert_eq!(
            resolve_owner_audit_trigger(OwnerAuditLifecycle::Rehydrate, false),
            None,
        );
    }

    #[test]
    fn owner_plain_lifecycle_never_emits_owner_audit() {
        assert_eq!(
            resolve_owner_audit_trigger(OwnerAuditLifecycle::Plain, true),
            None,
        );
        assert_eq!(
            resolve_owner_audit_trigger(OwnerAuditLifecycle::Plain, false),
            None,
        );
    }

    fn test_workspace_mount() -> agentdash_domain::common::Mount {
        agentdash_domain::common::Mount {
            id: "workspace".to_string(),
            provider: "relay_fs".to_string(),
            backend_id: "backend-test".to_string(),
            root_ref: "workspace://test".to_string(),
            capabilities: vec![
                agentdash_domain::common::MountCapability::Read,
                agentdash_domain::common::MountCapability::List,
            ],
            default_write: false,
            display_name: "Workspace".to_string(),
            metadata: serde_json::Value::Null,
        }
    }

    fn test_activity_activation(run_id: Uuid) -> crate::workflow::ActivityActivation {
        let lifecycle_mount = build_lifecycle_mount_with_ports(
            run_id,
            Uuid::new_v4(),
            "test-node",
            "test-lifecycle",
            &["report".to_string()],
        );
        crate::workflow::ActivityActivation {
            capability_state: Default::default(),
            mcp_servers: Vec::new(),
            capability_keys: BTreeSet::new(),
            kickoff_prompt: crate::workflow::KickoffPromptFragment {
                title_line: String::new(),
                output_section: String::new(),
                input_section: String::new(),
            },
            lifecycle_mount: lifecycle_mount.clone(),
            lifecycle_vfs: Vfs {
                mounts: vec![lifecycle_mount],
                default_mount_id: None,
                source_project_id: None,
                source_story_id: None,
                links: Vec::new(),
            },
            mount_directives: Vec::new(),
        }
    }

    #[test]
    fn append_lifecycle_mount_creates_vfs_when_base_is_absent() {
        let prepared = SessionAssemblyBuilder::new()
            .append_lifecycle_mount(crate::workflow::LifecycleMountSurface {
                run_id: Uuid::new_v4(),
                orchestration_id: Uuid::new_v4(),
                node_path: "test-node",
                lifecycle_key: "test-lifecycle",
                attempt: 1,
                writable_port_keys: Vec::new(),
            })
            .build();

        let vfs = prepared.vfs.expect("lifecycle mount should create VFS");
        let lifecycle = vfs
            .mounts
            .iter()
            .find(|mount| mount.id == "lifecycle")
            .expect("lifecycle mount should be visible");
        assert!(
            lifecycle
                .capabilities
                .contains(&agentdash_domain::common::MountCapability::Write)
        );
    }

    #[test]
    fn apply_lifecycle_activation_merges_existing_vfs() {
        let activation = test_activity_activation(Uuid::new_v4());
        let base_vfs = Vfs {
            mounts: vec![test_workspace_mount()],
            default_mount_id: Some("workspace".to_string()),
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        };

        let prepared = SessionAssemblyBuilder::new()
            .with_vfs(base_vfs)
            .apply_lifecycle_activation(&activation, None)
            .build();

        let vfs = prepared.vfs.expect("merged VFS");
        let mount_ids = vfs
            .mounts
            .iter()
            .map(|mount| mount.id.as_str())
            .collect::<BTreeSet<_>>();
        assert!(mount_ids.contains("workspace"));
        assert!(mount_ids.contains("lifecycle"));
        assert_eq!(vfs.default_mount_id.as_deref(), Some("workspace"));
    }

    #[test]
    fn lifecycle_context_contribution_contains_workflow_and_runtime_fragments() {
        let project_id = Uuid::new_v4();
        let activity = ActivityDefinition {
            key: "implement".to_string(),
            description: "实现功能".to_string(),
            executor: ActivityExecutorSpec::Agent(
                AgentActivityExecutorSpec::create_activity_agent("wf_impl"),
            ),
            input_ports: vec![InputPortDefinition {
                key: "design".to_string(),
                description: "设计方案".to_string(),
                context_strategy: Default::default(),
                context_template: None,
                standalone_fulfillment: Default::default(),
            }],
            output_ports: vec![OutputPortDefinition {
                key: "summary".to_string(),
                description: "实现摘要".to_string(),
                gate_strategy: Default::default(),
                gate_params: None,
            }],
            completion_policy: Default::default(),
            iteration_policy: Default::default(),
            join_policy: Default::default(),
        };
        let lifecycle = WorkflowGraph::new(
            project_id,
            "dev",
            "Dev",
            "dev lifecycle",
            DefinitionSource::BuiltinSeed,
            "implement",
            vec![activity.clone()],
            vec![],
        )
        .expect("lifecycle");
        let run = agentdash_domain::workflow::LifecycleRun::new_control(project_id);
        let workflow = AgentProcedure::new(
            project_id,
            "wf_impl",
            "Implementation",
            "实现工作流",
            DefinitionSource::BuiltinSeed,
            AgentProcedureContract {
                injection: WorkflowInjectionSpec {
                    guidance: Some("交付可验证实现。\n\n保持上下文收口。".to_string()),
                    context_bindings: vec![],
                },
                ..AgentProcedureContract::default()
            },
        )
        .expect("workflow");
        let mount = crate::vfs::build_lifecycle_mount_with_ports(
            run.id,
            uuid::Uuid::new_v4(),
            "implement",
            &lifecycle.key,
            &["summary".into()],
        );
        let activation = crate::workflow::ActivityActivation {
            capability_state: Default::default(),
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
            mount_directives: Vec::new(),
        };

        let spec = LifecycleNodeSpec {
            run: &run,
            orchestration_id: uuid::Uuid::new_v4(),
            node_path: "implement",
            attempt: 1,
            lifecycle_key: &lifecycle.key,
            activity: &activity,
            workflow_contract: Some(&workflow.contract),
            workflow_label: Some("`wf_impl` (Implementation)"),
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
        let relevant_content: String = bundle
            .filter_for(agentdash_spi::FragmentScope::RuntimeAgent)
            .filter(|f| f.slot == "workflow_context" || f.slot == "runtime_policy")
            .map(|f| f.content.clone())
            .collect::<Vec<_>>()
            .join("\n\n");

        assert!(relevant_content.contains("## Lifecycle Node"));
        assert!(relevant_content.contains("交付可验证实现"));
        assert!(relevant_content.contains("complete_lifecycle_node"));
        assert!(relevant_content.contains("workflow_management"));
    }

    // ═══════════════════════════════════════════════════════════
    // apply_session_assembly 合并语义回归测试
    // ═══════════════════════════════════════════════════════════
    //
    // 这些测试锁定 `apply_session_assembly` 对称化后的行为（2026-04-30）：
    // - mcp_servers (Vec<SessionMcpServer>) 统一整体替换；
    // - vfs 语义三分支等价于"prepared 非空则覆盖"；
    // - workspace_defaults 顺序保持"先回填、再被 prepared.vfs 覆盖"。

    mod apply_session_assembly_tests {
        use super::super::*;
        use crate::session::UserPromptInput;
        use crate::session::assembly_builder::apply_session_assembly;
        use crate::session::construction::{ResolvedSessionOwner, RuntimeContextInspectionPlan};
        use agentdash_spi::Vfs;
        use std::collections::HashMap;

        fn base_plan() -> RuntimeContextInspectionPlan {
            let user_input = UserPromptInput::from_text("ping");
            let owner = ResolvedSessionOwner::project(uuid::Uuid::new_v4());
            RuntimeContextInspectionPlan::from_source_input("test-session", owner, &user_input)
        }

        fn session_server(name: &str, url: &str) -> agentdash_spi::SessionMcpServer {
            agentdash_spi::SessionMcpServer {
                name: name.to_string(),
                transport: agentdash_spi::McpTransportConfig::Http {
                    url: url.to_string(),
                    headers: vec![],
                },
                uses_relay: false,
            }
        }

        #[test]
        fn mcp_servers_prepared_overrides_base() {
            let mut base = base_plan();
            base.projections.mcp_servers = vec![session_server("base_only", "http://base")];

            let prepared = SessionAssemblyBuilder {
                mcp_servers: vec![
                    session_server("compose_a", "http://a"),
                    session_server("compose_b", "http://b"),
                ],
                ..Default::default()
            };

            let result = apply_session_assembly(base, prepared);
            let names: Vec<&str> = result
                .projections
                .mcp_servers
                .iter()
                .map(|s| s.name.as_str())
                .collect();
            assert_eq!(names, vec!["compose_a", "compose_b"]);
        }

        #[test]
        fn mcp_servers_prepared_empty_still_replaces() {
            let mut base = base_plan();
            base.projections.mcp_servers = vec![session_server("base_only", "http://base")];
            let prepared = SessionAssemblyBuilder::default();

            let result = apply_session_assembly(base, prepared);
            assert!(result.projections.mcp_servers.is_empty());
        }

        #[test]
        fn vfs_prepared_some_overrides_base() {
            // base 已有 vfs、prepared 也有 vfs → 以 prepared 为准（保留 compose 的 mount 组合）。
            let mut base = base_plan();
            base.surface.vfs = Some(Vfs {
                mounts: Vec::new(),
                default_mount_id: Some("base-mount".to_string()),
                source_project_id: None,
                source_story_id: None,
                links: Vec::new(),
            });
            let prepared = SessionAssemblyBuilder {
                vfs: Some(Vfs {
                    mounts: Vec::new(),
                    default_mount_id: Some("prepared-mount".to_string()),
                    source_project_id: None,
                    source_story_id: None,
                    links: Vec::new(),
                }),
                ..Default::default()
            };

            let result = apply_session_assembly(base, prepared);
            assert_eq!(
                result.surface.vfs.and_then(|v| v.default_mount_id),
                Some("prepared-mount".to_string()),
            );
        }

        #[test]
        fn vfs_prepared_none_preserves_base() {
            // base 有 vfs、prepared 没有 → 保留 base（不强制清空）。
            let mut base = base_plan();
            base.surface.vfs = Some(Vfs {
                mounts: Vec::new(),
                default_mount_id: Some("base-mount".to_string()),
                source_project_id: None,
                source_story_id: None,
                links: Vec::new(),
            });
            let prepared = SessionAssemblyBuilder::default();

            let result = apply_session_assembly(base, prepared);
            assert_eq!(
                result.surface.vfs.and_then(|v| v.default_mount_id),
                Some("base-mount".to_string()),
            );
        }

        #[test]
        fn prompt_blocks_prepared_overrides_base() {
            let mut base = base_plan();
            base.prompt.input = Some(agentdash_agent_protocol::text_user_input_blocks("base"));
            let prepared = SessionAssemblyBuilder {
                input: Some(agentdash_agent_protocol::text_user_input_blocks("compose")),
                ..Default::default()
            };

            let result = apply_session_assembly(base, prepared);
            let texts: Vec<&str> = result
                .prompt
                .input
                .as_ref()
                .unwrap()
                .iter()
                .filter_map(agentdash_agent_protocol::user_input_text)
                .collect();
            assert_eq!(texts, vec!["compose"]);
        }

        #[test]
        fn prompt_blocks_prepared_none_preserves_base() {
            let mut base = base_plan();
            base.prompt.input = Some(agentdash_agent_protocol::text_user_input_blocks("base"));
            let prepared = SessionAssemblyBuilder::default();

            let result = apply_session_assembly(base, prepared);
            let texts: Vec<&str> = result
                .prompt
                .input
                .as_ref()
                .unwrap()
                .iter()
                .filter_map(agentdash_agent_protocol::user_input_text)
                .collect();
            assert_eq!(texts, vec!["base"]);
        }

        #[test]
        fn context_bundle_prepared_overrides_base() {
            // Bundle 为 Option 整体替换语义：prepared = None 也会清掉 base。
            use agentdash_spi::SessionContextBundle;

            let mut base = base_plan();
            base.context.bundle =
                Some(SessionContextBundle::new(uuid::Uuid::new_v4(), "test-base"));
            // prepared 为 None 时整体替换：base bundle 被清除
            let prepared = SessionAssemblyBuilder::default();

            let result = apply_session_assembly(base, prepared);
            assert!(
                result.context.bundle.is_none(),
                "context_bundle 为整体替换字段，prepared=None 会清除 base"
            );
        }

        // ═══════════════════════════════════════════════════════════
        // PR 1 Phase 1c 新字段测试：env
        // ═══════════════════════════════════════════════════════════

        #[test]
        fn env_prepared_overrides_base_when_nonempty() {
            // prepared.env 非空 → 整体替换。
            let mut base = base_plan();
            base.prompt
                .environment_variables
                .insert("FOO".to_string(), "base".to_string());

            let mut prepared_env = HashMap::new();
            prepared_env.insert("BAR".to_string(), "prepared".to_string());
            let prepared = SessionAssemblyBuilder {
                env: prepared_env,
                ..Default::default()
            };

            let result = apply_session_assembly(base, prepared);
            assert!(!result.prompt.environment_variables.contains_key("FOO"));
            assert_eq!(
                result
                    .prompt
                    .environment_variables
                    .get("BAR")
                    .map(String::as_str),
                Some("prepared")
            );
        }

        #[test]
        fn env_prepared_empty_preserves_base() {
            // prepared.env 为空 → 保留 base.env。
            let mut base = base_plan();
            base.prompt
                .environment_variables
                .insert("FOO".to_string(), "base".to_string());

            let prepared = SessionAssemblyBuilder::default();
            let result = apply_session_assembly(base, prepared);
            assert_eq!(
                result
                    .prompt
                    .environment_variables
                    .get("FOO")
                    .map(String::as_str),
                Some("base"),
                "prepared.env 为空时 base.env 应被保留"
            );
        }

        #[test]
        fn system_routine_identity_shape() {
            // 固化 AuthIdentity::system_routine 产出形状（E1 契约）。
            let id = agentdash_spi::platform::auth::AuthIdentity::system_routine("r-abc");
            assert_eq!(id.user_id, "system:routine:r-abc");
            assert_eq!(id.subject, "system:routine:r-abc");
            assert_eq!(id.provider.as_deref(), Some("system.routine"));
            assert!(!id.is_admin);
            assert!(id.groups.is_empty());
            assert_eq!(id.display_name.as_deref(), Some("System Routine"));
            // auth_mode = Personal 避免匹配企业级 admin 策略
            assert!(matches!(
                id.auth_mode,
                agentdash_spi::platform::auth::AuthMode::Personal
            ));
        }

        #[test]
        fn builder_with_user_input_unpacks_fields() {
            // 验证 with_user_input 一次性吸收 prompt 输入字段。
            use crate::session::UserPromptInput;
            let mut env = HashMap::new();
            env.insert("PATH".to_string(), "/usr/bin".to_string());

            let input = UserPromptInput {
                input: Some(agentdash_agent_protocol::text_user_input_blocks("hi")),
                env,
                executor_config: None,
                backend_selection: None,
            };
            let prepared = SessionAssemblyBuilder::new().with_user_input(input).build();
            assert!(
                prepared.input.is_some(),
                "with_user_input 应把 input 写入 builder"
            );
            assert_eq!(
                prepared.env.get("PATH").map(String::as_str),
                Some("/usr/bin"),
                "with_user_input 应把 env 写入 builder"
            );
        }
    }
}
