use agentdash_domain::{
    agent::ProjectAgent,
    common::{AgentConfig, AgentPresetConfig},
    project::Project,
    workspace::Workspace,
};
#[cfg(test)]
use uuid::Uuid;

use crate::{mcp_preset::resolve_preset_mcp_refs, repository_set::RepositorySet};

#[cfg(test)]
use crate::{
    canvas::append_visible_canvas_mounts,
    capability::{
        CapabilityResolver, CapabilityResolverInput, ContextContributionSource,
        ContextContributions, McpCandidates, ToolContribution,
    },
    platform_config::PlatformConfig,
    runtime_bridge::session_mcp_servers_to_runtime,
    session::{
        ExecutorResolution, SessionCapabilityProjectionInput, SessionMeta,
        bootstrap::{
            BootstrapOwnerVariant, BootstrapPlanInput, build_bootstrap_plan,
            derive_session_context_snapshot,
        },
        context::{extract_story_overrides, normalize_optional_string},
        derive_session_skill_baseline,
    },
    vfs::{
        SessionMountTarget, VfsService, append_agent_knowledge_mounts,
        apply_agent_vfs_access_grants,
    },
    workflow::{
        ensure_active_workflow_lifecycle_mount, resolve_active_workflow_projection_for_session,
        resolve_current_frame_for_runtime_session,
    },
};
#[cfg(test)]
use agentdash_domain::{story::Story, task::TaskDispatchPreference};
#[cfg(test)]
use agentdash_spi::CapabilityScopeCtx;
#[cfg(test)]
use std::path::PathBuf;

#[cfg(test)]
use super::construction::{
    ResolvedSessionOwner, RuntimeContextInspectionPlan, SessionConstructionContextProjection,
};

#[cfg(test)]
pub struct RuntimeContextInspectionPlanner;

pub const PROJECT_AGENT_BINDING_LABEL_PREFIX: &str = "project_agent:";

#[derive(Debug, Clone)]
pub struct ResolvedProjectAgentContext {
    pub key: String,
    pub display_name: String,
    pub description: String,
    pub executor_config: agentdash_spi::AgentConfig,
    pub preset_config: AgentPresetConfig,
    pub preset_name: Option<String>,
    pub source: String,
    pub preset_mcp_servers: Vec<agentdash_spi::SessionMcpServer>,
    pub project_agent: ProjectAgent,
}

#[cfg(test)]
impl RuntimeContextInspectionPlanner {
    pub fn parse_project_dispatch_label(label: &str) -> Option<&str> {
        let agent_key = label
            .trim()
            .strip_prefix(PROJECT_AGENT_BINDING_LABEL_PREFIX)?;
        if agent_key.trim().is_empty() {
            return None;
        }
        Some(agent_key)
    }

    pub fn project_dispatch_label(agent_key: &str) -> String {
        format!("{PROJECT_AGENT_BINDING_LABEL_PREFIX}{}", agent_key.trim())
    }

    pub async fn resolve_project_workspace(
        repos: &RepositorySet,
        project: &Project,
    ) -> Result<Option<Workspace>, String> {
        resolve_project_workspace(repos, project).await
    }

    pub async fn resolve_project_agent_context(
        repos: &RepositorySet,
        project_id: Uuid,
        agent_key: &str,
    ) -> Result<Option<ResolvedProjectAgentContext>, String> {
        resolve_project_agent_context(repos, project_id, agent_key).await
    }

    pub async fn build_project_agent_context(
        repos: &RepositorySet,
        agent: &ProjectAgent,
    ) -> Result<ResolvedProjectAgentContext, String> {
        build_project_agent_context(repos, agent).await
    }

    pub async fn build_session_capabilities(
        vfs_service: &VfsService,
        vfs: Option<&agentdash_spi::Vfs>,
        extra_skill_dirs: &[PathBuf],
    ) -> Option<agentdash_spi::SessionBaselineCapabilities> {
        let caps = derive_session_skill_baseline(SessionCapabilityProjectionInput {
            vfs_service: Some(vfs_service),
            active_vfs: vfs,
            extra_skill_dirs,
            diagnostics_label: "construction_planner",
        })
        .await?;
        if caps.is_empty() { None } else { Some(caps) }
    }

