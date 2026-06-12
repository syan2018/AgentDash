use std::sync::Arc;

use agentdash_application::session::construction_planner::resolve_project_workspace;
use agentdash_application::vfs::{
    ResolvedVfsSurface, ResolvedVfsSurfaceSource, SessionMountTarget,
    build_project_agent_knowledge_vfs, build_project_skill_asset_management_mount,
    build_project_vfs_mount_mount,
};
use agentdash_application::workflow::AgentFrameSurfaceExt;
use agentdash_application::workflow::{
    ensure_active_workflow_lifecycle_mount, resolve_active_workflow_projection_for_session,
};
use agentdash_domain::workflow::{AgentFrame, LifecycleAgent, LifecycleRun};
use agentdash_spi::Vfs;

use crate::{
    app_state::AppState,
    auth::{
        ProjectPermission, load_project_with_permission, load_story_and_project_with_permission,
        load_task_story_project_with_permission,
    },
    routes::sessions::ensure_session_permission,
    rpc::ApiError,
    session_construction::resolve_session_frame_vfs,
    vfs_surface_runtime::ApiVfsSurfaceRuntimeProjection,
};

pub(crate) struct AgentRunFrameVfsResolution {
    pub(crate) frame: AgentFrame,
    pub(crate) vfs: Vfs,
}

pub(crate) async fn resolve_surface_from_source(
    state: &Arc<AppState>,
    current_user: &agentdash_integration_api::AuthIdentity,
    source: &ResolvedVfsSurfaceSource,
) -> Result<ResolvedVfsSurface, ApiError> {
    let (surface, _vfs) =
        resolve_surface_bundle(state, current_user, source, ProjectPermission::View).await?;
    Ok(surface)
}

