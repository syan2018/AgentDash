use agentdash_domain::{
    project::Project, session_binding::SessionOwnerType, story::Story, task::Task,
    workspace::Workspace,
};

use crate::capability::{CapabilityResolver, CapabilityResolverInput};
use crate::platform_config::PlatformConfig;
use crate::vfs::{
    RelayVfsService, ResolveBindingsOutput, SessionMountTarget,
    build_lifecycle_mount_with_ports, resolve_context_bindings,
};
use crate::repository_set::RepositorySet;
use crate::runtime::{Vfs, AgentConfig, RuntimeMcpServer};
use crate::runtime_bridge::acp_mcp_server_to_runtime;
use crate::task::config::resolve_task_executor_config;
use crate::task::execution::TaskExecutionError;
use crate::workflow::{ActiveWorkflowProjection, resolve_active_workflow_projection_for_session};

#[derive(Debug, Clone)]
pub struct TaskSessionRuntimeInputs {
    pub resolved_config: Option<AgentConfig>,
    pub executor_source: String,
    pub executor_resolution_error: Option<String>,
    pub vfs: Option<Vfs>,
    pub workflow: Option<ActiveWorkflowProjection>,
    /// context_bindings 预解析结果（session 创建时通过 VFS read 解析）
    pub resolved_bindings: Option<ResolveBindingsOutput>,
    pub mcp_servers: Vec<RuntimeMcpServer>,
}

pub async fn build_task_session_runtime_inputs(
    repos: &RepositorySet,
    vfs_service: &RelayVfsService,
    platform_config: &PlatformConfig,
    task: &Task,
    story: &Story,
    project: &Project,
    workspace: Option<&Workspace>,
    explicit_config: Option<AgentConfig>,
    strict_config_resolution: bool,
) -> Result<TaskSessionRuntimeInputs, TaskExecutionError> {
    let executor_source = resolve_task_executor_source(task, project, explicit_config.as_ref());
    let (resolved_config, executor_resolution_error) =
        match resolve_task_executor_config(explicit_config, task, project) {
            Ok(config) => (config, None),
            Err(err) if strict_config_resolution => return Err(err),
            Err(err) => (None, Some(err.to_string())),
        };

    // 通过 task 的 session binding 查找是否有关联 lifecycle run
    let workflow = resolve_workflow_via_task_sessions(repos, task).await?;

    // ── CapabilityResolver 统一计算 MCP server 列表 ──
    let workflow_capabilities = workflow
        .as_ref()
        .map(|projection| crate::capability::capabilities_from_active_step(&projection.active_step));

    let cap_input = CapabilityResolverInput {
        owner_type: SessionOwnerType::Task,
        project_id: task.project_id,
        story_id: Some(task.story_id),
        task_id: Some(task.id),
        agent_declared_capabilities: None,
        workflow_ctx: crate::capability::SessionWorkflowContext {
            has_active_workflow: workflow.is_some(),
            workflow_capabilities,
        },
        agent_mcp_servers: vec![],
        companion_slice_mode: None,
    };
    let cap_output = CapabilityResolver::resolve(&cap_input, platform_config);
    let mcp_servers: Vec<RuntimeMcpServer> = cap_output
        .platform_mcp_configs
        .iter()
        .map(|c| acp_mcp_server_to_runtime(&c.to_acp_mcp_server()))
        .collect();

    let use_vfs = resolved_config
        .as_ref()
        .is_some_and(|config| config.is_cloud_native());
    let effective_agent_type = resolved_config
        .as_ref()
        .map(|config| config.executor.as_str());
    let vfs = if use_vfs {
        let mut space = vfs_service
            .build_vfs(
                project,
                Some(story),
                workspace,
                SessionMountTarget::Task,
                effective_agent_type,
            )
            .map_err(|error| TaskExecutionError::Internal(error.to_string()))?;

        if let Some(active_workflow) = workflow.as_ref() {
            // port 归属已迁移到 step 级别
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

    // 解析 workflow context_bindings（如果有 vfs 和 workflow）
    let resolved_bindings = match (&vfs, &workflow) {
        (Some(space), Some(wf)) => {
            let bindings = &wf.effective_contract.injection.context_bindings;
            if bindings.is_empty() {
                None
            } else {
                Some(
                    resolve_context_bindings(bindings, space, vfs_service)
                        .await
                        .map_err(TaskExecutionError::UnprocessableEntity)?,
                )
            }
        }
        _ => None,
    };

    Ok(TaskSessionRuntimeInputs {
        resolved_config,
        executor_source,
        executor_resolution_error,
        vfs,
        workflow,
        resolved_bindings,
        mcp_servers,
    })
}

pub fn resolve_task_executor_source(
    task: &Task,
    project: &Project,
    explicit_config: Option<&AgentConfig>,
) -> String {
    if explicit_config.is_some() {
        return "explicit.executor_config".to_string();
    }
    if task
        .agent_binding
        .agent_type
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
    {
        return "task.agent_binding.agent_type".to_string();
    }
    if task
        .agent_binding
        .preset_name
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
    {
        return "task.agent_binding.preset_name".to_string();
    }
    if project
        .config
        .default_agent_type
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
    {
        return "project.config.default_agent_type".to_string();
    }
    "unresolved".to_string()
}

/// 通过 task 的 session binding 查找是否有 session 关联了活跃的 lifecycle run。
async fn resolve_workflow_via_task_sessions(
    repos: &RepositorySet,
    task: &Task,
) -> Result<Option<ActiveWorkflowProjection>, TaskExecutionError> {
    let bindings = repos
        .session_binding_repo
        .list_by_owner(SessionOwnerType::Task, task.id)
        .await
        .map_err(|e| TaskExecutionError::Internal(e.to_string()))?;

    for binding in &bindings {
        if let Some(projection) = resolve_active_workflow_projection_for_session(
            &binding.session_id,
            repos.session_binding_repo.as_ref(),
            repos.workflow_definition_repo.as_ref(),
            repos.lifecycle_definition_repo.as_ref(),
            repos.lifecycle_run_repo.as_ref(),
        )
        .await
        .map_err(TaskExecutionError::Internal)?
        {
            return Ok(Some(projection));
        }
    }
    Ok(None)
}
