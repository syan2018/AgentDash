//! Session launch construction use case.
//!
//! 这个模块承接原本挂在 `routes/acp_sessions.rs` 里的 owner/context/capability
//! 组装逻辑。它返回 `SessionLaunchPlan`，其中 owner/source 只作为
//! `SessionConstructionPlan` 的输入种子；route 层不再承载 launch composition 主分支。

use std::sync::Arc;

use agentdash_application::canvas::append_visible_canvas_mounts;
use agentdash_application::session::ownership::SessionOwnerResolver;
use agentdash_application::session::types::SessionLaunchPlan;
use agentdash_application::session::{
    AgentLevelMcp, CompanionSpec, CompanionWorkflowSpec, HookSnapshotReloadTrigger,
    LifecycleNodeSpec, OwnerBootstrapSpec, OwnerPromptLifecycle, OwnerScope,
    PromptAugmentCompanionInput, PromptAugmentInput, PromptAugmentTaskInput,
    PromptAugmentTaskPhase, SessionMeta, SessionPromptLifecycle, SessionRepositoryRehydrateMode,
    SessionRequestAssembler, StoryStepPhase, StoryStepSpec, compose_companion_prompt,
    compose_companion_with_workflow_prompt, compose_lifecycle_node_prompt_with_audit,
    resolve_session_prompt_lifecycle,
};
use agentdash_application::task::gateway::resolve_effective_task_workspace;
use agentdash_application::workflow::resolve_active_workflow_projection_for_session;
use agentdash_application::workflow::{LIFECYCLE_NODE_LABEL_PREFIX, select_active_run};
use agentdash_domain::{
    project::Project, session_binding::SessionOwnerType, story::Story, workspace::Workspace,
};
use agentdash_spi::hooks::ContextFrame;

use crate::app_state::AppState;
use crate::routes::project_agents::{
    parse_project_agent_session_label, resolve_project_agent_bridge_async,
    resolve_project_workspace,
};
use crate::routes::task_execution;
use crate::rpc::ApiError;

