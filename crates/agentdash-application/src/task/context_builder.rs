use agentdash_spi::CapabilityScopeCtx;
use uuid::Uuid;

use crate::canvas::append_visible_canvas_mounts;
use crate::capability::{
    CapabilityResolver, CapabilityResolverInput, ContextContributionSource, ContextContributions,
    McpCandidates, ToolContribution, tool_directives_from_active_workflow_projection,
};
use crate::platform_config::PlatformConfig;
use crate::repository_set::RepositorySet;
use crate::runtime::Vfs as RuntimeVfs;
use crate::runtime_bridge::mcp_declarations_to_runtime_servers;
use crate::session::bootstrap::{
    BootstrapOwnerVariant, BootstrapPlanInput, build_bootstrap_plan,
    derive_session_context_snapshot,
};
use crate::session::context::{
    SessionContextSnapshot, extract_story_overrides, normalize_optional_string,
};
use crate::session::{ExecutorResolution, load_available_presets};
use crate::task::config::{resolve_task_executor_config, resolve_task_executor_source};
use crate::vfs::{SessionMountTarget, VfsService};
use crate::workflow::{
    ActiveWorkflowProjection, ensure_active_workflow_lifecycle_mount,
    resolve_active_workflow_projection_for_session,
};
use agentdash_domain::common::Vfs;

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
/// 本函数与启动链路无关，仅复用底层相同的 executor / VFS 解析逻辑以保持
/// Task lifecycle projection 的上下文数据一致。
pub async fn build_task_session_context(
    repos: &RepositorySet,
    vfs_service: &VfsService,
    platform_config: &PlatformConfig,
    task_id: Uuid,
    runtime_session_id: Option<&str>,
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
    let (resolved_config, executor_resolution) =
        match resolve_task_executor_config(None, &task, &project) {
            Ok(config) => (config, ExecutorResolution::resolved(executor_source)),
            Err(err) => (
                None,
                ExecutorResolution::failed(executor_source, err.to_string()),
            ),
        };

    // ── 定位 task 关联的活跃 lifecycle run projection ──
    let workflow = find_active_workflow_via_task_sessions(repos, task.id).await;

    // ── 资源 Capability / MCP 列表 ──
    let workflow_directives = workflow
        .as_ref()
        .map(tool_directives_from_active_workflow_projection);
    let mut contributions = Vec::new();
    if let Some(directives) = workflow_directives {
        contributions.push(ContextContributions {
            source: ContextContributionSource::Workflow,
            tool: Some(ToolContribution {
                directives,
                has_active_workflow: true,
            }),
            companion: None,
        });
    }
    let cap_input = CapabilityResolverInput {
        owner_ctx: CapabilityScopeCtx::Task {
            project_id: task.project_id,
            story_id: task.story_id,
            task_id: task.id,
        },
        contributions,
        mcp_candidates: McpCandidates {
            presets: load_available_presets(repos, task.project_id).await,
            agent_servers: vec![],
        },
        mcp_runtime_context: None,
        capability_context: None,
    };
    let cap_output = CapabilityResolver::resolve(&cap_input, platform_config);
    let mcp_servers: Vec<agentdash_spi::RuntimeMcpServerDeclaration> =
        cap_output.tool.mcp_servers.clone();

    // ── 构建 VFS（cloud-native 场景）──
    let use_vfs = resolved_config
        .as_ref()
        .is_some_and(|config| config.is_cloud_native());
    let effective_agent_type = resolved_config
        .as_ref()
        .map(|config| config.executor.as_str());
    let project_vfs_mounts = repos
        .project_vfs_mount_repo
        .list_by_project(project.id)
        .await
        .ok()?;
    let mut runtime_vfs: Option<RuntimeVfs> = if use_vfs {
        Some(
            vfs_service
                .build_vfs(
                    &project,
                    &project_vfs_mounts,
                    Some(&story),
                    workspace.as_ref(),
                    SessionMountTarget::Task,
                    effective_agent_type,
                )
                .ok()?,
        )
    } else {
        None
    };
    runtime_vfs = ensure_active_workflow_lifecycle_mount(runtime_vfs, workflow.as_ref());

    let preset_name = normalize_optional_string(task.dispatch_preference.preset_name.clone());
    if let Some(space) = runtime_vfs.as_mut() {
        let visible_canvas_mount_ids =
            resolve_visible_canvas_mount_ids(repos, runtime_session_id).await;
        if append_visible_canvas_mounts(
            repos.canvas_repo.as_ref(),
            task.project_id,
            space,
            &visible_canvas_mount_ids,
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
        mcp_servers: mcp_declarations_to_runtime_servers(&mcp_servers),
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

/// 通过 task 关联的 agent → frame 查找活跃 lifecycle workflow projection。
///
/// 链路: LifecycleSubjectAssociation(Task) → LifecycleAgent → AgentFrame
///      → RuntimeSession trace lookup → RuntimeSessionExecutionAnchor → RuntimeNodeState
///
/// 只读视图辅助函数；失败或缺失均返回 None，绝不抛错。
async fn find_active_workflow_via_task_sessions(
    repos: &RepositorySet,
    task_id: Uuid,
) -> Option<ActiveWorkflowProjection> {
    let subject = agentdash_domain::workflow::SubjectRef::new("task", task_id);
    let associations = repos
        .lifecycle_subject_association_repo
        .list_by_subject(&subject)
        .await
        .ok()?;

    for assoc in associations
        .iter()
        .filter(|assoc| assoc.anchor_agent_id.is_some())
    {
        let Some(agent_id) = assoc.anchor_agent_id else {
            continue;
        };
        let agent = repos
            .lifecycle_agent_repo
            .get(agent_id)
            .await
            .ok()
            .flatten();
        let Some(agent) = agent else { continue };
        let Some(_run) = repos
            .lifecycle_run_repo
            .get_by_id(assoc.anchor_run_id)
            .await
            .ok()
            .flatten()
        else {
            continue;
        };
        if agent.current_frame_id.is_none() {
            continue;
        }
        let Some(session_id) = repos
            .execution_anchor_repo
            .latest_for_agent(agent.id)
            .await
            .ok()
            .flatten()
            .map(|anchor| anchor.runtime_session_id)
        else {
            continue;
        };
        if let Ok(Some(projection)) = resolve_active_workflow_projection_for_session(
            &session_id,
            repos.agent_procedure_repo.as_ref(),
            repos.agent_frame_repo.as_ref(),
            repos.lifecycle_agent_repo.as_ref(),
            repos.lifecycle_run_repo.as_ref(),
            repos.execution_anchor_repo.as_ref(),
        )
        .await
        {
            return Some(projection);
        }
    }
    None
}

async fn resolve_visible_canvas_mount_ids(
    repos: &RepositorySet,
    runtime_session_id: Option<&str>,
) -> Vec<String> {
    let Some(session_id) = runtime_session_id else {
        return Vec::new();
    };
    let Ok(Some(anchor)) = repos
        .execution_anchor_repo
        .find_by_session(session_id)
        .await
    else {
        return Vec::new();
    };
    let Ok(Some(agent)) = repos.lifecycle_agent_repo.get(anchor.agent_id).await else {
        return Vec::new();
    };
    if agent.run_id != anchor.run_id {
        return Vec::new();
    }
    match repos.agent_frame_repo.get_current(agent.id).await {
        Ok(Some(frame)) => frame.visible_canvas_mount_ids(),
        _ => match repos.agent_frame_repo.get(anchor.launch_frame_id).await {
            Ok(Some(frame)) => frame.visible_canvas_mount_ids(),
            _ => Vec::new(),
        },
    }
}
