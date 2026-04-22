use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::{Deserialize, Serialize};

use agentdash_application::vfs::{
    ListOptions, PROVIDER_INLINE_FS, ReadResult, ResolvedMountSummary, ResolvedVfsSurface,
    ResolvedVfsSurfaceSource, ResourceRef, SessionMountTarget, build_project_agent_knowledge_vfs,
    mount_container_id, mount_owner_id, mount_owner_kind, mount_purpose,
};
use agentdash_domain::session_binding::SessionOwnerType;
use agentdash_spi::Vfs;

use crate::{
    app_state::AppState,
    auth::{
        CurrentUser, ProjectPermission, load_project_with_permission,
        load_story_and_project_with_permission, load_task_story_project_with_permission,
    },
    routes::{
        acp_sessions::{ensure_session_permission, pick_primary_session_binding},
        project_agents::resolve_project_workspace,
        project_sessions::build_project_session_context_response,
        story_sessions::build_story_session_context_response,
    },
    rpc::ApiError,
};

const MAX_ENTRIES: usize = 200;

#[derive(Debug, Deserialize)]
pub struct ResolveSurfaceRequest {
    pub source: ResolvedVfsSurfaceSource,
}

#[derive(Debug, Deserialize)]
pub struct SurfaceEntriesQuery {
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub pattern: Option<String>,
    #[serde(default)]
    pub recursive: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct SurfaceMountEntry {
    pub path: String,
    pub entry_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    pub is_dir: bool,
}

#[derive(Debug, Serialize)]
pub struct SurfaceEntriesResponse {
    pub surface_ref: String,
    pub mount_id: String,
    pub entries: Vec<SurfaceMountEntry>,
}

#[derive(Debug, Deserialize)]
pub struct SurfaceReadFileRequest {
    pub surface_ref: String,
    pub mount_id: String,
    pub path: String,
}

#[derive(Debug, Serialize)]
pub struct SurfaceReadFileResponse {
    pub surface_ref: String,
    pub mount_id: String,
    pub path: String,
    pub content: String,
    pub size: u64,
}

#[derive(Debug, Deserialize)]
pub struct SurfaceWriteFileRequest {
    pub surface_ref: String,
    pub mount_id: String,
    pub path: String,
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct SurfaceWriteFileResponse {
    pub surface_ref: String,
    pub mount_id: String,
    pub path: String,
    pub size: u64,
    pub persisted: bool,
}

#[derive(Debug, Deserialize)]
pub struct SurfaceApplyPatchRequest {
    pub surface_ref: String,
    pub mount_id: String,
    pub patch: String,
}

#[derive(Debug, Serialize)]
pub struct SurfaceApplyPatchResponse {
    pub surface_ref: String,
    pub mount_id: String,
    pub added: Vec<String>,
    pub modified: Vec<String>,
    pub deleted: Vec<String>,
}

pub async fn resolve_surface(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Json(req): Json<ResolveSurfaceRequest>,
) -> Result<Json<ResolvedVfsSurface>, ApiError> {
    let surface = resolve_surface_from_source(&state, &current_user, &req.source).await?;
    Ok(Json(surface))
}

pub async fn get_surface(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(surface_ref): Path<String>,
) -> Result<Json<ResolvedVfsSurface>, ApiError> {
    let source =
        ResolvedVfsSurfaceSource::parse_surface_ref(&surface_ref).map_err(ApiError::BadRequest)?;
    let surface = resolve_surface_from_source(&state, &current_user, &source).await?;
    Ok(Json(surface))
}

pub async fn list_surface_mount_entries(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((surface_ref, mount_id)): Path<(String, String)>,
    Query(query): Query<SurfaceEntriesQuery>,
) -> Result<Json<SurfaceEntriesResponse>, ApiError> {
    let source =
        ResolvedVfsSurfaceSource::parse_surface_ref(&surface_ref).map_err(ApiError::BadRequest)?;
    let (_surface, vfs) =
        resolve_surface_bundle(&state, &current_user, &source, ProjectPermission::View).await?;

    check_mount_available(&state, &vfs, &mount_id).await?;

    let listed = state
        .services
        .vfs_service
        .list(
            &vfs,
            &mount_id,
            ListOptions {
                path: query.path.unwrap_or_else(|| ".".to_string()),
                pattern: query.pattern,
                recursive: query.recursive.unwrap_or(false),
            },
            None,
            None,
        )
        .await
        .map_err(ApiError::Internal)?;

    Ok(Json(SurfaceEntriesResponse {
        surface_ref,
        mount_id,
        entries: listed
            .entries
            .into_iter()
            .take(MAX_ENTRIES)
            .map(|entry| SurfaceMountEntry {
                path: entry.path,
                entry_type: if entry.is_dir {
                    "directory".to_string()
                } else {
                    "file".to_string()
                },
                size: entry.size,
                is_dir: entry.is_dir,
            })
            .collect(),
    }))
}

pub async fn read_surface_file(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Json(req): Json<SurfaceReadFileRequest>,
) -> Result<Json<SurfaceReadFileResponse>, ApiError> {
    let source = ResolvedVfsSurfaceSource::parse_surface_ref(&req.surface_ref)
        .map_err(ApiError::BadRequest)?;
    let (_surface, vfs) =
        resolve_surface_bundle(&state, &current_user, &source, ProjectPermission::View).await?;

    check_mount_available(&state, &vfs, &req.mount_id).await?;

    let result: ReadResult = state
        .services
        .vfs_service
        .read_text(
            &vfs,
            &ResourceRef {
                mount_id: req.mount_id.clone(),
                path: req.path.clone(),
            },
            None,
            None,
        )
        .await
        .map_err(ApiError::Internal)?;

    Ok(Json(SurfaceReadFileResponse {
        surface_ref: req.surface_ref,
        mount_id: req.mount_id,
        path: result.path,
        content: result.content.clone(),
        size: result.content.len() as u64,
    }))
}

pub async fn write_surface_file(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Json(req): Json<SurfaceWriteFileRequest>,
) -> Result<Json<SurfaceWriteFileResponse>, ApiError> {
    let source = ResolvedVfsSurfaceSource::parse_surface_ref(&req.surface_ref)
        .map_err(ApiError::BadRequest)?;
    let (_surface, vfs) =
        resolve_surface_bundle(&state, &current_user, &source, ProjectPermission::Edit).await?;

    let mount = vfs
        .mounts
        .iter()
        .find(|mount| mount.id == req.mount_id)
        .ok_or_else(|| ApiError::NotFound(format!("mount 不存在: {}", req.mount_id)))?;

    if !mount.supports(agentdash_spi::MountCapability::Write) {
        return Err(ApiError::BadRequest(format!(
            "挂载点 \"{}\" 没有 write 能力",
            mount.display_name,
        )));
    }

    let normalized_path =
        agentdash_application::vfs::normalize_mount_relative_path(&req.path, false)
            .map_err(ApiError::BadRequest)?;

    if mount.provider == PROVIDER_INLINE_FS {
        let persister =
            agentdash_application::vfs::inline_persistence::DbInlineContentPersister::new(
                state.repos.inline_file_repo.clone(),
            );
        let overlay = agentdash_application::vfs::inline_persistence::InlineContentOverlay::new(
            Arc::new(persister),
        );
        overlay
            .write(mount, &normalized_path, &req.content)
            .await
            .map_err(ApiError::Internal)?;

        return Ok(Json(SurfaceWriteFileResponse {
            surface_ref: req.surface_ref,
            mount_id: req.mount_id,
            path: normalized_path,
            size: req.content.len() as u64,
            persisted: true,
        }));
    }

    check_mount_available(&state, &vfs, &req.mount_id).await?;

    state
        .services
        .vfs_service
        .write_text(
            &vfs,
            &ResourceRef {
                mount_id: req.mount_id.clone(),
                path: req.path.clone(),
            },
            &req.content,
            None,
            None,
        )
        .await
        .map_err(ApiError::Internal)?;

    Ok(Json(SurfaceWriteFileResponse {
        surface_ref: req.surface_ref,
        mount_id: req.mount_id,
        path: normalized_path,
        size: req.content.len() as u64,
        persisted: false,
    }))
}

pub async fn apply_surface_patch(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Json(req): Json<SurfaceApplyPatchRequest>,
) -> Result<Json<SurfaceApplyPatchResponse>, ApiError> {
    let source = ResolvedVfsSurfaceSource::parse_surface_ref(&req.surface_ref)
        .map_err(ApiError::BadRequest)?;
    let (_surface, vfs) =
        resolve_surface_bundle(&state, &current_user, &source, ProjectPermission::Edit).await?;

    let mount = vfs
        .mounts
        .iter()
        .find(|mount| mount.id == req.mount_id)
        .ok_or_else(|| ApiError::NotFound(format!("mount 不存在: {}", req.mount_id)))?;

    if !mount.supports(agentdash_spi::MountCapability::Write) {
        return Err(ApiError::BadRequest(format!(
            "挂载点 \"{}\" 没有 write 能力",
            mount.display_name,
        )));
    }

    if mount.provider == PROVIDER_INLINE_FS {
        let persister =
            agentdash_application::vfs::inline_persistence::DbInlineContentPersister::new(
                state.repos.inline_file_repo.clone(),
            );
        let overlay = agentdash_application::vfs::inline_persistence::InlineContentOverlay::new(
            Arc::new(persister),
        );
        let result = state
            .services
            .vfs_service
            .apply_patch(&vfs, &req.mount_id, &req.patch, Some(&overlay), None)
            .await
            .map_err(ApiError::Internal)?;

        return Ok(Json(SurfaceApplyPatchResponse {
            surface_ref: req.surface_ref,
            mount_id: req.mount_id,
            added: result.added,
            modified: result.modified,
            deleted: result.deleted,
        }));
    }

    check_mount_available(&state, &vfs, &req.mount_id).await?;

    let result = state
        .services
        .vfs_service
        .apply_patch(&vfs, &req.mount_id, &req.patch, None, None)
        .await
        .map_err(ApiError::Internal)?;

    Ok(Json(SurfaceApplyPatchResponse {
        surface_ref: req.surface_ref,
        mount_id: req.mount_id,
        added: result.added,
        modified: result.modified,
        deleted: result.deleted,
    }))
}

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
            state
                .services
                .vfs_service
                .build_vfs(
                    &project,
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
            state
                .services
                .vfs_service
                .build_vfs(
                    &project,
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
            state
                .services
                .vfs_service
                .build_vfs(
                    &project,
                    Some(&story),
                    workspace.as_ref(),
                    SessionMountTarget::Task,
                    None,
                )
                .map_err(|error| ApiError::Internal(format!("构建 VFS 失败: {error}")))?
        }
        ResolvedVfsSurfaceSource::ProjectAgentKnowledge {
            project_id,
            agent_id,
            link_id,
        } => {
            load_project_with_permission(state.as_ref(), current_user, *project_id, permission)
                .await?;
            let link = state
                .repos
                .agent_link_repo
                .find_by_project_and_agent(*project_id, *agent_id)
                .await
                .map_err(|error| ApiError::Internal(error.to_string()))?
                .ok_or_else(|| ApiError::NotFound("该 Agent 未关联到此项目".into()))?;
            if link.id != *link_id {
                return Err(ApiError::Conflict(
                    "surface_ref 中的 link_id 与当前 ProjectAgentLink 不匹配".into(),
                ));
            }
            build_project_agent_knowledge_vfs(&link).map_err(|error| {
                ApiError::Internal(format!("构建 Agent 知识库 VFS 失败: {error}"))
            })?
        }
        ResolvedVfsSurfaceSource::SessionRuntime { session_id } => {
            let bindings =
                ensure_session_permission(state.as_ref(), current_user, session_id, permission)
                    .await?;
            let Some(primary) = pick_primary_session_binding(&bindings) else {
                let empty = Vfs::default();
                let surface = build_surface_summary(state, source, &empty).await?;
                return Ok((surface, empty));
            };
            match primary.owner_type {
                SessionOwnerType::Project => {
                    let project = load_project_with_permission(
                        state.as_ref(),
                        current_user,
                        primary.owner_id,
                        permission,
                    )
                    .await?;
                    build_project_session_context_response(
                        state,
                        &project,
                        session_id,
                        &primary.label,
                    )
                    .await?
                    .vfs
                    .unwrap_or_default()
                }
                SessionOwnerType::Story => {
                    let (story, _) = load_story_and_project_with_permission(
                        state.as_ref(),
                        current_user,
                        primary.owner_id,
                        permission,
                    )
                    .await?;
                    build_story_session_context_response(state, &story, session_id)
                        .await?
                        .and_then(|context| context.vfs)
                        .unwrap_or_default()
                }
                SessionOwnerType::Task => {
                    let task_id = primary.owner_id;
                    let (task, _, _) = load_task_story_project_with_permission(
                        state.as_ref(),
                        current_user,
                        task_id,
                        permission,
                    )
                    .await?;
                    let session_meta = state
                        .services
                        .session_hub
                        .get_session_meta(session_id)
                        .await
                        .map_err(|error| ApiError::Internal(error.to_string()))?;
                    let built_context =
                        agentdash_application::task::context_builder::build_task_session_context(
                            &state.repos,
                            &state.services.vfs_service,
                            &state.config.platform_config,
                            task.id,
                            session_meta.as_ref(),
                        )
                        .await;
                    built_context
                        .and_then(|context| context.vfs)
                        .unwrap_or_default()
                }
            }
        }
    };

