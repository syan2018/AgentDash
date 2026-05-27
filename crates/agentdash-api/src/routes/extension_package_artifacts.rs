//! Project Extension Package Artifact HTTP 路由。

use std::sync::Arc;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Multipart, Path, State};
use axum::http::{HeaderMap, HeaderValue, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use uuid::Uuid;

use agentdash_application::extension_package::{
    ExtensionPackageArtifactStorageError, InstallExtensionPackageArtifactInput,
    StoreExtensionPackageArtifactInput, digest_bytes, extension_package_archive_storage_ref_for,
    install_extension_package_artifact, read_extension_package_archive_object,
    store_extension_package_artifact, validate_extension_package_archive,
    write_extension_package_archive_object,
};
use agentdash_contracts::extension_package::{
    ExtensionPackageArtifactResponse, ExtensionPackageInstallationResponse,
    InstallExtensionPackageArtifactRequest,
};
use agentdash_domain::DomainError;
use agentdash_domain::extension_package::ExtensionPackageArtifact;

use crate::app_state::AppState;
use crate::auth::{CurrentUser, ProjectPermission, load_project_with_permission};
use crate::routes::backend_access::ensure_project_backend_access;
use crate::rpc::ApiError;

#[derive(Debug, Deserialize)]
pub struct ProjectExtensionArtifactsPath {
    pub project_id: String,
}

#[derive(Debug, Deserialize)]
pub struct ProjectExtensionArtifactItemPath {
    pub project_id: String,
    pub artifact_id: String,
}

pub async fn list_extension_package_artifacts(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<ProjectExtensionArtifactsPath>,
) -> Result<Json<Vec<ExtensionPackageArtifactResponse>>, ApiError> {
    let project_id = parse_uuid(&path.project_id, "project_id")?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::View,
    )
    .await?;
    let artifacts = state
        .repos
        .extension_package_artifact_repo
        .list_by_project(project_id)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(artifacts.into_iter().map(artifact_response).collect()))
}

pub async fn upload_extension_package_artifact(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<ProjectExtensionArtifactsPath>,
    mut multipart: Multipart,
) -> Result<Json<ExtensionPackageArtifactResponse>, ApiError> {
    let project_id = parse_uuid(&path.project_id, "project_id")?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Edit,
    )
    .await?;

    let mut archive_bytes = None;
    let mut expected_archive_digest = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|error| ApiError::BadRequest(format!("multipart 上传内容解析失败: {error}")))?
    {
        let name = field.name().unwrap_or_default().to_string();
        match name.as_str() {
            "archive" | "file" => {
                archive_bytes = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|error| {
                            ApiError::BadRequest(format!("读取上传文件失败: {error}"))
                        })?
                        .to_vec(),
                );
            }
            "archive_digest" | "expected_archive_digest" => {
                let value = field.text().await.map_err(|error| {
                    ApiError::BadRequest(format!("读取 archive_digest 失败: {error}"))
                })?;
                expected_archive_digest = Some(value);
            }
            _ => {
                let _ = field.bytes().await;
            }
        }
    }

    let archive_bytes =
        archive_bytes.ok_or_else(|| ApiError::BadRequest("缺少 archive 文件字段".into()))?;
    validate_extension_package_archive(&archive_bytes, expected_archive_digest.as_deref())?;
    let storage_ref =
        extension_package_archive_storage_ref_for(project_id, &digest_bytes(&archive_bytes))?;
    write_extension_package_archive_object(&storage_ref, &archive_bytes)
        .await
        .map_err(storage_error_to_api)?;

    let artifact = store_extension_package_artifact(
        &state.repos,
        StoreExtensionPackageArtifactInput {
            project_id,
            storage_ref,
            archive_bytes,
            expected_archive_digest,
        },
    )
    .await?;
    Ok(Json(artifact_response(artifact)))
}

pub async fn install_extension_package_artifact_route(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<ProjectExtensionArtifactItemPath>,
    Json(req): Json<InstallExtensionPackageArtifactRequest>,
) -> Result<Json<ExtensionPackageInstallationResponse>, ApiError> {
    let project_id = parse_uuid(&path.project_id, "project_id")?;
    let artifact_id = parse_uuid(&path.artifact_id, "artifact_id")?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Edit,
    )
    .await?;
    let installation = install_extension_package_artifact(
        &state.repos,
        InstallExtensionPackageArtifactInput {
            project_id,
            artifact_id,
            extension_key: req.extension_key,
            display_name: req.display_name,
            overwrite: req.overwrite,
        },
    )
    .await?;
    let artifact = installation.package_artifact.ok_or_else(|| {
        ApiError::Internal("packaged extension installation 缺少 artifact 引用".into())
    })?;
    Ok(Json(ExtensionPackageInstallationResponse {
        installation_id: installation.id.to_string(),
        extension_key: installation.extension_key,
        extension_id: installation.manifest.extension_id,
        package_artifact_id: artifact.artifact_id.to_string(),
        archive_digest: artifact.archive_digest,
    }))
}

