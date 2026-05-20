use std::sync::Arc;

use axum::{
    Json,
    body::Bytes,
    extract::Multipart,
    extract::{Path, Query, State},
    http::{HeaderValue, header},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};

use agentdash_application::vfs::{
    ListOptions, PROVIDER_INLINE_FS, ReadResult, ResolvedMountEditCapabilities,
    ResolvedMountSummary, ResolvedVfsSurface, ResolvedVfsSurfaceSource, ResourceRef,
    RuntimeFileEntry, SessionMountTarget, build_project_agent_knowledge_vfs,
    build_project_skill_asset_management_mount, mount_container_id, mount_owner_id,
    mount_owner_kind, mount_purpose,
};
use agentdash_domain::inline_file::{InlineFile, InlineFileContent};
use agentdash_spi::Vfs;

use crate::{
    app_state::AppState,
    auth::{
        CurrentUser, ProjectPermission, load_project_with_permission,
        load_story_and_project_with_permission, load_task_story_project_with_permission,
    },
    bootstrap::session_context_query::build_session_context_plan,
    routes::{acp_sessions::ensure_session_permission, project_agents::resolve_project_workspace},
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
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
    pub content_kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
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
    pub content_kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SurfaceCreateFileRequest {
    pub surface_ref: String,
    pub mount_id: String,
    pub path: String,
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct SurfaceCreateFileResponse {
    pub surface_ref: String,
    pub mount_id: String,
    pub path: String,
    pub size: u64,
    pub content_kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SurfaceDeleteFileRequest {
    pub surface_ref: String,
    pub mount_id: String,
    pub path: String,
}

#[derive(Debug, Serialize)]
pub struct SurfaceDeleteFileResponse {
    pub surface_ref: String,
    pub mount_id: String,
    pub path: String,
    pub deleted: bool,
}

#[derive(Debug, Deserialize)]
pub struct SurfaceRenameFileRequest {
    pub surface_ref: String,
    pub mount_id: String,
    pub from_path: String,
    pub to_path: String,
}

#[derive(Debug, Serialize)]
pub struct SurfaceRenameFileResponse {
    pub surface_ref: String,
    pub mount_id: String,
    pub from_path: String,
    pub to_path: String,
}

#[derive(Debug, Deserialize)]
pub struct SurfaceStatFileRequest {
    pub surface_ref: String,
    pub mount_id: String,
    pub path: String,
}

#[derive(Debug, Serialize)]
pub struct SurfaceStatFileResponse {
    pub surface_ref: String,
    pub mount_id: String,
    pub path: String,
    pub entry_type: String,
    pub size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    pub modified_at: Option<i64>,
    pub is_dir: bool,
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

#[derive(Debug, Deserialize)]
pub struct SurfaceReadBinaryFileRequest {
    pub surface_ref: String,
    pub mount_id: String,
    pub path: String,
}

#[derive(Debug, Serialize)]
pub struct SurfaceUploadBinaryFileResponse {
    pub surface_ref: String,
    pub mount_id: String,
    pub path: String,
    pub size: u64,
    pub content_kind: String,
    pub mime_type: String,
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
                content_kind: entry_content_kind(&entry),
                mime_type: entry_mime_type(&entry),
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
        content_kind: "text".to_string(),
        mime_type: Some("text/plain; charset=utf-8".to_string()),
    }))
}

pub async fn read_surface_file_blob(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Json(req): Json<SurfaceReadBinaryFileRequest>,
) -> Result<Response, ApiError> {
    let source = ResolvedVfsSurfaceSource::parse_surface_ref(&req.surface_ref)
        .map_err(ApiError::BadRequest)?;
    let (_surface, vfs) =
        resolve_surface_bundle(&state, &current_user, &source, ProjectPermission::View).await?;
    let mount = vfs
        .mounts
        .iter()
        .find(|mount| mount.id == req.mount_id)
        .ok_or_else(|| ApiError::NotFound(format!("mount 不存在: {}", req.mount_id)))?;
    if mount.provider != PROVIDER_INLINE_FS {
        return Err(ApiError::BadRequest(
            "blob 读取目前仅支持 inline_fs mount".to_string(),
        ));
    }

    let normalized_path =
        agentdash_application::vfs::normalize_mount_relative_path(&req.path, false)
            .map_err(ApiError::BadRequest)?;
    let (owner_kind, owner_id, container_id) =
        agentdash_application::vfs::parse_inline_mount_owner(mount)
            .map_err(ApiError::BadRequest)?;
    let file = state
        .repos
        .inline_file_repo
        .get_file(owner_kind, owner_id, &container_id, &normalized_path)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("文件不存在: {normalized_path}")))?;
    let InlineFileContent::Binary { bytes, mime_type } = file.content else {
        return Err(ApiError::BadRequest(format!(
            "文件不是二进制内容: {normalized_path}"
        )));
    };
    let content_type = HeaderValue::from_str(&mime_type)
        .map_err(|error| ApiError::Internal(format!("MIME 类型无效: {error}")))?;

    Ok(([(header::CONTENT_TYPE, content_type)], Bytes::from(bytes)).into_response())
}

