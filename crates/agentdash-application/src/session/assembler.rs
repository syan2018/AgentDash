//! Runtime launch assembly helpers.
//!
//! ## 设计
//!
//! Session 层保留 runtime launch 所需的共享 assembly builder，以及 lifecycle /
//! companion 这类 delivery-adjacent 组装路径。Project / Story / Routine owner
//! bootstrap composition 由 `workflow::frame_construction` 负责，因为它产出
//! 写入 `AgentFrame` 的 owner runtime surface。
//!
//! | 路径 | 实现入口 |
//! |---|---|
//! | Workflow AgentNode | `workflow::orchestrator::start_agent_node_prompt` → `compose_lifecycle_node` |
//! | Companion | `companion::tools` → `compose_companion` |
//!
//! 这些路径通过组合器内部草稿收束 VFS / capability / MCP / context bundle /
//! prompt 来源，并合入 frame construction handoff:
//!
//! ```text
//! compose fn(各自 Spec) → SessionAssemblyBuilder → AgentFrame / FrameLaunchEnvelope
//! ```
//!
//! compose 函数内部共享 lifecycle / companion building blocks，不再重复散落。
//! 后续必须继续把 task effect / hook 迁移字段拆入 `LaunchPlan` / outbox。

use std::{collections::BTreeSet, path::PathBuf, sync::Arc};

use agentdash_domain::canvas::CanvasRepository;
use agentdash_domain::common::AgentConfig;
use agentdash_domain::workflow::{
    ActivityDefinition, AgentProcedureContract, LifecycleRun, WorkflowGraph,
};
use agentdash_spi::{CapabilityScope, CapabilityScopeCtx, SkillDiscoveryProvider};
use agentdash_spi::{CapabilityState, SessionContextBundle, Vfs};
use async_trait::async_trait;
use uuid::Uuid;

use crate::capability::load_available_presets;
use crate::companion::{
    skill_projection::project_companion_system_skill_to_activation, tools::CompanionSliceMode,
};
use crate::context::{
    AuditTrigger, ContextBuildPhase, Contribution, SessionContextConfig, SharedContextAuditBus,
    build_session_context_bundle, emit_bundle_fragments,
};
use crate::platform_config::PlatformConfig;
use crate::repository_set::RepositorySet;
use crate::runtime::RuntimeMcpServer;
use crate::session::assembly_builder::SessionAssemblyBuilder;
#[cfg(test)]
use crate::session::assembly_builder::slice_companion_bundle;
use crate::vfs::VfsService;
use crate::workflow::{
    ActivityActivationInput, RuntimeNodeArtifactScope, activate_activity_with_platform,
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
    /// Host Integration 提供的静态/dynamic skill discovery 来源。
    ///
    /// 只在 session capability baseline 中按 provider 返回的 exposure 投影默认可见 skills。
    pub extra_skill_dirs: &'a [PathBuf],
    pub skill_discovery_providers: &'a [Arc<dyn SkillDiscoveryProvider>],
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
            extra_skill_dirs: &[],
            skill_discovery_providers: &[],
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

    pub fn with_skill_discovery(
        mut self,
        extra_skill_dirs: &'a [PathBuf],
        skill_discovery_providers: &'a [Arc<dyn SkillDiscoveryProvider>],
    ) -> Self {
        self.extra_skill_dirs = extra_skill_dirs;
        self.skill_discovery_providers = skill_discovery_providers;
        self
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
        })?;
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
            .await
            .ok_or_else(|| {
                format!(
                    "companion parent session `{parent_session_id}` 缺少 capability state，拒绝构造 child session"
                )
            })?;
        Ok(CompanionParentFacts {
            parent_vfs: parent_capability_state.vfs.active.clone(),
            parent_mcp_servers: parent_capability_state.tool.mcp_servers.clone(),
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
            base_vfs: spec.base_vfs,
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
    )?;
    project_companion_system_skill_to_activation(repos, spec.run.project_id, &mut activation)
        .await
        .map_err(|error| error.to_string())?;

    // Lifecycle node 与 owner 路径都追加 SessionPlan contribution，保持 vfs /
    // tools / persona / workflow / runtime_policy 的统一画像。
    let lifecycle_mcp_runtime: Vec<RuntimeMcpServer> = activation
        .mcp_servers
        .iter()
        .map(crate::runtime_bridge::mcp_declaration_to_runtime_server)
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

    if let Some(workflow_contract) = spec.workflow_contract
        && let Some(content) = crate::context::rendering::render_workflow_injection(
            &workflow_contract.injection,
            crate::context::rendering::WorkflowInjectionMode::Declarative,
        )
    {
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
pub(in crate::session) fn compose_companion(
    spec: CompanionSpec<'_>,
) -> Result<SessionAssemblyBuilder, String> {
    Ok(SessionAssemblyBuilder::new()
        .apply_companion_slice(
            spec.parent_vfs,
            spec.parent_mcp_servers,
            spec.parent_context_bundle,
            spec.slice_mode,
            spec.companion_executor_config,
            spec.dispatch_prompt,
        )?
        .build())
}

// ═══════════════════════════════════════════════════════════════════
// SECTION 5:其余 Spec 结构 + 辅助函数
// ═══════════════════════════════════════════════════════════════════

/// Lifecycle AgentNode compose 输入。
pub struct LifecycleNodeSpec<'a> {
    pub run: &'a LifecycleRun,
    pub orchestration_id: Uuid,
    pub node_path: &'a str,
    pub attempt: u32,
    pub lifecycle_key: &'a str,
    pub activity: &'a ActivityDefinition,
    pub workflow_contract: Option<&'a AgentProcedureContract>,
    pub base_vfs: Option<&'a Vfs>,
    pub workflow_label: Option<&'a str>,
    pub inherited_executor_config: Option<AgentConfig>,
}

/// Companion compose 输入。
pub struct CompanionSpec<'a> {
    pub parent_vfs: Option<&'a Vfs>,
    pub parent_mcp_servers: &'a [agentdash_spi::RuntimeMcpServerDeclaration],
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
    pub(crate) parent_mcp_servers: Vec<agentdash_spi::RuntimeMcpServerDeclaration>,
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
        build_companion_execution_slice(comp.parent_vfs, comp.parent_mcp_servers, comp.slice_mode)?;

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
            base_vfs: slice.vfs.as_ref(),
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
    )?;

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
        .with_vfs(slice.vfs.ok_or_else(|| {
            "companion workflow compose 未产出 VFS，拒绝构造 child session".to_string()
        })?)
        .apply_lifecycle_activation(&activation, Some(comp.companion_executor_config.clone()))
        .append_mcp_servers(slice.mcp_servers)
        .with_optional_context_bundle(merged_bundle)
        .with_input(user_input)
        .build())
}

