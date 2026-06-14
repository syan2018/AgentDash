//! Project 级 SkillAsset HTTP 路由。

use std::io::Read;
use std::sync::Arc;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Multipart, Path, Query, State};
use axum::http::{HeaderValue, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use uuid::Uuid;

use agentdash_application::skill_asset::{
    CreateSkillAssetInput, ImportRemoteSkillAssetInput, SkillAssetApplicationError,
    SkillAssetFileInput, SkillAssetService, UpdateSkillAssetInput, content_from_bytes,
    import_remote_skill_url_to_project,
};
use agentdash_contracts::common_response::DeletedIdResponse;
use agentdash_domain::skill_asset::{SkillAsset, SkillAssetFile};

use crate::app_state::AppState;
use crate::auth::{CurrentUser, ProjectPermission, load_project_with_permission};
use crate::dto::{
    CreateSkillAssetRequest, ImportRemoteSkillAssetRequest, ListSkillAssetQuery,
    SkillAssetFileBlobQuery, SkillAssetFileDto, SkillAssetResponse, UpdateSkillAssetRequest,
};
use crate::rpc::ApiError;

#[derive(Debug, Deserialize)]
pub struct ProjectSkillAssetsPath {
    pub project_id: String,
}

const SKILL_ASSET_UPLOAD_BODY_LIMIT_BYTES: usize = 80 * 1024 * 1024;

pub fn router() -> axum::Router<std::sync::Arc<crate::app_state::AppState>> {
    axum::Router::new()
        .route(
            "/projects/{project_id}/skill-assets",
            axum::routing::get(list_skill_assets).post(create_skill_asset),
        )
        .route(
            "/projects/{project_id}/skill-assets/upload",
            axum::routing::post(upload_skill_assets).layer(axum::extract::DefaultBodyLimit::max(
                SKILL_ASSET_UPLOAD_BODY_LIMIT_BYTES,
            )),
        )
        .route(
            "/projects/{project_id}/skill-assets/import",
            axum::routing::post(import_remote_skill_asset),
        )
        .route(
            "/projects/{project_id}/skill-assets/{id}",
            axum::routing::get(get_skill_asset)
                .patch(update_skill_asset)
                .delete(delete_skill_asset),
        )
        .route(
            "/projects/{project_id}/skill-assets/{id}/files/blob",
            axum::routing::get(read_skill_asset_file_blob),
        )
}

#[derive(Debug, Deserialize)]
pub struct SkillAssetItemPath {
    pub project_id: String,
    pub id: String,
}

pub async fn list_skill_assets(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<ProjectSkillAssetsPath>,
    Query(query): Query<ListSkillAssetQuery>,
) -> Result<Json<Vec<SkillAssetResponse>>, ApiError> {
    let project_id = parse_project_id(&path.project_id)?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::View,
    )
    .await?;

    let service = SkillAssetService::new(state.repos.skill_asset_repo.as_ref());
    let mut assets = service.list(project_id).await?;
    match query.source.as_deref() {
        Some("user") => assets.retain(|asset| asset.source.tag() == "user"),
        Some("builtin_seed") => assets.retain(SkillAsset::is_builtin_seed),
        Some("github") => assets.retain(|asset| asset.source.tag() == "github"),
        Some("clawhub") => assets.retain(|asset| asset.source.tag() == "clawhub"),
        Some("skills_sh") => assets.retain(|asset| asset.source.tag() == "skills_sh"),
        Some(other) if !other.is_empty() => {
            return Err(ApiError::BadRequest(format!(
                "无效的 source 过滤值: {other}（可选 user | builtin_seed | github | clawhub | skills_sh）"
            )));
        }
        _ => {}
    }

    Ok(Json(assets.into_iter().map(Into::into).collect()))
}

pub async fn create_skill_asset(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<ProjectSkillAssetsPath>,
    Json(req): Json<CreateSkillAssetRequest>,
) -> Result<Json<SkillAssetResponse>, ApiError> {
    let project_id = parse_project_id(&path.project_id)?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Edit,
    )
    .await?;

    let service = SkillAssetService::new(state.repos.skill_asset_repo.as_ref());
    let asset = service
        .create(CreateSkillAssetInput {
            project_id,
            key: req.key,
            display_name: req.display_name,
            description: req.description,
            disable_model_invocation: req.disable_model_invocation,
            files: dto_files_to_input(req.files)?,
        })
        .await?;
    Ok(Json(asset.into()))
}