    pub async fn plan_task_context_query(
        repos: &RepositorySet,
        vfs_service: &VfsService,
        extra_skill_dirs: &[PathBuf],
        platform_config: &PlatformConfig,
        session_id: impl Into<String>,
        owner: ResolvedSessionOwner,
        task_id: Uuid,
        workspace_id: Option<Uuid>,
        dispatch_preference: TaskDispatchPreference,
        _session_meta: Option<&SessionMeta>,
    ) -> RuntimeContextInspectionPlan {
        let session_id = session_id.into();
        let built_context = crate::task::context_builder::build_task_session_context(
            repos,
            vfs_service,
            platform_config,
            task_id,
            Some(session_id.as_str()),
        )
        .await;
        let resolved_vfs = built_context
            .as_ref()
            .and_then(|context| context.vfs.clone());
        let capabilities =
            Self::build_session_capabilities(vfs_service, resolved_vfs.as_ref(), extra_skill_dirs)
                .await;

        Self::plan_context(
            session_id,
            owner,
            SessionConstructionContextProjection {
                workspace_id,
                dispatch_preference: Some(dispatch_preference),
                vfs: resolved_vfs,
                runtime_surface: None,
                context_snapshot: built_context.and_then(|context| context.context_snapshot),
                session_capabilities: capabilities,
            },
        )
    }

    pub async fn plan_story_context_query(
        repos: &RepositorySet,
        vfs_service: &VfsService,
        extra_skill_dirs: &[PathBuf],
        platform_config: &PlatformConfig,
        session_id: impl Into<String>,
        owner: ResolvedSessionOwner,
        story: &Story,
        session_meta: Option<&SessionMeta>,
    ) -> Result<Option<RuntimeContextInspectionPlan>, String> {
        let Some(_session_meta) = session_meta else {
            return Ok(None);
        };
        let session_id = session_id.into();
        let project = repos
            .project_repo
            .get_by_id(story.project_id)
            .await
            .map_err(|error| format!("读取 story 所属 project 失败: {error}"))?
            .ok_or_else(|| format!("Story 所属 Project {} 不存在", story.project_id))?;
        let workspace = resolve_project_workspace(repos, &project).await?;
        let project_vfs_mounts = load_project_vfs_mounts(repos, project.id).await?;

        let default_agent_type =
            normalize_optional_string(project.config.default_agent_type.clone());
        let effective_agent_type = default_agent_type.clone();
        let use_vfs = default_agent_type.is_some();
        let active_workflow = resolve_active_workflow_projection_for_session(
            &session_id,
            repos.agent_procedure_repo.as_ref(),
            repos.workflow_graph_repo.as_ref(),
            repos.agent_frame_repo.as_ref(),
            repos.lifecycle_agent_repo.as_ref(),
            repos.lifecycle_run_repo.as_ref(),
            repos.execution_anchor_repo.as_ref(),
        )
        .await?;

        let mut vfs = if use_vfs {
            Some(
                vfs_service
                    .build_vfs(
                        &project,
                        &project_vfs_mounts,
                        Some(story),
                        workspace.as_ref(),
                        SessionMountTarget::Story,
                        effective_agent_type.as_deref(),
                    )
                    .map_err(|error| error.to_string())?,
            )
        } else {
            None
        };
        vfs = ensure_active_workflow_lifecycle_mount(vfs, active_workflow.as_ref());
        let canvas_mount_ids =
            resolve_visible_canvas_mount_ids_from_frame(repos, &session_id).await;
        if let Some(vfs) = vfs.as_mut() {
            append_visible_canvas_mounts(
                repos.canvas_repo.as_ref(),
                project.id,
                vfs,
                &canvas_mount_ids,
            )
            .await
            .map_err(|error| error.to_string())?;
        }

        let workflow_tool = crate::capability::resolve_session_workflow_context(
            crate::capability::SessionWorkflowRepos {
                project_agent: repos.project_agent_repo.as_ref(),
                activity_lifecycle_def: repos.workflow_graph_repo.as_ref(),
                workflow_def: repos.agent_procedure_repo.as_ref(),
            },
            crate::capability::SessionWorkflowOwner::Story {
                project_id: story.project_id,
            },
        )
        .await;

        let mut contributions = Vec::new();
        if let Some(wf_tool) = workflow_tool {
            contributions.push(ContextContributions {
                source: ContextContributionSource::Workflow,
                tool: Some(wf_tool),
                companion: None,
            });
        }
        let cap_output = CapabilityResolver::resolve(
            &CapabilityResolverInput {
                owner_ctx: CapabilityScopeCtx::Story {
                    project_id: story.project_id,
                    story_id: story.id,
                },
                contributions,
                mcp_candidates: McpCandidates {
                    presets: load_project_presets(repos, story.project_id).await,
                    agent_servers: vec![],
                },
                capability_context: None,
            },
            platform_config,
        );
        let effective_mcp_servers: Vec<agentdash_spi::SessionMcpServer> =
            cap_output.tool.mcp_servers.clone();
        let executor_source = if effective_agent_type.is_some() {
            "project.config.default_agent_type"
        } else {
            "unresolved"
        };
        let story_overrides = extract_story_overrides(story);
        let plan = build_bootstrap_plan(BootstrapPlanInput {
            project,
            story: Some(story.clone()),
            workspace,
            resolved_config: None,
            vfs,
            mcp_servers: session_mcp_servers_to_runtime(&effective_mcp_servers),
            working_dir: None,
            executor_preset_name: None,
            executor_resolution: ExecutorResolution::resolved(executor_source),
            owner_variant: BootstrapOwnerVariant::Story { story_overrides },
            workflow: active_workflow,
        });
        let snapshot = derive_session_context_snapshot(&plan);
        let capabilities =
            Self::build_session_capabilities(vfs_service, plan.vfs.as_ref(), extra_skill_dirs)
                .await;

        Ok(Some(Self::plan_context(
            session_id,
            owner,
            SessionConstructionContextProjection {
                workspace_id: None,
                dispatch_preference: None,
                vfs: plan.vfs.clone(),
                runtime_surface: None,
                context_snapshot: Some(snapshot),
                session_capabilities: capabilities,
            },
        )))
    }