pub async fn upload_surface_file_blob(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    mut multipart: Multipart,
) -> Result<Json<SurfaceUploadBinaryFileResponse>, ApiError> {
    let mut surface_ref: Option<String> = None;
    let mut mount_id: Option<String> = None;
    let mut target_path: Option<String> = None;
    let mut upload: Option<(String, Vec<u8>, String)> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|error| ApiError::BadRequest(format!("multipart 上传内容解析失败: {error}")))?
    {
        let name = field.name().unwrap_or_default().to_string();
        match name.as_str() {
            "surface_ref" => {
                surface_ref = Some(field.text().await.map_err(|error| {
                    ApiError::BadRequest(format!("读取 surface_ref 失败: {error}"))
                })?);
            }
            "mount_id" => {
                mount_id = Some(field.text().await.map_err(|error| {
                    ApiError::BadRequest(format!("读取 mount_id 失败: {error}"))
                })?);
            }
            "path" => {
                target_path =
                    Some(field.text().await.map_err(|error| {
                        ApiError::BadRequest(format!("读取 path 失败: {error}"))
                    })?);
            }
            "file" => {
                let filename = field.file_name().map(ToString::to_string);
                let content_type = field.content_type().map(ToString::to_string);
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|error| ApiError::BadRequest(format!("读取上传文件失败: {error}")))?
                    .to_vec();
                let filename = filename.unwrap_or_else(|| "image".to_string());
                let mime_type = content_type.unwrap_or_else(|| guess_image_mime_type(&filename));
                upload = Some((filename, bytes, mime_type));
            }
            _ => {}
        }
    }

    let surface_ref =
        surface_ref.ok_or_else(|| ApiError::BadRequest("缺少 surface_ref".to_string()))?;
    let mount_id = mount_id.ok_or_else(|| ApiError::BadRequest("缺少 mount_id".to_string()))?;
    let (filename, bytes, mime_type) =
        upload.ok_or_else(|| ApiError::BadRequest("缺少上传文件".to_string()))?;
    if !mime_type.starts_with("image/") {
        return Err(ApiError::BadRequest(format!(
            "inline_fs 图片上传仅支持 image/* MIME: {mime_type}"
        )));
    }

    let source =
        ResolvedVfsSurfaceSource::parse_surface_ref(&surface_ref).map_err(ApiError::BadRequest)?;
    let (_surface, vfs) =
        resolve_surface_bundle(&state, &current_user, &source, ProjectPermission::Edit).await?;
    let mount = ensure_mount_can_edit(&state, &vfs, &mount_id, "create")?;
    if mount.provider != PROVIDER_INLINE_FS {
        return Err(ApiError::BadRequest(
            "图片上传目前仅支持 inline_fs mount".to_string(),
        ));
    }
    let raw_path = target_path.unwrap_or_else(|| format!("assets/{filename}"));
    let normalized_path =
        agentdash_application::vfs::normalize_mount_relative_path(&raw_path, false)
            .map_err(ApiError::BadRequest)?;
    let (owner_kind, owner_id, container_id) =
        agentdash_application::vfs::parse_inline_mount_owner(mount)
            .map_err(ApiError::BadRequest)?;
    let size = bytes.len() as u64;
    let file = InlineFile::new_binary(
        owner_kind,
        owner_id,
        &container_id,
        &normalized_path,
        bytes,
        mime_type.clone(),
    );
    state.repos.inline_file_repo.upsert_file(&file).await?;

    Ok(Json(SurfaceUploadBinaryFileResponse {
        surface_ref,
        mount_id,
        path: normalized_path,
        size,
        content_kind: "binary".to_string(),
        mime_type,
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
            content_kind: "text".to_string(),
            mime_type: Some("text/plain; charset=utf-8".to_string()),
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
        content_kind: "text".to_string(),
        mime_type: Some("text/plain; charset=utf-8".to_string()),
    }))
}

