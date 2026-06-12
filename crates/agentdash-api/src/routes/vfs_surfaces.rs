use std::sync::Arc;

use axum::{
    Json,
    body::Bytes,
    extract::Multipart,
    extract::{Path, Query, State},
    http::{HeaderValue, header},
    response::{IntoResponse, Response},
};

use agentdash_application::vfs::{ListOptions, ReadResult, ResourceRef};

use crate::{
    app_state::AppState,
    auth::{CurrentUser, ProjectPermission},
    rpc::ApiError,
};

pub(crate) mod dto;
mod helpers;
pub(crate) mod resolver;

pub use dto::*;
use helpers::{
    api_error_from_vfs_mutation, check_mount_available, ensure_not_skill_asset_document_path,
    entry_content_kind, entry_mime_type, guess_image_mime_type, surface_stat_response,
};
use resolver::{resolve_surface_bundle, resolve_surface_from_source};

const MAX_ENTRIES: usize = 200;
const VFS_BINARY_UPLOAD_BODY_LIMIT_BYTES: usize = 80 * 1024 * 1024;

pub fn router() -> axum::Router<std::sync::Arc<crate::app_state::AppState>> {
    axum::Router::new()
        .route(
            "/vfs-surfaces/resolve",
            axum::routing::post(resolve_surface),
        )
        .route(
            "/vfs-surfaces/{surface_ref}",
            axum::routing::get(get_surface),
        )
        .route(
            "/vfs-surfaces/{surface_ref}/mounts/{mount_id}/entries",
            axum::routing::get(list_surface_mount_entries),
        )
        .route(
            "/vfs-surfaces/read-file",
            axum::routing::post(read_surface_file),
        )
        .route(
            "/vfs-surfaces/read-file-blob",
            axum::routing::post(read_surface_file_blob),
        )
        .route(
            "/vfs-surfaces/upload-file-blob",
            axum::routing::post(upload_surface_file_blob).layer(
                axum::extract::DefaultBodyLimit::max(VFS_BINARY_UPLOAD_BODY_LIMIT_BYTES),
            ),
        )
        .route(
            "/vfs-surfaces/write-file",
            axum::routing::post(write_surface_file),
        )
        .route(
            "/vfs-surfaces/create-file",
            axum::routing::post(create_surface_file),
        )
        .route(
            "/vfs-surfaces/delete-file",
            axum::routing::post(delete_surface_file),
        )
        .route(
            "/vfs-surfaces/rename-file",
            axum::routing::post(rename_surface_file),
        )
        .route(
            "/vfs-surfaces/stat-file",
            axum::routing::post(stat_surface_file),
        )
        .route(
            "/vfs-surfaces/apply-patch",
            axum::routing::post(apply_surface_patch),
        )
}

pub async fn resolve_surface(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Json(req): Json<ResolveSurfaceRequest>,
) -> Result<Json<ResolvedVfsSurface>, ApiError> {
    let source = dto::surface_source_to_application(&req.source).map_err(ApiError::BadRequest)?;
    let surface = resolve_surface_from_source(&state, &current_user, &source).await?;
    Ok(Json(dto::surface_from_application(surface)))
}

pub async fn get_surface(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(surface_ref): Path<String>,
) -> Result<Json<ResolvedVfsSurface>, ApiError> {
    let source =
        agentdash_application::vfs::ResolvedVfsSurfaceSource::parse_surface_ref(&surface_ref)
            .map_err(ApiError::BadRequest)?;
    let surface = resolve_surface_from_source(&state, &current_user, &source).await?;
    Ok(Json(dto::surface_from_application(surface)))
}

