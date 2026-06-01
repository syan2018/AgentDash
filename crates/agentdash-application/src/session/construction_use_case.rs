//! Session launch construction use case.
//!
//! 这个模块承接原本挂在 `routes/acp_sessions.rs` 里的 owner/context/capability
//! 组装逻辑。它只返回 application construction plan；
//! route 层不再承载 launch composition 主分支。

use std::path::PathBuf;
use std::sync::Arc;

use crate::canvas::append_visible_canvas_mounts;
use crate::extension_runtime::{
    ExtensionRuntimeProjection, extension_runtime_projection_from_installations,
};
use crate::session::UserPromptInput;
use crate::session::construction::{
    ConstructionResolutionPlan, SessionConstructionPlan, SessionConstructionTraceEntry,
};
use crate::session::construction::{OwnerResolutionTrace, ResolvedSessionOwner};
use crate::session::construction_planner::SessionConstructionPlanner;
use crate::session::construction_provider::{
    CompanionLaunchSource, RoutineLaunchSource, SessionConstructionProviderInput, TaskLaunchPhase,
    TaskLaunchSource,
};
use crate::session::local_workspace_vfs;
use crate::session::replay_runtime_capability_transitions;
use crate::session::{
    AgentLevelMcp, CompanionParentSpec, CompanionParentWorkflowSpec, LifecycleNodeSpec,
    OwnerBootstrapSpec, OwnerPromptLifecycle, OwnerScope, SessionMeta, SessionPromptLifecycle,
    SessionRepositoryRehydrateMode, SessionRequestAssembler, StoryStepPhase, StoryStepSpec,
    compose_lifecycle_node_prompt_with_audit, resolve_session_prompt_lifecycle,
};
use crate::session::{
    SessionCapabilityProjectionInput, derive_session_capability_projection,
    normalize_capability_state_dimensions,
};
use crate::skill_asset::SkillAssetService;
use crate::task::gateway::resolve_effective_task_workspace;
use crate::workflow::resolve_active_workflow_projection_for_session;
use crate::workflow::{LIFECYCLE_NODE_LABEL_PREFIX, SessionRunContextResolver};
use agentdash_domain::routine::ROUTINE_MEMORY_SKILL_NAME;
use agentdash_domain::{project::Project, story::Story, workspace::Workspace};
use agentdash_spi::CapabilityScope;
use agentdash_spi::hooks::ContextFrame;

use crate::context::SharedContextAuditBus;
use crate::error::ApplicationError;
use crate::platform_config::SharedPlatformConfig;
use crate::repository_set::RepositorySet;
use crate::session::{SessionCapabilityService, SessionEventingService};
use crate::vfs::VfsService;
use crate::workspace::BackendAvailability;
use agentdash_executor::AgentConnector;

pub struct SessionConstructionUseCaseDeps<'a> {
    pub repos: &'a RepositorySet,
    pub services: SessionConstructionServiceDeps<'a>,
    pub config: SessionConstructionConfigDeps,
}

pub struct SessionConstructionServiceDeps<'a> {
    pub connector: Arc<dyn AgentConnector>,
    pub vfs_service: Arc<VfsService>,
    pub extra_skill_dirs: &'a [PathBuf],
    pub backend_registry: Arc<dyn BackendAvailability>,
    pub audit_bus: SharedContextAuditBus,
    pub session_capability: &'a SessionCapabilityService,
    pub session_eventing: &'a SessionEventingService,
}

