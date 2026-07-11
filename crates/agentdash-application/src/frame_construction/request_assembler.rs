//! Runtime launch assembly helpers.
//!
//! ## 设计
//!
//! Session 层保留 runtime launch 所需的共享 assembly builder，以及 lifecycle /
//! companion 这类 delivery-adjacent 组装路径。Project / Story / Routine owner
//! bootstrap composition 由 `agent_run::frame::construction` 负责，因为它产出
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
//! compose fn(各自 Spec) → FrameAssemblyBuilder → AgentFrame / FrameLaunchEnvelope
//! ```
//!
//! compose 函数内部共享 lifecycle / companion building blocks，不再重复散落。
//! 后续必须继续把 task effect / hook 迁移字段拆入 `LaunchPlan` / outbox。

use std::collections::BTreeSet;

use agentdash_application_ports::lifecycle_surface_projection as ports_lifecycle_surface;
use agentdash_domain::common::AgentConfig;
use agentdash_domain::workflow::{
    ActivityDefinition, AgentProcedureContract, LifecycleRun, WorkflowGraph,
};
use agentdash_spi::{CapabilityScope, CapabilityScopeCtx};
use agentdash_spi::{CapabilityState, SessionContextBundle, Vfs};
use async_trait::async_trait;
use uuid::Uuid;

use super::activity_activation::{
    ActivityActivationInput, activate_activity_with_platform, load_scoped_port_output_map,
};
#[cfg(test)]
use super::assembly::slice_companion_bundle;
use super::assembly::{
    FrameAssemblyBuilder, FrameAssemblyLaunchExtras, project_frame_assembly_to_frame,
};
use crate::agent_run::{ResolvedProjectAgentContext, build_project_agent_context};
use crate::capability::{
    AuthorityState, CapabilityResolver, CapabilityResolverInput, ContextContributionSource,
    ContextContributions, McpCandidates, ToolContribution, load_available_presets,
};
use crate::companion::tools::CompanionSliceMode;
use crate::context::{
    AuditTrigger, ContextBuildPhase, Contribution, SessionContextConfig, SharedContextAuditBus,
    build_session_context_bundle, emit_bundle_fragments,
};
use crate::platform_config::PlatformConfig;
use crate::repository_set::RepositorySet;
use crate::runtime::McpServerSummary;
use agentdash_application_vfs::apply_project_vfs_mount_exposure_grants;

// ═══════════════════════════════════════════════════════════════════
// SECTION 1:内部 builder prompt 投影
// ═══════════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════════
// SECTION 2:Assembler 共享服务容器
// ═══════════════════════════════════════════════════════════════════

/// `FrameRequestAssembler` 依赖的基础设施引用集合。
///
/// 由 `AppState` / 各 handler 构造后传入各 compose 函数,避免每个 compose
/// 签名都携带 6-7 个 service 参数。
pub struct FrameRequestAssembler<'a> {
    pub repos: &'a RepositorySet,
    pub platform_config: &'a PlatformConfig,
    pub lifecycle_surface_projection:
        &'a dyn ports_lifecycle_surface::LifecycleSurfaceProjectionPort,
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

