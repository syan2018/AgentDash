use agent_client_protocol::McpServer;
use uuid::Uuid;

use crate::platform_config::PlatformConfig;
use crate::vfs::RelayVfsService;
use crate::canvas::append_visible_canvas_mounts;
use crate::repository_set::RepositorySet;
use crate::runtime_bridge::{acp_mcp_servers_to_runtime, runtime_mcp_servers_to_acp};
use crate::session::bootstrap::{
    BootstrapOwnerVariant, BootstrapPlanInput, build_bootstrap_plan,
    derive_session_context_snapshot,
};
use crate::session::context::{
    SessionContextSnapshot, extract_story_overrides, normalize_optional_string,
};
use crate::task::session_runtime_inputs::build_task_session_runtime_inputs;
use agentdash_domain::common::Vfs;

#[derive(Debug)]
pub struct BuiltTaskSessionContext {
    pub vfs: Option<Vfs>,
    pub context_snapshot: Option<SessionContextSnapshot>,
}

/// 为 task session 按需构建结构化上下文快照（VFS + context snapshot）。
/// 非关键路径：任何 repo 查询失败都静默降级为 None。
pub async fn build_task_session_context(
    repos: &RepositorySet,
    vfs_service: &RelayVfsService,
    platform_config: &PlatformConfig,
    task_id: Uuid,
    session_meta: Option<&crate::session::SessionMeta>,
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
        vfs_service,
        platform_config,
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
    let mut runtime_vfs = session_runtime_inputs.vfs.clone();
    if let Some(space) = runtime_vfs.as_mut() {
        let visible_canvas_mount_ids = session_meta
            .map(|meta| meta.visible_canvas_mount_ids.as_slice())
            .unwrap_or(&[]);
        if append_visible_canvas_mounts(
            repos.canvas_repo.as_ref(),
            task.project_id,
            space,
            visible_canvas_mount_ids,
        )
        .await
        .is_err()
        {
            return None;
        }
    }

    let plan = build_bootstrap_plan(BootstrapPlanInput {
        project,
        story: Some(story),
        workspace,
        resolved_config: session_runtime_inputs.resolved_config,
        vfs: runtime_vfs,
        mcp_servers: acp_mcp_servers_to_runtime(&mcp_servers),
        working_dir: None,
        executor_preset_name: preset_name,
        executor_source: session_runtime_inputs.executor_source,
        executor_resolution_error: session_runtime_inputs.executor_resolution_error,
        owner_variant: BootstrapOwnerVariant::Task { story_overrides },
        workflow: session_runtime_inputs.workflow,
    });

    let snapshot = derive_session_context_snapshot(&plan);

    Some(BuiltTaskSessionContext {
        vfs: plan.vfs.clone(),
        context_snapshot: Some(snapshot),
    })
}