pub struct SessionConstructionConfigDeps {
    pub platform_config: SharedPlatformConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionConstructionProjectionMode {
    Launch,
    Inspect,
}

#[deprecated(note = "迁移到 AgentFrameBuilder；新 launch 路径从 AgentFrame 投影")]
pub async fn build_session_construction_for_launch(
    state: &SessionConstructionUseCaseDeps<'_>,
    session_id: &str,
    user_input: &UserPromptInput,
    _task_input: Option<TaskLaunchSource>,
    companion_input: Option<CompanionLaunchSource>,
    source_mcp_declarations: Vec<agentdash_spi::SessionMcpServer>,
    local_relay_workspace_root: Option<PathBuf>,
    facts: SessionConstructionProviderInput,
) -> Result<SessionConstructionPlan, ApplicationError> {
    let meta = &facts.session_meta;
    let visible_canvas_mount_ids = meta.visible_canvas_mount_ids.clone();
    let effective_executor = user_input
        .executor_config
        .clone()
        .or_else(|| meta.executor_config.clone());
    let supports_repository_restore = effective_executor.as_ref().is_some_and(|config| {
        state
            .services
            .connector
            .supports_repository_restore(config.executor.as_str())
    });
    let lifecycle_kind = resolve_session_prompt_lifecycle(
        meta,
        facts.had_existing_runtime,
        supports_repository_restore,
    );

    // TODO(permission-system): owner 路由由 Permission System + CapabilityScope 接管
    // 当前通过 LifecycleRunLink 或 meta.project_id 推断 scope 并走 Project 分支。
    let scope = resolve_session_scope(state, session_id, meta).await?;

    let owner = ResolvedSessionOwner {
        owner_type: scope,
        project_id: meta
            .project_id
            .as_deref()
            .and_then(|id| uuid::Uuid::parse_str(id).ok()),
        trace: OwnerResolutionTrace {
            selected_reason: "meta.project_id + lifecycle scope".to_string(),
        },
    };
    let plan = SessionConstructionPlan::from_source_input(session_id, owner, user_input);

    if let Some(companion) = companion_input {
        let plan = build_companion_dispatch_prompt_request(state, plan, companion).await?;
        return finalize_session_construction_projection(
            state,
            plan,
            source_mcp_declarations,
            local_relay_workspace_root,
            &facts,
            SessionConstructionProjectionMode::Launch,
        )
        .await;
    }

    let _scope = scope;
    let project_id = meta
        .project_id
        .as_deref()
        .and_then(|id| uuid::Uuid::parse_str(id).ok());
    let Some(project_id) = project_id else {
        return Err(ApplicationError::BadRequest(format!(
            "session {session_id} 没有关联 project_id，无法构建 SessionConstructionPlan"
        )));
    };

    let project = state
        .repos
        .project_repo
        .get_by_id(project_id)
        .await
        .map_err(ApplicationError::from)?
        .ok_or_else(|| ApplicationError::NotFound(format!("Project {project_id} 不存在")))?;

    let binding_label = if let Some(routine_source) = facts.command.routine_hint() {
        let routine = state
            .repos
            .routine_repo
            .get_by_id(routine_source.routine_id)
            .await
            .map_err(ApplicationError::from)?
            .ok_or_else(|| {
                ApplicationError::NotFound(format!("Routine {} 不存在", routine_source.routine_id))
            })?;
        if routine.project_id != project.id {
            return Err(ApplicationError::BadRequest(format!(
                "Routine {} 不属于 Project {}",
                routine.id, project.id
            )));
        }
        SessionConstructionPlanner::project_agent_session_label(
            &routine.project_agent_id.to_string(),
        )
    } else {
        crate::workflow::FREEFORM_SESSION_LABEL.to_string()
    };

    let plan = build_project_owner_prompt_request(
        state,
        session_id,
        user_input,
        plan,
        &project,
        &binding_label,
        meta,
        lifecycle_kind,
        &visible_canvas_mount_ids,
        source_mcp_declarations,
    )
    .await?;
    finalize_session_construction_projection(
        state,
        plan,
        Vec::new(),
        local_relay_workspace_root,
        &facts,
        SessionConstructionProjectionMode::Launch,
    )
    .await
}

/// 根据 SessionMeta 或 LifecycleRunLink 推导 session 的 CapabilityScope。
async fn resolve_session_scope(
    state: &SessionConstructionUseCaseDeps<'_>,
    session_id: &str,
    _meta: &SessionMeta,
) -> Result<CapabilityScope, ApplicationError> {
    let resolver = SessionRunContextResolver::new(
        state.repos.lifecycle_run_repo.as_ref(),
        state.repos.lifecycle_run_link_repo.as_ref(),
        state.repos.agent_frame_repo.as_ref(),
        state.repos.lifecycle_agent_repo.as_ref(),
        state.repos.story_repo.as_ref(),
    );
    Ok(resolver
        .resolve_for_session(session_id)
        .await?
        .map(|context| context.scope)
        .unwrap_or(CapabilityScope::Project))
}

#[deprecated(note = "迁移到 AgentFrameBuilder；surface projection 由 frame builder 统一产出")]
pub async fn finalize_session_construction_projection(
    state: &SessionConstructionUseCaseDeps<'_>,
    mut plan: SessionConstructionPlan,
    source_mcp_declarations: Vec<agentdash_spi::SessionMcpServer>,
    local_relay_workspace_root: Option<PathBuf>,
    facts: &SessionConstructionProviderInput,
    mode: SessionConstructionProjectionMode,
) -> Result<SessionConstructionPlan, ApplicationError> {
    plan.source.launch_source = Some(facts.command.reason_tag().to_string());
    if plan.identity.identity.is_none() {
        plan.identity.identity = facts.command.identity();
    }

    let (mut base_vfs, mut vfs_source) = if let Some(vfs) = plan.surface.vfs.clone() {
        (vfs, "construction.surface.vfs".to_string())
    } else if let Some(root) = local_relay_workspace_root.as_ref() {
        (
            local_workspace_vfs(root),
            "source.local_relay_workspace_root".to_string(),
        )
    } else {
        return Err(ApplicationError::BadRequest(
            "construction 未产出 VFS，且来源事实中没有可解析 workspace root".to_string(),
        ));
    };

    if let Some(routine_source) = facts.command.routine_hint() {
        if let Some(pid) = plan.owner.project_id {
            append_routine_projection(state, &mut base_vfs, pid, &routine_source).await?;
        }
        vfs_source = format!("{vfs_source}+routine_source");
    }

    let (base_mcp_servers, base_mcp_source) = if !plan.projections.mcp_servers.is_empty() {
        (
            plan.projections.mcp_servers.clone(),
            "construction.projections.mcp_servers".to_string(),
        )
    } else if !source_mcp_declarations.is_empty() {
        (
            source_mcp_declarations,
            "source.mcp_declarations".to_string(),
        )
    } else {
        (Vec::new(), "empty".to_string())
    };

    let mut base_capability_state = plan
        .projections
        .capability_state
        .clone()
        .unwrap_or_default();
    base_capability_state.vfs.active = Some(base_vfs.clone());
    base_capability_state.tool.mcp_servers = base_mcp_servers.clone();

    let requested_transitions = facts
        .requested_runtime_commands
        .iter()
        .map(|command| command.transition.clone())
        .collect::<Vec<_>>();
    let replay = if requested_transitions.is_empty() {
        None
    } else {
        Some(
            replay_runtime_capability_transitions(&base_capability_state, &requested_transitions)
                .map_err(ApplicationError::BadRequest)?,
        )
    };
    let effective_vfs = replay
        .as_ref()
        .and_then(|replay| replay.effective_vfs.clone())
        .unwrap_or_else(|| base_vfs.clone());
    let pending_overlay_applied = requested_transitions.iter().any(|transition| {
        transition
            .transition
            .effects
            .iter()
            .any(|effect| effect.dimension.as_str() == "vfs")
    });
    let (mcp_servers, mcp_source) = if let Some(replay) = replay.as_ref() {
        (
            replay
                .effective_mcp_servers
                .clone()
                .unwrap_or_else(|| replay.capability_state.tool.mcp_servers.clone()),
            "runtime_command.pending_transition".to_string(),
        )
    } else {
        (base_mcp_servers.clone(), base_mcp_source)
    };

    let working_directory = effective_vfs
        .default_mount()
        .map(|mount| PathBuf::from(mount.root_ref.trim()))
        .filter(|path| !path.as_os_str().is_empty())
        .ok_or_else(|| {
            ApplicationError::BadRequest("vfs 缺少 default_mount 或 root_ref 无效".to_string())
        })?;

    let projection = derive_session_capability_projection(SessionCapabilityProjectionInput {
        vfs_service: Some(&state.services.vfs_service),
        active_vfs: Some(&effective_vfs),
        extra_skill_dirs: state.services.extra_skill_dirs,
        diagnostics_label: "session_construction_finalize",
    })
    .await;
    let session_capabilities = projection.session_capabilities;
    let discovered_guidelines = projection.discovered_guidelines;

    let executor_source = if plan.execution_profile.executor_config.is_some() {
        "construction.execution_profile.executor_config"
    } else if facts.command.user_input().executor_config.is_some() {
        "source.user_input.executor_config"
    } else if facts.session_meta.executor_config.is_some() {
        "session.meta.executor_config"
    } else if mode == SessionConstructionProjectionMode::Inspect {
        "unresolved.inspect"
    } else {
        "unresolved"
    };
    let executor_config = plan
        .execution_profile
        .executor_config
        .clone()
        .or_else(|| facts.command.user_input().executor_config.clone())
        .or_else(|| facts.session_meta.executor_config.clone());
    if executor_config.is_none() && mode == SessionConstructionProjectionMode::Launch {
        return Err(ApplicationError::BadRequest(
            "construction 未产出 executor_config，且来源/meta 中没有可复用配置".to_string(),
        ));
    }

    normalize_capability_state_dimensions(
        &mut base_capability_state,
        Some(base_vfs),
        base_mcp_servers,
        &session_capabilities,
    );

    let mut final_capability_state = replay
        .map(|replay| replay.capability_state)
        .unwrap_or_else(|| base_capability_state.clone());
    normalize_capability_state_dimensions(
        &mut final_capability_state,
        Some(effective_vfs.clone()),
        mcp_servers.clone(),
        &session_capabilities,
    );
    let extension_runtime = if let Some(pid) = plan.owner.project_id {
        build_extension_runtime_projection(state, pid).await?
    } else {
        ExtensionRuntimeProjection::default()
    };

    plan.workspace.working_directory = Some(working_directory);
    plan.execution_profile.executor_config = executor_config;
    plan.context_projection.session_capabilities = Some(session_capabilities.clone());
    plan.projections.mcp_servers = mcp_servers;
    plan.projections.capability_state = Some(final_capability_state);
    plan.set_active_vfs(effective_vfs);
    plan.projections.session_capabilities = Some(session_capabilities);
    plan.projections.discovered_guidelines = discovered_guidelines;
    plan.projections.extension_runtime = Some(extension_runtime);
    plan.resolution = ConstructionResolutionPlan {
        vfs_source: Some(if pending_overlay_applied {
            "runtime_command.pending_vfs_overlay".to_string()
        } else {
            vfs_source
        }),
        mcp_source: Some(mcp_source),
        capability_source: Some(if facts.requested_runtime_commands.is_empty() {
            "construction.base_capability_state".to_string()
        } else {
            "runtime_command.pending_transition".to_string()
        }),
        executor_source: Some(executor_source.to_string()),
        working_directory_source: Some("vfs.default_mount.root_ref".to_string()),
        pending_overlay_applied,
        runtime_base_capability_state: Some(base_capability_state),
    };
    plan.trace.entries.extend([
        SessionConstructionTraceEntry {
            stage: "vfs_source",
            source: plan.resolution.vfs_source.clone().unwrap_or_default(),
        },
        SessionConstructionTraceEntry {
            stage: "mcp_source",
            source: plan.resolution.mcp_source.clone().unwrap_or_default(),
        },
        SessionConstructionTraceEntry {
            stage: "capability_source",
            source: plan
                .resolution
                .capability_source
                .clone()
                .unwrap_or_default(),
        },
        SessionConstructionTraceEntry {
            stage: "working_directory_source",
            source: plan
                .resolution
                .working_directory_source
                .clone()
                .unwrap_or_default(),
        },
        SessionConstructionTraceEntry {
            stage: "extension_runtime",
            source: "project.extension_installations".to_string(),
        },
    ]);
    if mode == SessionConstructionProjectionMode::Launch {
        plan.validate_for_launch()
            .map_err(ApplicationError::BadRequest)?;
    }
    Ok(plan)
}

async fn build_extension_runtime_projection(
    state: &SessionConstructionUseCaseDeps<'_>,
    project_id: uuid::Uuid,
) -> Result<ExtensionRuntimeProjection, ApplicationError> {
    let installations = state
        .repos
        .project_extension_installation_repo
        .list_enabled_by_project(project_id)
        .await
        .map_err(ApplicationError::from)?;
    Ok(extension_runtime_projection_from_installations(
        installations,
    )?)
}

async fn append_routine_projection(
    state: &SessionConstructionUseCaseDeps<'_>,
    vfs: &mut agentdash_spi::Vfs,
    project_id: uuid::Uuid,
    source: &RoutineLaunchSource,
) -> Result<(), ApplicationError> {
    SkillAssetService::new(state.repos.skill_asset_repo.as_ref())
        .bootstrap_builtins(project_id, Some(ROUTINE_MEMORY_SKILL_NAME))
        .await
        .map_err(ApplicationError::from)?;

    let routine_mount = crate::vfs::build_routine_mount(
        source.routine_id,
        source.execution_id,
        &source.trigger_source,
        source.entity_key.as_deref(),
    );
    if let Some(existing) = vfs
        .mounts
        .iter_mut()
        .find(|candidate| candidate.id == routine_mount.id)
    {
        *existing = routine_mount;
    } else {
        vfs.mounts.push(routine_mount);
    }

    crate::vfs::append_skill_asset_projection(
        vfs,
        project_id,
        &[ROUTINE_MEMORY_SKILL_NAME.to_string()],
    );
    Ok(())
}

fn clear_plain_lifecycle_context(
    user_input: &UserPromptInput,
    mut plan: SessionConstructionPlan,
) -> Result<SessionConstructionPlan, ApplicationError> {
    let user_prompt_blocks = user_input
        .prompt_blocks
        .clone()
        .ok_or_else(|| ApplicationError::BadRequest("必须提供 promptBlocks".to_string()))?;
    plan.prompt.prompt_blocks = Some(user_prompt_blocks);
    plan.context.bundle = None;
    plan.context.bundle_id = None;
    plan.context.bootstrap_fragment_count = 0;
    plan.context.continuation_context_frame = None;
    Ok(plan)
}

#[allow(dead_code)]
async fn build_story_owner_prompt_request(
    state: &SessionConstructionUseCaseDeps<'_>,
    session_id: &str,
    user_input: &UserPromptInput,
    plan: SessionConstructionPlan,
    story: &Story,
    project: &Project,
    workspace: Option<&Workspace>,
    _meta: &SessionMeta,
    lifecycle_kind: SessionPromptLifecycle,
    visible_canvas_mount_ids: &[String],
    source_mcp_declarations: Vec<agentdash_spi::SessionMcpServer>,
) -> Result<SessionConstructionPlan, ApplicationError> {
    let effective_executor_config = user_input
        .executor_config
        .clone()
        .or_else(|| {
            project
                .config
                .default_agent_type
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(agentdash_spi::AgentConfig::new)
        })
        .ok_or_else(|| {
            ApplicationError::BadRequest(
                "Story owner prompt 缺少 executor_config，且 project 没有 default_agent_type"
                    .to_string(),
            )
        })?;

    let user_prompt_blocks = user_input
        .prompt_blocks
        .clone()
        .ok_or_else(|| ApplicationError::BadRequest("必须提供 promptBlocks".to_string()))?;

    let lifecycle = map_owner_prompt_lifecycle(lifecycle_kind, None);
    let (lifecycle, continuation_context_frame) =
        resolve_continuation_system_context(state, session_id, lifecycle).await?;
    let active_workflow = resolve_active_workflow_projection_for_session(
        session_id,
        state.repos.workflow_definition_repo.as_ref(),
        state.repos.workflow_graph_repo.as_ref(),
        state.repos.activity_execution_claim_repo.as_ref(),
        state.repos.lifecycle_run_repo.as_ref(),
    )
    .await
    .map_err(ApplicationError::Internal)?;

    let existing_vfs = plan.surface.vfs.clone();
    let assembler = build_session_assembler(state);
    let mut plan = assembler
        .compose_owner_bootstrap_prompt(
            plan,
            OwnerBootstrapSpec {
                owner: OwnerScope::Story {
                    story,
                    project,
                    workspace,
                },
                executor_config: effective_executor_config,
                user_prompt_blocks,
                agent_mcp: AgentLevelMcp::default(),
                agent_tool_directives: Vec::new(),
                agent_skill_asset_keys: Vec::new(),
                agent_vfs_access_grants: Vec::new(),
                request_mcp_servers: source_mcp_declarations,
                existing_vfs,
                visible_canvas_mount_ids: visible_canvas_mount_ids.to_vec(),
                active_workflow,
                lifecycle,
                audit_session_key: Some(session_id.to_string()),
                caller_agent_id: None,
            },
        )
        .await
        .map_err(ApplicationError::BadRequest)?;

    plan.context.continuation_context_frame = continuation_context_frame;
    if matches!(lifecycle_kind, SessionPromptLifecycle::Plain) {
        return clear_plain_lifecycle_context(user_input, plan);
    }
    Ok(plan)
}

async fn build_project_owner_prompt_request(
    state: &SessionConstructionUseCaseDeps<'_>,
    session_id: &str,
    user_input: &UserPromptInput,
    plan: SessionConstructionPlan,
    project: &Project,
    binding_label: &str,
    _meta: &SessionMeta,
    lifecycle_kind: SessionPromptLifecycle,
    visible_canvas_mount_ids: &[String],
    source_mcp_declarations: Vec<agentdash_spi::SessionMcpServer>,
) -> Result<SessionConstructionPlan, ApplicationError> {
    if binding_label.starts_with(LIFECYCLE_NODE_LABEL_PREFIX) {
        return build_lifecycle_node_prompt_request(
            state,
            session_id,
            user_input,
            plan,
            lifecycle_kind,
        )
        .await;
    }

    let agent_key = SessionConstructionPlanner::parse_project_agent_session_label(binding_label)
        .ok_or_else(|| {
            ApplicationError::BadRequest(format!("无效的项目 Agent session label: {binding_label}"))
        })?;
    let project_agent = SessionConstructionPlanner::resolve_project_agent_context(
        state.repos,
        project.id,
        agent_key,
    )
    .await
    .map_err(ApplicationError::Internal)?
    .ok_or_else(|| ApplicationError::NotFound(format!("Project Agent `{agent_key}` 不存在")))?;
    let workspace = SessionConstructionPlanner::resolve_project_workspace(state.repos, project)
        .await
        .map_err(ApplicationError::Internal)?;

    let effective_executor_config = match user_input.executor_config.clone() {
        Some(mut user_ec) => {
            let preset_ec = &project_agent.executor_config;
            if user_ec.system_prompt.is_none() {
                user_ec.system_prompt = preset_ec.system_prompt.clone();
            }
            if user_ec.system_prompt_mode.is_none() {
                user_ec.system_prompt_mode = preset_ec.system_prompt_mode;
            }
            user_ec
        }
        None => project_agent.executor_config.clone(),
    };

    let user_prompt_blocks = user_input
        .prompt_blocks
        .clone()
        .ok_or_else(|| ApplicationError::BadRequest("必须提供 promptBlocks".to_string()))?;

    let agent_id = uuid::Uuid::parse_str(agent_key).ok();
    let agent_display_name = project_agent.display_name.clone();
    let preset_name = project_agent.preset_name.clone();
    let preset_mcp_servers = project_agent.preset_mcp_servers.clone();
    let agent_tool_directives = project_agent
        .preset_config
        .capability_directives
        .clone()
        .unwrap_or_default();
    let agent_skill_asset_keys = project_agent
        .preset_config
        .skill_asset_keys
        .clone()
        .unwrap_or_default();
    let agent_vfs_access_grants = project_agent
        .preset_config
        .vfs_access_grants
        .clone()
        .unwrap_or_default();

    let lifecycle = map_owner_prompt_lifecycle(lifecycle_kind, None);
    let (lifecycle, continuation_context_frame) =
        resolve_continuation_system_context(state, session_id, lifecycle).await?;
    let active_workflow = resolve_active_workflow_projection_for_session(
        session_id,
        state.repos.workflow_definition_repo.as_ref(),
        state.repos.workflow_graph_repo.as_ref(),
        state.repos.activity_execution_claim_repo.as_ref(),
        state.repos.lifecycle_run_repo.as_ref(),
    )
    .await
    .map_err(ApplicationError::Internal)?;

    let existing_vfs = plan.surface.vfs.clone();
    let assembler = build_session_assembler(state);
    let mut plan = assembler
        .compose_owner_bootstrap_prompt(
            plan,
            OwnerBootstrapSpec {
                owner: OwnerScope::Project {
                    project,
                    workspace: workspace.as_ref(),
                    agent_id,
                    agent_display_name,
                    preset_name,
                },
                executor_config: effective_executor_config,
                user_prompt_blocks,
                agent_mcp: AgentLevelMcp { preset_mcp_servers },
                agent_tool_directives,
                agent_skill_asset_keys,
                agent_vfs_access_grants,
                request_mcp_servers: source_mcp_declarations,
                existing_vfs,
                visible_canvas_mount_ids: visible_canvas_mount_ids.to_vec(),
                active_workflow,
                lifecycle,
                audit_session_key: Some(session_id.to_string()),
                caller_agent_id: agent_id,
            },
        )
        .await
        .map_err(ApplicationError::BadRequest)?;

    plan.context.continuation_context_frame = continuation_context_frame;
    if matches!(lifecycle_kind, SessionPromptLifecycle::Plain) {
        return clear_plain_lifecycle_context(user_input, plan);
    }
    Ok(plan)
}

async fn build_lifecycle_node_prompt_request(
    state: &SessionConstructionUseCaseDeps<'_>,
    session_id: &str,
    user_input: &UserPromptInput,
    plan: SessionConstructionPlan,
    lifecycle_kind: SessionPromptLifecycle,
) -> Result<SessionConstructionPlan, ApplicationError> {
    let frame = state
        .repos
        .agent_frame_repo
        .find_by_runtime_session(session_id)
        .await
        .map_err(ApplicationError::from)?
        .ok_or_else(|| {
            ApplicationError::BadRequest(format!(
                "Lifecycle node session {session_id} 无关联 AgentFrame"
            ))
        })?;
    let agent = state
        .repos
        .lifecycle_agent_repo
        .get(frame.agent_id)
        .await
        .map_err(ApplicationError::from)?
        .ok_or_else(|| {
            ApplicationError::NotFound(format!(
                "LifecycleAgent {} 不存在",
                frame.agent_id
            ))
        })?;
    let run = state
        .repos
        .lifecycle_run_repo
        .get_by_id(agent.run_id)
        .await
        .map_err(ApplicationError::from)?
        .ok_or_else(|| {
            ApplicationError::BadRequest(format!("Lifecycle node session {session_id} 无活跃 run"))
        })?;
    let lifecycle = state
        .repos
        .workflow_graph_repo
        .get_by_id(run.lifecycle_id)
        .await
        .map_err(ApplicationError::from)?
        .ok_or_else(|| {
            ApplicationError::NotFound(format!("Lifecycle {} 不存在", run.lifecycle_id))
        })?;
    let current_activity_key = run.current_activity_key().ok_or_else(|| {
        ApplicationError::BadRequest(format!(
            "Lifecycle node session {session_id} 无当前 activity"
        ))
    })?;
    let activity = lifecycle
        .activities
        .iter()
        .find(|item| item.key == current_activity_key)
        .cloned()
        .ok_or_else(|| {
            ApplicationError::BadRequest(format!(
                "Lifecycle {} 中不存在当前 activity `{}`",
                lifecycle.id, current_activity_key
            ))
        })?;
    let workflow = match &activity.executor {
        agentdash_domain::workflow::ActivityExecutorSpec::Agent(spec) => state
            .repos
            .workflow_definition_repo
            .get_by_project_and_key(run.project_id, &spec.procedure_key)
            .await
            .map_err(ApplicationError::from)?,
        _ => None,
    };
    let audit_bus = Some(state.services.audit_bus.clone());

    let plan = compose_lifecycle_node_prompt_with_audit(
        plan,
        state.repos,
        &state.config.platform_config,
        LifecycleNodeSpec {
            run: &run,
            lifecycle: &lifecycle,
            activity: &activity,
            workflow: workflow.as_ref(),
            inherited_executor_config: None,
        },
        audit_bus,
        Some(session_id),
    )
    .await
    .map_err(ApplicationError::BadRequest)?;

    if matches!(lifecycle_kind, SessionPromptLifecycle::Plain) {
        return clear_plain_lifecycle_context(user_input, plan);
    }
    Ok(plan)
}

async fn build_companion_dispatch_prompt_request(
    state: &SessionConstructionUseCaseDeps<'_>,
    plan: SessionConstructionPlan,
    companion: CompanionLaunchSource,
) -> Result<SessionConstructionPlan, ApplicationError> {
    let assembler = build_session_assembler(state);
    if let Some(workflow) = companion.workflow {
        assembler
            .compose_companion_with_workflow_prompt_from_parent(
                plan,
                CompanionParentWorkflowSpec {
                    companion: CompanionParentSpec {
                        parent_session_id: &companion.parent_session_id,
                        slice_mode: companion.slice_mode,
                        companion_executor_config: companion.companion_executor_config,
                        dispatch_prompt: companion.dispatch_prompt,
                    },
                    run: &workflow.run,
                    lifecycle: &workflow.lifecycle,
                    activity: &workflow.activity,
                    workflow: workflow.workflow.as_ref(),
                },
            )
            .await
            .map_err(ApplicationError::BadRequest)
    } else {
        assembler
            .compose_companion_prompt_from_parent(
                plan,
                CompanionParentSpec {
                    parent_session_id: &companion.parent_session_id,
                    slice_mode: companion.slice_mode,
                    companion_executor_config: companion.companion_executor_config,
                    dispatch_prompt: companion.dispatch_prompt,
                },
            )
            .await
            .map_err(ApplicationError::BadRequest)
    }
}

fn build_session_assembler<'a>(
    state: &'a SessionConstructionUseCaseDeps<'a>,
) -> SessionRequestAssembler<'a> {
    SessionRequestAssembler::new(
        state.services.vfs_service.as_ref(),
        state.repos.canvas_repo.as_ref(),
        state.services.backend_registry.as_ref(),
        state.repos,
        &state.config.platform_config,
    )
    .with_audit_bus(state.services.audit_bus.clone())
    .with_companion_parent_facts_provider(state.services.session_capability)
}

