//! Session launch construction use case.
//!
//! 这个模块承接原本挂在 `routes/acp_sessions.rs` 里的 owner/context/capability
//! 组装逻辑。它只返回 application construction plan；
//! route 层不再承载 launch composition 主分支。

use std::path::PathBuf;
use std::sync::Arc;

use agentdash_application::canvas::append_visible_canvas_mounts;
use agentdash_application::extension_runtime::{
    ExtensionRuntimeProjection, extension_runtime_projection_from_installations,
};
use agentdash_application::session::UserPromptInput;
use agentdash_application::session::construction::{
    ConstructionResolutionPlan, SessionConstructionPlan, SessionConstructionTraceEntry,
};
use agentdash_application::session::construction_planner::SessionConstructionPlanner;
use agentdash_application::session::construction_provider::{
    CompanionLaunchSource, SessionConstructionProviderInput, TaskLaunchPhase, TaskLaunchSource,
};
use agentdash_application::session::local_workspace_vfs;
use agentdash_application::session::ownership::SessionOwnerResolver;
use agentdash_application::session::replay_runtime_capability_transitions;
use agentdash_application::session::{
    AgentLevelMcp, CompanionParentSpec, CompanionParentWorkflowSpec, LifecycleNodeSpec,
    OwnerBootstrapSpec, OwnerPromptLifecycle, OwnerScope, SessionMeta, SessionPromptLifecycle,
    SessionRepositoryRehydrateMode, SessionRequestAssembler, StoryStepPhase, StoryStepSpec,
    compose_lifecycle_node_prompt_with_audit, resolve_session_prompt_lifecycle,
};
use agentdash_application::session::{
    SessionCapabilityProjectionInput, derive_session_capability_projection,
    normalize_capability_state_dimensions,
};
use agentdash_application::task::gateway::resolve_effective_task_workspace;
use agentdash_application::workflow::resolve_active_workflow_projection_for_session;
use agentdash_application::workflow::{LIFECYCLE_NODE_LABEL_PREFIX, select_active_run};
use agentdash_domain::{
    project::Project, session_binding::SessionOwnerType, story::Story, workspace::Workspace,
};
use agentdash_spi::hooks::ContextFrame;

use crate::app_state::AppState;
use crate::rpc::ApiError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SessionConstructionProjectionMode {
    Launch,
    Inspect,
}