pub(crate) async fn resolve_surface_bundle(
    state: &Arc<AppState>,
    current_user: &agentdash_integration_api::AuthIdentity,
    source: &ResolvedVfsSurfaceSource,
    permission: ProjectPermission,
) -> Result<(ResolvedVfsSurface, Vfs), ApiError> {
    let vfs = match source {
        ResolvedVfsSurfaceSource::ProjectPreview { project_id } => {
            let project =
                load_project_with_permission(state.as_ref(), current_user, *project_id, permission)
                    .await?;
            let workspace = resolve_project_workspace(&state.repos, &project)
                .await
                .map_err(ApiError::Internal)?;
            let project_vfs_mounts = load_project_vfs_mounts(state, project.id).await?;
            state
                .services
                .vfs_service
                .build_vfs(
                    &project,
                    &project_vfs_mounts,
                    None,
                    workspace.as_ref(),
                    SessionMountTarget::Project,
                    None,
                )
                .map_err(|error| ApiError::Internal(format!("构建 VFS 失败: {error}")))?
        }
        ResolvedVfsSurfaceSource::StoryPreview {
            project_id,
            story_id,
        } => {
            let (story, project) = load_story_and_project_with_permission(
                state.as_ref(),
                current_user,
                *story_id,
                permission,
            )
            .await?;
            if project.id != *project_id {
                return Err(ApiError::Conflict(
                    "story_id 与 project_id 不属于同一 Project".into(),
                ));
            }
            let workspace = resolve_project_workspace(&state.repos, &project)
                .await
                .map_err(ApiError::Internal)?;
            let project_vfs_mounts = load_project_vfs_mounts(state, project.id).await?;
            state
                .services
                .vfs_service
                .build_vfs(
                    &project,
                    &project_vfs_mounts,
                    Some(&story),
                    workspace.as_ref(),
                    SessionMountTarget::Story,
                    None,
                )
                .map_err(|error| ApiError::Internal(format!("构建 VFS 失败: {error}")))?
        }
        ResolvedVfsSurfaceSource::TaskPreview {
            project_id,
            task_id,
        } => {
            let (task, story, project) = load_task_story_project_with_permission(
                state.as_ref(),
                current_user,
                *task_id,
                permission,
            )
            .await?;
            if project.id != *project_id {
                return Err(ApiError::Conflict(
                    "task_id 与 project_id 不属于同一 Project".into(),
                ));
            }
            let workspace = if let Some(workspace_id) = task.workspace_id {
                state
                    .repos
                    .workspace_repo
                    .get_by_id(workspace_id)
                    .await
                    .map_err(ApiError::from)?
            } else {
                resolve_project_workspace(&state.repos, &project)
                    .await
                    .map_err(ApiError::Internal)?
            };
            let project_vfs_mounts = load_project_vfs_mounts(state, project.id).await?;
            state
                .services
                .vfs_service
                .build_vfs(
                    &project,
                    &project_vfs_mounts,
                    Some(&story),
                    workspace.as_ref(),
                    SessionMountTarget::Task,
                    None,
                )
                .map_err(|error| ApiError::Internal(format!("构建 VFS 失败: {error}")))?
        }
        ResolvedVfsSurfaceSource::ProjectAgentKnowledge {
            project_id,
            project_agent_id,
        } => {
            load_project_with_permission(state.as_ref(), current_user, *project_id, permission)
                .await?;
            let agent = state
                .repos
                .project_agent_repo
                .get_by_project_and_id(*project_id, *project_agent_id)
                .await
                .map_err(ApiError::from)?
                .ok_or_else(|| ApiError::NotFound("Project Agent 不存在".into()))?;
            build_project_agent_knowledge_vfs(&agent).map_err(|error| {
                ApiError::Internal(format!("构建 Agent 知识库 VFS 失败: {error}"))
            })?
        }
        ResolvedVfsSurfaceSource::ProjectSkillAssets { project_id } => {
            load_project_with_permission(state.as_ref(), current_user, *project_id, permission)
                .await?;
            let service = agentdash_application::skill_asset::SkillAssetService::new(
                state.repos.skill_asset_repo.as_ref(),
            );
            let keys = service
                .list(*project_id)
                .await?
                .into_iter()
                .map(|asset| asset.key)
                .collect::<Vec<_>>();
            Vfs {
                mounts: vec![build_project_skill_asset_management_mount(
                    *project_id,
                    &keys,
                )],
                default_mount_id: Some("skill-assets".to_string()),
                source_project_id: Some(project_id.to_string()),
                source_story_id: None,
                links: Vec::new(),
            }
        }
        ResolvedVfsSurfaceSource::ProjectVfsMount {
            project_id,
            mount_id,
        } => {
            load_project_with_permission(state.as_ref(), current_user, *project_id, permission)
                .await?;
            let mount = state
                .repos
                .project_vfs_mount_repo
                .get_by_project_and_mount_id(*project_id, mount_id)
                .await
                .map_err(ApiError::from)?
                .ok_or_else(|| ApiError::NotFound("Project VFS Mount 不存在".into()))?;
            let runtime_mount = build_project_vfs_mount_mount(&mount).map_err(|error| {
                ApiError::Internal(format!("构建 Project VFS Mount 失败: {error}"))
            })?;
            let default_mount_id = runtime_mount.id.clone();
            Vfs {
                mounts: vec![runtime_mount],
                default_mount_id: Some(default_mount_id),
                source_project_id: Some(project_id.to_string()),
                source_story_id: None,
                links: Vec::new(),
            }
        }
        ResolvedVfsSurfaceSource::SessionRuntime { session_id } => {
            ensure_session_permission(state.as_ref(), current_user, session_id, permission).await?;
            resolve_session_frame_vfs(state, current_user, session_id)
                .await?
                .vfs
                .unwrap_or_default()
        }
        ResolvedVfsSurfaceSource::AgentRun { run_id, agent_id } => {
            resolve_agent_run_frame_vfs(state, current_user, *run_id, *agent_id, permission).await?
        }
    };

    let surface = build_surface_summary(state, source, &vfs).await;
    Ok((surface, vfs))
}

async fn load_project_vfs_mounts(
    state: &Arc<AppState>,
    project_id: uuid::Uuid,
) -> Result<Vec<agentdash_domain::project_vfs_mount::ProjectVfsMount>, ApiError> {
    state
        .repos
        .project_vfs_mount_repo
        .list_by_project(project_id)
        .await
        .map_err(ApiError::from)
}