async fn build_continuation_context_frame_for_session(
    state: &SessionConstructionUseCaseDeps<'_>,
    session_id: &str,
) -> Result<Option<ContextFrame>, ApplicationError> {
    let transcript = state
        .services
        .session_eventing
        .build_projected_transcript(session_id)
        .await
        .map_err(ApplicationError::from)?;
    Ok(crate::session::continuation::build_continuation_context_frame(&transcript, None))
}

fn map_owner_prompt_lifecycle(
    kind: SessionPromptLifecycle,
    prebuilt_continuation_bundle: Option<agentdash_spi::SessionContextBundle>,
) -> OwnerPromptLifecycle {
    match kind {
        SessionPromptLifecycle::OwnerBootstrap => OwnerPromptLifecycle::OwnerBootstrap,
        SessionPromptLifecycle::RepositoryRehydrate(
            SessionRepositoryRehydrateMode::SystemContext,
        ) => OwnerPromptLifecycle::RepositoryRehydrate {
            prebuilt_continuation_bundle,
            include_owner_bundle: false,
        },
        SessionPromptLifecycle::RepositoryRehydrate(
            SessionRepositoryRehydrateMode::ExecutorState,
        ) => OwnerPromptLifecycle::RepositoryRehydrate {
            prebuilt_continuation_bundle: None,
            include_owner_bundle: true,
        },
        SessionPromptLifecycle::Plain => OwnerPromptLifecycle::Plain,
    }
}