    pub async fn plan_project_context_query(
        repos: &RepositorySet,
        vfs_service: &VfsService,
        extra_skill_dirs: &[PathBuf],
        platform_config: &PlatformConfig,
        session_id: impl Into<String>,
        owner: ResolvedSessionOwner,
        project: &Project,
        binding_label: &str,
        _session_meta: &SessionMeta,
    ) -> Result<RuntimeContextInspectionPlan, String> {
        let session_id = session_id.into();
        let agent_key = Self::parse_project_dispatch_label(binding_label)
            .ok_or_else(|| format!("无效的项目 Agent session label: {binding_label}"))?;
        let project_agent = resolve_project_agent_context(repos, project.id, agent_key)
            .await?
            .ok_or_else(|| format!("Project Agent `{agent_key}` 不存在"))?;
        let workspace = resolve_project_workspace(repos, project).await?;
        let project_vfs_mounts = load_project_vfs_mounts(repos, project.id).await?;

        let connector_config = Some(project_agent.executor_config.clone());
        let resolved_config = connector_config.clone();
        let use_vfs = connector_config
            .as_ref()
            .is_some_and(|c| c.is_cloud_native());
        let active_workflow = resolve_active_workflow_projection_for_session(
            &session_id,
            repos.agent_procedure_repo.as_ref(),
            repos.workflow_graph_repo.as_ref(),
            repos.agent_frame_repo.as_ref(),
            repos.lifecycle_agent_repo.as_ref(),
            repos.lifecycle_run_repo.as_ref(),
            repos.execution_anchor_repo.as_ref(),
        )
        .await?;

        let mut vfs = if use_vfs {
            Some(
                vfs_service
                    .build_vfs(
                        project,
                        &project_vfs_mounts,
                        None,
                        workspace.as_ref(),
                        SessionMountTarget::Project,
                        resolved_config.as_ref().map(|c| c.executor.as_str()),
                    )
                    .map_err(|error| error.to_string())?,
            )
        } else {
            None
        };

        if let Some(vfs) = vfs.as_mut() {
            apply_agent_vfs_access_grants(
                vfs,
                project_agent.preset_config.vfs_access_grants.as_deref(),
            );
            append_agent_knowledge_mounts(vfs, &project_agent.project_agent)?;
        }

        vfs = ensure_active_workflow_lifecycle_mount(vfs, active_workflow.as_ref());

        let canvas_mount_ids =
            resolve_visible_canvas_mount_ids_from_frame(repos, &session_id).await;
        if let Some(vfs) = vfs.as_mut() {
            append_visible_canvas_mounts(
                repos.canvas_repo.as_ref(),
                project.id,
                vfs,
                &canvas_mount_ids,
            )
            .await
            .map_err(|error| error.to_string())?;
        }

        let mut contributions = Vec::new();
        if let Some(directives) = project_agent.preset_config.capability_directives.clone()
            && !directives.is_empty()
        {
            contributions.push(ContextContributions {
                source: ContextContributionSource::Agent,
                tool: Some(ToolContribution {
                    directives,
                    has_active_workflow: false,
                }),
                companion: None,
            });
        }
        let workflow_tool = crate::capability::resolve_session_workflow_context(
            crate::capability::SessionWorkflowRepos {
                project_agent: repos.project_agent_repo.as_ref(),
                activity_lifecycle_def: repos.workflow_graph_repo.as_ref(),
                workflow_def: repos.agent_procedure_repo.as_ref(),
            },
            crate::capability::SessionWorkflowOwner::Project {
                project_id: project.id,
                project_agent_id: project_agent.project_agent.id,
            },
        )
        .await;
        if let Some(wf_tool) = workflow_tool {
            contributions.push(ContextContributions {
                source: ContextContributionSource::Workflow,
                tool: Some(wf_tool),
                companion: None,
            });
        }

        let agent_mcp_entries: Vec<crate::capability::AgentMcpServerEntry> =
            crate::session::extract_agent_mcp_entries(&project_agent.preset_mcp_servers);
        let cap_output = CapabilityResolver::resolve(
            &CapabilityResolverInput {
                owner_ctx: CapabilityScopeCtx::Project {
                    project_id: project.id,
                },
                contributions,
                mcp_candidates: McpCandidates {
                    presets: load_project_presets(repos, project.id).await,
                    agent_servers: agent_mcp_entries,
                },
                capability_context: None,
            },
            platform_config,
        );
        let mut effective_mcp_servers: Vec<agentdash_spi::SessionMcpServer> =
            cap_output.tool.mcp_servers.clone();
        effective_mcp_servers.extend(project_agent.preset_mcp_servers.iter().cloned());
        let executor_source = project_agent.source.clone();

        let plan = build_bootstrap_plan(BootstrapPlanInput {
            project: project.clone(),
            story: None,
            workspace,
            resolved_config,
            vfs,
            mcp_servers: session_mcp_servers_to_runtime(&effective_mcp_servers),
            working_dir: None,
            executor_preset_name: project_agent.preset_name,
            executor_resolution: ExecutorResolution::resolved(executor_source),
            owner_variant: BootstrapOwnerVariant::Project {
                agent_key: project_agent.key,
                agent_display_name: project_agent.display_name,
            },
            workflow: active_workflow,
        });
        let snapshot = derive_session_context_snapshot(&plan);
        let capabilities =
            Self::build_session_capabilities(vfs_service, plan.vfs.as_ref(), extra_skill_dirs)
                .await;

        Ok(Self::plan_context(
            session_id,
            owner,
            SessionConstructionContextProjection {
                workspace_id: None,
                dispatch_preference: None,
                vfs: plan.vfs.clone(),
                runtime_surface: None,
                context_snapshot: Some(snapshot),
                session_capabilities: capabilities,
            },
        ))
    }