pub async fn create_surface_file(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Json(req): Json<SurfaceCreateFileRequest>,
) -> Result<Json<SurfaceCreateFileResponse>, ApiError> {
    let source = ResolvedVfsSurfaceSource::parse_surface_ref(&req.surface_ref)
        .map_err(ApiError::BadRequest)?;
    let (_surface, vfs) =
        resolve_surface_bundle(&state, &current_user, &source, ProjectPermission::Edit).await?;
    ensure_mount_can_edit(&state, &vfs, &req.mount_id, "create")?;
    let normalized_path =
        agentdash_application::vfs::normalize_mount_relative_path(&req.path, false)
            .map_err(ApiError::BadRequest)?;
    check_mount_available(&state, &vfs, &req.mount_id).await?;

    state
        .services
        .vfs_service
        .create_text(
            &vfs,
            &ResourceRef {
                mount_id: req.mount_id.clone(),
                path: normalized_path.clone(),
            },
            &req.content,
            None,
            None,
        )
        .await
        .map_err(ApiError::Internal)?;

    Ok(Json(SurfaceCreateFileResponse {
        surface_ref: req.surface_ref,
        mount_id: req.mount_id,
        path: normalized_path,
        size: req.content.len() as u64,
        content_kind: "text".to_string(),
        mime_type: Some("text/plain; charset=utf-8".to_string()),
    }))
}

pub async fn delete_surface_file(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Json(req): Json<SurfaceDeleteFileRequest>,
) -> Result<Json<SurfaceDeleteFileResponse>, ApiError> {
    let source = ResolvedVfsSurfaceSource::parse_surface_ref(&req.surface_ref)
        .map_err(ApiError::BadRequest)?;
    let (_surface, vfs) =
        resolve_surface_bundle(&state, &current_user, &source, ProjectPermission::Edit).await?;
    let mount = ensure_mount_can_edit(&state, &vfs, &req.mount_id, "delete")?;
    let normalized_path =
        agentdash_application::vfs::normalize_mount_relative_path(&req.path, false)
            .map_err(ApiError::BadRequest)?;
    ensure_not_skill_asset_document_path(&source, &normalized_path, "删除")?;
    if mount.provider == PROVIDER_INLINE_FS {
        let (owner_kind, owner_id, container_id) =
            agentdash_application::vfs::parse_inline_mount_owner(mount)
                .map_err(ApiError::BadRequest)?;
        state
            .repos
            .inline_file_repo
            .delete_file(owner_kind, owner_id, &container_id, &normalized_path)
            .await?;
        return Ok(Json(SurfaceDeleteFileResponse {
            surface_ref: req.surface_ref,
            mount_id: req.mount_id,
            path: normalized_path,
            deleted: true,
        }));
    }

    let overlay = inline_overlay_for_mount(&state, &vfs, &req.mount_id);
    check_mount_available(&state, &vfs, &req.mount_id).await?;

    state
        .services
        .vfs_service
        .delete_text(
            &vfs,
            &ResourceRef {
                mount_id: req.mount_id.clone(),
                path: normalized_path.clone(),
            },
            overlay.as_ref(),
            None,
        )
        .await
        .map_err(ApiError::Internal)?;

    Ok(Json(SurfaceDeleteFileResponse {
        surface_ref: req.surface_ref,
        mount_id: req.mount_id,
        path: normalized_path,
        deleted: true,
    }))
}

