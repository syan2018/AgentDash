//! Project Extension 管理 HTTP 路由。

use std::sync::Arc;

use axum::Json;
use axum::extract::{Multipart, Path, State};
use serde::Deserialize;
use uuid::Uuid;

use agentdash_application::extension_management::list_project_extension_management_items;
use agentdash_application::extension_package::{
    InstallExtensionPackageArtifactInput, StoreExtensionPackageArchiveInput,
    install_extension_package_artifact, store_extension_package_archive,
};
use agentdash_contracts::extension_package::{
    ExtensionPackageInstallationResponse, ImportExtensionPackageResponse,
};

use crate::app_state::AppState;
use crate::auth::{CurrentUser, ProjectPermission, load_project_with_permission};
use crate::dto::{
    ProjectExtensionManagementListResponse, project_extension_management_list_response,
};
use crate::routes::extension_package_artifacts::{
    artifact_response, extension_package_error_to_api,
};
use crate::rpc::ApiError;

#[derive(Debug, Deserialize)]
pub struct ProjectExtensionsPath {
    pub project_id: String,
}

/// GET `/api/projects/:project_id/extensions`
pub async fn list_project_extensions(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<ProjectExtensionsPath>,
) -> Result<Json<ProjectExtensionManagementListResponse>, ApiError> {
    let project_id = parse_project_id(&path.project_id)?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::View,
    )
    .await?;
    let items = list_project_extension_management_items(&state.repos, project_id).await?;
    Ok(Json(project_extension_management_list_response(items)))
}

/// POST `/api/projects/:project_id/extensions/import-package`
pub async fn import_extension_package(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<ProjectExtensionsPath>,
    mut multipart: Multipart,
) -> Result<Json<ImportExtensionPackageResponse>, ApiError> {
    let project_id = parse_project_id(&path.project_id)?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Edit,
    )
    .await?;

    let mut archive_bytes = None;
    let mut expected_archive_digest = None;
    let mut extension_key = None;
    let mut display_name = None;
    let mut overwrite = false;
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
                expected_archive_digest = Some(field.text().await.map_err(|error| {
                    ApiError::BadRequest(format!("读取 archive_digest 失败: {error}"))
                })?);
            }
            "extension_key" => {
                extension_key = Some(field.text().await.map_err(|error| {
                    ApiError::BadRequest(format!("读取 extension_key 失败: {error}"))
                })?);
            }
            "display_name" => {
                display_name = Some(field.text().await.map_err(|error| {
                    ApiError::BadRequest(format!("读取 display_name 失败: {error}"))
                })?);
            }
            "overwrite" => {
                let value = field.text().await.map_err(|error| {
                    ApiError::BadRequest(format!("读取 overwrite 失败: {error}"))
                })?;
                overwrite = matches!(value.trim(), "true" | "1" | "yes");
            }
            _ => {
                let _ = field.bytes().await;
            }
        }
    }

    let archive_bytes =
        archive_bytes.ok_or_else(|| ApiError::BadRequest("缺少 archive 文件字段".into()))?;
    let artifact = store_extension_package_archive(
        &state.repos,
        state.services.extension_package_artifact_storage.as_ref(),
        StoreExtensionPackageArchiveInput {
            project_id,
            archive_bytes,
            expected_archive_digest,
        },
    )
    .await
    .map_err(extension_package_error_to_api)?;
    let installation = install_extension_package_artifact(
        &state.repos,
        InstallExtensionPackageArtifactInput {
            project_id,
            artifact_id: artifact.id,
            extension_key,
            display_name,
            overwrite,
        },
    )
    .await?;
    let package_artifact = installation.package_artifact.clone().ok_or_else(|| {
        ApiError::Internal("packaged extension installation 缺少 artifact 引用".into())
    })?;

    Ok(Json(ImportExtensionPackageResponse {
        artifact: artifact_response(artifact),
        installation: ExtensionPackageInstallationResponse {
            installation_id: installation.id.to_string(),
            extension_key: installation.extension_key,
            extension_id: installation.manifest.extension_id,
            package_artifact_id: package_artifact.artifact_id.to_string(),
            archive_digest: package_artifact.archive_digest,
        },
    }))
}

fn parse_project_id(raw: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(raw).map_err(|_| ApiError::BadRequest("无效的 Project ID".into()))
}