    pub fn plan_context(
        session_id: impl Into<String>,
        owner: ResolvedSessionOwner,
        projection: SessionConstructionContextProjection,
    ) -> RuntimeContextInspectionPlan {
        RuntimeContextInspectionPlan::new(session_id, owner, projection)
    }
}

pub async fn resolve_project_workspace(
    repos: &RepositorySet,
    project: &Project,
) -> Result<Option<Workspace>, String> {
    if let Some(workspace_id) = project.config.default_workspace_id {
        return repos
            .workspace_repo
            .get_by_id(workspace_id)
            .await
            .map_err(|error| error.to_string());
    }
    Ok(None)
}

#[cfg(test)]
async fn load_project_presets(
    repos: &RepositorySet,
    project_id: Uuid,
) -> crate::capability::AvailableMcpPresets {
    match repos.mcp_preset_repo.list_by_project(project_id).await {
        Ok(presets) => presets.into_iter().map(|p| (p.key.clone(), p)).collect(),
        Err(error) => {
            tracing::warn!(
                project_id = %project_id,
                error = %error,
                "construction planner: 加载 MCP Preset 列表失败"
            );
            Default::default()
        }
    }
}

#[cfg(test)]
async fn load_project_vfs_mounts(
    repos: &RepositorySet,
    project_id: Uuid,
) -> Result<Vec<agentdash_domain::project_vfs_mount::ProjectVfsMount>, String> {
    repos
        .project_vfs_mount_repo
        .list_by_project(project_id)
        .await
        .map_err(|error| format!("读取 Project VFS Mount 失败: {error}"))
}

