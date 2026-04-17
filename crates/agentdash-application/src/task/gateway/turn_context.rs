use agentdash_domain::common::Vfs;
use agentdash_domain::task::Task;

use crate::vfs::RelayVfsService;
use crate::context::{
    BuiltTaskAgentContext, ContextContributor, ContextContributorRegistry, McpContextContributor,
    StaticFragmentsContributor, TaskAgentBuildInput, TaskExecutionPhase,
    WorkflowContextBindingsContributor, build_declared_source_warning_fragment,
    build_task_agent_context, resolve_workspace_declared_sources,
};
use crate::repository_set::RepositorySet;
use crate::runtime::RuntimeMcpBinding;
use crate::task::execution::{ExecutionPhase, TaskExecutionError};
use crate::task::gateway::repo_ops::{load_related_context, map_internal_error};
use crate::task::session_runtime_inputs::build_task_session_runtime_inputs;
use crate::workspace::BackendAvailability;
use agentdash_domain::common::AgentConfig;

/// 基础设施引用 — prepare_task_turn_context 中不因调用而变化的部分
pub struct TaskTurnServices<'a> {
    pub repos: &'a RepositorySet,
    pub availability: &'a dyn BackendAvailability,
    pub vfs_service: &'a RelayVfsService,
    pub contributor_registry: &'a ContextContributorRegistry,
    pub mcp_base_url: Option<&'a str>,
}

/// 准备好的 turn 上下文 — 包含 dispatch 所需的所有数据
pub struct PreparedTurnContext {
    pub built: BuiltTaskAgentContext,
    pub vfs: Option<Vfs>,
    pub resolved_config: Option<AgentConfig>,
    pub use_cloud_native_agent: bool,
    pub workspace: Option<agentdash_domain::workspace::Workspace>,
    /// 发起本次 task 执行的用户身份（由 HTTP handler 注入）。
    pub identity: Option<agentdash_spi::auth::AuthIdentity>,
    /// Hook effect 回调（cloud-native 路径取代 TurnMonitor）。
    /// Relay 路径暂不使用此字段。
    pub post_turn_handler: Option<crate::session::post_turn_handler::DynPostTurnHandler>,
}

/// 从 Task / Story / Project / Workspace 等上下文中构建 turn 执行所需的完整信息
///
/// 这是 `start_task_turn` 中"准备阶段"的核心逻辑，不涉及实际 dispatch。
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

    let mut extra_contributors: Vec<Box<dyn ContextContributor>> = Vec::new();

    // declared sources resolution
    let mut declared_sources = story.context.source_refs.clone();
    declared_sources.extend(task.agent_binding.context_sources.clone());
    let resolved_workspace_sources = resolve_workspace_declared_sources(
        svc.availability,
        svc.vfs_service,
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
    if let Some(base_url) = svc.mcp_base_url {
        let binding = RuntimeMcpBinding::for_task(
            base_url.to_string(),
            story.project_id,
            task.story_id,
            task.id,
        );
        extra_contributors.push(Box::new(McpContextContributor::new(binding)));
    }

    let session_runtime_inputs = build_task_session_runtime_inputs(
        svc.repos,
        svc.vfs_service,
        svc.mcp_base_url,
        task,
        &story,
        &project,
        workspace.as_ref(),
        connector_config.cloned(),
        true,
    )
    .await?;
    if let (Some(workflow), Some(resolved_bindings)) = (
        session_runtime_inputs.workflow.clone(),
        session_runtime_inputs.resolved_bindings.clone(),
    ) {
        extra_contributors.push(Box::new(WorkflowContextBindingsContributor::new(
            workflow,
            resolved_bindings,
        )));
    }
    let resolved_config = session_runtime_inputs.resolved_config.clone();
    let use_cloud_native_agent = resolved_config
        .as_ref()
        .is_some_and(|config| config.is_cloud_native());
    let vfs = session_runtime_inputs.vfs.clone();

    // build full agent context
    let built = build_task_agent_context(
        TaskAgentBuildInput {
            task,
            story: &story,
            project: &project,
            workspace: workspace.as_ref(),
            vfs: vfs.as_ref(),
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
        svc.contributor_registry,
    )
    .map_err(TaskExecutionError::UnprocessableEntity)?;

    Ok(PreparedTurnContext {
        built,
        vfs,
        resolved_config,
        use_cloud_native_agent,
        workspace,
        identity: None,
        post_turn_handler: None,
    })
}
