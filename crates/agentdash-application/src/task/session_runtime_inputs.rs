use agentdash_domain::{
    project::Project, session_binding::SessionOwnerType, story::Story, task::Task,
    workspace::Workspace,
};

use crate::address_space::{
    RelayAddressSpaceService, ResolveBindingsOutput, SessionMountTarget,
    build_lifecycle_mount_with_ports, resolve_context_bindings,
};
use crate::repository_set::RepositorySet;
use crate::runtime::{AddressSpace, AgentConfig, RuntimeMcpBinding, RuntimeMcpServer};
use crate::task::config::resolve_task_executor_config;
use crate::task::execution::TaskExecutionError;
use crate::workflow::{ActiveWorkflowProjection, resolve_active_workflow_projection_for_session};

#[derive(Debug, Clone)]
pub struct TaskSessionRuntimeInputs {
    pub resolved_config: Option<AgentConfig>,
    pub executor_source: String,
    pub executor_resolution_error: Option<String>,
    pub address_space: Option<AddressSpace>,
    pub workflow: Option<ActiveWorkflowProjection>,
    /// context_bindings 预解析结果（session 创建时通过 VFS read 解析）
    pub resolved_bindings: Option<ResolveBindingsOutput>,
    pub mcp_servers: Vec<RuntimeMcpServer>,
}

pub async fn build_task_session_runtime_inputs(
    repos: &RepositorySet,
    address_space_service: &RelayAddressSpaceService,
    mcp_base_url: Option<&str>,
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

    let mcp_servers = mcp_base_url
        .map(|base_url| {
            vec![
                RuntimeMcpBinding::for_task(base_url, task.project_id, task.story_id, task.id)
                    .to_runtime_server(),
            ]
        })
        .unwrap_or_default();

    let use_address_space = resolved_config
        .as_ref()
        .is_some_and(|config| config.is_cloud_native());
    let effective_agent_type = resolved_config
        .as_ref()
        .map(|config| config.executor.as_str());
    let address_space = if use_address_space {
        let mut space = address_space_service
            .build_address_space(
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

    // 解析 workflow context_bindings（如果有 address_space 和 workflow）
    let resolved_bindings = match (&address_space, &workflow) {
        (Some(space), Some(wf)) => {
            let bindings = &wf.effective_contract.injection.context_bindings;
            if bindings.is_empty() {
                None
            } else {
                Some(
                    resolve_context_bindings(bindings, space, address_space_service)
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
        address_space,
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
