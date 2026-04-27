use agentdash_domain::common::Vfs;
use agentdash_domain::session_binding::SessionOwnerType;
use agentdash_domain::task::Task;
use agentdash_domain::workspace::Workspace;

use crate::context::{BuiltTaskAgentContext, ContextContributorRegistry, TaskExecutionPhase};
use crate::platform_config::PlatformConfig;
use crate::repository_set::RepositorySet;
use crate::session::TaskRuntimePhase;
use crate::task::execution::{ExecutionPhase, TaskExecutionError};
use crate::task::gateway::{load_related_context, map_internal_error};
use crate::vfs::RelayVfsService;
use crate::workspace::BackendAvailability;
use agentdash_domain::common::AgentConfig;

/// 基础设施引用 — prepare_task_turn_context 中不因调用而变化的部分
pub struct TaskTurnServices<'a> {
    pub repos: &'a RepositorySet,
    pub availability: &'a dyn BackendAvailability,
    pub vfs_service: &'a RelayVfsService,
    pub contributor_registry: &'a ContextContributorRegistry,
    pub platform_config: &'a PlatformConfig,
}

/// 准备好的 turn 上下文 — 包含 dispatch 所需的所有数据
///
/// M5 之后，`prepare_task_turn_context` 内部改走 `compose_story_step` 统一链路，
/// 但对外仍保留 `PreparedTurnContext` 结构以兼容 `acp_sessions.rs::augment_session_prompt`
/// 等历史消费者。`task_lifecycle_service::start_task / continue_task` 新路径
/// 走 `activate_story_step → compose_story_step → finalize_request` 派发，
/// 不再消费本结构。
pub struct PreparedTurnContext {
    pub built: BuiltTaskAgentContext,
    pub vfs: Option<Vfs>,
    pub resolved_config: Option<AgentConfig>,
    pub use_cloud_native_agent: bool,
    pub workspace: Option<Workspace>,
    /// CapabilityResolver 产出的内置工具簇（dispatcher 直接使用，不再硬编码）。
    pub flow_capabilities: agentdash_spi::FlowCapabilities,
    /// CapabilityResolver 产出的有效 capability key（用于 hook runtime 追踪）。
    pub effective_capability_keys: std::collections::BTreeSet<String>,
    /// 需走 relay 的自定义 MCP server name 集合。
    pub relay_mcp_server_names: std::collections::HashSet<String>,
    /// 发起本次 task 执行的用户身份（由 HTTP handler 注入）。
    pub identity: Option<agentdash_spi::auth::AuthIdentity>,
    /// Hook effect 回调（cloud-native 路径取代 TurnMonitor）。
    /// Relay 路径暂不使用此字段。
    pub post_turn_handler: Option<crate::session::post_turn_handler::DynPostTurnHandler>,
}

/// 从 Task / Story / Project / Workspace 等上下文中构建 turn 执行所需的完整信息。
///
/// M5 之后，内部走 `SessionRequestAssembler::compose_story_step`；但由于
/// 本函数仍被 `acp_sessions.rs::augment_session_prompt` 消费（该路径不经过
/// `start_task / continue_task` facade），保留外部签名兼容。
pub async fn prepare_task_turn_context(
    svc: &TaskTurnServices<'_>,
    task: &Task,
    phase: ExecutionPhase,
    override_prompt: Option<&str>,
    additional_prompt: Option<&str>,
    connector_config: Option<&AgentConfig>,
) -> Result<PreparedTurnContext, TaskExecutionError> {
    let (story, project, workspace) = load_related_context(svc.repos, task)
        .await
        .map_err(map_internal_error)?;

    let runtime_phase = match phase {
        ExecutionPhase::Start => TaskRuntimePhase::Start,
        ExecutionPhase::Continue => TaskRuntimePhase::Continue,
    };

    // 定位 task 当前活跃 lifecycle run 投影（若存在）。
    let active_workflow = find_active_workflow_via_task_sessions(svc.repos, task.id)
        .await
        .map_err(TaskExecutionError::Internal)?;

    // 为保留历史结构 `PreparedTurnContext`（含 built / resolved_config / vfs 等独立字段），
    // 我们走底层 compose pipeline 的“去壳版本”——直接调 build_task_agent_context 的
    // 附属逻辑，然后把同一份数据同时填回 PreparedTurnContext。
    //
    // 注意：这里不能直接调 compose_story_step，因为它的输出是 PreparedSessionInputs
    // （prompt_blocks 已被 contributor pipeline 打包），而 acp_sessions.rs 需要读取
    // 结构化的 `built.prompt_blocks / built.system_context / built.source_summary`
    // 等中间产物以便自行编排。
    //
    // 折中方案：在此调用底层 helper（pure-Rust），与 compose_story_step 共享实现。
    let prepared = build_prepared_turn_context(
        svc,
        task,
        &story,
        &project,
        workspace.as_ref(),
        runtime_phase,
        override_prompt,
        additional_prompt,
        connector_config.cloned(),
        active_workflow,
    )
    .await?;

    Ok(prepared)
}