pub async fn rename_surface_file(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Json(req): Json<SurfaceRenameFileRequest>,
) -> Result<Json<SurfaceRenameFileResponse>, ApiError> {
    let source = ResolvedVfsSurfaceSource::parse_surface_ref(&req.surface_ref)
        .map_err(ApiError::BadRequest)?;
    let (_surface, vfs) =
        resolve_surface_bundle(&state, &current_user, &source, ProjectPermission::Edit).await?;
    let mount = ensure_mount_can_edit(&state, &vfs, &req.mount_id, "rename")?;
    let from_path =
        agentdash_application::vfs::normalize_mount_relative_path(&req.from_path, false)
            .map_err(ApiError::BadRequest)?;
    let to_path = agentdash_application::vfs::normalize_mount_relative_path(&req.to_path, false)
        .map_err(ApiError::BadRequest)?;
    if from_path == to_path {
        return Ok(Json(SurfaceRenameFileResponse {
            surface_ref: req.surface_ref,
            mount_id: req.mount_id,
            from_path,
            to_path,
        }));
    }
    ensure_not_skill_asset_document_path(&source, &from_path, "重命名")?;
    ensure_not_skill_asset_document_path(&source, &to_path, "重命名为")?;
    if mount.provider == PROVIDER_INLINE_FS {
        let (owner_kind, owner_id, container_id) =
            agentdash_application::vfs::parse_inline_mount_owner(mount)
                .map_err(ApiError::BadRequest)?;
        let mut file = state
            .repos
            .inline_file_repo
            .get_file(owner_kind, owner_id, &container_id, &from_path)
            .await?
            .ok_or_else(|| ApiError::NotFound(format!("文件不存在: {from_path}")))?;
        file.path = to_path.clone();
        state.repos.inline_file_repo.upsert_file(&file).await?;
        state
            .repos
            .inline_file_repo
            .delete_file(owner_kind, owner_id, &container_id, &from_path)
            .await?;
        return Ok(Json(SurfaceRenameFileResponse {
            surface_ref: req.surface_ref,
            mount_id: req.mount_id,
            from_path,
            to_path,
        }));
    }

    let overlay = inline_overlay_for_mount(&state, &vfs, &req.mount_id);
    check_mount_available(&state, &vfs, &req.mount_id).await?;

    state
        .services
        .vfs_service
        .rename_text(
            &vfs,
            &req.mount_id,
            &from_path,
            &to_path,
            overlay.as_ref(),
            None,
        )
        .await
        .map_err(ApiError::Internal)?;

    Ok(Json(SurfaceRenameFileResponse {
        surface_ref: req.surface_ref,
        mount_id: req.mount_id,
        from_path,
        to_path,
    }))
}

pub async fn stat_surface_file(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Json(req): Json<SurfaceStatFileRequest>,
) -> Result<Json<SurfaceStatFileResponse>, ApiError> {
    let source = ResolvedVfsSurfaceSource::parse_surface_ref(&req.surface_ref)
        .map_err(ApiError::BadRequest)?;
    let (_surface, vfs) =
        resolve_surface_bundle(&state, &current_user, &source, ProjectPermission::View).await?;
    let overlay = inline_overlay_for_mount(&state, &vfs, &req.mount_id);
    check_mount_available(&state, &vfs, &req.mount_id).await?;

    let entry = state
        .services
        .vfs_service
        .stat(
            &vfs,
            &ResourceRef {
                mount_id: req.mount_id.clone(),
                path: req.path,
            },
            overlay.as_ref(),
            None,
        )
        .await
        .map_err(ApiError::Internal)?;

    Ok(Json(surface_stat_response(
        req.surface_ref,
        req.mount_id,
        entry,
    )))
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

    let mount = ensure_mount_can_write(&vfs, &req.mount_id)?;

    if mount.provider == PROVIDER_INLINE_FS {
        let overlay = db_inline_overlay(&state);
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

fn ensure_mount_can_write<'a>(
    vfs: &'a Vfs,
    mount_id: &str,
) -> Result<&'a agentdash_spi::Mount, ApiError> {
    let mount = vfs
        .mounts
        .iter()
        .find(|mount| mount.id == mount_id)
        .ok_or_else(|| ApiError::NotFound(format!("mount 不存在: {mount_id}")))?;

    if !mount.supports(agentdash_spi::MountCapability::Write) {
        return Err(ApiError::BadRequest(format!(
            "挂载点 \"{}\" 没有 write 能力",
            mount.display_name,
        )));
    }
    Ok(mount)
}