pub(crate) async fn augment_prompt_request_for_owner(
    state: &Arc<AppState>,
    session_id: &str,
    input: PromptAugmentInput,
) -> Result<SessionLaunchPlan, ApiError> {
    let task_input = input.task.clone();
    let companion_input = input.companion.clone();
    let mut req = input.into_launch_plan();
    if let Some(companion) = companion_input {
        return build_companion_dispatch_prompt_request(state, req, companion).await;
    }
    let meta = state
        .services
        .session_hub
        .get_session_meta(session_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("会话 {} 不存在", session_id)))?;
    let bindings = state
        .repos
        .session_binding_repo
        .list_by_session(session_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let visible_canvas_mount_ids = meta.visible_canvas_mount_ids.clone();
    let effective_executor = req
        .user_input
        .executor_config
        .clone()
        .or_else(|| meta.executor_config.clone());
    let has_live_executor_session = state
        .services
        .session_hub
        .has_live_executor_session(session_id)
        .await;
    let supports_repository_restore = effective_executor.as_ref().is_some_and(|config| {
        state
            .services
            .connector
            .supports_repository_restore(config.executor.as_str())
    });
    let lifecycle_kind = resolve_session_prompt_lifecycle(
        &meta,
        has_live_executor_session,
        supports_repository_restore,
    );

    if let Some(owner) = SessionOwnerResolver::resolve_primary(&bindings) {
        req.construction_owner = Some(owner.clone());
        match owner.owner_type {
            SessionOwnerType::Task => {
                return build_task_owner_prompt_request(
                    state,
                    session_id,
                    req,
                    owner.owner_id,
                    &meta,
                    lifecycle_kind,
                    &visible_canvas_mount_ids,
                    task_input,
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
                let workspace = resolve_project_workspace(state, &project).await?;

                return build_story_owner_prompt_request(
                    state,
                    session_id,
                    req,
                    &story,
                    &project,
                    workspace.as_ref(),
                    &meta,
                    lifecycle_kind,
                    &visible_canvas_mount_ids,
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

                return build_project_owner_prompt_request(
                    state,
                    session_id,
                    req,
                    &project,
                    &owner.label,
                    &meta,
                    lifecycle_kind,
                    &visible_canvas_mount_ids,
                )
                .await;
            }
        }
    }

    if let SessionPromptLifecycle::RepositoryRehydrate(
        SessionRepositoryRehydrateMode::SystemContext,
    ) = lifecycle_kind
    {
        let continuation_context_frame =
            build_continuation_context_frame_for_session(state, session_id).await?;
        return apply_plain_lifecycle_request(
            req,
            None,
            continuation_context_frame,
            HookSnapshotReloadTrigger::None,
        );
    }

    Ok(req)
}

fn apply_plain_lifecycle_request(
    mut req: SessionLaunchPlan,
    context_bundle: Option<agentdash_spi::SessionContextBundle>,
    continuation_context_frame: Option<ContextFrame>,
    hook_snapshot_reload: HookSnapshotReloadTrigger,
) -> Result<SessionLaunchPlan, ApiError> {
    let user_prompt_blocks = req
        .user_input
        .prompt_blocks
        .take()
        .ok_or_else(|| ApiError::BadRequest("必须提供 promptBlocks".to_string()))?;
    req.user_input.prompt_blocks = Some(user_prompt_blocks);
    req.context_bundle = context_bundle;
    req.continuation_context_frame = continuation_context_frame;
    req.hook_snapshot_reload = hook_snapshot_reload;
    Ok(req)
}

async fn build_story_owner_prompt_request(
    state: &Arc<AppState>,
    session_id: &str,
    mut req: SessionLaunchPlan,
    story: &Story,
    project: &Project,
    workspace: Option<&Workspace>,
    _meta: &SessionMeta,
    lifecycle_kind: SessionPromptLifecycle,
    visible_canvas_mount_ids: &[String],
) -> Result<SessionLaunchPlan, ApiError> {
    if matches!(lifecycle_kind, SessionPromptLifecycle::Plain) {
        return apply_plain_lifecycle_request(req, None, None, HookSnapshotReloadTrigger::None);
    }

    let effective_executor_config = req
        .user_input
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

    let user_prompt_blocks = req
        .user_input
        .prompt_blocks
        .take()
        .ok_or_else(|| ApiError::BadRequest("必须提供 promptBlocks".to_string()))?;

    let lifecycle = map_owner_prompt_lifecycle(lifecycle_kind, None);
    let (lifecycle, continuation_context_frame) =
        resolve_continuation_system_context(state, session_id, lifecycle).await?;
    let active_workflow = resolve_active_workflow_projection_for_session(
        session_id,
        state.repos.session_binding_repo.as_ref(),
        state.repos.workflow_definition_repo.as_ref(),
        state.repos.lifecycle_definition_repo.as_ref(),
        state.repos.lifecycle_run_repo.as_ref(),
    )
    .await
    .map_err(ApiError::Internal)?;

    let request_mcp_servers = req.mcp_servers.clone();
    let existing_vfs = req.vfs.clone();
    let assembler = build_session_assembler(state);
    let mut req = assembler
        .compose_owner_bootstrap_prompt(
            req,
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
                request_mcp_servers,
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

    req.continuation_context_frame = continuation_context_frame;
    Ok(req)
}

async fn build_project_owner_prompt_request(
    state: &Arc<AppState>,
    session_id: &str,
    mut req: SessionLaunchPlan,
    project: &Project,
    binding_label: &str,
    _meta: &SessionMeta,
    lifecycle_kind: SessionPromptLifecycle,
    visible_canvas_mount_ids: &[String],
) -> Result<SessionLaunchPlan, ApiError> {
    if matches!(lifecycle_kind, SessionPromptLifecycle::Plain) {
        return apply_plain_lifecycle_request(req, None, None, HookSnapshotReloadTrigger::None);
    }

    if binding_label.starts_with(LIFECYCLE_NODE_LABEL_PREFIX) {
        return build_lifecycle_node_prompt_request(state, session_id, req, lifecycle_kind).await;
    }

    let agent_key = parse_project_agent_session_label(binding_label).ok_or_else(|| {
        ApiError::BadRequest(format!("无效的项目 Agent session label: {binding_label}"))
    })?;
    let project_agent = resolve_project_agent_bridge_async(state, project.id, agent_key)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Project Agent `{agent_key}` 不存在")))?;
    let workspace = resolve_project_workspace(state, project).await?;

    let effective_executor_config = match req.user_input.executor_config.clone() {
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

    let user_prompt_blocks = req
        .user_input
        .prompt_blocks
        .take()
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

    let lifecycle = map_owner_prompt_lifecycle(lifecycle_kind, None);
    let (lifecycle, continuation_context_frame) =
        resolve_continuation_system_context(state, session_id, lifecycle).await?;
    let active_workflow = resolve_active_workflow_projection_for_session(
        session_id,
        state.repos.session_binding_repo.as_ref(),
        state.repos.workflow_definition_repo.as_ref(),
        state.repos.lifecycle_definition_repo.as_ref(),
        state.repos.lifecycle_run_repo.as_ref(),
    )
    .await
    .map_err(ApiError::Internal)?;

    let request_mcp_servers = req.mcp_servers.clone();
    let existing_vfs = req.vfs.clone();
    let assembler = build_session_assembler(state);
    let mut req = assembler
        .compose_owner_bootstrap_prompt(
            req,
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
                request_mcp_servers,
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

    req.continuation_context_frame = continuation_context_frame;
    Ok(req)
}

async fn build_lifecycle_node_prompt_request(
    state: &Arc<AppState>,
    session_id: &str,
    req: SessionLaunchPlan,
    lifecycle_kind: SessionPromptLifecycle,
) -> Result<SessionLaunchPlan, ApiError> {
    if matches!(lifecycle_kind, SessionPromptLifecycle::Plain) {
        return apply_plain_lifecycle_request(req, None, None, HookSnapshotReloadTrigger::None);
    }

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

    compose_lifecycle_node_prompt_with_audit(
        req,
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
    .map_err(ApiError::BadRequest)
}

async fn build_companion_dispatch_prompt_request(
    state: &Arc<AppState>,
    req: SessionLaunchPlan,
    companion: PromptAugmentCompanionInput,
) -> Result<SessionLaunchPlan, ApiError> {
    if let Some(workflow) = companion.workflow {
        compose_companion_with_workflow_prompt(
            req,
            &state.repos,
            &state.config.platform_config,
            CompanionWorkflowSpec {
                companion: CompanionSpec {
                    parent_vfs: companion.parent_vfs.as_ref(),
                    parent_mcp_servers: &companion.parent_mcp_servers,
                    parent_context_bundle: companion.parent_context_bundle.as_ref(),
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
        Ok(compose_companion_prompt(
            req,
            CompanionSpec {
                parent_vfs: companion.parent_vfs.as_ref(),
                parent_mcp_servers: &companion.parent_mcp_servers,
                parent_context_bundle: companion.parent_context_bundle.as_ref(),
                slice_mode: companion.slice_mode,
                companion_executor_config: companion.companion_executor_config,
                dispatch_prompt: companion.dispatch_prompt,
            },
        ))
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
}

async fn build_continuation_context_frame_for_session(
    state: &Arc<AppState>,
    session_id: &str,
) -> Result<Option<ContextFrame>, ApiError> {
    let transcript = state
        .services
        .session_hub
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
    req: SessionLaunchPlan,
    task_id: uuid::Uuid,
    meta: &SessionMeta,
    lifecycle_kind: SessionPromptLifecycle,
    visible_canvas_mount_ids: &[String],
    task_input: Option<PromptAugmentTaskInput>,
) -> Result<SessionLaunchPlan, ApiError> {
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

    let effective_executor_config = req
        .user_input
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
        .map_err(task_execution::map_task_execution_error)?;
    let active_workflow = resolve_active_workflow_projection_for_session(
        session_id,
        state.repos.session_binding_repo.as_ref(),
        state.repos.workflow_definition_repo.as_ref(),
        state.repos.lifecycle_definition_repo.as_ref(),
        state.repos.lifecycle_run_repo.as_ref(),
    )
    .await
    .map_err(ApiError::Internal)?
    .ok_or_else(|| {
        ApiError::BadRequest(format!(
            "Task session {session_id} 未绑定活跃 lifecycle step"
        ))
    })?;

    let user_prompt_blocks = req
        .user_input
        .prompt_blocks
        .clone()
        .ok_or_else(|| ApiError::BadRequest("必须提供 promptBlocks".to_string()))?;

    let task_phase = task_input
        .as_ref()
        .and_then(|input| input.phase)
        .unwrap_or(PromptAugmentTaskPhase::Continue);
    let assembler = build_session_assembler(state);
    let mut req = assembler
        .compose_story_step_prompt(
            req,
            StoryStepSpec {
                run: &active_workflow.run,
                lifecycle: &active_workflow.lifecycle,
                step: &active_workflow.active_step,
                task: &task,
                story: &story,
                project: &project,
                workspace: workspace.as_ref(),
                phase: match task_phase {
                    PromptAugmentTaskPhase::Start => StoryStepPhase::Start,
                    PromptAugmentTaskPhase::Continue => StoryStepPhase::Continue,
                },
                override_prompt: task_input
                    .as_ref()
                    .and_then(|input| input.override_prompt.as_deref()),
                additional_prompt: task_input
                    .as_ref()
                    .and_then(|input| input.additional_prompt.as_deref()),
                explicit_executor_config: effective_executor_config.clone(),
                strict_config_resolution: true,
                active_workflow: Some(active_workflow.clone()),
                audit_session_key: Some(session_id.to_string()),
            },
        )
        .await
        .map_err(task_execution::map_task_execution_error)?;

    if let Some(space) = req.vfs.as_mut() {
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
    req.user_input.prompt_blocks = Some(user_prompt_blocks);
    req.hook_snapshot_reload = HookSnapshotReloadTrigger::None;

    match lifecycle_kind {
        SessionPromptLifecycle::OwnerBootstrap => {
            req.hook_snapshot_reload = HookSnapshotReloadTrigger::Reload;
        }
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
            req.context_bundle = None;
        }
    }

    if req.user_input.executor_config.is_none()
        && let Some(config) = effective_executor_config
    {
        req.user_input.executor_config = Some(config);
    }

    if req.capability_state.is_none() {
        return Err(ApiError::Internal(
            "Task session compose 未产出 capability_state".to_string(),
        ));
    }

    req.continuation_context_frame = continuation_context_frame;

    Ok(req)
}