pub async fn get_skill_asset(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<SkillAssetItemPath>,
) -> Result<Json<SkillAssetResponse>, ApiError> {
    let (_project_id, asset) = load_asset_with_project(
        state.as_ref(),
        &current_user,
        &path,
        ProjectPermission::View,
    )
    .await?;
    Ok(Json(asset.into()))
}

pub async fn update_skill_asset(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<SkillAssetItemPath>,
    Json(req): Json<UpdateSkillAssetRequest>,
) -> Result<Json<SkillAssetResponse>, ApiError> {
    let (_project_id, asset) = load_asset_with_project(
        state.as_ref(),
        &current_user,
        &path,
        ProjectPermission::Edit,
    )
    .await?;

    let service = SkillAssetService::new(state.repos.skill_asset_repo.as_ref());
    let updated = service
        .update(
            asset.id,
            UpdateSkillAssetInput {
                key: req.key,
                display_name: req.display_name,
                description: req.description,
                disable_model_invocation: req.disable_model_invocation,
                files: req
                    .files
                    .map(|files| dto_files_to_update_input(files, &asset.files))
                    .transpose()?,
            },
        )
        .await?;
    Ok(Json(updated.into()))
}

pub async fn delete_skill_asset(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<SkillAssetItemPath>,
) -> Result<Json<DeletedIdResponse>, ApiError> {
    let (_project_id, asset) = load_asset_with_project(
        state.as_ref(),
        &current_user,
        &path,
        ProjectPermission::Edit,
    )
    .await?;
    let service = SkillAssetService::new(state.repos.skill_asset_repo.as_ref());
    service.delete(asset.id).await?;
    Ok(Json(DeletedIdResponse {
        deleted: asset.id.to_string(),
    }))
}

pub async fn read_skill_asset_file_blob(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<SkillAssetItemPath>,
    Query(query): Query<SkillAssetFileBlobQuery>,
) -> Result<Response, ApiError> {
    let (_project_id, asset) = load_asset_with_project(
        state.as_ref(),
        &current_user,
        &path,
        ProjectPermission::View,
    )
    .await?;
    let normalized_path = query.path.trim().replace('\\', "/");
    let file = asset
        .files
        .into_iter()
        .find(|file| file.path == normalized_path)
        .ok_or_else(|| ApiError::NotFound(format!("SkillAsset 文件不存在: {normalized_path}")))?;
    let Some(bytes) = file.binary_content().map(|bytes| bytes.to_vec()) else {
        return Err(ApiError::BadRequest(format!(
            "SkillAsset 文件不是二进制文件: {normalized_path}"
        )));
    };
    let mime_type = file.mime_type().unwrap_or("application/octet-stream");
    let content_type = HeaderValue::from_str(mime_type)
        .map_err(|_| ApiError::BadRequest(format!("非法 MIME 类型: {mime_type}")))?;
    Ok(([(header::CONTENT_TYPE, content_type)], Bytes::from(bytes)).into_response())
}

pub async fn import_remote_skill_asset(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<ProjectSkillAssetsPath>,
    Json(req): Json<ImportRemoteSkillAssetRequest>,
) -> Result<Json<SkillAssetResponse>, ApiError> {
    let project_id = parse_project_id(&path.project_id)?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Edit,
    )
    .await?;

    let remote_source = agentdash_infrastructure::HttpRemoteSkillSource::new();
    let asset = import_remote_skill_url_to_project(
        &state.repos,
        ImportRemoteSkillAssetInput {
            project_id,
            owner_id: current_user.user_id.clone(),
            url: req.url,
        },
        &remote_source,
    )
    .await?;
    Ok(Json(asset.into()))
}

pub async fn upload_skill_assets(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<ProjectSkillAssetsPath>,
    mut multipart: Multipart,
) -> Result<Json<Vec<SkillAssetResponse>>, ApiError> {
    let project_id = parse_project_id(&path.project_id)?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Edit,
    )
    .await?;

    let mut files = Vec::new();
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|error| ApiError::BadRequest(format!("multipart 上传内容解析失败: {error}")))?
    {
        let filename = field.file_name().map(ToString::to_string);
        let content_type = field.content_type().map(ToString::to_string);
        let bytes = field
            .bytes()
            .await
            .map_err(|error| ApiError::BadRequest(format!("读取上传文件失败: {error}")))?;
        let Some(filename) = filename else {
            continue;
        };
        if filename.to_ascii_lowercase().ends_with(".zip") {
            files.extend(extract_zip_skill_files(&bytes)?);
        } else {
            let content = content_from_bytes(&filename, bytes.to_vec(), content_type.as_deref())?;
            files.push(SkillAssetFileInput {
                path: filename,
                content,
            });
        }
    }

    let service = SkillAssetService::new(state.repos.skill_asset_repo.as_ref());
    let assets = service.import_uploaded_files(project_id, files).await?;
    Ok(Json(assets.into_iter().map(Into::into).collect()))
}

