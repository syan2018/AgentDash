use agent_client_protocol::McpServer;
use agentdash_domain::session_binding::SessionOwnerCtx;
use uuid::Uuid;

use crate::canvas::append_visible_canvas_mounts;
use crate::capability::{
    CapabilityResolver, CapabilityResolverInput, SessionWorkflowContext,
    capability_directives_from_active_workflow,
};
use crate::platform_config::PlatformConfig;
use crate::repository_set::RepositorySet;
use crate::runtime::Vfs as RuntimeVfs;
use crate::runtime_bridge::acp_mcp_servers_to_runtime;
use crate::session::bootstrap::{
    BootstrapOwnerVariant, BootstrapPlanInput, build_bootstrap_plan,
    derive_session_context_snapshot,
};
use crate::session::context::{
    SessionContextSnapshot, extract_story_overrides, normalize_optional_string,
};
use crate::session::{ExecutorResolution, load_available_presets};
use crate::task::config::{resolve_task_executor_config, resolve_task_executor_source};
use crate::vfs::{RelayVfsService, SessionMountTarget, build_lifecycle_mount_with_ports};
use crate::workflow::{ActiveWorkflowProjection, resolve_active_workflow_projection_for_session};
use agentdash_domain::common::Vfs;
use agentdash_domain::session_binding::SessionOwnerType;

#[derive(Debug)]
pub struct BuiltTaskSessionContext {
    pub vfs: Option<Vfs>,
    pub context_snapshot: Option<SessionContextSnapshot>,
}

/// 为 task session 按需构建结构化上下文快照（VFS + context snapshot）。
///
/// **定位**：只读视图构建器 —— 服务于 `/tasks/{id}/session`、canvases、
/// vfs_surfaces 等查询接口，不负责 session 启动。任何 repo 查询失败
/// 都静默降级为 None。
///
/// M5 之后，task 启动路径（start_task / continue_task 面向 session_hub 下发）
/// 统一走 `TaskLifecycleService::activate_story_step` → `compose_story_step`，
/// 本函数与启动链路无关，仅复用底层相同的 executor / VFS 解析逻辑以保持
/// 上下文数据一致。
pub async fn build_task_session_context(
    repos: &RepositorySet,
    vfs_service: &RelayVfsService,
    platform_config: &PlatformConfig,
    task_id: Uuid,
    session_meta: Option<&crate::session::SessionMeta>,
) -> Option<BuiltTaskSessionContext> {
    let task = crate::task::load_task(repos.story_repo.as_ref(), task_id)
        .await
        .ok()??;
    let story = repos.story_repo.get_by_id(task.story_id).await.ok()??;
    let project = repos.project_repo.get_by_id(task.project_id).await.ok()??;
    let workspace = if let Some(ws_id) = task.workspace_id {
        repos.workspace_repo.get_by_id(ws_id).await.ok()?
    } else {
        None
    };

    // ── 解析 executor config（非 strict：失败时降级为 None）──
    let executor_source = resolve_task_executor_source(&task, &project, None);
    let (resolved_config, executor_resolution) = match resolve_task_executor_config(None, &task, &project) {
        Ok(config) => (config, ExecutorResolution::resolved(executor_source)),
        Err(err) => (None, ExecutorResolution::failed(executor_source, err.to_string())),
    };

    // ── 定位 task 关联的活跃 lifecycle run projection ──
    let workflow = find_active_workflow_via_task_sessions(repos, task.id).await;

    // ── 资源 Capability / MCP 列表 ──
    let workflow_directives = workflow.as_ref().and_then(|p| {
        p.primary_workflow
            .as_ref()
            .map(capability_directives_from_active_workflow)
    });
    let cap_input = CapabilityResolverInput {
        owner_ctx: SessionOwnerCtx::Task {
            project_id: task.project_id,
            story_id: task.story_id,
            task_id: task.id,
        },
        agent_declared_capabilities: None,
        workflow_ctx: SessionWorkflowContext {
            has_active_workflow: workflow.is_some(),
            workflow_capability_directives: workflow_directives,
        },
        agent_mcp_servers: vec![],
        available_presets: load_available_presets(repos, task.project_id).await,
        companion_slice_mode: None,
    };
    let cap_output = CapabilityResolver::resolve(&cap_input, platform_config);
    let mcp_servers: Vec<McpServer> = {
        let mut list: Vec<McpServer> = cap_output
            .platform_mcp_configs
            .iter()
            .map(|c| c.to_acp_mcp_server())
            .collect();
        list.extend(cap_output.custom_mcp_servers.iter().cloned());
        list
    };

    // ── 构建 VFS（cloud-native 场景）──
    let use_vfs = resolved_config
        .as_ref()
        .is_some_and(|config| config.is_cloud_native());
    let effective_agent_type = resolved_config
        .as_ref()
        .map(|config| config.executor.as_str());
    let mut runtime_vfs: Option<RuntimeVfs> = if use_vfs {
        let mut space = vfs_service
            .build_vfs(
                &project,
                Some(&story),
                workspace.as_ref(),
                SessionMountTarget::Task,
                effective_agent_type,
            )
            .ok()?;
        if let Some(active_workflow) = workflow.as_ref() {
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

    let preset_name = normalize_optional_string(task.agent_binding.preset_name.clone());
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

    let story_overrides = extract_story_overrides(&story);
    let plan = build_bootstrap_plan(BootstrapPlanInput {
        project,
        story: Some(story),
        workspace,
        resolved_config,
        vfs: runtime_vfs,
        mcp_servers: acp_mcp_servers_to_runtime(&mcp_servers),
        working_dir: None,
        executor_preset_name: preset_name,
        executor_resolution,
        owner_variant: BootstrapOwnerVariant::Task { story_overrides },
        workflow,
    });

    let snapshot = derive_session_context_snapshot(&plan);

    Some(BuiltTaskSessionContext {
        vfs: plan.vfs.clone(),
        context_snapshot: Some(snapshot),
    })
}

/// 通过 task 的 session binding 反查是否存在活跃 lifecycle run 投影。
///
/// 只读视图辅助函数；失败或缺失均返回 None，绝不抛错。
async fn find_active_workflow_via_task_sessions(
    repos: &RepositorySet,
    task_id: Uuid,
) -> Option<ActiveWorkflowProjection> {
    let bindings = repos
        .session_binding_repo
        .list_by_owner(SessionOwnerType::Task, task_id)
        .await
        .ok()?;

    for binding in &bindings {
        if let Ok(Some(projection)) = resolve_active_workflow_projection_for_session(
            &binding.session_id,
            repos.session_binding_repo.as_ref(),
            repos.workflow_definition_repo.as_ref(),
            repos.lifecycle_definition_repo.as_ref(),
            repos.lifecycle_run_repo.as_ref(),
        )
        .await
        {
            return Some(projection);
        }
    }
    None
}