async fn resolve_agent_run_frame_vfs(
    state: &Arc<AppState>,
    current_user: &agentdash_integration_api::AuthIdentity,
    run_id: uuid::Uuid,
    agent_id: uuid::Uuid,
    permission: ProjectPermission,
) -> Result<Vfs, ApiError> {
    let run = state
        .repos
        .lifecycle_run_repo
        .get_by_id(run_id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::NotFound(format!("lifecycle_run 不存在: {run_id}")))?;
    load_project_with_permission(state.as_ref(), current_user, run.project_id, permission).await?;

    let agent = state
        .repos
        .lifecycle_agent_repo
        .get(agent_id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::NotFound(format!("lifecycle_agent 不存在: {agent_id}")))?;
    if agent.run_id != run.id || agent.project_id != run.project_id {
        return Err(ApiError::Conflict(
            "agent_id 与 run_id 不属于同一 AgentRun".to_string(),
        ));
    }

    resolve_agent_run_frame_vfs_for_agent(state.as_ref(), &run, &agent)
        .await?
        .map(|resolution| resolution.vfs)
        .ok_or_else(|| {
            ApiError::NotFound(format!("lifecycle_agent {} 没有可用 AgentFrame", agent.id))
        })
}

pub(crate) async fn resolve_agent_run_frame_vfs_for_agent(
    state: &AppState,
    run: &LifecycleRun,
    agent: &LifecycleAgent,
) -> Result<Option<AgentRunFrameVfsResolution>, ApiError> {
    let anchor = state
        .repos
        .execution_anchor_repo
        .list_by_run(run.id)
        .await
        .map_err(ApiError::from)?
        .into_iter()
        .filter(|anchor| anchor.agent_id == agent.id)
        .max_by_key(|anchor| anchor.updated_at);
    let anchor_frame_id = anchor.as_ref().map(|anchor| anchor.launch_frame_id);
    let delivery_runtime_session_id = anchor.map(|anchor| anchor.runtime_session_id);
    let current_frame = state
        .repos
        .agent_frame_repo
        .get_current(agent.id)
        .await
        .map_err(ApiError::from)?;
    let frame = match (current_frame, anchor_frame_id) {
        (Some(frame), _) => Some(frame),
        (None, Some(frame_id)) => state
            .repos
            .agent_frame_repo
            .get(frame_id)
            .await
            .map_err(ApiError::from)?,
        (None, None) => None,
    };
    let Some(frame) = frame else {
        return Ok(None);
    };
    let active_workflow = match delivery_runtime_session_id.as_deref() {
        Some(session_id) => resolve_active_workflow_projection_for_session(
            session_id,
            state.repos.agent_procedure_repo.as_ref(),
            state.repos.agent_frame_repo.as_ref(),
            state.repos.lifecycle_agent_repo.as_ref(),
            state.repos.lifecycle_run_repo.as_ref(),
            state.repos.execution_anchor_repo.as_ref(),
        )
        .await
        .map_err(|error| {
            ApiError::Internal(format!(
                "解析 AgentRun active workflow projection 失败: {error}"
            ))
        })?,
        None => None,
    };
    let vfs = ensure_active_workflow_lifecycle_mount(frame.typed_vfs(), active_workflow.as_ref())
        .unwrap_or_default();

    Ok(Some(AgentRunFrameVfsResolution { frame, vfs }))
}

pub(crate) async fn build_surface_summary(
    state: &AppState,
    source: &ResolvedVfsSurfaceSource,
    vfs: &Vfs,
) -> ResolvedVfsSurface {
    let runtime_projection = ApiVfsSurfaceRuntimeProjection::new(
        state.services.backend_registry.clone(),
        state.services.mount_provider_registry.clone(),
    );
    agentdash_application::vfs::build_surface_summary(
        state.repos.inline_file_repo.as_ref(),
        &runtime_projection,
        source,
        vfs,
    )
    .await
}