pub async fn download_extension_package_archive(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<ProjectExtensionArtifactItemPath>,
) -> Result<Response, ApiError> {
    let project_id = parse_uuid(&path.project_id, "project_id")?;
    let artifact_id = parse_uuid(&path.artifact_id, "artifact_id")?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::View,
    )
    .await?;
    extension_package_archive_response(state, project_id, artifact_id).await
}

pub async fn download_extension_package_archive_for_backend(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(path): Path<ProjectExtensionArtifactItemPath>,
) -> Result<Response, ApiError> {
    let project_id = parse_uuid(&path.project_id, "project_id")?;
    let artifact_id = parse_uuid(&path.artifact_id, "artifact_id")?;
    let token = extract_bearer_token(&headers)
        .ok_or_else(|| ApiError::Unauthorized("缺少 backend relay token".into()))?;
    let backend = match state
        .repos
        .backend_repo
        .get_backend_by_auth_token(token)
        .await
    {
        Ok(backend) => backend,
        Err(DomainError::NotFound { .. }) => {
            return Err(ApiError::Unauthorized("backend relay token 无效".into()));
        }
        Err(error) => return Err(ApiError::from(error)),
    };
    if !backend.enabled {
        return Err(ApiError::Forbidden(format!(
            "backend `{}` 未启用",
            backend.id
        )));
    }
    ensure_project_backend_access(&state, project_id, &backend.id).await?;
    extension_package_archive_response(state, project_id, artifact_id).await
}

async fn extension_package_archive_response(
    state: Arc<AppState>,
    project_id: Uuid,
    artifact_id: Uuid,
) -> Result<Response, ApiError> {
    let artifact = state
        .repos
        .extension_package_artifact_repo
        .get(artifact_id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::NotFound("Extension package artifact 不存在".into()))?;
    if artifact.project_id != project_id {
        return Err(ApiError::NotFound(
            "Extension package artifact 不存在".into(),
        ));
    }
    let bytes = read_extension_package_archive_object(&artifact.storage_ref)
        .await
        .map_err(storage_error_to_api)?;
    let actual_digest = digest_bytes(&bytes);
    if actual_digest != artifact.archive_digest {
        return Err(ApiError::Internal(format!(
            "extension package artifact 存储 digest 不匹配: expected {}, actual {}",
            artifact.archive_digest, actual_digest
        )));
    }

    let filename = format!("{}.agentdash-extension.tgz", artifact.extension_id);
    let content_type = HeaderValue::from_static("application/vnd.agentdash.extension+gzip");
    let disposition = HeaderValue::from_str(&format!("attachment; filename=\"{filename}\""))
        .map_err(|error| ApiError::Internal(format!("下载文件名非法: {error}")))?;
    Ok((
        [
            (header::CONTENT_TYPE, content_type),
            (header::CONTENT_DISPOSITION, disposition),
        ],
        Bytes::from(bytes),
    )
        .into_response())
}

fn extract_bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| {
            value
                .strip_prefix("Bearer ")
                .or_else(|| value.strip_prefix("bearer "))
        })
}

fn artifact_response(artifact: ExtensionPackageArtifact) -> ExtensionPackageArtifactResponse {
    ExtensionPackageArtifactResponse {
        id: artifact.id.to_string(),
        project_id: artifact.project_id.to_string(),
        extension_id: artifact.extension_id,
        package_name: artifact.package_name,
        package_version: artifact.package_version,
        asset_version: artifact.asset_version,
        source_version: artifact.source_version,
        storage_ref: artifact.storage_ref,
        archive_digest: artifact.archive_digest,
        manifest_digest: artifact.manifest_digest,
        manifest: serde_json::to_value(artifact.manifest).unwrap_or(serde_json::Value::Null),
        byte_size: artifact.byte_size,
        created_at: artifact.created_at.to_rfc3339(),
        updated_at: artifact.updated_at.to_rfc3339(),
    }
}

fn parse_uuid(raw: &str, field: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(raw).map_err(|_| ApiError::BadRequest(format!("{field} 非法")))
}

fn storage_error_to_api(error: ExtensionPackageArtifactStorageError) -> ApiError {
    ApiError::Internal(error.to_string())
}
