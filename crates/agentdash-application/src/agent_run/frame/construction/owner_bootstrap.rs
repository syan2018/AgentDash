//! Owner bootstrap frame composer.
//!
//! Project/Story/Routine owner surface composition belongs to frame construction because it
//! produces the `FrameSurfaceDraft` written into `AgentFrame` and handed to runtime launch.

use std::{collections::BTreeSet, path::PathBuf, sync::Arc};

use agentdash_domain::agent::ProjectAgent;
use agentdash_domain::canvas::CanvasRepository;
use agentdash_domain::common::{AgentConfig, AgentVfsAccessGrant};
use agentdash_domain::project::Project;
use agentdash_domain::story::Story;
use agentdash_domain::workflow::ToolCapabilityDirective;
use agentdash_domain::workspace::Workspace;
use agentdash_spi::context::capability::CompanionAgentEntry;
use agentdash_spi::{AuthIdentity, CapabilityScopeCtx, SkillDiscoveryProvider};
use agentdash_spi::{
    CapabilityState, ContextFragment, MergeStrategy, SessionContextBundle, ToolCapability,
    ToolCluster, Vfs,
};
use uuid::Uuid;

use crate::agent_run::frame::builder::AgentFrameBuilder;
use crate::canvas::append_visible_canvas_mounts;
use crate::capability::{
    AuthorityState, CapabilityResolver, CapabilityResolverInput, CompanionContribution,
    ContextContributionSource, ContextContributions, McpCandidates, ToolContribution,
    load_available_presets, tool_directives_from_active_workflow,
    tool_directives_from_active_workflow_projection,
};
use crate::companion::skill_projection::{
    append_companion_system_skill_key, ensure_companion_system_skill_asset,
};
use crate::context::{
    AuditTrigger, ContextBuildPhase, Contribution, SessionContextConfig, SharedContextAuditBus,
    build_session_context_bundle, emit_bundle_fragments, resolve_workspace_declared_sources,
};
use crate::lifecycle::surface::surface_projector::{
    AgentRunLifecycleNodeRuntimeFacts, AgentRunLifecycleSessionEvidenceFacts,
    AgentRunLifecycleSkillProjectionFacts,
};
use crate::lifecycle::{
    ActiveWorkflowProjection, AgentRunLifecycleSurfaceProjector, AgentRunRuntimeAddress,
    BuiltinLifecycleSkill, MessageStreamProjectionRef, MessageStreamTraceKind,
    OrchestrationNodeProjectionInput, project_active_workflow_lifecycle_vfs,
    writable_port_keys_for_active_workflow,
};
use crate::mcp_preset::{McpRuntimeBindingContext, resolve_preset_mcp_server};
use crate::platform_config::PlatformConfig;
use crate::project::context_builder::{ProjectContextBuildInput, contribute_project_context};
use crate::repository_set::RepositorySet;
use crate::runtime::McpServerSummary;
use crate::runtime_bridge::runtime_mcp_servers_to_summaries;
use crate::session::assembly_builder::{SessionAssemblyBuilder, project_assembly_to_frame};
use crate::session::capability_projection::{
    SessionCapabilityProjectionInput, derive_session_skill_baseline,
};
use crate::story::context_builder::{StoryContextBuildInput, contribute_story_context};
use crate::vfs::{SessionMountTarget, VfsService, apply_agent_vfs_access_grants};
use crate::workspace::BackendAvailability;
use crate::workspace_module::skill_projection::project_workspace_module_system_skill_to_vfs;

/// Owner 级 frame bootstrap 的 owner scope 描述。
#[allow(dead_code)]
pub(crate) enum OwnerScope<'a> {
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

/// agent 级 MCP 配置，来自 ProjectAgent / Routine agent context。
#[derive(Default, Clone)]
pub(crate) struct AgentLevelMcp {
    pub preset_mcp_presets: Vec<agentdash_domain::mcp_preset::McpPreset>,
}

