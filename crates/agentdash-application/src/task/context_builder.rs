use agentdash_spi::CapabilityScopeCtx;
use agentdash_spi::hooks::HookControlTarget;
use uuid::Uuid;

use crate::canvas::append_visible_canvas_mounts;
use crate::capability::{
    AuthorityState, CapabilityResolver, CapabilityResolverInput, ContextContributionSource,
    ContextContributions, McpCandidates, ToolContribution, load_available_presets,
    tool_directives_from_active_workflow_projection,
};
use crate::lifecycle::{
    ActiveWorkflowProjection, project_active_workflow_lifecycle_vfs,
    resolve_active_workflow_projection_for_target, resolve_current_frame_from_delivery_trace_ref,
};
use crate::platform_config::PlatformConfig;
use crate::repository_set::RepositorySet;
use crate::runtime::Vfs as RuntimeVfs;
use crate::runtime_bridge::runtime_mcp_servers_to_summaries;
use crate::session::ExecutorResolution;
use crate::session::bootstrap::{
    BootstrapPlanInput, build_bootstrap_plan, derive_session_context_snapshot,
};
use crate::session::context::{
    SessionContextSnapshot, SessionStoryOverrides, extract_story_overrides,
};
use crate::vfs::{SessionMountTarget, VfsService};
use agentdash_domain::common::Vfs;
use agentdash_domain::story::Story;
use agentdash_domain::workflow::{LifecycleRun, SubjectRef};

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
    let located = crate::task::plan::find_task_plan_item_by_subject(
        repos.lifecycle_run_repo.as_ref(),
        repos.lifecycle_subject_association_repo.as_ref(),
        task_id,
    )
    .await
    .ok()??;
    let task = located.task;
    let run = located.run;
    let project = repos.project_repo.get_by_id(run.project_id).await.ok()??;
    let story = resolve_story_for_task_context(repos, &run, task.story_ref.as_ref()).await;
    let workspace = resolve_task_workspace(repos, &project, story.as_ref()).await?;

    // Task plan facts 不保存 executor 选择；只读 context projection 使用 Project 默认值。
    let resolved_config = project
        .config
        .default_agent_type
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|agent_type| crate::runtime::AgentConfig::new(agent_type.to_string()));
    let executor_resolution = ExecutorResolution::resolved(if resolved_config.is_some() {
        "project.config.default_agent_type"
    } else {
        "unresolved"
    });

    // ── 定位 task 关联的活跃 lifecycle run projection ──
    let workflow = find_active_workflow_for_task_target(repos, task.id).await;

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
            project_id: run.project_id,
            story_id: story.as_ref().map(|story| story.id),
            task_id: task.id,
        },
        contributions,
        mcp_candidates: McpCandidates {
            presets: load_available_presets(repos, run.project_id).await,
        },
        mcp_runtime_context: None,
        capability_context: None,
        authority_state: AuthorityState::main_project_agent(),
    };
    let cap_output = CapabilityResolver::resolve(&cap_input, platform_config);
    let mcp_servers: Vec<agentdash_spi::RuntimeMcpServer> = cap_output.tool.mcp_servers.clone();

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
                    story.as_ref(),
                    workspace.as_ref(),
                    SessionMountTarget::Task,
                    effective_agent_type,
                )
                .ok()?,
        )
    } else {
        None
    };
    runtime_vfs = project_active_workflow_lifecycle_vfs(runtime_vfs, workflow.as_ref());

    if let Some(space) = runtime_vfs.as_mut() {
        let visible_canvas_mount_ids =
            resolve_visible_canvas_mount_ids(repos, runtime_session_id).await;
        if append_visible_canvas_mounts(
            repos.canvas_repo.as_ref(),
            run.project_id,
            space,
            &visible_canvas_mount_ids,
            None,
        )
        .await
        .is_err()
        {
            return None;
        }
    }

    let story_overrides = story
        .as_ref()
        .map(extract_story_overrides)
        .unwrap_or_else(|| SessionStoryOverrides {
            context_containers: Vec::new(),
            disabled_container_ids: Vec::new(),
            session_composition: None,
        });
    let plan = build_bootstrap_plan(BootstrapPlanInput {
        project,
        story,
        workspace_attached: workspace.is_some(),
        resolved_config,
        vfs: runtime_vfs,
        mcp_servers: runtime_mcp_servers_to_summaries(&mcp_servers),
        executor_preset_name: None,
        executor_resolution,
        story_overrides,
    });

    let snapshot = derive_session_context_snapshot(&plan);

    Some(BuiltTaskSessionContext {
        vfs: plan.vfs.clone(),
        context_snapshot: Some(snapshot),
    })
}

async fn resolve_story_for_task_context(
    repos: &RepositorySet,
    run: &LifecycleRun,
    story_ref: Option<&SubjectRef>,
) -> Option<Story> {
    let story_id = if let Some(story_ref) = story_ref {
        (story_ref.kind == "story").then_some(story_ref.id)
    } else {
        repos
            .lifecycle_subject_association_repo
            .list_by_anchor(run.id, None)
            .await
            .ok()?
            .into_iter()
            .find(|assoc| assoc.subject_kind == "story")
            .map(|assoc| assoc.subject_id)
    }?;
    repos
        .story_repo
        .get_by_id(story_id)
        .await
        .ok()
        .flatten()
        .filter(|story| story.project_id == run.project_id)
}

async fn resolve_task_workspace(
    repos: &RepositorySet,
    project: &agentdash_domain::project::Project,
    story: Option<&Story>,
) -> Option<Option<agentdash_domain::workspace::Workspace>> {
    if let Some(workspace_id) = story.and_then(|story| story.default_workspace_id) {
        return repos.workspace_repo.get_by_id(workspace_id).await.ok();
    }
    crate::agent_run::resolve_project_workspace(repos, project)
        .await
        .ok()
}

/// 通过 task 关联的 AgentRun target 查找活跃 lifecycle workflow projection。
///
/// 链路: LifecycleSubjectAssociation(Task) → AgentFrame 最新 revision
///      → HookControlTarget(run/agent/frame) → LifecycleRun.orchestrations[].RuntimeNodeState
///
/// 只读视图辅助函数；失败或缺失均返回 None，绝不抛错。
async fn find_active_workflow_for_task_target(
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
        if agent.run_id != assoc.anchor_run_id {
            continue;
        };
        let Some(frame_id) = repos
            .agent_frame_repo
            .get_current(agent.id)
            .await
            .ok()
            .flatten()
            .map(|frame| frame.id)
        else {
            continue;
        };
        let target = HookControlTarget {
            run_id: assoc.anchor_run_id,
            agent_id: agent.id,
            frame_id,
        };
        if let Ok(Some(projection)) = resolve_active_workflow_projection_for_target(
            &target,
            repos.agent_procedure_repo.as_ref(),
            repos.agent_frame_repo.as_ref(),
            repos.lifecycle_run_repo.as_ref(),
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
    match resolve_current_frame_from_delivery_trace_ref(
        &anchor.runtime_session_id,
        repos.execution_anchor_repo.as_ref(),
        repos.lifecycle_agent_repo.as_ref(),
        repos.agent_frame_repo.as_ref(),
    )
    .await
    {
        Ok(Some((_anchor, _agent, frame))) => frame.visible_canvas_mount_ids(),
        _ => Vec::new(),
    }
}