pub(crate) async fn build_session_construction_for_launch(
    state: &Arc<AppState>,
    session_id: &str,
    user_input: &UserPromptInput,
    task_input: Option<TaskLaunchSource>,
    companion_input: Option<CompanionLaunchSource>,
    source_mcp_declarations: Vec<agentdash_spi::SessionMcpServer>,
    local_relay_workspace_root: Option<PathBuf>,
    facts: SessionConstructionProviderInput,
) -> Result<SessionConstructionPlan, ApiError> {
    let meta = &facts.session_meta;
    let bindings = state
        .repos
        .session_binding_repo
        .list_by_session(session_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
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

    if let Some(owner) = SessionOwnerResolver::resolve_primary(&bindings) {
        let plan =
            SessionConstructionPlan::from_source_input(session_id, owner.clone(), user_input);
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
        match owner.owner_type {
            SessionOwnerType::Task => {
                let plan = build_task_owner_prompt_request(
                    state,
                    session_id,
                    user_input,
                    plan,
                    owner.owner_id,
                    meta,
                    lifecycle_kind,
                    &visible_canvas_mount_ids,
                    task_input,
                    source_mcp_declarations,
                )
                .await?;
                return finalize_session_construction_projection(
                    state,
                    plan,
                    Vec::new(),
                    local_relay_workspace_root,
                    &facts,
                    SessionConstructionProjectionMode::Launch,
                )
                .await;
            }
            SessionOwnerType::Story => {
                let story = state
                    .repos
                    .story_repo
                    .get_by_id(owner.owner_id)
                    .await
                    .map_err(|e| ApiError::Internal(e.to_string()))?
                    .ok_or_else(|| {
                        ApiError::NotFound(format!("Story {} 不存在", owner.owner_id))
                    })?;
                let project = state
                    .repos
                    .project_repo
                    .get_by_id(story.project_id)
                    .await
                    .map_err(|e| ApiError::Internal(e.to_string()))?
                    .ok_or_else(|| {
                        ApiError::NotFound(format!("Project {} 不存在", story.project_id))
                    })?;
                let workspace =
                    SessionConstructionPlanner::resolve_project_workspace(&state.repos, &project)
                        .await
                        .map_err(ApiError::Internal)?;

                let plan = build_story_owner_prompt_request(
                    state,
                    session_id,
                    user_input,
                    plan,
                    &story,
                    &project,
                    workspace.as_ref(),
                    meta,
                    lifecycle_kind,
                    &visible_canvas_mount_ids,
                    source_mcp_declarations,
                )
                .await?;
                return finalize_session_construction_projection(
                    state,
                    plan,
                    Vec::new(),
                    local_relay_workspace_root,
                    &facts,
                    SessionConstructionProjectionMode::Launch,
                )
                .await;
            }
            SessionOwnerType::Project => {
                let project = state
                    .repos
                    .project_repo
                    .get_by_id(owner.owner_id)
                    .await
                    .map_err(|e| ApiError::Internal(e.to_string()))?
                    .ok_or_else(|| {
                        ApiError::NotFound(format!("Project {} 不存在", owner.owner_id))
                    })?;

                let plan = build_project_owner_prompt_request(
                    state,
                    session_id,
                    user_input,
                    plan,
                    &project,
                    &owner.label,
                    meta,
                    lifecycle_kind,
                    &visible_canvas_mount_ids,
                    source_mcp_declarations,
                )
                .await?;
                return finalize_session_construction_projection(
                    state,
                    plan,
                    Vec::new(),
                    local_relay_workspace_root,
                    &facts,
                    SessionConstructionProjectionMode::Launch,
                )
                .await;
            }
        }
    }

    Err(ApiError::BadRequest(format!(
        "session {session_id} 缺少 owner binding，无法构建 SessionConstructionPlan"
    )))
}

pub(crate) async fn finalize_session_construction_projection(
    state: &Arc<AppState>,
    mut plan: SessionConstructionPlan,
    source_mcp_declarations: Vec<agentdash_spi::SessionMcpServer>,
    local_relay_workspace_root: Option<PathBuf>,
    facts: &SessionConstructionProviderInput,
    mode: SessionConstructionProjectionMode,
) -> Result<SessionConstructionPlan, ApiError> {
    plan.source.launch_source = Some(facts.command.reason_tag().to_string());
    if plan.identity.identity.is_none() {
        plan.identity.identity = facts.command.identity();
    }

    let (base_vfs, vfs_source) = if let Some(vfs) = plan.surface.vfs.clone() {
        (vfs, "construction.surface.vfs".to_string())
    } else if let Some(root) = local_relay_workspace_root.as_ref() {
        (
            local_workspace_vfs(root),
            "source.local_relay_workspace_root".to_string(),
        )
    } else {
        return Err(ApiError::BadRequest(
            "construction 未产出 VFS，且来源事实中没有可解析 workspace root".to_string(),
        ));
    };

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
                .map_err(ApiError::BadRequest)?,
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
            ApiError::BadRequest("vfs 缺少 default_mount 或 root_ref 无效".to_string())
        })?;

    let projection = derive_session_capability_projection(SessionCapabilityProjectionInput {
        vfs_service: Some(&state.services.vfs_service),
        active_vfs: Some(&effective_vfs),
        extra_skill_dirs: &state.services.extra_skill_dirs,
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
        return Err(ApiError::BadRequest(
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
    let extension_runtime =
        build_extension_runtime_projection(state, plan.owner.project_id).await?;

    plan.workspace.working_directory = Some(working_directory);
    plan.execution_profile.executor_config = executor_config;
    plan.surface.vfs = Some(effective_vfs.clone());
    plan.context_projection.vfs = Some(effective_vfs.clone());
    plan.context_projection.session_capabilities = Some(session_capabilities.clone());
    plan.projections.context.vfs = Some(effective_vfs);
    plan.projections.context.session_capabilities = Some(session_capabilities.clone());
    plan.projections.mcp_servers = mcp_servers;
    plan.projections.capability_state = Some(final_capability_state);
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
        plan.validate_for_launch().map_err(ApiError::BadRequest)?;
    }
    Ok(plan)
}

async fn build_extension_runtime_projection(
    state: &Arc<AppState>,
    project_id: uuid::Uuid,
) -> Result<ExtensionRuntimeProjection, ApiError> {
    let installations = state
        .repos
        .project_extension_installation_repo
        .list_enabled_by_project(project_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(extension_runtime_projection_from_installations(
        installations,
    )?)
}

fn clear_plain_lifecycle_context(
    user_input: &UserPromptInput,
    mut plan: SessionConstructionPlan,
) -> Result<SessionConstructionPlan, ApiError> {
    let user_prompt_blocks = user_input
        .prompt_blocks
        .clone()
        .ok_or_else(|| ApiError::BadRequest("必须提供 promptBlocks".to_string()))?;
    plan.prompt.prompt_blocks = Some(user_prompt_blocks);
    plan.context.bundle = None;
    plan.context.bundle_id = None;
    plan.context.bootstrap_fragment_count = 0;
    plan.context.continuation_context_frame = None;
    Ok(plan)
}

async fn build_story_owner_prompt_request(
    state: &Arc<AppState>,
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
) -> Result<SessionConstructionPlan, ApiError> {
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
            ApiError::BadRequest(
                "Story owner prompt 缺少 executor_config，且 project 没有 default_agent_type"
                    .to_string(),
            )
        })?;

    let user_prompt_blocks = user_input
        .prompt_blocks
        .clone()
        .ok_or_else(|| ApiError::BadRequest("必须提供 promptBlocks".to_string()))?;

    let lifecycle = map_owner_prompt_lifecycle(lifecycle_kind, None);
    let (lifecycle, continuation_context_frame) =
        resolve_continuation_system_context(state, session_id, lifecycle).await?;
    let active_workflow = resolve_active_workflow_projection_for_session(
        session_id,
        state.repos.session_binding_repo.as_ref(),
        state.repos.workflow_definition_repo.as_ref(),
        state.repos.activity_lifecycle_definition_repo.as_ref(),
        state.repos.lifecycle_run_repo.as_ref(),
    )
    .await
    .map_err(ApiError::Internal)?;

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
        .map_err(ApiError::BadRequest)?;

    plan.context.continuation_context_frame = continuation_context_frame;
    if matches!(lifecycle_kind, SessionPromptLifecycle::Plain) {
        return clear_plain_lifecycle_context(user_input, plan);
    }
    Ok(plan)
}

async fn build_project_owner_prompt_request(
    state: &Arc<AppState>,
    session_id: &str,
    user_input: &UserPromptInput,
    plan: SessionConstructionPlan,
    project: &Project,
    binding_label: &str,
    _meta: &SessionMeta,
    lifecycle_kind: SessionPromptLifecycle,
    visible_canvas_mount_ids: &[String],
    source_mcp_declarations: Vec<agentdash_spi::SessionMcpServer>,
) -> Result<SessionConstructionPlan, ApiError> {
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
            ApiError::BadRequest(format!("无效的项目 Agent session label: {binding_label}"))
        })?;
    let project_agent = SessionConstructionPlanner::resolve_project_agent_context(
        &state.repos,
        project.id,
        agent_key,
    )
    .await
    .map_err(ApiError::Internal)?
    .ok_or_else(|| ApiError::NotFound(format!("Project Agent `{agent_key}` 不存在")))?;
    let workspace = SessionConstructionPlanner::resolve_project_workspace(&state.repos, project)
        .await
        .map_err(ApiError::Internal)?;

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
        .ok_or_else(|| ApiError::BadRequest("必须提供 promptBlocks".to_string()))?;

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
        state.repos.session_binding_repo.as_ref(),
        state.repos.workflow_definition_repo.as_ref(),
        state.repos.activity_lifecycle_definition_repo.as_ref(),
        state.repos.lifecycle_run_repo.as_ref(),
    )
    .await
    .map_err(ApiError::Internal)?;

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
        .map_err(ApiError::BadRequest)?;

    plan.context.continuation_context_frame = continuation_context_frame;
    if matches!(lifecycle_kind, SessionPromptLifecycle::Plain) {
        return clear_plain_lifecycle_context(user_input, plan);
    }
    Ok(plan)
}