async fn resolve_continuation_system_context(
    state: &SessionConstructionUseCaseDeps<'_>,
    session_id: &str,
    lifecycle: OwnerPromptLifecycle,
) -> Result<(OwnerPromptLifecycle, Option<ContextFrame>), ApplicationError> {
    if let OwnerPromptLifecycle::RepositoryRehydrate {
        prebuilt_continuation_bundle: None,
        include_owner_bundle: false,
    } = lifecycle
    {
        let continuation_context_frame =
            build_continuation_context_frame_for_session(state, session_id).await?;
        return Ok((
            OwnerPromptLifecycle::RepositoryRehydrate {
                prebuilt_continuation_bundle: None,
                include_owner_bundle: false,
            },
            continuation_context_frame,
        ));
    }
    Ok((lifecycle, None))
}

#[allow(dead_code)]
async fn build_task_owner_prompt_request(
    state: &SessionConstructionUseCaseDeps<'_>,
    session_id: &str,
    user_input: &UserPromptInput,
    plan: SessionConstructionPlan,
    task_id: uuid::Uuid,
    meta: &SessionMeta,
    lifecycle_kind: SessionPromptLifecycle,
    visible_canvas_mount_ids: &[String],
    task_input: Option<TaskLaunchSource>,
    source_mcp_declarations: Vec<agentdash_spi::SessionMcpServer>,
) -> Result<SessionConstructionPlan, ApplicationError> {
    let story = state
        .repos
        .story_repo
        .find_by_task_id(task_id)
        .await
        .map_err(ApplicationError::from)?
        .ok_or_else(|| ApplicationError::NotFound(format!("Task {task_id} 不存在")))?;
    let task = story
        .find_task(task_id)
        .cloned()
        .ok_or_else(|| ApplicationError::NotFound(format!("Task {task_id} 不存在")))?;

    let effective_executor_config = user_input
        .executor_config
        .clone()
        .or_else(|| meta.executor_config.clone());

    let project = state
        .repos
        .project_repo
        .get_by_id(story.project_id)
        .await
        .map_err(ApplicationError::from)?
        .ok_or_else(|| {
            ApplicationError::NotFound(format!("Project {} 不存在", story.project_id))
        })?;
    let workspace = resolve_effective_task_workspace(state.repos, &task, &story, &project)
        .await
        .map_err(ApplicationError::from)?;
    // Task execution session 没有 `lifecycle_activity:*` binding，因此容忍无 active
    // workflow projection：此时走纯 task 装配（`StoryStepSpec.active_workflow = None`），
    // 不带 lifecycle workflow injection。
    let active_workflow = resolve_active_workflow_projection_for_session(
        session_id,
        state.repos.workflow_definition_repo.as_ref(),
        state.repos.workflow_graph_repo.as_ref(),
        state.repos.activity_execution_claim_repo.as_ref(),
        state.repos.lifecycle_run_repo.as_ref(),
    )
    .await
    .map_err(ApplicationError::Internal)?;

    let user_prompt_blocks = user_input
        .prompt_blocks
        .clone()
        .ok_or_else(|| ApplicationError::BadRequest("必须提供 promptBlocks".to_string()))?;

    let task_phase = task_input
        .as_ref()
        .and_then(|input| input.phase)
        .unwrap_or(TaskLaunchPhase::Continue);
    let assembler = build_session_assembler(state);
    let mut plan = assembler
        .compose_story_step_prompt(
            plan,
            StoryStepSpec {
                task: &task,
                story: &story,
                project: &project,
                workspace: workspace.as_ref(),
                phase: match task_phase {
                    TaskLaunchPhase::Start => StoryStepPhase::Start,
                    TaskLaunchPhase::Continue => StoryStepPhase::Continue,
                },
                override_prompt: task_input
                    .as_ref()
                    .and_then(|input| input.override_prompt.as_deref()),
                additional_prompt: task_input
                    .as_ref()
                    .and_then(|input| input.additional_prompt.as_deref()),
                request_mcp_servers: &source_mcp_declarations,
                explicit_executor_config: effective_executor_config.clone(),
                strict_config_resolution: true,
                active_workflow,
                audit_session_key: Some(session_id.to_string()),
            },
        )
        .await
        .map_err(ApplicationError::from)?;

    if let Some(space) = plan.surface.vfs.as_mut() {
        append_visible_canvas_mounts(
            state.repos.canvas_repo.as_ref(),
            task.project_id,
            space,
            visible_canvas_mount_ids,
        )
        .await
        .map_err(ApplicationError::from)?;
    }

    let mut continuation_context_frame = None;
    plan.prompt.prompt_blocks = Some(user_prompt_blocks);

    match lifecycle_kind {
        SessionPromptLifecycle::OwnerBootstrap => {}
        SessionPromptLifecycle::RepositoryRehydrate(
            SessionRepositoryRehydrateMode::SystemContext,
        ) => {
            continuation_context_frame =
                build_continuation_context_frame_for_session(state, session_id).await?;
        }
        SessionPromptLifecycle::RepositoryRehydrate(
            SessionRepositoryRehydrateMode::ExecutorState,
        ) => {}
        SessionPromptLifecycle::Plain => {
            plan.context.bundle = None;
            plan.context.bundle_id = None;
            plan.context.bootstrap_fragment_count = 0;
        }
    }

    if plan.execution_profile.executor_config.is_none()
        && let Some(config) = effective_executor_config
    {
        plan.execution_profile.executor_config = Some(config);
    }

    if plan.projections.capability_state.is_none() {
        return Err(ApplicationError::Internal(
            "Task session compose 未产出 capability_state".to_string(),
        ));
    }

    plan.context.continuation_context_frame = continuation_context_frame;

    Ok(plan)
}