/// Owner bootstrap compose 的完整输入。
pub(crate) struct OwnerBootstrapSpec<'a> {
    pub owner: OwnerScope<'a>,
    pub identity: Option<&'a AuthIdentity>,
    /// `LifecycleSubjectAssociation` 动态解析出的 subject 上下文。
    ///
    /// ProjectAgent 仍以 Project owner 启动；Story/Task 作为 subject profile 在这里叠加，
    /// 从而让 owner composer 只产生一份 AgentFrame surface。
    pub subject_context_contributions: Vec<Contribution>,
    pub subject_owner_ctx: Option<CapabilityScopeCtx>,
    pub subject_workspace: Option<&'a Workspace>,
    pub executor_config: AgentConfig,
    pub user_input: Vec<agentdash_agent_protocol::UserInputBlock>,
    pub agent_mcp: AgentLevelMcp,
    pub agent_tool_directives: Vec<ToolCapabilityDirective>,
    pub agent_skill_asset_keys: Vec<String>,
    pub agent_vfs_access_grants: Vec<AgentVfsAccessGrant>,
    pub request_mcp_servers: Vec<agentdash_spi::RuntimeMcpServer>,
    pub existing_vfs: Option<Vfs>,
    pub visible_canvas_mount_ids: Vec<String>,
    /// ProjectAgent preset 声明的 workspace module 可见性白名单。
    ///
    /// `None` / `Some([])` 代表全集可见，非空列表代表 allowlist。
    pub visible_workspace_module_refs: Option<Vec<String>>,
    pub active_workflow: Option<ActiveWorkflowProjection>,
    pub launch_path: OwnerPromptLaunchPath,
    pub audit_session_key: Option<String>,
    pub caller_agent_id: Option<Uuid>,
}

/// Owner bootstrap 的 prompt launch path。
pub(crate) enum OwnerPromptLaunchPath {
    OwnerBootstrap,
    RepositoryRehydrate {
        prebuilt_continuation_bundle: Option<SessionContextBundle>,
        include_owner_bundle: bool,
    },
    Plain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OwnerAuditLaunchPath {
    Bootstrap,
    Rehydrate,
    Plain,
}

pub(crate) struct OwnerBootstrapComposer<'a> {
    pub vfs_service: &'a VfsService,
    pub canvas_repo: &'a dyn CanvasRepository,
    pub availability: &'a dyn BackendAvailability,
    pub repos: &'a RepositorySet,
    pub platform_config: &'a PlatformConfig,
    pub audit_bus: Option<SharedContextAuditBus>,
    pub extra_skill_dirs: &'a [PathBuf],
    pub skill_discovery_providers: &'a [Arc<dyn SkillDiscoveryProvider>],
}