async fn build_lifecycle_node_prompt_request(
    state: &Arc<AppState>,
    session_id: &str,
    user_input: &UserPromptInput,
    plan: SessionConstructionPlan,
    lifecycle_kind: SessionPromptLifecycle,
) -> Result<SessionConstructionPlan, ApiError> {
    let runs = state
        .repos
        .lifecycle_run_repo
        .list_by_session(session_id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    let run = select_active_run(runs).ok_or_else(|| {
        ApiError::BadRequest(format!("Lifecycle node session {session_id} 无活跃 run"))
    })?;
    let lifecycle = state
        .repos
        .lifecycle_definition_repo
        .get_by_id(run.lifecycle_id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Lifecycle {} 不存在", run.lifecycle_id)))?;
    let current_step_key = run.current_step_key().ok_or_else(|| {
        ApiError::BadRequest(format!("Lifecycle node session {session_id} 无当前 step"))
    })?;
    let step = lifecycle
        .steps
        .iter()
        .find(|item| item.key == current_step_key)
        .cloned()
        .ok_or_else(|| {
            ApiError::BadRequest(format!(
                "Lifecycle {} 中不存在当前 step `{}`",
                lifecycle.id, current_step_key
            ))
        })?;
    let workflow = match step.effective_workflow_key() {
        Some(key) => state
            .repos
            .workflow_definition_repo
            .get_by_project_and_key(run.project_id, key)
            .await
            .map_err(|error| ApiError::Internal(error.to_string()))?,
        None => None,
    };
    let audit_bus = Some(state.services.audit_bus.clone());

    let plan = compose_lifecycle_node_prompt_with_audit(
        plan,
        &state.repos,
        &state.config.platform_config,
        LifecycleNodeSpec {
            run: &run,
            lifecycle: &lifecycle,
            step: &step,
            workflow: workflow.as_ref(),
            inherited_executor_config: None,
        },
        audit_bus,
        Some(session_id),
    )
    .await
    .map_err(ApiError::BadRequest)?;

    if matches!(lifecycle_kind, SessionPromptLifecycle::Plain) {
        return clear_plain_lifecycle_context(user_input, plan);
    }
    Ok(plan)
}

async fn build_companion_dispatch_prompt_request(
    state: &Arc<AppState>,
    plan: SessionConstructionPlan,
    companion: CompanionLaunchSource,
) -> Result<SessionConstructionPlan, ApiError> {
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
                    step: &workflow.step,
                    workflow: workflow.workflow.as_ref(),
                },
            )
            .await
            .map_err(ApiError::BadRequest)
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
            .map_err(ApiError::BadRequest)
    }
}