/// 与 `compose_story_step` 共享实现的“解壳版本”，产出 `PreparedTurnContext` 结构。
///
/// 保留 `built: BuiltTaskAgentContext`、`vfs: Option<Vfs>`、`resolved_config: Option<AgentConfig>`
/// 等中间字段独立暴露，供 `acp_sessions.rs::augment_session_prompt` 消费。
#[allow(clippy::too_many_arguments)]
async fn build_prepared_turn_context(
    svc: &TaskTurnServices<'_>,
    task: &Task,
    story: &agentdash_domain::story::Story,
    project: &agentdash_domain::project::Project,
    workspace: Option<&Workspace>,
    phase: TaskRuntimePhase,
    override_prompt: Option<&str>,
    additional_prompt: Option<&str>,
    explicit_executor_config: Option<AgentConfig>,
    active_workflow: Option<crate::workflow::ActiveWorkflowProjection>,
) -> Result<PreparedTurnContext, TaskExecutionError> {
    use crate::capability::{
        CapabilityResolver, CapabilityResolverInput, SessionWorkflowContext,
        capability_directives_from_active_workflow,
    };
    use crate::context::{
        McpContextContributor, StaticFragmentsContributor, TaskAgentBuildInput,
        WorkflowContextBindingsContributor, build_declared_source_warning_fragment,
        build_task_agent_context, resolve_workspace_declared_sources,
    };
    use crate::session::{ExecutorResolution, load_available_presets};
    use crate::task::config::{resolve_task_executor_config, resolve_task_executor_source};
    use crate::vfs::{SessionMountTarget, build_lifecycle_mount_with_ports, resolve_context_bindings};
    use agentdash_domain::session_binding::SessionOwnerCtx;
    use std::collections::BTreeSet;

    let executor_source =
        resolve_task_executor_source(task, project, explicit_executor_config.as_ref());
    let (resolved_config, _executor_resolution) = match resolve_task_executor_config(
        explicit_executor_config,
        task,
        project,
    ) {
        Ok(config) => (config, ExecutorResolution::resolved(executor_source)),
        Err(err) => return Err(err),
    };

    let effective_agent_type = resolved_config.as_ref().map(|c| c.executor.as_str());
    let use_cloud_native = resolved_config
        .as_ref()
        .is_some_and(|c| c.is_cloud_native());

    let vfs = if use_cloud_native {
        let mut space = svc
            .vfs_service
            .build_vfs(
                project,
                Some(story),
                workspace,
                SessionMountTarget::Task,
                effective_agent_type,
            )
            .map_err(|error| TaskExecutionError::Internal(error.to_string()))?;

        if let Some(active_workflow) = active_workflow.as_ref() {
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

    let resolved_bindings = match (&vfs, &active_workflow) {
        (Some(space), Some(wf)) => {
            let bindings = wf
                .active_contract()
                .map(|c| c.injection.context_bindings.as_slice())
                .unwrap_or(&[]);
            if bindings.is_empty() {
                None
            } else {
                Some(
                    resolve_context_bindings(bindings, space, svc.vfs_service)
                        .await
                        .map_err(TaskExecutionError::UnprocessableEntity)?,
                )
            }
        }
        _ => None,
    };

    let workflow_directives = active_workflow.as_ref().and_then(|p| {
        p.primary_workflow
            .as_ref()
            .map(capability_directives_from_active_workflow)
    });
    let workflow_ctx = SessionWorkflowContext {
        has_active_workflow: active_workflow.is_some(),
        workflow_capability_directives: workflow_directives,
    };
    let cap_input = CapabilityResolverInput {
        owner_ctx: SessionOwnerCtx::Task {
            project_id: task.project_id,
            story_id: task.story_id,
            task_id: task.id,
        },
        agent_declared_capabilities: None,
        workflow_ctx,
        agent_mcp_servers: vec![],
        available_presets: load_available_presets(svc.repos, task.project_id).await,
        companion_slice_mode: None,
    };
    let cap_output = CapabilityResolver::resolve(&cap_input, svc.platform_config);

    let platform_mcp_configs = cap_output.platform_mcp_configs.clone();
    let relay_mcp_server_names: std::collections::HashSet<String> = cap_output
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

    let mut extra_contributors: Vec<Box<dyn crate::context::ContextContributor>> = Vec::new();
    let mut declared_sources = story.context.source_refs.clone();
    declared_sources.extend(task.agent_binding.context_sources.clone());
    let resolved_workspace_sources = resolve_workspace_declared_sources(
        svc.availability,
        svc.vfs_service,
        &declared_sources,
        workspace,
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
    if let (Some(wf), Some(bindings_out)) = (active_workflow.clone(), resolved_bindings.clone()) {
        extra_contributors.push(Box::new(WorkflowContextBindingsContributor::new(
            wf,
            bindings_out,
        )));
    }

    let task_phase = match phase {
        TaskRuntimePhase::Start => TaskExecutionPhase::Start,
        TaskRuntimePhase::Continue => TaskExecutionPhase::Continue,
    };
    let built = build_task_agent_context(
        TaskAgentBuildInput {
            task,
            story,
            project,
            workspace,
            vfs: vfs.as_ref(),
            effective_agent_type,
            phase: task_phase,
            override_prompt,
            additional_prompt,
            extra_contributors,
        },
        svc.contributor_registry,
    )
    .map_err(TaskExecutionError::UnprocessableEntity)?;

    Ok(PreparedTurnContext {
        built,
        vfs,
        resolved_config,
        use_cloud_native_agent: use_cloud_native,
        workspace: workspace.cloned(),
        flow_capabilities,
        effective_capability_keys,
        relay_mcp_server_names,
        identity: None,
        post_turn_handler: None,
    })
}

/// 通过 task 的 session binding 反查活跃 lifecycle run projection。
async fn find_active_workflow_via_task_sessions(
    repos: &RepositorySet,
    task_id: uuid::Uuid,
) -> Result<Option<crate::workflow::ActiveWorkflowProjection>, String> {
    use crate::workflow::resolve_active_workflow_projection_for_session;

    let bindings = repos
        .session_binding_repo
        .list_by_owner(SessionOwnerType::Task, task_id)
        .await
        .map_err(|e| e.to_string())?;

    for binding in &bindings {
        if let Some(projection) = resolve_active_workflow_projection_for_session(
            &binding.session_id,
            repos.session_binding_repo.as_ref(),
            repos.workflow_definition_repo.as_ref(),
            repos.lifecycle_definition_repo.as_ref(),
            repos.lifecycle_run_repo.as_ref(),
        )
        .await?
        {
            return Ok(Some(projection));
        }
    }
    Ok(None)
}
