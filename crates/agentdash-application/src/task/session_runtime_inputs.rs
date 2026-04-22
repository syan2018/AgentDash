use agentdash_domain::{
    project::Project, session_binding::SessionOwnerType, story::Story, task::Task,
    workspace::Workspace,
};
use agentdash_mcp::injection::McpInjectionConfig;
use agentdash_spi::FlowCapabilities;

use crate::capability::{CapabilityResolver, CapabilityResolverInput};
use crate::platform_config::PlatformConfig;
use crate::repository_set::RepositorySet;
use crate::runtime::{AgentConfig, RuntimeMcpServer, Vfs};
use crate::runtime_bridge::acp_mcp_server_to_runtime;
use crate::task::config::resolve_task_executor_config;
use crate::task::execution::TaskExecutionError;
use crate::vfs::{
    RelayVfsService, ResolveBindingsOutput, SessionMountTarget, build_lifecycle_mount_with_ports,
    resolve_context_bindings,
};
use crate::workflow::{ActiveWorkflowProjection, resolve_active_workflow_projection_for_session};

#[derive(Debug, Clone)]
pub struct TaskSessionRuntimeInputs {
    pub resolved_config: Option<AgentConfig>,
    pub executor_resolution: crate::session::ExecutorResolution,
    pub vfs: Option<Vfs>,
    pub workflow: Option<ActiveWorkflowProjection>,
    /// context_bindings 预解析结果（session 创建时通过 VFS read 解析）
    pub resolved_bindings: Option<ResolveBindingsOutput>,
    pub mcp_servers: Vec<RuntimeMcpServer>,
    pub relay_mcp_server_names: std::collections::HashSet<String>,
    /// CapabilityResolver 产出的内置工具簇 —— turn_context / dispatcher 直接复用,
    /// 避免 turn 路径第二次调用 Resolver。
    pub flow_capabilities: FlowCapabilities,
    /// CapabilityResolver 产出的 effective capability key 集合(供 hook runtime 追踪)。
    pub effective_capability_keys: std::collections::BTreeSet<String>,
    /// CapabilityResolver 产出的 platform MCP 注入配置 —— turn_context 用来
    /// 构造 McpContextContributor,不再自己调 Resolver。
    pub platform_mcp_configs: Vec<McpInjectionConfig>,
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
    let (resolved_config, executor_resolution) =
        match resolve_task_executor_config(explicit_config, task, project) {
            Ok(config) => (
                config,
                crate::session::ExecutorResolution::resolved(executor_source),
            ),
            Err(err) if strict_config_resolution => return Err(err),
            Err(err) => (
                None,
                crate::session::ExecutorResolution::failed(executor_source, err.to_string()),
            ),
        };

    // 通过 task 的 session binding 查找是否有关联 lifecycle run
    let workflow = resolve_workflow_via_task_sessions(repos, task).await?;

    // ── CapabilityResolver 统一计算 MCP server 列表 ──
    // capabilities 来源于 active workflow 的 contract.capability_directives（工作流级基线),
    // step 不再承担能力声明。
    let workflow_capability_directives = workflow.as_ref().and_then(|projection| {
        projection
            .primary_workflow
            .as_ref()
            .map(crate::capability::capability_directives_from_active_workflow)
    });

    let cap_input = CapabilityResolverInput {
        owner_ctx: agentdash_domain::session_binding::SessionOwnerCtx::Task {
            project_id: task.project_id,
            story_id: task.story_id,
            task_id: task.id,
        },
        agent_declared_capabilities: None,
        workflow_ctx: crate::capability::SessionWorkflowContext {
            has_active_workflow: workflow.is_some(),
            workflow_capability_directives,
        },
        agent_mcp_servers: vec![],
        available_presets: build_available_presets(repos, task.project_id).await,
        companion_slice_mode: None,
    };
    let cap_output = CapabilityResolver::resolve(&cap_input, platform_config);
    let mut mcp_servers: Vec<RuntimeMcpServer> = cap_output
        .platform_mcp_configs
        .iter()
        .map(|c| acp_mcp_server_to_runtime(&c.to_acp_mcp_server()))
        .collect();
    mcp_servers.extend(
        cap_output
            .custom_mcp_servers
            .iter()
            .map(acp_mcp_server_to_runtime),
    );
    let relay_mcp_server_names = cap_output
        .custom_relay_mcp_server_names
        .iter()
        .cloned()
        .collect();
    let platform_mcp_configs = cap_output.platform_mcp_configs.clone();
    let flow_capabilities = cap_output.flow_capabilities.clone();
    let effective_capability_keys: std::collections::BTreeSet<String> = cap_output
        .effective_capabilities
        .iter()
        .map(|c| c.key().to_string())
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
            let bindings = wf
                .active_contract()
                .map(|c| c.injection.context_bindings.as_slice())
                .unwrap_or(&[]);
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
        executor_resolution,
        vfs,
        workflow,
        resolved_bindings,
        mcp_servers,
        relay_mcp_server_names,
        flow_capabilities,
        effective_capability_keys,
        platform_mcp_configs,
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

/// 查询 project 下全部 MCP Preset 并展开成 resolver 可消费的 map。
/// 查询失败时返回空 map（降级不中断 session 构造，只落 warn）。
async fn build_available_presets(
    repos: &RepositorySet,
    project_id: uuid::Uuid,
) -> crate::capability::AvailableMcpPresets {
    match repos.mcp_preset_repo.list_by_project(project_id).await {
        Ok(presets) => presets.into_iter().map(|p| (p.key.clone(), p)).collect(),
        Err(error) => {
            tracing::warn!(
                project_id = %project_id,
                error = %error,
                "加载 project MCP Preset 列表失败,mcp:<X> 能力将退化到 inline agent_mcp_servers"
            );
            Default::default()
        }
    }
}
