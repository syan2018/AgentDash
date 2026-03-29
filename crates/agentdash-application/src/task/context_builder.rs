use agent_client_protocol::McpServer;
use uuid::Uuid;

use crate::address_space::RelayAddressSpaceService;
use crate::bootstrap_plan::{
    BootstrapOwnerVariant, BootstrapPlanInput, build_bootstrap_plan,
    derive_session_context_snapshot,
};
use crate::repository_set::RepositorySet;
use crate::runtime_bridge::{acp_mcp_servers_to_runtime, runtime_mcp_servers_to_acp};
use crate::session_context::{
    SessionContextSnapshot, extract_story_overrides, normalize_optional_string,
};
use crate::task::session_runtime_inputs::build_task_session_runtime_inputs;
use agentdash_domain::common::AddressSpace;

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

    let session_runtime_inputs = build_task_session_runtime_inputs(
        repos,
        address_space_service,
        mcp_base_url,
        &task,
        &story,
        &project,
        workspace.as_ref(),
        None,
        false,
    )
    .await
    .ok()?;

    let preset_name = normalize_optional_string(task.agent_binding.preset_name.clone());
    let mcp_servers: Vec<McpServer> =
        runtime_mcp_servers_to_acp(&session_runtime_inputs.mcp_servers);

    let story_overrides = extract_story_overrides(&story);
    let runtime_address_space = session_runtime_inputs.address_space.clone();

    let plan = build_bootstrap_plan(BootstrapPlanInput {
        project,
        story: Some(story),
        workspace,
        resolved_config: session_runtime_inputs.resolved_config,
        address_space: runtime_address_space,
        mcp_servers: acp_mcp_servers_to_runtime(&mcp_servers),
        working_dir: None,
        workspace_root: None,
        executor_preset_name: preset_name,
        executor_source: session_runtime_inputs.executor_source,
        executor_resolution_error: session_runtime_inputs.executor_resolution_error,
        owner_variant: BootstrapOwnerVariant::Task { story_overrides },
        workflow: session_runtime_inputs.workflow,
    });

    let snapshot = derive_session_context_snapshot(&plan);

    Some(BuiltTaskSessionContext {
        address_space: plan.address_space.clone(),
        context_snapshot: Some(snapshot),
    })
}