impl<'a> OwnerBootstrapComposer<'a> {
    pub(crate) fn new(
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
            extra_skill_dirs: &[],
            skill_discovery_providers: &[],
        }
    }

    pub(crate) fn with_audit_bus(mut self, bus: SharedContextAuditBus) -> Self {
        self.audit_bus = Some(bus);
        self
    }

    pub(crate) fn with_skill_discovery(
        mut self,
        extra_skill_dirs: &'a [PathBuf],
        skill_discovery_providers: &'a [Arc<dyn SkillDiscoveryProvider>],
    ) -> Self {
        self.extra_skill_dirs = extra_skill_dirs;
        self.skill_discovery_providers = skill_discovery_providers;
        self
    }

    pub(crate) async fn compose_owner_bootstrap_to_frame(
        &self,
        frame_builder: AgentFrameBuilder,
        spec: OwnerBootstrapSpec<'_>,
    ) -> Result<(AgentFrameBuilder, crate::session::AssemblyLaunchExtras), String> {
        let prepared = self.compose_owner_bootstrap(spec).await?;
        Ok(project_assembly_to_frame(frame_builder, prepared))
    }

    async fn compose_owner_bootstrap(
        &self,
        mut spec: OwnerBootstrapSpec<'_>,
    ) -> Result<SessionAssemblyBuilder, String> {
        let project_id = spec.owner.project_id();
        let owner_ctx = spec
            .subject_owner_ctx
            .clone()
            .unwrap_or_else(|| spec.owner.owner_ctx());
        let subject_context_contributions = std::mem::take(&mut spec.subject_context_contributions);
        let active_workflow = spec.active_workflow.clone();
        let mut vfs = self
            .prepare_owner_bootstrap_vfs(&spec, project_id, active_workflow.as_ref())
            .await?;
        let mut cap_output = self
            .resolve_owner_capabilities(
                &spec,
                project_id,
                owner_ctx,
                active_workflow.as_ref(),
                vfs.as_ref(),
            )
            .await?;
        if cap_output
            .tool
            .enabled_clusters
            .contains(&ToolCluster::WorkspaceModule)
        {
            let projected =
                project_workspace_module_system_skill_to_vfs(self.repos, project_id, &mut vfs)
                    .await
                    .map_err(|error| error.to_string())?;
            if projected {
                self.apply_skill_baseline(
                    &mut cap_output,
                    vfs.as_ref(),
                    spec.identity,
                    "owner_bootstrap",
                )
                .await;
            }
        }
        cap_output.workspace_module =
            crate::session::capability_state::project_workspace_module_dimension(
                spec.visible_workspace_module_refs.as_deref(),
            );
        let mcp_runtime_context = McpRuntimeBindingContext { vfs: vfs.as_ref() };
        let runtime_mcp_servers = normalize_owner_bootstrap_mcp_projection(
            &mut cap_output,
            &spec.request_mcp_servers,
            &spec.agent_mcp,
            &mcp_runtime_context,
        )?;
        let context_bundle = self
            .build_owner_context_bundle(
                &spec,
                vfs.as_ref(),
                &runtime_mcp_servers,
                &cap_output.companion.agents,
                subject_context_contributions,
            )
            .await?;
        let audit_launch_path = owner_audit_launch_path(&spec.launch_path);
        let (user_input, effective_bundle) = match spec.launch_path {
            OwnerPromptLaunchPath::OwnerBootstrap => {
                (spec.user_input.clone(), Some(context_bundle))
            }
            OwnerPromptLaunchPath::RepositoryRehydrate {
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
            OwnerPromptLaunchPath::Plain => (spec.user_input.clone(), None),
        };
        if let (Some(bundle), Some(trigger)) = (
            effective_bundle.as_ref(),
            resolve_owner_audit_trigger(audit_launch_path, effective_bundle.is_some()),
        ) {
            self.audit_bundle(bundle, spec.audit_session_key.as_deref(), trigger);
        }

        let workspace_defaults = match &spec.owner {
            OwnerScope::Story { workspace, .. } => workspace.cloned(),
            OwnerScope::Project { workspace, .. } => spec
                .subject_workspace
                .cloned()
                .or_else(|| workspace.as_deref().cloned()),
        };

        let mut builder = SessionAssemblyBuilder::new()
            .with_input(user_input)
            .with_executor_config(spec.executor_config.clone())
            .with_mcp_servers(runtime_mcp_servers)
            .with_resolved_capabilities(cap_output)
            .with_optional_workspace_defaults(workspace_defaults)
            .with_optional_context_bundle(effective_bundle);

        if let Some(vfs) = vfs {
            builder = builder.with_vfs(vfs);
        }

        Ok(builder.build())
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
                        spec.subject_workspace.or(*workspace),
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

        let mut skill_asset_keys = spec.agent_skill_asset_keys.clone();
        if matches!(spec.owner, OwnerScope::Project { .. }) {
            ensure_companion_system_skill_asset(self.repos, project_id)
                .await
                .map_err(|error| error.to_string())?;
            append_companion_system_skill_key(&mut skill_asset_keys);
        }

        let mut vfs = if matches!(spec.owner, OwnerScope::Project { .. }) {
            let anchor = match spec.audit_session_key.as_deref() {
                Some(session_id) => self
                    .repos
                    .execution_anchor_repo
                    .find_by_session(session_id)
                    .await
                    .map_err(|error| error.to_string())?,
                None => None,
            };
            match anchor {
                Some(anchor) => {
                    let projector = AgentRunLifecycleSurfaceProjector::new(self.repos);
                    let address = AgentRunRuntimeAddress {
                        run_id: anchor.run_id,
                        agent_id: anchor.agent_id,
                        frame_id: anchor.launch_frame_id,
                    };
                    let message_stream = MessageStreamProjectionRef {
                        runtime_session_id: anchor.runtime_session_id,
                        trace_kind: MessageStreamTraceKind::ConnectorRuntimeSession,
                    };
                    let skill_projection = AgentRunLifecycleSkillProjectionFacts::ensure(
                        skill_asset_keys.clone(),
                        [BuiltinLifecycleSkill::CompanionSystem],
                    );
                    let surface = if let Some(workflow) = active_workflow {
                        projector
                            .project_workflow_node_execution_surface(
                                AgentRunLifecycleNodeRuntimeFacts {
                                    base_vfs: vfs,
                                    address,
                                    message_stream: Some(message_stream),
                                    project_id,
                                    node_projection: OrchestrationNodeProjectionInput {
                                        run_id: workflow.run.id,
                                        orchestration_id: workflow.orchestration_id,
                                        node_path: workflow.node_path.clone(),
                                        lifecycle_key: workflow.lifecycle_key.clone(),
                                        attempt: workflow.active_attempt.attempt,
                                        writable_port_keys: writable_port_keys_for_active_workflow(
                                            workflow,
                                        ),
                                    },
                                    skill_projection,
                                },
                            )
                            .await?
                    } else {
                        projector
                            .project_launch_evidence_surface(
                                AgentRunLifecycleSessionEvidenceFacts {
                                    base_vfs: vfs,
                                    address,
                                    message_stream,
                                    project_id,
                                    node_evidence: None,
                                    skill_projection,
                                },
                            )
                            .await?
                    };
                    Some(surface.vfs)
                }
                None => project_active_workflow_lifecycle_vfs(vfs, active_workflow),
            }
        } else {
            project_active_workflow_lifecycle_vfs(vfs, active_workflow)
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
            if !skill_asset_keys.is_empty()
                && !crate::vfs::append_lifecycle_skill_asset_projection(
                    space,
                    project_id,
                    &skill_asset_keys,
                )
            {
                return Err(
                    "session VFS 缺少 lifecycle mount，无法投影 SkillAsset baseline".to_string(),
                );
            }
        }

        Ok(vfs)
    }

    async fn resolve_owner_capabilities(
        &self,
        spec: &OwnerBootstrapSpec<'_>,
        project_id: Uuid,
        owner_ctx: CapabilityScopeCtx,
        active_workflow: Option<&ActiveWorkflowProjection>,
        vfs: Option<&Vfs>,
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
            },
            mcp_runtime_context: Some(McpRuntimeBindingContext { vfs }),
            capability_context: None,
            authority_state: AuthorityState::main_project_agent(),
        };
        let mut capability_state =
            CapabilityResolver::resolve_checked(&cap_input, self.platform_config)?;
        self.apply_skill_baseline(&mut capability_state, vfs, spec.identity, "owner_bootstrap")
            .await;
        Ok(capability_state)
    }

    async fn apply_skill_baseline(
        &self,
        capability_state: &mut CapabilityState,
        active_vfs: Option<&Vfs>,
        identity: Option<&AuthIdentity>,
        diagnostics_label: &'static str,
    ) {
        let Some(caps) = derive_session_skill_baseline(SessionCapabilityProjectionInput {
            vfs_service: Some(self.vfs_service),
            active_vfs,
            identity,
            extra_skill_dirs: self.extra_skill_dirs,
            skill_discovery_providers: self.skill_discovery_providers,
            diagnostics_label,
        })
        .await
        else {
            return;
        };
        capability_state.skill.skills = caps.skills;
    }

    async fn build_owner_context_bundle(
        &self,
        spec: &OwnerBootstrapSpec<'_>,
        vfs: Option<&Vfs>,
        runtime_mcp_servers: &[agentdash_spi::RuntimeMcpServer],
        companion_agents: &[CompanionAgentEntry],
        subject_context_contributions: Vec<Contribution>,
    ) -> Result<SessionContextBundle, String> {
        let runtime_mcp_servers = runtime_mcp_servers_to_summaries(runtime_mcp_servers);
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

        let mut contributions = Vec::with_capacity(2 + subject_context_contributions.len());
        contributions.push(owner_contribution);
        contributions.extend(subject_context_contributions);
        if let Some(companion_contribution) =
            build_companion_agents_context_contribution(companion_agents)
        {
            contributions.push(companion_contribution);
        }
        contributions.push(session_plan_contribution);

        Ok(build_session_context_bundle(
            SessionContextConfig {
                session_id: Uuid::new_v4(),
                phase: owner_scope_phase(&spec.owner),
                default_scope: agentdash_spi::ContextFragment::default_scope(),
            },
            contributions,
        ))
    }

    fn audit_bundle(
        &self,
        bundle: &SessionContextBundle,
        session_key: Option<&str>,
        trigger: AuditTrigger,
    ) {
        let (Some(bus), Some(session_key)) = (self.audit_bus.as_deref(), session_key) else {
            return;
        };
        emit_bundle_fragments(bus, bundle, session_key, trigger);
    }
}