impl<'a> FrameRequestAssembler<'a> {
    pub fn new(
        repos: &'a RepositorySet,
        platform_config: &'a PlatformConfig,
        lifecycle_surface_projection: &'a dyn ports_lifecycle_surface::LifecycleSurfaceProjectionPort,
    ) -> Self {
        Self {
            repos,
            platform_config,
            lifecycle_surface_projection,
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

    /// companion 的 frame builder 路径。
    pub async fn compose_companion_to_frame(
        &self,
        frame_builder: crate::agent_run::frame::AgentFrameBuilder,
        spec: CompanionParentSpec<'_>,
    ) -> Result<
        (
            crate::agent_run::frame::AgentFrameBuilder,
            FrameAssemblyLaunchExtras,
        ),
        String,
    > {
        let parent_facts = self
            .resolve_companion_parent_facts(spec.parent_session_id)
            .await?;
        let mut prepared = compose_companion(CompanionSpec {
            parent_vfs: parent_facts.parent_vfs.as_ref(),
            parent_mcp_servers: &parent_facts.parent_mcp_servers,
            parent_context_bundle: parent_facts.parent_context_bundle.as_ref(),
            slice_mode: spec.slice_mode,
            companion_executor_config: spec.companion_executor_config,
            dispatch_prompt: spec.dispatch_prompt,
            selected_context: None,
        })?;
        let selected_context = self
            .apply_selected_companion_project_agent(
                &mut prepared,
                spec.selected_project_agent_id,
                spec.selected_agent_key.as_deref(),
            )
            .await?;
        let selected_skill_keys = selected_context
            .as_ref()
            .and_then(|context| context.preset_config.skill_asset_keys.clone())
            .unwrap_or_default();
        self.project_companion_system_to_agent_run_lifecycle(
            spec.child_session_id,
            &mut prepared,
            selected_skill_keys,
        )
        .await?;
        if let Some(context) = selected_context.as_ref() {
            self.resolve_selected_companion_capabilities(&mut prepared, context, spec.slice_mode)
                .await?;
        }
        inject_companion_role_fragment(&mut prepared, spec.child_session_id);
        Ok(project_frame_assembly_to_frame(frame_builder, prepared))
    }

    /// companion + workflow 的 frame builder 路径。
    pub async fn compose_companion_with_workflow_to_frame(
        &self,
        frame_builder: crate::agent_run::frame::AgentFrameBuilder,
        spec: CompanionParentWorkflowSpec<'_>,
    ) -> Result<
        (
            crate::agent_run::frame::AgentFrameBuilder,
            FrameAssemblyLaunchExtras,
        ),
        String,
    > {
        let parent_facts = self
            .resolve_companion_parent_facts(spec.companion.parent_session_id)
            .await?;
        let selected_context = self
            .resolve_selected_companion_project_agent_context(
                spec.companion.selected_project_agent_id,
                spec.companion.selected_agent_key.as_deref(),
            )
            .await?;
        let mut prepared = compose_companion_with_workflow(
            self.repos,
            self.platform_config,
            self.lifecycle_surface_projection,
            CompanionWorkflowSpec {
                companion: CompanionSpec {
                    parent_vfs: parent_facts.parent_vfs.as_ref(),
                    parent_mcp_servers: &parent_facts.parent_mcp_servers,
                    parent_context_bundle: parent_facts.parent_context_bundle.as_ref(),
                    slice_mode: spec.companion.slice_mode,
                    companion_executor_config: spec.companion.companion_executor_config,
                    dispatch_prompt: spec.companion.dispatch_prompt,
                    selected_context,
                },
                run: spec.run,
                orchestration_id: spec.orchestration_id,
                node_path: spec.node_path,
                attempt: spec.attempt,
                lifecycle: spec.lifecycle,
                activity: spec.activity,
                workflow: spec.workflow,
                child_session_id: spec.companion.child_session_id,
            },
        )
        .await?;
        inject_companion_role_fragment(&mut prepared, spec.companion.child_session_id);
        Ok(project_frame_assembly_to_frame(frame_builder, prepared))
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

    async fn apply_selected_companion_project_agent(
        &self,
        prepared: &mut FrameAssemblyBuilder,
        selected_project_agent_id: Option<Uuid>,
        selected_agent_key_snapshot: Option<&str>,
    ) -> Result<Option<ResolvedProjectAgentContext>, String> {
        let Some(context) = self
            .resolve_selected_companion_project_agent_context(
                selected_project_agent_id,
                selected_agent_key_snapshot,
            )
            .await?
        else {
            return Ok(None);
        };
        if let Some(vfs) = prepared.vfs.as_mut() {
            apply_project_vfs_mount_exposure_grants(
                vfs,
                Some(
                    context
                        .preset_config
                        .project_vfs_mount_exposure_grants
                        .as_deref()
                        .unwrap_or_default(),
                ),
            );
        }
        prepared.executor_config = Some(context.executor_config.clone());
        Ok(Some(context))
    }

    async fn resolve_selected_companion_project_agent_context(
        &self,
        selected_project_agent_id: Option<Uuid>,
        selected_agent_key_snapshot: Option<&str>,
    ) -> Result<Option<ResolvedProjectAgentContext>, String> {
        let Some(project_agent_id) = selected_project_agent_id else {
            return Ok(None);
        };
        let agent = self
            .repos
            .project_agent_repo
            .get_by_id(project_agent_id)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("selected companion ProjectAgent {project_agent_id} 不存在"))?;
        let context = build_project_agent_context(&agent).await?;
        validate_selected_agent_key_snapshot(&context, selected_agent_key_snapshot)?;
        Ok(Some(context))
    }

    async fn project_companion_system_to_agent_run_lifecycle(
        &self,
        child_session_id: &str,
        prepared: &mut FrameAssemblyBuilder,
        explicit_skill_asset_keys: Vec<String>,
    ) -> Result<(), String> {
        let Some((address, message_stream)) =
            super::resolve_runtime_surface_refs(self.repos, child_session_id).await?
        else {
            return Ok(());
        };
        let run = self
            .repos
            .lifecycle_run_repo
            .get_by_id(address.run_id)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| {
                format!(
                    "LifecycleRun {} 不存在，无法投影 companion-system",
                    address.run_id
                )
            })?;
        let surface = self
            .lifecycle_surface_projection
            .project_lifecycle_surface(ports_lifecycle_surface::AgentRunLifecycleSurfaceInput {
                base_vfs: prepared.vfs.take(),
                address,
                message_stream: Some(message_stream),
                project_id: run.project_id,
                mode: ports_lifecycle_surface::AgentRunLifecycleSurfaceMode::CompanionChildSurface,
                explicit_skill_asset_keys,
                builtin_skills:
                    ports_lifecycle_surface::BuiltinLifecycleSkillPolicy::EnsureAndProject(vec![
                        ports_lifecycle_surface::BuiltinLifecycleSkill::CompanionSystem,
                    ]),
                node_evidence: None,
                node_projection: None,
            })
            .await
            .map_err(|error| error.to_string())?;
        let vfs = surface.vfs;
        prepared.vfs = Some(vfs.clone());
        if let Some(capability_state) = prepared.capability_state.as_mut() {
            capability_state.vfs.active = Some(vfs);
        }
        Ok(())
    }

    async fn resolve_selected_companion_capabilities(
        &self,
        prepared: &mut FrameAssemblyBuilder,
        context: &ResolvedProjectAgentContext,
        slice_mode: CompanionSliceMode,
    ) -> Result<(), String> {
        let active_vfs = prepared.vfs.as_ref();
        let cap_input = CapabilityResolverInput {
            owner_ctx: CapabilityScopeCtx::Project {
                project_id: context.project_agent.project_id,
            },
            contributions: vec![ContextContributions {
                source: ContextContributionSource::Agent,
                tool: Some(ToolContribution {
                    directives: context
                        .preset_config
                        .capability_directives
                        .clone()
                        .unwrap_or_default(),
                    has_active_workflow: false,
                }),
                companion: None,
            }],
            mcp_candidates: McpCandidates {
                presets: load_available_presets(self.repos, context.project_agent.project_id).await,
            },
            mcp_runtime_context: Some(crate::mcp_preset::McpRuntimeBindingContext {
                vfs: active_vfs,
                backend_anchor: None,
            }),
            capability_context: None,
            authority_state: AuthorityState::companion_child(),
        };
        let mut capability_state =
            CapabilityResolver::resolve_checked(&cap_input, self.platform_config)?;
        capability_state = CapabilityResolver::apply_companion_slice(capability_state, slice_mode);
        // Skill baseline / guidelines / memory 由 launch-time 单入口在 runtime surface
        // 闭包后从最终 launch VFS 统一派生，companion route 不再各自 derive skill baseline。

        prepared.mcp_servers = capability_state.tool.mcp_servers.clone();
        prepared.capability_state = Some(capability_state);
        Ok(())
    }
}

/// lifecycle_node 的 frame builder 路径（free-standing 版本）。
#[allow(clippy::too_many_arguments)]
pub async fn compose_lifecycle_node_to_frame_with_audit(
    frame_builder: crate::agent_run::frame::AgentFrameBuilder,
    repos: &RepositorySet,
    platform_config: &PlatformConfig,
    lifecycle_surface_projection: &dyn ports_lifecycle_surface::LifecycleSurfaceProjectionPort,
    spec: LifecycleNodeSpec<'_>,
    audit_bus: Option<SharedContextAuditBus>,
    audit_session_key: Option<&str>,
    audit_run_id: Option<&str>,
    audit_agent_id: Option<&str>,
) -> Result<
    (
        crate::agent_run::frame::AgentFrameBuilder,
        FrameAssemblyLaunchExtras,
    ),
    String,
> {
    let prepared = compose_lifecycle_node_with_audit(
        repos,
        platform_config,
        lifecycle_surface_projection,
        spec,
        audit_bus,
        audit_session_key,
        audit_run_id,
        audit_agent_id,
    )
    .await?;
    Ok(project_frame_assembly_to_frame(frame_builder, prepared))
}

#[allow(clippy::too_many_arguments)]
async fn compose_lifecycle_node_with_audit(
    repos: &RepositorySet,
    platform_config: &PlatformConfig,
    lifecycle_surface_projection: &dyn ports_lifecycle_surface::LifecycleSurfaceProjectionPort,
    spec: LifecycleNodeSpec<'_>,
    audit_bus: Option<SharedContextAuditBus>,
    audit_session_key: Option<&str>,
    audit_run_id: Option<&str>,
    audit_agent_id: Option<&str>,
) -> Result<FrameAssemblyBuilder, String> {
    let owner_ctx = CapabilityScopeCtx::Project {
        project_id: spec.run.project_id,
    };

    let artifact_scope = ports_lifecycle_surface::RuntimeNodeArtifactScope {
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
            available_presets: load_available_presets(repos, spec.run.project_id).await,
            authority_state: AuthorityState::main_project_agent(),
            agent_tool_directives: Vec::new(),
            companion_slice_mode: None,
            baseline_override: None,
            tool_directives: &[],
            ready_port_keys: ready_port_keys.clone(),
            available_companions: Vec::new(),
        },
        platform_config,
    )?;
    if let Some((address, message_stream)) = match audit_session_key {
        Some(session_id) => super::resolve_runtime_surface_refs(repos, session_id).await?,
        None => None,
    } {
        let base_vfs = activation.lifecycle_vfs.clone();
        let node_projection = ports_lifecycle_surface::OrchestrationNodeProjectionInput {
            run_id: spec.run.id,
            orchestration_id: spec.orchestration_id,
            node_path: spec.node_path.to_string(),
            lifecycle_key: spec.lifecycle_key.to_string(),
            attempt: spec.attempt,
            writable_port_keys: spec
                .activity
                .output_ports
                .iter()
                .map(|port| port.key.clone())
                .collect(),
        };
        let surface = lifecycle_surface_projection
            .project_lifecycle_surface(ports_lifecycle_surface::AgentRunLifecycleSurfaceInput {
                base_vfs: Some(base_vfs),
                address,
                message_stream: Some(message_stream),
                project_id: spec.run.project_id,
                mode: ports_lifecycle_surface::AgentRunLifecycleSurfaceMode::WorkflowNodeExecutionSurface,
                explicit_skill_asset_keys: Vec::new(),
                builtin_skills:
                    ports_lifecycle_surface::BuiltinLifecycleSkillPolicy::EnsureAndProject(vec![
                        ports_lifecycle_surface::BuiltinLifecycleSkill::CompanionSystem,
                    ]),
                node_evidence: Some(node_projection.evidence_ref()),
                node_projection: Some(node_projection),
            })
            .await
            .map_err(|error| error.to_string())?;
        activation.lifecycle_vfs = surface.vfs;
        activation.lifecycle_mount = surface.lifecycle_mount;
    }

