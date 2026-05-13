//! Project 级 SkillAsset HTTP 路由。

use std::io::Read;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Multipart, Path, Query, State};
use serde::Deserialize;
use uuid::Uuid;

use agentdash_application::skill_asset::{
    CreateSkillAssetInput, ImportRemoteSkillAssetInput, RawSkillUploadFile,
    SkillAssetApplicationError, SkillAssetFileInput, SkillAssetService, UpdateSkillAssetInput,
};
use agentdash_domain::skill_asset::SkillAsset;

use crate::app_state::AppState;
use crate::auth::{CurrentUser, ProjectPermission, load_project_with_permission};
use crate::dto::{
    BootstrapSkillAssetRequest, CreateSkillAssetRequest, ImportRemoteSkillAssetRequest,
    ListSkillAssetQuery, SkillAssetFileDto, SkillAssetResponse, UpdateSkillAssetRequest,
};
use crate::rpc::ApiError;

#[derive(Debug, Deserialize)]
pub struct ProjectSkillAssetsPath {
    pub project_id: String,
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
        Some(other) if !other.is_empty() => {
            return Err(ApiError::BadRequest(format!(
                "无效的 source 过滤值: {other}（可选 user | builtin_seed | github）"
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
            files: dto_files_to_input(req.files),
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
                files: req.files.map(dto_files_to_input),
            },
        )
        .await?;
    Ok(Json(updated.into()))
}

pub async fn delete_skill_asset(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<SkillAssetItemPath>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let (_project_id, asset) = load_asset_with_project(
        state.as_ref(),
        &current_user,
        &path,
        ProjectPermission::Edit,
    )
    .await?;
    let service = SkillAssetService::new(state.repos.skill_asset_repo.as_ref());
    service.delete(asset.id).await?;
    Ok(Json(serde_json::json!({ "deleted": asset.id })))
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

    let service = SkillAssetService::new(state.repos.skill_asset_repo.as_ref());
    let asset = service
        .import_remote(ImportRemoteSkillAssetInput {
            project_id,
            url: req.url,
        })
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
            let content = String::from_utf8(bytes.to_vec()).map_err(|error| {
                ApiError::BadRequest(format!("Skill 文件必须是 UTF-8 文本: {filename}: {error}"))
            })?;
            files.push(RawSkillUploadFile {
                path: filename,
                content,
            });
        }
    }

    let service = SkillAssetService::new(state.repos.skill_asset_repo.as_ref());
    let assets = service.import_uploaded_files(project_id, files).await?;
    Ok(Json(assets.into_iter().map(Into::into).collect()))
}

pub async fn bootstrap_skill_assets(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<ProjectSkillAssetsPath>,
    Json(req): Json<BootstrapSkillAssetRequest>,
) -> Result<Json<Vec<SkillAssetResponse>>, ApiError> {
    let project_id = parse_project_id(&path.project_id)?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Edit,
    )
    .await?;

    let service = SkillAssetService::new(state.repos.skill_asset_repo.as_ref());
    let assets = service
        .bootstrap_builtins(project_id, req.builtin_key.as_deref())
        .await?;
    Ok(Json(assets.into_iter().map(Into::into).collect()))
}

pub async fn reset_skill_asset_from_builtin(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<SkillAssetItemPath>,
) -> Result<Json<SkillAssetResponse>, ApiError> {
    let (_project_id, asset) = load_asset_with_project(
        state.as_ref(),
        &current_user,
        &path,
        ProjectPermission::Edit,
    )
    .await?;

    let service = SkillAssetService::new(state.repos.skill_asset_repo.as_ref());
    let reset = service.reset_from_builtin(asset.id).await?;
    Ok(Json(reset.into()))
}

async fn load_asset_with_project(
    state: &AppState,
    current_user: &agentdash_plugin_api::AuthIdentity,
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

fn dto_files_to_input(files: Vec<SkillAssetFileDto>) -> Vec<SkillAssetFileInput> {
    files
        .into_iter()
        .map(|file| SkillAssetFileInput {
            path: file.path,
            content: file.content,
        })
        .collect()
}

fn extract_zip_skill_files(bytes: &[u8]) -> Result<Vec<RawSkillUploadFile>, ApiError> {
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
        let mut content = String::new();
        entry.read_to_string(&mut content).map_err(|error| {
            ApiError::BadRequest(format!("ZIP 条目必须是 UTF-8 文本: {path}: {error}"))
        })?;
        files.push(RawSkillUploadFile { path, content });
    }
    Ok(files)
}

fn parse_project_id(raw: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(raw).map_err(|_| ApiError::BadRequest("无效的 Project ID".into()))
}

fn parse_asset_id(raw: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(raw).map_err(|_| ApiError::BadRequest("无效的 skill_asset ID".into()))
}