fn build_companion_agents_context_contribution(
    agents: &[CompanionAgentEntry],
) -> Option<Contribution> {
    if agents.is_empty() {
        return None;
    }

    let mut lines = vec![
        "## Companion Agents".to_string(),
        "可通过 `companion_request` 派发以下协作 Agent；调用时使用 `payload.agent_key` 填写 agent_key。"
            .to_string(),
    ];
    for agent in agents {
        let display = agent.display_name.trim();
        let display_suffix = if display.is_empty() || display.eq_ignore_ascii_case(&agent.name) {
            String::new()
        } else {
            format!("; display_name: {}", display)
        };
        lines.push(format!(
            "- agent_key: `{}`; executor: `{}`{}",
            agent.name, agent.executor, display_suffix
        ));
    }

    Some(Contribution::fragments_only(vec![ContextFragment {
        slot: "companion_agents".to_string(),
        label: "available_companion_agents".to_string(),
        order: 60,
        strategy: MergeStrategy::Override,
        scope: ContextFragment::default_scope(),
        source: "capability:companion".to_string(),
        content: lines.join("\n"),
    }]))
}

fn owner_audit_launch_path(launch_path: &OwnerPromptLaunchPath) -> OwnerAuditLaunchPath {
    match launch_path {
        OwnerPromptLaunchPath::OwnerBootstrap => OwnerAuditLaunchPath::Bootstrap,
        OwnerPromptLaunchPath::RepositoryRehydrate { .. } => OwnerAuditLaunchPath::Rehydrate,
        OwnerPromptLaunchPath::Plain => OwnerAuditLaunchPath::Plain,
    }
}