pub async fn list_surface_mount_entries(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((surface_ref, mount_id)): Path<(String, String)>,
    Query(query): Query<SurfaceEntriesQuery>,
) -> Result<Json<SurfaceEntriesResponse>, ApiError> {
    let source =
        agentdash_application::vfs::ResolvedVfsSurfaceSource::parse_surface_ref(&surface_ref)
            .map_err(ApiError::BadRequest)?;
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
            Some(&current_user),
        )
        .await
        .map_err(|e| ApiError::Internal(format!("VFS surface 条目检索失败: {e}")))?;

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
    let source =
        agentdash_application::vfs::ResolvedVfsSurfaceSource::parse_surface_ref(&req.surface_ref)
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
            Some(&current_user),
        )
        .await
        .map_err(|e| ApiError::Internal(format!("VFS surface 文件读取失败: {e}")))?;

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
    let source =
        agentdash_application::vfs::ResolvedVfsSurfaceSource::parse_surface_ref(&req.surface_ref)
            .map_err(ApiError::BadRequest)?;
    let (_surface, vfs) =
        resolve_surface_bundle(&state, &current_user, &source, ProjectPermission::View).await?;
    check_mount_available(&state, &vfs, &req.mount_id).await?;

    let result = state
        .services
        .vfs_service
        .read_binary(
            &vfs,
            &ResourceRef {
                mount_id: req.mount_id.clone(),
                path: req.path.clone(),
            },
            None,
            Some(&current_user),
        )
        .await
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;
    let content_type = HeaderValue::from_str(&result.mime_type)
        .map_err(|error| ApiError::Internal(format!("MIME 类型无效: {error}")))?;

    Ok((
        [(header::CONTENT_TYPE, content_type)],
        Bytes::from(result.data),
    )
        .into_response())
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
        agentdash_application::vfs::ResolvedVfsSurfaceSource::parse_surface_ref(&surface_ref)
            .map_err(ApiError::BadRequest)?;
    let (_surface, vfs) =
        resolve_surface_bundle(&state, &current_user, &source, ProjectPermission::Edit).await?;
    let raw_path = target_path.unwrap_or_else(|| format!("assets/{filename}"));
    let result = state
        .services
        .vfs_mutation_dispatcher
        .upload_inline_binary(
            &vfs,
            ResourceRef {
                mount_id: mount_id.clone(),
                path: raw_path,
            },
            bytes,
            mime_type,
            Some(&current_user),
        )
        .await
        .map_err(api_error_from_vfs_mutation)?;

    Ok(Json(SurfaceUploadBinaryFileResponse {
        surface_ref,
        mount_id,
        path: result.path,
        size: result.size,
        content_kind: result.content_kind,
        mime_type: result.mime_type,
    }))
}

pub async fn write_surface_file(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Json(req): Json<SurfaceWriteFileRequest>,
) -> Result<Json<SurfaceWriteFileResponse>, ApiError> {
    let source =
        agentdash_application::vfs::ResolvedVfsSurfaceSource::parse_surface_ref(&req.surface_ref)
            .map_err(ApiError::BadRequest)?;
    let (_surface, vfs) =
        resolve_surface_bundle(&state, &current_user, &source, ProjectPermission::Edit).await?;

    check_mount_available(&state, &vfs, &req.mount_id).await?;
    let result = state
        .services
        .vfs_mutation_dispatcher
        .write_text(
            &vfs,
            ResourceRef {
                mount_id: req.mount_id.clone(),
                path: req.path.clone(),
            },
            &req.content,
            Some(&current_user),
        )
        .await
        .map_err(api_error_from_vfs_mutation)?;

    Ok(Json(SurfaceWriteFileResponse {
        surface_ref: req.surface_ref,
        mount_id: req.mount_id,
        path: result.path,
        size: result.size,
        persisted: result.persisted,
        content_kind: result.content_kind,
        mime_type: result.mime_type,
    }))
}

