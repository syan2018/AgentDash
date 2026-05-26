//! Project Extension Package Artifact HTTP 路由。

use std::path::{Component, PathBuf};
use std::sync::Arc;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Multipart, Path, State};
use axum::http::{HeaderMap, HeaderValue, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use uuid::Uuid;

use agentdash_application::extension_package::{
    InstallExtensionPackageArtifactInput, StoreExtensionPackageArtifactInput, digest_bytes,
    install_extension_package_artifact, store_extension_package_artifact,
    validate_extension_package_archive,
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
    let storage_ref = storage_ref_for(project_id, &digest_bytes(&archive_bytes))?;
    write_storage_object(&storage_ref, &archive_bytes).await?;

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
    let bytes = read_storage_object(&artifact.storage_ref).await?;
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

pub(crate) fn storage_ref_for(project_id: Uuid, archive_digest: &str) -> Result<String, ApiError> {
    let digest = archive_digest
        .strip_prefix("sha256:")
        .ok_or_else(|| ApiError::BadRequest("archive_digest 格式非法".into()))?;
    Ok(format!(
        "extension-packages/{project_id}/{digest}.agentdash-extension.tgz"
    ))
}

pub(crate) async fn write_storage_object(storage_ref: &str, bytes: &[u8]) -> Result<(), ApiError> {
    let path = storage_path(storage_ref)?;
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|error| ApiError::Internal(format!("创建 artifact 存储目录失败: {error}")))?;
    }
    tokio::fs::write(&path, bytes)
        .await
        .map_err(|error| ApiError::Internal(format!("写入 artifact 存储失败: {error}")))?;
    Ok(())
}

pub(crate) async fn read_storage_object(storage_ref: &str) -> Result<Vec<u8>, ApiError> {
    let path = storage_path(storage_ref)?;
    tokio::fs::read(&path)
        .await
        .map_err(|error| ApiError::Internal(format!("读取 artifact 存储失败: {error}")))
}

fn storage_path(storage_ref: &str) -> Result<PathBuf, ApiError> {
    let mut path = storage_root();
    for component in std::path::Path::new(storage_ref).components() {
        match component {
            Component::Normal(part) => path.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(ApiError::Internal(format!(
                    "artifact storage_ref 非法: {storage_ref}"
                )));
            }
        }
    }
    Ok(path)
}

fn storage_root() -> PathBuf {
    std::env::var_os("AGENTDASH_EXTENSION_ARTIFACT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::current_dir()
                .unwrap_or_else(|_| std::env::temp_dir())
                .join(".agentdash")
                .join("extension-artifacts")
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_ref_uses_archive_digest() {
        let project_id = Uuid::nil();
        let storage_ref = storage_ref_for(
            project_id,
            "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        )
        .expect("storage ref");
        assert_eq!(
            storage_ref,
            "extension-packages/00000000-0000-0000-0000-000000000000/0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef.agentdash-extension.tgz"
        );
    }
}