fn resolve_owner_audit_trigger(
    launch_path: OwnerAuditLaunchPath,
    has_effective_bundle: bool,
) -> Option<AuditTrigger> {
    if !has_effective_bundle {
        return None;
    }

    match launch_path {
        OwnerAuditLaunchPath::Bootstrap => Some(AuditTrigger::SessionBootstrap),
        OwnerAuditLaunchPath::Rehydrate => Some(AuditTrigger::ComposerRebuild),
        OwnerAuditLaunchPath::Plain => None,
    }
}

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

fn build_owner_session_plan_contribution(
    owner: &OwnerScope<'_>,
    vfs: Option<&Vfs>,
    mcp_servers: &[McpServerSummary],
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

fn owner_scope_phase(owner: &OwnerScope<'_>) -> ContextBuildPhase {
    match owner {
        OwnerScope::Story { .. } => ContextBuildPhase::StoryOwner,
        OwnerScope::Project { .. } => ContextBuildPhase::ProjectAgent,
    }
}

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

    build_companion_roster_from_project_agents(&agents, caller_agent_id)
}

fn build_companion_roster_from_project_agents(
    agents: &[ProjectAgent],
    caller_agent_id: Option<Uuid>,
) -> Result<Vec<agentdash_spi::context::capability::CompanionAgentEntry>, String> {
    let caller_extra: BTreeSet<String> = if let Some(caller_id) = caller_agent_id {
        if let Some(agent) = agents.iter().find(|item| item.id == caller_id) {
            let preset = agent.preset_config().map_err(|error| error.to_string())?;
            preset
                .extra_companions
                .unwrap_or_default()
                .into_iter()
                .map(|value| value.to_ascii_lowercase())
                .collect()
        } else {
            BTreeSet::new()
        }
    } else {
        BTreeSet::new()
    };

    let mut entries = Vec::new();
    let mut seen = BTreeSet::new();
    for agent in agents.iter() {
        if caller_agent_id.is_some_and(|caller_id| caller_id == agent.id) {
            continue;
        }
        let preset = agent.preset_config().map_err(|error| error.to_string())?;
        let agent_key = agent.name.clone();
        let is_default_enabled = preset.default_companion_enabled.unwrap_or(false);
        let is_extra_enabled = caller_extra.contains(&agent_key.to_ascii_lowercase());
        if !is_default_enabled && !is_extra_enabled {
            continue;
        }
        if !seen.insert(agent_key.to_ascii_lowercase()) {
            continue;
        }
        let display = preset
            .display_name
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
            .unwrap_or_else(|| agent_key.clone());
        entries.push(agentdash_spi::context::capability::CompanionAgentEntry {
            name: agent_key,
            executor: agent.agent_type.clone(),
            display_name: display,
        });
    }
    Ok(entries)
}