fn ensure_mount_can_edit<'a>(
    state: &Arc<AppState>,
    vfs: &'a Vfs,
    mount_id: &str,
    operation: &str,
) -> Result<&'a agentdash_spi::Mount, ApiError> {
    let mount = ensure_mount_can_write(vfs, mount_id)?;
    let capabilities = resolved_edit_capabilities(state, mount);
    let supported = match operation {
        "create" => capabilities.create,
        "delete" => capabilities.delete,
        "rename" => capabilities.rename,
        _ => false,
    };
    if !supported {
        return Err(ApiError::BadRequest(format!(
            "挂载点 \"{}\" 不支持 {operation} 操作",
            mount.display_name,
        )));
    }
    Ok(mount)
}

fn ensure_not_skill_asset_document_path(
    source: &ResolvedVfsSurfaceSource,
    path: &str,
    operation: &str,
) -> Result<(), ApiError> {
    if matches!(source, ResolvedVfsSurfaceSource::ProjectSkillAssets { .. })
        && skill_asset_relative_path(path) == Some("SKILL.md")
    {
        return Err(ApiError::BadRequest(format!(
            "不能通过 VFS {operation} Skill 主文档 SKILL.md"
        )));
    }
    Ok(())
}

fn skill_asset_relative_path(path: &str) -> Option<&str> {
    let rest = path.strip_prefix("skills/")?;
    let (_key, relative_path) = rest.split_once('/')?;
    Some(relative_path)
}

fn db_inline_overlay(
    state: &Arc<AppState>,
) -> agentdash_application::vfs::inline_persistence::InlineContentOverlay {
    let persister = agentdash_application::vfs::inline_persistence::DbInlineContentPersister::new(
        state.repos.inline_file_repo.clone(),
    );
    agentdash_application::vfs::inline_persistence::InlineContentOverlay::new(Arc::new(persister))
}

fn inline_overlay_for_mount(
    state: &Arc<AppState>,
    vfs: &Vfs,
    mount_id: &str,
) -> Option<agentdash_application::vfs::inline_persistence::InlineContentOverlay> {
    vfs.mounts
        .iter()
        .any(|mount| mount.id == mount_id && mount.provider == PROVIDER_INLINE_FS)
        .then(|| db_inline_overlay(state))
}

fn surface_stat_response(
    surface_ref: String,
    mount_id: String,
    entry: RuntimeFileEntry,
) -> SurfaceStatFileResponse {
    SurfaceStatFileResponse {
        surface_ref,
        mount_id,
        content_kind: entry_content_kind(&entry),
        mime_type: entry_mime_type(&entry),
        path: entry.path,
        entry_type: if entry.is_dir {
            "directory".to_string()
        } else {
            "file".to_string()
        },
        size: entry.size,
        modified_at: entry.modified_at,
        is_dir: entry.is_dir,
    }
}

fn entry_content_kind(entry: &RuntimeFileEntry) -> Option<String> {
    entry
        .attributes
        .as_ref()
        .and_then(|attrs| attrs.get("content_kind"))
        .and_then(|value| value.as_str())
        .map(ToString::to_string)
}

fn entry_mime_type(entry: &RuntimeFileEntry) -> Option<String> {
    entry
        .attributes
        .as_ref()
        .and_then(|attrs| attrs.get("mime_type"))
        .and_then(|value| value.as_str())
        .map(ToString::to_string)
}

fn guess_image_mime_type(path: &str) -> String {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".png") {
        "image/png".to_string()
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg".to_string()
    } else if lower.ends_with(".webp") {
        "image/webp".to_string()
    } else if lower.ends_with(".gif") {
        "image/gif".to_string()
    } else if lower.ends_with(".svg") {
        "image/svg+xml".to_string()
    } else {
        "application/octet-stream".to_string()
    }
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
