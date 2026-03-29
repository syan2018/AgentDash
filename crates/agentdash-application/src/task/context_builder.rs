use agent_client_protocol::McpServer;
use uuid::Uuid;

use agentdash_domain::common::AddressSpace;
use agentdash_domain::project::Project;
use agentdash_domain::task::Task;
use crate::address_space::RelayAddressSpaceService;
use crate::address_space::mount::SessionMountTarget;
use crate::bootstrap_plan::{
    BootstrapOwnerVariant, BootstrapPlanInput, build_bootstrap_plan, derive_session_context_snapshot,
};
use crate::repository_set::RepositorySet;
use crate::runtime::RuntimeMcpBinding;
use crate::runtime_bridge::{
    acp_mcp_servers_to_runtime, runtime_mcp_servers_to_acp,
};
use crate::session_context::{
    SessionContextSnapshot, extract_story_overrides, normalize_optional_string,
};
use crate::task::config::resolve_task_agent_config;

#[derive(Debug)]
pub struct BuiltTaskSessionContext {
    pub address_space: Option<AddressSpace>,
    pub context_snapshot: Option<SessionContextSnapshot>,
}

/// 为 task session 按需构建结构化上下文快照（address space + context snapshot）。
/// 非关键路径：任何 repo 查询失败都静默降级为 None。
pub async fn build_task_session_context(
    repos: &RepositorySet,
    address_space_service: &RelayAddressSpaceService,
    mcp_base_url: Option<&str>,
    task_id: Uuid,
) -> Option<BuiltTaskSessionContext> {
    let task = repos.task_repo.get_by_id(task_id).await.ok()??;
    let story = repos.story_repo.get_by_id(task.story_id).await.ok()??;
    let project = repos.project_repo.get_by_id(task.project_id).await.ok()??;
    let workspace = if let Some(ws_id) = task.workspace_id {
        repos.workspace_repo.get_by_id(ws_id).await.ok()?
    } else {
        None
    };

    let preset_name = normalize_optional_string(task.agent_binding.preset_name.clone());
    let executor_source = resolve_task_executor_source(&task, &project).to_string();
    let (resolved_config, resolution_error) = match resolve_task_agent_config(&task, &project) {
        Ok(config) => (config, None),
        Err(err) => (None, Some(err.to_string())),
    };
    let effective_agent_type = resolved_config.as_ref().map(|c| c.executor.as_str());
    let use_address_space = resolved_config
        .as_ref()
        .is_some_and(|c| c.is_cloud_native());
    let address_space = if use_address_space {
        address_space_service
            .build_address_space(
                &project,
                Some(&story),
                workspace.as_ref(),
                SessionMountTarget::Task,
                effective_agent_type,
            )
            .ok()
    } else {
        None
    };

    let mcp_servers: Vec<McpServer> = mcp_base_url
        .map(|base_url| {
            runtime_mcp_servers_to_acp(&[RuntimeMcpBinding::for_task(
                base_url.to_string(), task.project_id, task.story_id, task.id,
            )
            .to_runtime_server()])
        })
        .unwrap_or_default();

    let story_overrides = extract_story_overrides(&story);
    let runtime_address_space = address_space.clone();

    let plan = build_bootstrap_plan(BootstrapPlanInput {
        project,
        story: Some(story),
        workspace,
        resolved_config,
        address_space: runtime_address_space,
        mcp_servers: acp_mcp_servers_to_runtime(&mcp_servers),
        working_dir: None,
        workspace_root: None,
        executor_preset_name: preset_name,
        executor_source,
        executor_resolution_error: resolution_error,
        owner_variant: BootstrapOwnerVariant::Task { story_overrides },
        workflow: None,
    });

    let snapshot = derive_session_context_snapshot(&plan);

    Some(BuiltTaskSessionContext {
        address_space: plan.address_space.clone(),
        context_snapshot: Some(snapshot),
    })
}

fn resolve_task_executor_source(task: &Task, project: &Project) -> &'static str {
    if task
        .agent_binding
        .agent_type
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
    {
        return "task.agent_binding.agent_type";
    }
    if task
        .agent_binding
        .preset_name
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
    {
        return "task.agent_binding.preset_name";
    }
    if project
        .config
        .default_agent_type
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
    {
        return "project.config.default_agent_type";
    }
    "unresolved"
}