async fn resolve_owner_workflow_tool_directives(
    repos: &RepositorySet,
    owner: &OwnerScope<'_>,
) -> Option<Vec<ToolCapabilityDirective>> {
    let project_id = owner.project_id();

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

    let workflow = repos
        .agent_procedure_repo
        .get_by_project_and_key(project_id, procedure_key)
        .await
        .ok()
        .flatten()?;

    Some(tool_directives_from_active_workflow(&workflow))
}

fn normalize_owner_bootstrap_mcp_projection(
    capability_state: &mut CapabilityState,
    request_mcp_servers: &[agentdash_spi::RuntimeMcpServer],
    agent_mcp: &AgentLevelMcp,
    runtime_context: &McpRuntimeBindingContext<'_>,
) -> Result<Vec<agentdash_spi::RuntimeMcpServer>, String> {
    let mut servers = Vec::new();
    servers.extend(request_mcp_servers.iter().cloned());
    servers.extend(capability_state.tool.mcp_servers.iter().cloned());
    for preset in &agent_mcp.preset_mcp_presets {
        let server = resolve_preset_mcp_server(preset, Some(runtime_context))
            .map_err(|error| error.to_string())?;
        servers.push(server);
    }
    normalize_runtime_mcp_servers(&mut servers);

    for server in &servers {
        capability_state
            .tool
            .capabilities
            .insert(capability_for_runtime_mcp_server(server));
    }
    capability_state.tool.mcp_servers = servers.clone();
    Ok(servers)
}

fn normalize_runtime_mcp_servers(servers: &mut Vec<agentdash_spi::RuntimeMcpServer>) {
    let mut seen = BTreeSet::<String>::new();
    servers.retain(|server| seen.insert(server.name.clone()));
}

fn capability_for_runtime_mcp_server(server: &agentdash_spi::RuntimeMcpServer) -> ToolCapability {
    match agent_facing_mcp_server_name(&server.name).as_str() {
        "agentdash-relay-tools" => {
            ToolCapability::new(agentdash_spi::platform::tool_capability::CAP_RELAY_MANAGEMENT)
        }
        "agentdash-story-tools" => {
            ToolCapability::new(agentdash_spi::platform::tool_capability::CAP_STORY_MANAGEMENT)
        }
        "agentdash-workflow-tools" => {
            ToolCapability::new(agentdash_spi::platform::tool_capability::CAP_WORKFLOW_MANAGEMENT)
        }
        custom => ToolCapability::custom_mcp(custom),
    }
}