async fn load_asset_with_project(
    state: &AppState,
    current_user: &agentdash_integration_api::AuthIdentity,
    path: &SkillAssetItemPath,
    permission: ProjectPermission,
) -> Result<(Uuid, SkillAsset), ApiError> {
    let project_id = parse_project_id(&path.project_id)?;
    let asset_id = parse_asset_id(&path.id)?;
    load_project_with_permission(state, current_user, project_id, permission).await?;

    let service = SkillAssetService::new(state.repos.skill_asset_repo.as_ref());
    let asset = service.get(asset_id).await.map_err(|err| match err {
        SkillAssetApplicationError::NotFound(msg) => ApiError::NotFound(msg),
        other => other.into(),
    })?;
    if asset.project_id != project_id {
        return Err(ApiError::NotFound(format!(
            "skill_asset 不存在: {asset_id}"
        )));
    }
    Ok((project_id, asset))
}

fn dto_files_to_input(files: Vec<SkillAssetFileDto>) -> Result<Vec<SkillAssetFileInput>, ApiError> {
    files
        .into_iter()
        .map(|file| {
            Ok(SkillAssetFileInput {
                path: file.path,
                content: file
                    .content
                    .map(agentdash_domain::common::StoredFileContent::text)
                    .ok_or_else(|| {
                        ApiError::BadRequest("创建 SkillAsset 时文件 content 不能为空".to_string())
                    })?,
            })
        })
        .collect()
}

fn dto_files_to_update_input(
    files: Vec<SkillAssetFileDto>,
    existing_files: &[SkillAssetFile],
) -> Result<Vec<SkillAssetFileInput>, ApiError> {
    files
        .into_iter()
        .map(|file| {
            let content = match file.content {
                Some(content) => agentdash_domain::common::StoredFileContent::text(content),
                None if file.content_kind == "binary" => existing_files
                    .iter()
                    .find(|existing| existing.path == file.path)
                    .map(|existing| existing.content.clone())
                    .ok_or_else(|| {
                        ApiError::BadRequest(format!(
                            "无法保留不存在的二进制 Skill 文件: {}",
                            file.path
                        ))
                    })?,
                None => {
                    return Err(ApiError::BadRequest(format!(
                        "文本 Skill 文件 content 不能为空: {}",
                        file.path
                    )));
                }
            };
            Ok(SkillAssetFileInput {
                path: file.path,
                content,
            })
        })
        .collect()
}

fn extract_zip_skill_files(bytes: &[u8]) -> Result<Vec<SkillAssetFileInput>, ApiError> {
    let reader = std::io::Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(reader)
        .map_err(|error| ApiError::BadRequest(format!("ZIP 文件解析失败: {error}")))?;
    let mut files = Vec::new();
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| ApiError::BadRequest(format!("读取 ZIP 条目失败: {error}")))?;
        if entry.is_dir() {
            continue;
        }
        let Some(path) = entry.enclosed_name().map(|path| {
            path.to_string_lossy()
                .replace('\\', "/")
                .trim_matches('/')
                .to_string()
        }) else {
            return Err(ApiError::BadRequest(format!(
                "ZIP 包含不安全路径: {}",
                entry.name()
            )));
        };
        let mut bytes = Vec::new();
        entry
            .read_to_end(&mut bytes)
            .map_err(|error| ApiError::BadRequest(format!("读取 ZIP 条目失败: {path}: {error}")))?;
        let content = content_from_bytes(&path, bytes, None)?;
        files.push(SkillAssetFileInput { path, content });
    }
    Ok(files)
}

fn parse_project_id(raw: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(raw).map_err(|_| ApiError::BadRequest("无效的 Project ID".into()))
}

fn parse_asset_id(raw: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(raw).map_err(|_| ApiError::BadRequest("无效的 skill_asset ID".into()))
}