pub async fn create_surface_file(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Json(req): Json<SurfaceCreateFileRequest>,
) -> Result<Json<SurfaceCreateFileResponse>, ApiError> {
    let source =
        agentdash_application::vfs::ResolvedVfsSurfaceSource::parse_surface_ref(&req.surface_ref)
            .map_err(ApiError::BadRequest)?;
    let (_surface, vfs) =
        resolve_surface_bundle(&state, &current_user, &source, ProjectPermission::Edit).await?;
    check_mount_available(&state, &vfs, &req.mount_id).await?;
    let result = state
        .services
        .vfs_mutation_dispatcher
        .create_text(
            &vfs,
            ResourceRef {
                mount_id: req.mount_id.clone(),
                path: req.path.clone(),
            },
            &req.content,
            Some(&current_user),
        )
        .await
        .map_err(api_error_from_vfs_mutation)?;

    Ok(Json(SurfaceCreateFileResponse {
        surface_ref: req.surface_ref,
        mount_id: req.mount_id,
        path: result.path,
        size: result.size,
        content_kind: result.content_kind,
        mime_type: result.mime_type,
    }))
}

pub async fn delete_surface_file(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Json(req): Json<SurfaceDeleteFileRequest>,
) -> Result<Json<SurfaceDeleteFileResponse>, ApiError> {
    let source =
        agentdash_application::vfs::ResolvedVfsSurfaceSource::parse_surface_ref(&req.surface_ref)
            .map_err(ApiError::BadRequest)?;
    let (_surface, vfs) =
        resolve_surface_bundle(&state, &current_user, &source, ProjectPermission::Edit).await?;
    let normalized_path =
        agentdash_application::vfs::normalize_mount_relative_path(&req.path, false)
            .map_err(ApiError::BadRequest)?;
    ensure_not_skill_asset_document_path(&source, &normalized_path, "删除")?;
    check_mount_available(&state, &vfs, &req.mount_id).await?;
    state
        .services
        .vfs_mutation_dispatcher
        .delete_text(
            &vfs,
            ResourceRef {
                mount_id: req.mount_id.clone(),
                path: normalized_path.clone(),
            },
            Some(&current_user),
        )
        .await
        .map_err(api_error_from_vfs_mutation)?;

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
    let source =
        agentdash_application::vfs::ResolvedVfsSurfaceSource::parse_surface_ref(&req.surface_ref)
            .map_err(ApiError::BadRequest)?;
    let (_surface, vfs) =
        resolve_surface_bundle(&state, &current_user, &source, ProjectPermission::Edit).await?;
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
    check_mount_available(&state, &vfs, &req.mount_id).await?;
    let (from_path, to_path) = state
        .services
        .vfs_mutation_dispatcher
        .rename_text(
            &vfs,
            &req.mount_id,
            &from_path,
            &to_path,
            Some(&current_user),
        )
        .await
        .map_err(api_error_from_vfs_mutation)?;

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
    let source =
        agentdash_application::vfs::ResolvedVfsSurfaceSource::parse_surface_ref(&req.surface_ref)
            .map_err(ApiError::BadRequest)?;
    let (_surface, vfs) =
        resolve_surface_bundle(&state, &current_user, &source, ProjectPermission::View).await?;
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
            None,
            Some(&current_user),
        )
        .await
        .map_err(|e| ApiError::Internal(format!("VFS surface stat 失败: {e}")))?;

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
    let source =
        agentdash_application::vfs::ResolvedVfsSurfaceSource::parse_surface_ref(&req.surface_ref)
            .map_err(ApiError::BadRequest)?;
    let (_surface, vfs) =
        resolve_surface_bundle(&state, &current_user, &source, ProjectPermission::Edit).await?;

    check_mount_available(&state, &vfs, &req.mount_id).await?;
    let result = state
        .services
        .vfs_mutation_dispatcher
        .apply_patch(&vfs, &req.mount_id, &req.patch, Some(&current_user))
        .await
        .map_err(api_error_from_vfs_mutation)?;

    Ok(Json(SurfaceApplyPatchResponse {
        surface_ref: req.surface_ref,
        mount_id: req.mount_id,
        added: result.added,
        modified: result.modified,
        deleted: result.deleted,
    }))
}