fn agent_facing_mcp_server_name(server_name: &str) -> String {
    const PLATFORM_SCOPED_PREFIXES: &[(&str, &str)] = &[
        ("agentdash-story-tools-", "agentdash-story-tools"),
        ("agentdash-workflow-tools-", "agentdash-workflow-tools"),
    ];

    for (prefix, stable_name) in PLATFORM_SCOPED_PREFIXES {
        if server_name.starts_with(prefix) {
            return (*stable_name).to_string();
        }
    }

    server_name.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runtime_mcp_server(name: &str, url: &str) -> agentdash_spi::RuntimeMcpServer {
        agentdash_spi::RuntimeMcpServer {
            name: name.to_string(),
            transport: agentdash_spi::McpTransportConfig::Http {
                url: url.to_string(),
                headers: vec![],
            },
            uses_relay: false,
        }
    }

    fn runtime_mcp_preset(name: &str, url: &str) -> agentdash_domain::mcp_preset::McpPreset {
        agentdash_domain::mcp_preset::McpPreset::new_user(
            Uuid::new_v4(),
            name,
            name,
            None,
            agentdash_domain::mcp_preset::McpTransportConfig::Http {
                url: url.to_string(),
                headers: vec![],
            },
            agentdash_domain::mcp_preset::McpRoutePolicy::Direct,
        )
    }

    fn project_agent_with_companion_config(
        project_id: Uuid,
        name: &str,
        default_companion_enabled: bool,
        extra_companions: &[&str],
    ) -> ProjectAgent {
        let mut agent = ProjectAgent::new(project_id, name, "PI_AGENT");
        agent.config = serde_json::json!({
            "display_name": format!("{name} Agent"),
            "default_companion_enabled": default_companion_enabled,
            "extra_companions": extra_companions,
        });
        agent
    }

    fn server_url(server: &agentdash_spi::RuntimeMcpServer) -> &str {
        match &server.transport {
            agentdash_spi::McpTransportConfig::Http { url, .. } => url.as_str(),
            _ => "",
        }
    }

    #[test]
    fn companion_agents_context_contribution_renders_agent_key_roster() {
        let agents = vec![CompanionAgentEntry {
            name: "reviewer".to_string(),
            executor: "pi_agent".to_string(),
            display_name: "Review Agent".to_string(),
        }];

        let contribution =
            build_companion_agents_context_contribution(&agents).expect("companion contribution");

        assert_eq!(contribution.fragments.len(), 1);
        let fragment = &contribution.fragments[0];
        assert_eq!(fragment.slot, "companion_agents");
        assert_eq!(fragment.label, "available_companion_agents");
        assert_eq!(fragment.source, "capability:companion");
        assert_eq!(fragment.strategy, MergeStrategy::Override);
        assert!(fragment.content.contains("agent_key: `reviewer`"));
        assert!(fragment.content.contains("display_name: Review Agent"));
    }

    #[test]
    fn companion_agents_context_contribution_skips_empty_roster() {
        assert!(build_companion_agents_context_contribution(&[]).is_none());
    }

    #[test]
    fn companion_roster_uses_default_enabled_union_extra_minus_self() {
        let project_id = Uuid::new_v4();
        let caller = project_agent_with_companion_config(
            project_id,
            "caller",
            true,
            &["special", "reviewer"],
        );
        let reviewer = project_agent_with_companion_config(project_id, "reviewer", true, &[]);
        let special = project_agent_with_companion_config(project_id, "special", false, &[]);
        let hidden = project_agent_with_companion_config(project_id, "hidden", false, &[]);

        let roster = build_companion_roster_from_project_agents(
            &[caller.clone(), reviewer, special, hidden],
            Some(caller.id),
        )
        .expect("roster");

        let keys = roster
            .iter()
            .map(|entry| entry.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(keys, vec!["reviewer", "special"]);
        assert!(!keys.contains(&"caller"));
    }

    #[test]
    fn owner_bootstrap_mcp_projection_grants_agent_preset_without_directive() {
        let agent_server = runtime_mcp_server("code_analyzer", "http://agent/mcp");
        let mut capability_state = CapabilityState::default();

        let servers = normalize_owner_bootstrap_mcp_projection(
            &mut capability_state,
            &[],
            &AgentLevelMcp {
                preset_mcp_presets: vec![runtime_mcp_preset("code_analyzer", "http://agent/mcp")],
            },
            &McpRuntimeBindingContext { vfs: None },
        )
        .expect("static agent preset should resolve");

        assert_eq!(servers, vec![agent_server.clone()]);
        assert_eq!(capability_state.tool.mcp_servers, vec![agent_server]);
        assert!(
            capability_state
                .tool
                .capabilities
                .contains(&ToolCapability::custom_mcp("code_analyzer"))
        );
        assert!(capability_state.is_capability_tool_enabled(
            "mcp:code_analyzer",
            "scan_repo",
            None
        ));
    }

    #[test]
    fn owner_bootstrap_mcp_projection_dedupes_by_source_priority() {
        let mut capability_state = CapabilityState::default();
        capability_state.tool.mcp_servers = vec![
            runtime_mcp_server("shared", "http://resolver/mcp"),
            runtime_mcp_server("resolver_only", "http://resolver-only/mcp"),
        ];

        let servers = normalize_owner_bootstrap_mcp_projection(
            &mut capability_state,
            &[runtime_mcp_server("shared", "http://request/mcp")],
            &AgentLevelMcp {
                preset_mcp_presets: vec![
                    runtime_mcp_preset("shared", "http://agent/mcp"),
                    runtime_mcp_preset("agent_only", "http://agent-only/mcp"),
                ],
            },
            &McpRuntimeBindingContext { vfs: None },
        )
        .expect("static agent presets should resolve");

        let names = servers
            .iter()
            .map(|server| server.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["shared", "resolver_only", "agent_only"]);
        assert_eq!(server_url(&servers[0]), "http://request/mcp");
        assert_eq!(capability_state.tool.mcp_servers, servers);
        assert!(
            capability_state
                .tool
                .capabilities
                .contains(&ToolCapability::custom_mcp("shared"))
        );
        assert!(
            capability_state
                .tool
                .capabilities
                .contains(&ToolCapability::custom_mcp("agent_only"))
        );
    }

    #[test]
    fn owner_bootstrap_mcp_projection_maps_platform_scoped_server_to_platform_capability() {
        let mut capability_state = CapabilityState::default();
        capability_state.tool.mcp_servers = vec![runtime_mcp_server(
            "agentdash-workflow-tools-123",
            "http://workflow/mcp",
        )];

        normalize_owner_bootstrap_mcp_projection(
            &mut capability_state,
            &[],
            &AgentLevelMcp::default(),
            &McpRuntimeBindingContext { vfs: None },
        )
        .expect("platform scoped server should normalize");

        assert!(
            capability_state
                .tool
                .capabilities
                .contains(&ToolCapability::new(
                    agentdash_spi::platform::tool_capability::CAP_WORKFLOW_MANAGEMENT
                ))
        );
        assert!(
            !capability_state
                .tool
                .capabilities
                .contains(&ToolCapability::custom_mcp("agentdash-workflow-tools-123"))
        );
    }

    #[test]
    fn owner_bootstrap_audit_trigger_requires_effective_bundle() {
        assert_eq!(
            resolve_owner_audit_trigger(OwnerAuditLaunchPath::Bootstrap, true),
            Some(AuditTrigger::SessionBootstrap),
        );
        assert_eq!(
            resolve_owner_audit_trigger(OwnerAuditLaunchPath::Bootstrap, false),
            None,
        );
    }

    #[test]
    fn owner_rehydrate_audit_trigger_maps_to_composer_rebuild() {
        assert_eq!(
            resolve_owner_audit_trigger(OwnerAuditLaunchPath::Rehydrate, true),
            Some(AuditTrigger::ComposerRebuild),
        );
        assert_eq!(
            resolve_owner_audit_trigger(OwnerAuditLaunchPath::Rehydrate, false),
            None,
        );
    }

    #[test]
    fn owner_plain_launch_path_never_emits_owner_audit() {
        assert_eq!(
            resolve_owner_audit_trigger(OwnerAuditLaunchPath::Plain, true),
            None,
        );
        assert_eq!(
            resolve_owner_audit_trigger(OwnerAuditLaunchPath::Plain, false),
            None,
        );
    }
}