    let surface = build_surface_summary(state, source, &vfs).await?;
    Ok((surface, vfs))
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
            if let Ok((owner_kind, owner_id, container_id)) =
                agentdash_application::vfs::parse_inline_mount_owner(mount)
            {
                state
                    .repos
                    .inline_file_repo
                    .count_files(owner_kind, owner_id, &container_id)
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
            root_ref: mount.root_ref.clone(),
            capabilities: mount
                .capabilities
                .iter()
                .map(|capability| format!("{capability:?}").to_lowercase())
                .collect(),
            default_write: mount.default_write,
            purpose: mount_purpose(mount),
            owner_kind: mount_owner_kind(mount),
            owner_id: mount_owner_id(mount),
            container_id: mount_container_id(mount).map(str::to_string),
            backend_online,
            file_count,
        });
    }

    Ok(ResolvedVfsSurface {
        surface_ref: source.surface_ref(),
        source: source.clone(),
        mounts,
        default_mount_id: vfs.default_mount_id.clone(),
    })
}

async fn check_mount_available(
    state: &Arc<AppState>,
    vfs: &Vfs,
    mount_id: &str,
) -> Result<(), ApiError> {
    if let Some(mount) = vfs.mounts.iter().find(|mount| mount.id == mount_id)
        && let Some(provider) = state.services.mount_provider_registry.get(&mount.provider)
        && !provider.is_available(mount).await
    {
        return Err(ApiError::ServiceUnavailable(format!(
            "挂载点 \"{}\" 的 Backend 当前不在线（{}），无法浏览文件。请确认 Backend 已连接。",
            mount.display_name, mount.backend_id,
        )));
    }
    Ok(())
}