#[cfg(test)]
async fn resolve_project_agent_context(
    repos: &RepositorySet,
    project_id: Uuid,
    agent_key: &str,
) -> Result<Option<ResolvedProjectAgentContext>, String> {
    let project_agent_id = match Uuid::parse_str(agent_key) {
        Ok(project_agent_id) => project_agent_id,
        Err(_) => return Ok(None),
    };
    let agent = repos
        .project_agent_repo
        .get_by_project_and_id(project_id, project_agent_id)
        .await
        .map_err(|error| error.to_string())?;
    let Some(agent) = agent else {
        return Ok(None);
    };
    build_project_agent_context(repos, &agent).await.map(Some)
}

pub async fn build_project_agent_context(
    repos: &RepositorySet,
    agent: &ProjectAgent,
) -> Result<ResolvedProjectAgentContext, String> {
    let preset = agent.preset_config().map_err(|error| error.to_string())?;
    let executor_config: AgentConfig = preset.to_agent_config(&agent.agent_type);
    let display_name = preset
        .display_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(&agent.name)
        .to_string();
    let description = preset
        .description
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(String::from)
        .unwrap_or_else(|| format!("Agent `{}`，执行器 {}。", agent.name, agent.agent_type));
    let preset_mcp_servers = resolve_preset_mcp_refs(
        repos.mcp_preset_repo.as_ref(),
        agent.project_id,
        preset.mcp_preset_keys.as_deref().unwrap_or_default(),
    )
    .await
    .map_err(|error| {
        format!(
            "Project Agent `{}` 的 mcp_preset_keys 配置非法: {error}",
            agent.id
        )
    })?;

    Ok(ResolvedProjectAgentContext {
        key: agent.id.to_string(),
        display_name,
        description,
        executor_config,
        preset_config: preset,
        preset_name: Some(agent.name.clone()),
        source: format!("project_agents[{}]", agent.id),
        preset_mcp_servers,
        project_agent: agent.clone(),
    })
}

/// 从 AgentFrame 中读取 visible_canvas_mount_ids（通过 runtime session ref 反查）。
#[cfg(test)]
async fn resolve_visible_canvas_mount_ids_from_frame(
    repos: &RepositorySet,
    runtime_session_id: &str,
) -> Vec<String> {
    match resolve_current_frame_for_runtime_session(
        runtime_session_id,
        repos.execution_anchor_repo.as_ref(),
        repos.lifecycle_agent_repo.as_ref(),
        repos.agent_frame_repo.as_ref(),
    )
    .await
    {
        Ok(Some((_anchor, _agent, frame))) => frame.visible_canvas_mount_ids(),
        _ => Vec::new(),
    }
}
