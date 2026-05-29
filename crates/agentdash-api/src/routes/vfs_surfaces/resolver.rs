use std::sync::Arc;

use agentdash_application::session::construction_planner::SessionConstructionPlanner;
use agentdash_application::vfs::{
    ResolvedVfsSurface, ResolvedVfsSurfaceSource, SessionMountTarget,
    build_project_agent_knowledge_vfs, build_project_skill_asset_management_mount,
    build_project_vfs_mount_mount,
};
use agentdash_spi::Vfs;

use crate::{
    app_state::AppState,
    auth::{
        ProjectPermission, load_project_with_permission, load_story_and_project_with_permission,
        load_task_story_project_with_permission,
    },
    routes::acp_sessions::ensure_session_permission,
    rpc::ApiError,
    session_use_cases::context_query::build_session_context_plan,
    vfs_surface_runtime::ApiVfsSurfaceRuntimeProjection,
};

pub(crate) async fn resolve_surface_from_source(
    state: &Arc<AppState>,
    current_user: &agentdash_plugin_api::AuthIdentity,
    source: &ResolvedVfsSurfaceSource,
) -> Result<ResolvedVfsSurface, ApiError> {
    let (surface, _vfs) =
        resolve_surface_bundle(state, current_user, source, ProjectPermission::View).await?;
    Ok(surface)
}

pub(crate) async fn resolve_surface_bundle(
    state: &Arc<AppState>,
    current_user: &agentdash_plugin_api::AuthIdentity,
    source: &ResolvedVfsSurfaceSource,
    permission: ProjectPermission,
) -> Result<(ResolvedVfsSurface, Vfs), ApiError> {
    let vfs = match source {
        ResolvedVfsSurfaceSource::ProjectPreview { project_id } => {
            let project =
                load_project_with_permission(state.as_ref(), current_user, *project_id, permission)
                    .await?;
            let workspace =
                SessionConstructionPlanner::resolve_project_workspace(&state.repos, &project)
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
            let workspace =
                SessionConstructionPlanner::resolve_project_workspace(&state.repos, &project)
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
                SessionConstructionPlanner::resolve_project_workspace(&state.repos, &project)
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
            let bindings =
                ensure_session_permission(state.as_ref(), current_user, session_id, permission)
                    .await?;
            build_session_context_plan(state, current_user, session_id, &bindings)
                .await?
                .and_then(|plan| plan.context_projection.vfs)
                .unwrap_or_default()
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

pub(crate) async fn build_surface_summary(
    state: &Arc<AppState>,
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