#[cfg(test)]
mod tests {
    use crate::session::construction::{
        SessionConstructionContextProjection, SessionConstructionPlan,
    };
    use agentdash_domain::common::{Mount, MountCapability};
    use agentdash_domain::extension_package::ExtensionPackageMetadata;
    use agentdash_domain::shared_library::{
        ExtensionBundleKind, ExtensionBundleRef, ExtensionCommandDefinition,
        ExtensionCommandHandler, ExtensionFlagDefinition, ExtensionFlagType,
        ExtensionMessageRendererDefinition, ExtensionPermissionAccess,
        ExtensionPermissionDeclaration, ExtensionRendererDeclaration,
        ExtensionRuntimeActionDefinition, ExtensionRuntimeActionKind, ExtensionTemplatePayload,
        ExtensionWorkspaceTabDefinition, ExtensionWorkspaceTabRendererDeclaration,
        InstalledAssetSource, ProjectExtensionInstallation,
    };
    use agentdash_spi::{CapabilityState, McpTransportConfig, SessionMcpServer, Vfs};

    use super::*;

    fn project_plan() -> SessionConstructionPlan {
        let owner = ResolvedSessionOwner {
            owner_type: agentdash_spi::CapabilityScope::Project,
            project_id: None,
            trace: OwnerResolutionTrace {
                selected_reason: "test".to_string(),
            },
        };
        let mut plan = SessionConstructionPlan::new(
            "sess-plain",
            owner,
            SessionConstructionContextProjection::default(),
        );
        let vfs = Vfs {
            mounts: vec![Mount {
                id: "workspace".to_string(),
                provider: "relay_fs".to_string(),
                backend_id: "backend".to_string(),
                root_ref: "/workspace".to_string(),
                capabilities: vec![MountCapability::Read, MountCapability::List],
                default_write: false,
                display_name: "Workspace".to_string(),
                metadata: serde_json::Value::Null,
            }],
            default_mount_id: Some("workspace".to_string()),
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        };
        let mcp = SessionMcpServer {
            name: "request-mcp".to_string(),
            transport: McpTransportConfig::Http {
                url: "http://127.0.0.1:18080/mcp".to_string(),
                headers: Vec::new(),
            },
            uses_relay: false,
        };
        let mut capability = CapabilityState::default();
        capability.vfs.active = Some(vfs.clone());
        capability.tool.mcp_servers = vec![mcp.clone()];

        plan.context.bundle = Some(agentdash_spi::SessionContextBundle::new(
            uuid::Uuid::new_v4(),
            "project_agent",
        ));
        plan.context.bundle_id = plan.context.bundle.as_ref().map(|bundle| bundle.bundle_id);
        plan.context.bootstrap_fragment_count = 1;
        plan.projections.mcp_servers = vec![mcp];
        plan.projections.capability_state = Some(capability);
        plan.set_active_vfs(vfs);
        plan
    }