fn build_session_assembler(state: &Arc<AppState>) -> SessionRequestAssembler<'_> {
    SessionRequestAssembler::new(
        state.services.vfs_service.as_ref(),
        state.repos.canvas_repo.as_ref(),
        state.services.backend_registry.as_ref(),
        &state.repos,
        &state.config.platform_config,
    )
    .with_audit_bus(state.services.audit_bus.clone())
    .with_companion_parent_facts_provider(&state.services.session_capability)
}

async fn build_continuation_context_frame_for_session(
    state: &Arc<AppState>,
    session_id: &str,
) -> Result<Option<ContextFrame>, ApiError> {
    let transcript = state
        .services
        .session_eventing
        .build_projected_transcript(session_id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    Ok(
        agentdash_application::session::continuation::build_continuation_context_frame(
            &transcript,
            None,
        ),
    )
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
    state: &Arc<AppState>,
    session_id: &str,
    lifecycle: OwnerPromptLifecycle,
) -> Result<(OwnerPromptLifecycle, Option<ContextFrame>), ApiError> {
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

async fn build_task_owner_prompt_request(
    state: &Arc<AppState>,
    session_id: &str,
    user_input: &UserPromptInput,
    plan: SessionConstructionPlan,
    task_id: uuid::Uuid,
    meta: &SessionMeta,
    lifecycle_kind: SessionPromptLifecycle,
    visible_canvas_mount_ids: &[String],
    task_input: Option<TaskLaunchSource>,
    source_mcp_declarations: Vec<agentdash_spi::SessionMcpServer>,
) -> Result<SessionConstructionPlan, ApiError> {
    let story = state
        .repos
        .story_repo
        .find_by_task_id(task_id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Task {task_id} 不存在")))?;
    let task = story
        .find_task(task_id)
        .cloned()
        .ok_or_else(|| ApiError::NotFound(format!("Task {task_id} 不存在")))?;

    let effective_executor_config = user_input
        .executor_config
        .clone()
        .or_else(|| meta.executor_config.clone());

    let project = state
        .repos
        .project_repo
        .get_by_id(story.project_id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Project {} 不存在", story.project_id)))?;
    let workspace = resolve_effective_task_workspace(&state.repos, &task, &story, &project)
        .await
        .map_err(ApiError::from)?;
    let active_workflow = resolve_active_workflow_projection_for_session(
        session_id,
        state.repos.session_binding_repo.as_ref(),
        state.repos.workflow_definition_repo.as_ref(),
        state.repos.activity_lifecycle_definition_repo.as_ref(),
        state.repos.lifecycle_run_repo.as_ref(),
    )
    .await
    .map_err(ApiError::Internal)?
    .ok_or_else(|| {
        ApiError::BadRequest(format!(
            "Task session {session_id} 未绑定活跃 lifecycle step"
        ))
    })?;

    let user_prompt_blocks = user_input
        .prompt_blocks
        .clone()
        .ok_or_else(|| ApiError::BadRequest("必须提供 promptBlocks".to_string()))?;

    let task_phase = task_input
        .as_ref()
        .and_then(|input| input.phase)
        .unwrap_or(TaskLaunchPhase::Continue);
    let assembler = build_session_assembler(state);
    let mut plan = assembler
        .compose_story_step_prompt(
            plan,
            StoryStepSpec {
                run: &active_workflow.run,
                lifecycle: &active_workflow.lifecycle,
                step: &active_workflow.active_step,
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
                active_workflow: Some(active_workflow.clone()),
                audit_session_key: Some(session_id.to_string()),
            },
        )
        .await
        .map_err(ApiError::from)?;

    if let Some(space) = plan.surface.vfs.as_mut() {
        append_visible_canvas_mounts(
            state.repos.canvas_repo.as_ref(),
            task.project_id,
            space,
            visible_canvas_mount_ids,
        )
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
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
        return Err(ApiError::Internal(
            "Task session compose 未产出 capability_state".to_string(),
        ));
    }

    plan.context.continuation_context_frame = continuation_context_frame;

    Ok(plan)
}

#[cfg(test)]
mod tests {
    use agentdash_application::session::construction::{
        SessionConstructionContextProjection, SessionConstructionPlan,
    };
    use agentdash_application::session::ownership::SessionOwnerResolver;
    use agentdash_domain::common::{Mount, MountCapability};
    use agentdash_domain::extension_package::ExtensionPackageMetadata;
    use agentdash_domain::session_binding::{SessionBinding, SessionOwnerType};
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
        let binding = SessionBinding::new(
            uuid::Uuid::new_v4(),
            "sess-plain".to_string(),
            SessionOwnerType::Project,
            uuid::Uuid::new_v4(),
            "project_agent:test",
        );
        let owner = SessionOwnerResolver::resolve_primary(&[binding]).expect("owner");
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

        plan.surface.vfs = Some(vfs.clone());
        plan.context.bundle = Some(agentdash_spi::SessionContextBundle::new(
            uuid::Uuid::new_v4(),
            "project_agent",
        ));
        plan.context.bundle_id = plan.context.bundle.as_ref().map(|bundle| bundle.bundle_id);
        plan.context.bootstrap_fragment_count = 1;
        plan.projections.mcp_servers = vec![mcp];
        plan.projections.capability_state = Some(capability);
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
