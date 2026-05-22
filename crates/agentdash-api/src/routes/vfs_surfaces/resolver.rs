use std::sync::Arc;

use agentdash_application::vfs::{
    PROVIDER_INLINE_FS, ResolvedMountEditCapabilities, ResolvedMountSummary, ResolvedVfsSurface,
    ResolvedVfsSurfaceSource, SessionMountTarget, build_project_agent_knowledge_vfs,
    build_project_skill_asset_management_mount, build_project_vfs_mount_mount,
    inline_storage_key_from_mount, mount_purpose,
};
use agentdash_spi::Vfs;

use crate::{
    app_state::AppState,
    auth::{
        ProjectPermission, load_project_with_permission, load_story_and_project_with_permission,
        load_task_story_project_with_permission,
    },
    bootstrap::session_context_query::build_session_context_plan,
    routes::{acp_sessions::ensure_session_permission, project_agents::resolve_project_workspace},
    rpc::ApiError,
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
            let workspace = resolve_project_workspace(state, &project).await?;
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
            let workspace = resolve_project_workspace(state, &project).await?;
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
                    .map_err(|error| ApiError::Internal(error.to_string()))?
            } else {
                resolve_project_workspace(state, &project).await?
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
                .map_err(|error| ApiError::Internal(error.to_string()))?
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
                .map_err(|error| ApiError::Internal(error.to_string()))?
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

    let surface = build_surface_summary(state, source, &vfs).await?;
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
        .map_err(|error| ApiError::Internal(error.to_string()))
}

pub(crate) async fn build_surface_summary(
    state: &Arc<AppState>,
    source: &ResolvedVfsSurfaceSource,
    vfs: &Vfs,
) -> Result<ResolvedVfsSurface, ApiError> {
    let mut mounts = Vec::with_capacity(vfs.mounts.len());

    for mount in &vfs.mounts {
        let backend_online = if !mount.backend_id.is_empty() {
            Some(
                state
                    .services
                    .backend_registry
                    .is_online(&mount.backend_id)
                    .await,
            )
        } else {
            None
        };

        let file_count = if mount.provider == PROVIDER_INLINE_FS {
            if let Ok(storage_key) = inline_storage_key_from_mount(mount) {
                state
                    .repos
                    .inline_file_repo
                    .count_files(
                        storage_key.owner_kind,
                        storage_key.owner_id,
                        &storage_key.container_id,
                    )
                    .await
                    .ok()
                    .map(|count| count as usize)
            } else {
                None
            }
        } else {
            None
        };

        mounts.push(ResolvedMountSummary {
            id: mount.id.clone(),
            display_name: mount.display_name.clone(),
            provider: mount.provider.clone(),
            backend_id: mount.backend_id.clone(),
            capabilities: mount
                .capabilities
                .iter()
                .map(|capability| format!("{capability:?}").to_lowercase())
                .collect(),
            default_write: mount.default_write,
            purpose: mount_purpose(mount),
            backend_online,
            file_count,
            edit_capabilities: resolved_edit_capabilities(state, mount),
        });
    }

    Ok(ResolvedVfsSurface {
        surface_ref: source.surface_ref(),
        source: source.clone(),
        mounts,
        default_mount_id: vfs.default_mount_id.clone(),
    })
}

fn resolved_edit_capabilities(
    state: &Arc<AppState>,
    mount: &agentdash_spi::Mount,
) -> ResolvedMountEditCapabilities {
    if mount.provider == PROVIDER_INLINE_FS && mount.supports(agentdash_spi::MountCapability::Write)
    {
        return ResolvedMountEditCapabilities {
            create: true,
            delete: true,
            rename: true,
        };
    }

    state
        .services
        .mount_provider_registry
        .get(&mount.provider)
        .map(|provider| provider.edit_capabilities(mount))
        .map(|capabilities| ResolvedMountEditCapabilities {
            create: capabilities.create,
            delete: capabilities.delete,
            rename: capabilities.rename,
        })
        .unwrap_or_default()
}