    #[test]
    fn plain_lifecycle_cleanup_keeps_resolved_execution_surface() {
        let user_input = UserPromptInput::from_text("continue");
        let plan = clear_plain_lifecycle_context(&user_input, project_plan())
            .expect("plain cleanup should keep execution facts");

        assert!(plan.context.bundle.is_none());
        assert!(plan.context.bundle_id.is_none());
        assert_eq!(plan.context.bootstrap_fragment_count, 0);
        assert!(plan.surface.vfs.is_some());
        assert_eq!(plan.projections.mcp_servers.len(), 1);
        assert!(
            plan.projections
                .capability_state
                .as_ref()
                .and_then(|state| state.vfs.active.as_ref())
                .is_some()
        );
    }

    #[test]
    fn extension_runtime_projection_flattens_enabled_installations() {
        let source = InstalledAssetSource::new(
            uuid::Uuid::new_v4(),
            "plugin:test:extension_template:demo",
            "0.1.0",
            "digest",
        );
        let manifest = ExtensionTemplatePayload {
            manifest_version: "2".to_string(),
            extension_id: "demo".to_string(),
            package: ExtensionPackageMetadata {
                name: "demo".to_string(),
                version: "0.1.0".to_string(),
            },
            asset_version: "0.1.0".to_string(),
            commands: vec![ExtensionCommandDefinition {
                name: "demo:run".to_string(),
                description: "run demo".to_string(),
                handler: ExtensionCommandHandler::InjectMessage {
                    content: "run".to_string(),
                },
            }],
            flags: vec![ExtensionFlagDefinition {
                name: "demo.verbose".to_string(),
                flag_type: ExtensionFlagType::Bool,
                default: serde_json::Value::Bool(false),
                description: "verbose".to_string(),
            }],
            message_renderers: vec![ExtensionMessageRendererDefinition {
                custom_type: "demo.card".to_string(),
                renderer: ExtensionRendererDeclaration::JsonCard,
            }],
            runtime_actions: vec![ExtensionRuntimeActionDefinition {
                action_key: "demo.profile".to_string(),
                kind: ExtensionRuntimeActionKind::SessionRuntime,
                description: "read profile".to_string(),
                input_schema: serde_json::json!({}),
                output_schema: serde_json::json!({}),
                permissions: vec!["local.profile.read".to_string()],
            }],
            protocol_channels: vec![],
            extension_dependencies: vec![],
            workspace_tabs: vec![ExtensionWorkspaceTabDefinition {
                type_id: "demo.profile-panel".to_string(),
                label: "Profile".to_string(),
                uri_scheme: "demo".to_string(),
                renderer: ExtensionWorkspaceTabRendererDeclaration::Webview {
                    entry: "dist/panel/index.html".to_string(),
                },
            }],
            permissions: vec![ExtensionPermissionDeclaration::LocalProfile {
                access: ExtensionPermissionAccess::Read,
            }],
            bundles: vec![ExtensionBundleRef {
                kind: ExtensionBundleKind::ExtensionHost,
                entry: "dist/extension.js".to_string(),
                digest: "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                    .to_string(),
            }],
            capability_directives: vec![],
            asset_refs: vec![],
        };
        let installation = ProjectExtensionInstallation::new(
            uuid::Uuid::new_v4(),
            "demo",
            "Demo Extension",
            manifest,
            source,
        )
        .expect("valid installation");

        let projection = extension_runtime_projection_from_installations(vec![installation])
            .expect("projection");

        assert_eq!(projection.installations.len(), 1);
        assert_eq!(projection.commands[0].name, "demo:run");
        assert_eq!(projection.flags[0].name, "demo.verbose");
        assert_eq!(projection.message_renderers[0].custom_type, "demo.card");
        assert_eq!(projection.runtime_actions[0].action_key, "demo.profile");
        assert_eq!(projection.workspace_tabs[0].type_id, "demo.profile-panel");
        assert_eq!(projection.permissions.len(), 1);
        assert_eq!(projection.bundles[0].entry, "dist/extension.js");
    }
}
