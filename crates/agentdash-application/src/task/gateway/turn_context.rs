use agentdash_domain::common::AddressSpace;
use agentdash_domain::task::Task;
use uuid::Uuid;

use crate::address_space::{
    build_lifecycle_mount, RelayAddressSpaceService, SessionMountTarget,
};
use crate::context::{
    BuiltTaskAgentContext, ContextContributor, ContextContributorRegistry,
    McpContextContributor, StaticFragmentsContributor, TaskAgentBuildInput,
    TaskExecutionPhase, build_declared_source_warning_fragment, build_task_agent_context,
    resolve_workspace_declared_sources,
};
use agentdash_domain::common::AgentConfig;
use agentdash_executor::is_native_agent;
use agentdash_mcp::injection::McpInjectionConfig;

use crate::repository_set::RepositorySet;
use crate::runtime_bridge::mcp_injection_config_to_runtime_binding;
use crate::task::config::resolve_task_executor_config;
use crate::task::execution::{ExecutionPhase, TaskExecutionError};
use crate::task::gateway::repo_ops::{load_related_context, map_internal_error};
use crate::workflow::select_active_run;
use crate::workspace::BackendAvailability;

/// 准备好的 turn 上下文 — 包含 dispatch 所需的所有数据
pub struct PreparedTurnContext {
    pub built: BuiltTaskAgentContext,
    pub address_space: Option<AddressSpace>,
    pub resolved_config: Option<AgentConfig>,
    pub use_cloud_native_agent: bool,
    pub workspace: Option<agentdash_domain::workspace::Workspace>,
}

/// 从 Task / Story / Project / Workspace 等上下文中构建 turn 执行所需的完整信息
///
/// 这是 `start_task_turn` 中"准备阶段"的核心逻辑，不涉及实际 dispatch。
pub async fn prepare_task_turn_context(
    repos: &RepositorySet,
    availability: &dyn BackendAvailability,
    address_space_service: &RelayAddressSpaceService,
    contributor_registry: &ContextContributorRegistry,
    task: &Task,
    phase: ExecutionPhase,
    override_prompt: Option<&str>,
    additional_prompt: Option<&str>,
    connector_config: Option<&AgentConfig>,
    mcp_base_url: Option<&str>,
) -> Result<PreparedTurnContext, TaskExecutionError> {
    let (story, project, workspace) = load_related_context(repos, task)
        .await
        .map_err(map_internal_error)?;

    let mut extra_contributors: Vec<Box<dyn ContextContributor>> = Vec::new();

    // declared sources resolution
    let mut declared_sources = story.context.source_refs.clone();
    declared_sources.extend(task.agent_binding.context_sources.clone());
    let resolved_workspace_sources = resolve_workspace_declared_sources(
        availability,
        address_space_service,
        &declared_sources,
        workspace.as_ref(),
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

    // MCP injection
    if let Some(base_url) = mcp_base_url {
        let config = McpInjectionConfig::for_task(
            base_url.to_string(),
            story.project_id,
            task.story_id,
            task.id,
        );
        extra_contributors.push(Box::new(McpContextContributor::new(
            mcp_injection_config_to_runtime_binding(&config),
        )));
    }

    // executor config resolution
    let resolved_config = resolve_task_executor_config(
        connector_config.cloned(),
        task,
        &project,
    )
    .map_err(map_internal_error)?;
    let use_cloud_native_agent = resolved_config
        .as_ref()
        .is_some_and(|config| is_native_agent(config));

    // address space building
    let address_space = if use_cloud_native_agent {
        let agent_type = resolved_config
            .as_ref()
            .map(|config| config.executor.as_str());
        let mut space = address_space_service
            .build_address_space(
                &project,
                Some(&story),
                workspace.as_ref(),
                SessionMountTarget::Task,
                agent_type,
            )
            .map_err(map_internal_error)?;

        // lifecycle mount injection
        if let Some(active_run) = find_active_lifecycle_run(repos, task).await? {
            let lifecycle_key = resolve_lifecycle_key(repos, active_run.lifecycle_id).await;
            space
                .mounts
                .push(build_lifecycle_mount(active_run.id, &lifecycle_key));
        }

        Some(space)
    } else {
        None
    };

    // build full agent context
    let built = build_task_agent_context(
        TaskAgentBuildInput {
            task,
            story: &story,
            project: &project,
            workspace: workspace.as_ref(),
            address_space: address_space.as_ref(),
            effective_agent_type: resolved_config
                .as_ref()
                .map(|config| config.executor.as_str()),
            phase: match phase {
                ExecutionPhase::Start => TaskExecutionPhase::Start,
                ExecutionPhase::Continue => TaskExecutionPhase::Continue,
            },
            override_prompt,
            additional_prompt,
            extra_contributors,
        },
        contributor_registry,
    )
    .map_err(TaskExecutionError::UnprocessableEntity)?;

    Ok(PreparedTurnContext {
        built,
        address_space,
        resolved_config,
        use_cloud_native_agent,
        workspace,
    })
}

async fn find_active_lifecycle_run(
    repos: &RepositorySet,
    task: &Task,
) -> Result<Option<agentdash_domain::workflow::LifecycleRun>, TaskExecutionError> {
    let runs = repos
        .lifecycle_run_repo
        .list_by_target(
            agentdash_domain::workflow::WorkflowTargetKind::Task,
            task.id,
        )
        .await
        .map_err(|e| TaskExecutionError::Internal(format!("查询 lifecycle runs 失败: {e}")))?;
    Ok(select_active_run(runs))
}

async fn resolve_lifecycle_key(repos: &RepositorySet, lifecycle_id: Uuid) -> String {
    match repos
        .lifecycle_definition_repo
        .get_by_id(lifecycle_id)
        .await
    {
        Ok(Some(def)) => def.key,
        _ => "unknown".to_string(),
    }
}