// ═══════════════════════════════════════════════════════════════════
// SECTION 6:内部 helper
// ═══════════════════════════════════════════════════════════════════

#[cfg(test)]
#[allow(deprecated)]
mod tests {
    use super::*;
    use crate::vfs::build_lifecycle_mount_with_ports;
    use agentdash_domain::workflow::{
        ActivityDefinition, ActivityExecutorSpec, AgentActivityExecutorSpec, AgentProcedure,
        AgentProcedureContract, DefinitionSource, InputPortDefinition, LifecycleNodeType,
        OutputPortDefinition, WorkflowGraph, WorkflowGraphDraft, WorkflowInjectionSpec,
    };
    use std::collections::BTreeSet;

    // ── companion bundle fragment 裁剪回归 ──

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
        let lifecycle = WorkflowGraph::new(WorkflowGraphDraft {
            project_id,
            key: "dev".to_string(),
            name: "Dev".to_string(),
            description: "dev lifecycle".to_string(),
            source: DefinitionSource::BuiltinSeed,
            entry_activity_key: "implement".to_string(),
            activities: vec![activity.clone()],
            transitions: vec![],
        })
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
            base_vfs: None,
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
    // 这些测试锁定 `apply_session_assembly` 的 frame surface handoff 行为：
    // - frame surface draft 承载 capability / VFS / MCP；
    // - prepared VFS 优先表达 compose 后的最终 mount 组合；
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

        fn runtime_mcp_declaration(
            name: &str,
            url: &str,
        ) -> agentdash_spi::RuntimeMcpServerDeclaration {
            agentdash_spi::RuntimeMcpServerDeclaration {
                name: name.to_string(),
                transport: agentdash_spi::McpTransportConfig::Http {
                    url: url.to_string(),
                    headers: vec![],
                },
                uses_relay: false,
            }
        }

        #[test]
        fn prepared_surface_is_handed_off_as_frame_surface_draft() {
            let base = base_plan();
            let mut capability_state =
                agentdash_spi::CapabilityState::from_clusters([agentdash_spi::ToolCluster::Read]);
            let vfs = Vfs {
                mounts: Vec::new(),
                default_mount_id: Some("prepared-mount".to_string()),
                source_project_id: None,
                source_story_id: None,
                links: Vec::new(),
            };
            capability_state.vfs.active = Some(vfs.clone());
            let mcp_servers = vec![runtime_mcp_declaration("compose_a", "http://a")];
            let prepared = SessionAssemblyBuilder {
                vfs: Some(vfs),
                capability_state: Some(capability_state),
                mcp_servers,
                ..Default::default()
            };

            let result = apply_session_assembly(base, prepared);
            let draft = result
                .projections
                .frame_surface_draft
                .expect("frame surface draft");

            assert_eq!(
                draft.vfs.and_then(|vfs| vfs.default_mount_id).as_deref(),
                Some("prepared-mount")
            );
            assert_eq!(draft.mcp_servers[0].name, "compose_a");
            assert!(draft.capability_state.is_some());
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