    // Lifecycle node 与 owner 路径都追加 SessionPlan contribution，保持 vfs /
    // tools / persona / workflow / runtime_policy 的统一画像。
    let lifecycle_mcp_runtime: Vec<McpServerSummary> = activation
        .mcp_servers
        .iter()
        .map(crate::runtime_bridge::runtime_mcp_server_to_summary)
        .collect();
    let lifecycle_plan = crate::frame_construction::plan::build_session_plan_fragments(
        crate::frame_construction::plan::SessionPlanInput {
            scope: CapabilityScope::Project,
            phase: crate::frame_construction::plan::SessionPlanPhase::ProjectAgent,
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
    if let (Some(bus), Some(run_id), Some(agent_id)) =
        (audit_bus.as_ref(), audit_run_id, audit_agent_id)
    {
        emit_bundle_fragments(
            bus.as_ref(),
            &context_bundle,
            run_id,
            agent_id,
            AuditTrigger::ComposerRebuild,
        );
    }
    Ok(FrameAssemblyBuilder::new()
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
    activation: &ports_lifecycle_surface::ActivityActivation,
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

    let mut delivery_parts = vec![format!(
        "## Lifecycle Delivery Contract\n{}\n\n完成当前节点后调用 `complete_lifecycle_node` 提交总结与产物。",
        activation.kickoff_prompt.title_line
    )];
    if !activation.kickoff_prompt.output_section.trim().is_empty() {
        delivery_parts.push(activation.kickoff_prompt.output_section.trim().to_string());
    }
    if !activation.kickoff_prompt.input_section.trim().is_empty() {
        delivery_parts.push(activation.kickoff_prompt.input_section.trim().to_string());
    }
    fragments.push(agentdash_spi::ContextFragment {
        slot: "workflow_context".to_string(),
        label: "lifecycle_delivery_contract".to_string(),
        order: 84,
        strategy: agentdash_spi::MergeStrategy::Append,
        scope: agentdash_spi::ContextFragment::default_scope(),
        source: "lifecycle:delivery_contract".to_string(),
        content: delivery_parts.join("\n\n"),
    });

    Contribution::fragments_only(fragments)
}

/// 为 companion 子 session 注入 agent_identity fragment，声明子代理身份。
///
/// 确保 `prepared.context_bundle` 存在，并追加一个 slot=`agent_identity` 的 fragment。
/// 该 fragment 会被 `preparation.rs::find_agent_identity_markdown` 拾取，
/// 最终渲染进 Identity Frame 的 "## Agent Identity" section。
fn inject_companion_role_fragment(prepared: &mut FrameAssemblyBuilder, child_session_id: &str) {
    use agentdash_spi::context::bundle::SessionContextBundle;
    use agentdash_spi::context::injection::{ContextFragment, MergeStrategy};

    let role_content = "\
## Agent Identity

You are operating as a companion sub-agent dispatched by a parent agent session.
- Complete the assigned task, then call `companion_respond` to return your results.
- Do not interact with the user directly — your output flows back through the parent agent.
- Focus exclusively on the dispatched task; do not take autonomous actions beyond its scope.
- If you need clarification, state what is unclear in your response rather than guessing.";

    let fragment = ContextFragment {
        slot: "agent_identity".to_string(),
        label: "companion_role".to_string(),
        order: 0,
        strategy: MergeStrategy::Override,
        scope: ContextFragment::default_scope(),
        source: "builtin:companion_role".to_string(),
        content: role_content.to_string(),
    };

    let bundle = prepared.context_bundle.get_or_insert_with(|| {
        let session_uuid =
            uuid::Uuid::parse_str(child_session_id).unwrap_or_else(|_| uuid::Uuid::new_v4());
        SessionContextBundle::new(session_uuid, "companion_bootstrap")
    });
    bundle.bootstrap_fragments.push(fragment);
}

/// Companion 子 session 组装(脱离 `FrameRequestAssembler`,companion tool
/// 在父 session 作用域内即可完成,不需要 assembler 的完整服务依赖)。
///
/// 内部委托给 `FrameAssemblyBuilder::apply_companion_slice`。
fn compose_companion(spec: CompanionSpec<'_>) -> Result<FrameAssemblyBuilder, String> {
    Ok(FrameAssemblyBuilder::new()
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

fn dedupe_runtime_mcp_servers(servers: &mut Vec<agentdash_spi::RuntimeMcpServer>) {
    let mut seen = BTreeSet::<String>::new();
    servers.retain(|server| seen.insert(server.name.clone()));
}

fn validate_selected_agent_key_snapshot(
    context: &ResolvedProjectAgentContext,
    selected_agent_key_snapshot: Option<&str>,
) -> Result<(), String> {
    let Some(snapshot) = selected_agent_key_snapshot
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(());
    };
    let expected = context.project_agent.name.as_str();
    if snapshot.eq_ignore_ascii_case(expected) {
        return Ok(());
    }
    Err(format!(
        "selected companion agent_key snapshot `{snapshot}` 与 ProjectAgent `{}` 的 canonical name `{expected}` 不一致",
        context.project_agent.id
    ))
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
    pub parent_mcp_servers: &'a [agentdash_spi::RuntimeMcpServer],
    /// 父 session 的结构化上下文 Bundle，companion 直接继承（按 slice_mode 过滤）。
    pub parent_context_bundle: Option<&'a SessionContextBundle>,
    pub slice_mode: CompanionSliceMode,
    pub companion_executor_config: AgentConfig,
    pub dispatch_prompt: String,
    pub selected_context: Option<ResolvedProjectAgentContext>,
}

pub struct CompanionParentSpec<'a> {
    pub parent_session_id: &'a str,
    pub child_session_id: &'a str,
    pub slice_mode: CompanionSliceMode,
    pub companion_executor_config: AgentConfig,
    pub dispatch_prompt: String,
    pub selected_project_agent_id: Option<Uuid>,
    pub selected_agent_key: Option<String>,
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
    pub(crate) parent_mcp_servers: Vec<agentdash_spi::RuntimeMcpServer>,
    pub(crate) parent_context_bundle: Option<SessionContextBundle>,
}

/// Companion + Workflow 组合 compose 输入。
pub struct CompanionWorkflowSpec<'a> {
    pub companion: CompanionSpec<'a>,
    pub child_session_id: &'a str,
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
/// 通过 `FrameAssemblyBuilder` 声明式组合两个关注点。
async fn compose_companion_with_workflow(
    repos: &RepositorySet,
    platform_config: &PlatformConfig,
    lifecycle_surface_projection: &dyn ports_lifecycle_surface::LifecycleSurfaceProjectionPort,
    spec: CompanionWorkflowSpec<'_>,
) -> Result<FrameAssemblyBuilder, String> {
    use crate::companion::tools::build_companion_execution_slice;

    let project_id = spec.run.project_id;
    let comp = &spec.companion;

    // ── 1. Companion VFS slice 作为基础 ──
    let mut slice =
        build_companion_execution_slice(comp.parent_vfs, comp.parent_mcp_servers, comp.slice_mode)?;
    if let Some(context) = comp.selected_context.as_ref()
        && let Some(vfs) = slice.vfs.as_mut()
    {
        apply_project_vfs_mount_exposure_grants(
            vfs,
            Some(
                context
                    .preset_config
                    .project_vfs_mount_exposure_grants
                    .as_deref()
                    .unwrap_or_default(),
            ),
        );
    }

    // ── 2. Workflow activity activation（产出 lifecycle mount + 能力 + MCP） ──
    let owner_ctx = CapabilityScopeCtx::Project { project_id };
    let artifact_scope = ports_lifecycle_surface::RuntimeNodeArtifactScope {
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
            workflow_contract: spec.workflow.map(|workflow| &workflow.contract),
            base_vfs: slice.vfs.as_ref(),
            run_id: spec.run.id,
            orchestration_id: spec.orchestration_id,
            node_path: spec.node_path,
            attempt: spec.attempt,
            lifecycle_key: &spec.lifecycle.key,
            available_presets: load_available_presets(repos, project_id).await,
            authority_state: AuthorityState::companion_child(),
            agent_tool_directives: comp
                .selected_context
                .as_ref()
                .and_then(|context| context.preset_config.capability_directives.clone())
                .unwrap_or_default(),
            companion_slice_mode: Some(comp.slice_mode),
            baseline_override: None,
            tool_directives: &[],
            ready_port_keys,
            available_companions: Vec::new(),
        },
        platform_config,
    )?;
    if let Some((address, message_stream)) =
        super::resolve_runtime_surface_refs(repos, spec.child_session_id).await?
    {
        let base_vfs = activation.lifecycle_vfs.clone();
        let node_projection = ports_lifecycle_surface::OrchestrationNodeProjectionInput {
            run_id: spec.run.id,
            orchestration_id: spec.orchestration_id,
            node_path: spec.node_path.to_string(),
            lifecycle_key: spec.lifecycle.key.clone(),
            attempt: spec.attempt,
            writable_port_keys: spec
                .activity
                .output_ports
                .iter()
                .map(|port| port.key.clone())
                .collect(),
        };
        let surface = lifecycle_surface_projection
            .project_lifecycle_surface(ports_lifecycle_surface::AgentRunLifecycleSurfaceInput {
                base_vfs: Some(base_vfs),
                address,
                message_stream: Some(message_stream),
                project_id,
                mode: ports_lifecycle_surface::AgentRunLifecycleSurfaceMode::WorkflowNodeExecutionSurface,
                explicit_skill_asset_keys: comp
                    .selected_context
                    .as_ref()
                    .and_then(|context| context.preset_config.skill_asset_keys.clone())
                    .unwrap_or_default(),
                builtin_skills:
                    ports_lifecycle_surface::BuiltinLifecycleSkillPolicy::EnsureAndProject(vec![
                        ports_lifecycle_surface::BuiltinLifecycleSkill::CompanionSystem,
                    ]),
                node_evidence: Some(node_projection.evidence_ref()),
                node_projection: Some(node_projection),
            })
            .await
            .map_err(|error| error.to_string())?;
        activation.lifecycle_vfs = surface.vfs;
        activation.lifecycle_mount = surface.lifecycle_mount;
    }
    dedupe_runtime_mcp_servers(&mut activation.mcp_servers);

    // ── 3. 用 builder 组合 companion + workflow 两个层 ──
    //
    // 继承父 bundle 并叠加 workflow injection 片段。workflow injection 作为独立
    // fragment 注入 Bundle，保持 workflow injection 与其他上下文同一合并路径。
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

    Ok(FrameAssemblyBuilder::new()
        .with_vfs(slice.vfs.ok_or_else(|| {
            "companion workflow compose 未产出 VFS，拒绝构造 child session".to_string()
        })?)
        .apply_lifecycle_activation(
            &activation,
            Some(
                comp.selected_context
                    .as_ref()
                    .map(|context| context.executor_config.clone())
                    .unwrap_or_else(|| comp.companion_executor_config.clone()),
            ),
        )
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
    use agentdash_application_ports::lifecycle_surface_projection::{
        KickoffPromptFragment, LifecycleMountSurface, lifecycle_mount_overlay_for_surface,
    };
    use agentdash_domain::agent::ProjectAgent;
    use agentdash_domain::common::AgentPresetConfig;
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

    fn selected_context_with_name(name: &str) -> ResolvedProjectAgentContext {
        let project_id = Uuid::new_v4();
        let project_agent = ProjectAgent::new(project_id, name, "PI_AGENT");
        ResolvedProjectAgentContext {
            key: project_agent.id.to_string(),
            display_name: name.to_string(),
            description: String::new(),
            executor_config: agentdash_spi::AgentConfig::new("PI_AGENT"),
            preset_config: AgentPresetConfig::default(),
            preset_name: Some(name.to_string()),
            source: "test".to_string(),
            project_agent,
        }
    }

    #[test]
    fn selected_agent_key_snapshot_must_match_project_agent_name() {
        let context = selected_context_with_name("reviewer");

        assert!(validate_selected_agent_key_snapshot(&context, Some("reviewer")).is_ok());
        assert!(validate_selected_agent_key_snapshot(&context, Some("Reviewer")).is_ok());
        assert!(validate_selected_agent_key_snapshot(&context, None).is_ok());
        assert!(validate_selected_agent_key_snapshot(&context, Some("planner")).is_err());
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

    fn test_lifecycle_mount(
        run_id: Uuid,
        orchestration_id: Uuid,
        node_path: &str,
        lifecycle_key: &str,
        writable_port_keys: Vec<String>,
    ) -> agentdash_domain::common::Mount {
        lifecycle_mount_overlay_for_surface(&LifecycleMountSurface {
            run_id,
            orchestration_id,
            node_path: node_path.to_string(),
            lifecycle_key: lifecycle_key.to_string(),
            attempt: 1,
            writable_port_keys,
        })
        .mounts
        .into_iter()
        .next()
        .expect("lifecycle mount")
    }

    fn test_activity_activation(run_id: Uuid) -> ports_lifecycle_surface::ActivityActivation {
        let lifecycle_mount = test_lifecycle_mount(
            run_id,
            Uuid::new_v4(),
            "test-node",
            "test-lifecycle",
            vec!["report".to_string()],
        );
        ports_lifecycle_surface::ActivityActivation {
            capability_state: Default::default(),
            mcp_servers: Vec::new(),
            capability_keys: BTreeSet::new(),
            kickoff_prompt: KickoffPromptFragment {
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
        let prepared = FrameAssemblyBuilder::new()
            .append_lifecycle_mount(LifecycleMountSurface {
                run_id: Uuid::new_v4(),
                orchestration_id: Uuid::new_v4(),
                node_path: "test-node".to_string(),
                lifecycle_key: "test-lifecycle".to_string(),
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

        let prepared = FrameAssemblyBuilder::new()
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
    fn lifecycle_context_contribution_contains_workflow_assignment_fragments() {
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
        let mount = test_lifecycle_mount(
            run.id,
            uuid::Uuid::new_v4(),
            "implement",
            &lifecycle.key,
            vec!["summary".into()],
        );
        let activation = ports_lifecycle_surface::ActivityActivation {
            capability_state: Default::default(),
            mcp_servers: vec![],
            capability_keys: BTreeSet::from(["workflow_management".to_string()]),
            kickoff_prompt: KickoffPromptFragment {
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
            .filter(|f| f.slot == "workflow_context")
            .map(|f| f.content.clone())
            .collect::<Vec<_>>()
            .join("\n\n");

        assert!(relevant_content.contains("## Lifecycle Node"));
        assert!(relevant_content.contains("交付可验证实现"));
        assert!(relevant_content.contains("complete_lifecycle_node"));
        assert!(relevant_content.contains("## 必须交付的产出"));
        assert!(relevant_content.contains("## 输入上下文"));
        assert!(!relevant_content.contains("workflow_management"));
        assert!(
            !bundle
                .filter_for(agentdash_spi::FragmentScope::RuntimeAgent)
                .any(|f| f.slot == "runtime_policy")
        );
    }

    mod frame_assembly_builder_tests {
        use super::super::*;
        use std::collections::HashMap;

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
            use agentdash_application_ports::launch::LaunchPromptInput;
            let mut env = HashMap::new();
            env.insert("PATH".to_string(), "/usr/bin".to_string());

            let input = LaunchPromptInput {
                input: Some(agentdash_agent_protocol::text_user_input_blocks("hi")),
                environment_variables: env,
                executor_config: None,
            };
            let prepared = FrameAssemblyBuilder::new().with_user_input(input).build();
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
