//! Project Extension Runtime HTTP 路由。

use agentdash_diagnostics::{DiagnosticErrorContext, Subsystem, diag_error};
use std::sync::Arc;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderValue, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::auth::{CurrentUser, ProjectPermission, load_project_with_permission};
use crate::dto::{ExtensionRuntimeProjectionResponse, extension_runtime_projection_response};
use crate::rpc::ApiError;
use agentdash_application::extension_package::{
    ExtensionPackageArtifactUseCaseError, ReadExtensionPackageComponentAssetInput,
    ReadExtensionPackageWebviewAssetInput, read_extension_package_component_asset,
    read_extension_package_webview_asset,
};
use agentdash_application::extension_runtime::{
    UninstallExtensionInstallationInput, extension_runtime_projection_from_installations,
    uninstall_extension_installation,
};
use agentdash_contracts::extension_runtime::UninstallExtensionInstallationResponse;
use agentdash_domain::DomainError;

#[derive(Debug, Deserialize)]
pub struct ProjectExtensionRuntimePath {
    pub project_id: String,
}

pub fn router() -> axum::Router<std::sync::Arc<crate::app_state::AppState>> {
    axum::Router::new()
        .route(
            "/projects/{project_id}/extension-runtime",
            axum::routing::get(get_project_extension_runtime),
        )
        .route(
            "/projects/{project_id}/extension-runtime/webviews/{extension_key}/{*asset_path}",
            axum::routing::get(get_project_extension_webview_asset),
        )
        .route(
            "/projects/{project_id}/extension-runtime/artifacts/{artifact_id}/{archive_digest}/components/{component_key}/{*asset_path}",
            axum::routing::get(get_project_extension_component_asset),
        )
        .route(
            "/projects/{project_id}/extensions/{installation_id}",
            axum::routing::delete(uninstall_extension_installation_route),
        )
}

#[derive(Debug, Deserialize)]
pub struct ProjectExtensionRuntimeWebviewPath {
    pub project_id: String,
    pub extension_key: String,
    pub asset_path: String,
}

#[derive(Debug, Deserialize)]
pub struct ProjectExtensionRuntimeComponentPath {
    pub project_id: String,
    pub artifact_id: String,
    pub archive_digest: String,
    pub component_key: String,
    pub asset_path: String,
}

#[derive(Debug, Deserialize)]
pub struct ProjectExtensionInstallationPath {
    pub project_id: String,
    pub installation_id: String,
}

/// GET `/api/projects/:project_id/extension-runtime`
pub async fn get_project_extension_runtime(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<ProjectExtensionRuntimePath>,
) -> Result<Json<ExtensionRuntimeProjectionResponse>, ApiError> {
    let project_id = parse_project_id(&path.project_id)?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Use,
    )
    .await?;
    let installations = state
        .repos
        .project_extension_installation_repo
        .list_enabled_by_project(project_id)
        .await
        .map_err(ApiError::from)?;
    let projection = extension_runtime_projection_from_installations(installations)?;
    Ok(Json(extension_runtime_projection_response(projection)))
}

/// DELETE `/api/projects/:project_id/extensions/:installation_id`
pub async fn uninstall_extension_installation_route(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<ProjectExtensionInstallationPath>,
) -> Result<Json<UninstallExtensionInstallationResponse>, ApiError> {
    let project_id = parse_project_id(&path.project_id)?;
    let installation_id = Uuid::parse_str(&path.installation_id)
        .map_err(|_| ApiError::BadRequest("无效的 Installation ID".into()))?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Configure,
    )
    .await?;
    let output = uninstall_extension_installation(
        &state.repos,
        UninstallExtensionInstallationInput {
            project_id,
            installation_id,
        },
    )
    .await
    .map_err(|error| match error {
        DomainError::NotFound { .. } => ApiError::NotFound("Extension installation 不存在".into()),
        other => ApiError::from(other),
    })?;
    Ok(Json(UninstallExtensionInstallationResponse {
        installation_id: output.installation_id.to_string(),
        extension_key: output.extension_key,
    }))
}

/// GET `/api/projects/:project_id/extension-runtime/webviews/:extension_key/*asset_path`
pub async fn get_project_extension_webview_asset(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<ProjectExtensionRuntimeWebviewPath>,
) -> Result<Response, ApiError> {
    let project_id = parse_project_id(&path.project_id)?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Use,
    )
    .await?;
    let asset = read_extension_package_webview_asset(
        &state.repos,
        state.services.extension_package_artifact_storage.as_ref(),
        ReadExtensionPackageWebviewAssetInput {
            project_id,
            extension_key: path.extension_key,
            asset_path: path.asset_path,
        },
    )
    .await
    .map_err(extension_package_error_to_api)?;
    let content_type = HeaderValue::from_static(content_type_for_path(&asset.asset_path));
    Ok((
        [
            (header::CONTENT_TYPE, content_type),
            (header::CACHE_CONTROL, HeaderValue::from_static("no-store")),
        ],
        Bytes::from(asset.bytes),
    )
        .into_response())
}

/// 读取由 Interaction runtime binding 固定的 exact Extension component artifact。
pub async fn get_project_extension_component_asset(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<ProjectExtensionRuntimeComponentPath>,
) -> Result<Response, ApiError> {
    let project_id = parse_project_id(&path.project_id)?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Use,
    )
    .await?;
    let artifact_id = Uuid::parse_str(&path.artifact_id)
        .map_err(|_| ApiError::BadRequest("artifact_id 格式非法".to_string()))?;
    if path.archive_digest.len() != 64
        || !path
            .archive_digest
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        return Err(ApiError::BadRequest(
            "archive_digest 必须为 sha256 hex".to_string(),
        ));
    }
    let asset = read_extension_package_component_asset(
        &state.repos,
        state.services.extension_package_artifact_storage.as_ref(),
        ReadExtensionPackageComponentAssetInput {
            project_id,
            artifact_id,
            expected_archive_digest: format!("sha256:{}", path.archive_digest),
            component_key: path.component_key,
            asset_path: path.asset_path,
        },
    )
    .await
    .map_err(extension_package_error_to_api)?;
    let content_type = HeaderValue::from_static(content_type_for_path(&asset.asset_path));
    Ok((
        [
            (header::CONTENT_TYPE, content_type),
            (
                header::CACHE_CONTROL,
                HeaderValue::from_static("private, immutable, max-age=31536000"),
            ),
            (
                header::CONTENT_SECURITY_POLICY,
                HeaderValue::from_static(
                    "sandbox allow-scripts; default-src 'none'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; connect-src 'none'; object-src 'none'; base-uri 'none'; frame-ancestors 'self'",
                ),
            ),
            (
                header::X_CONTENT_TYPE_OPTIONS,
                HeaderValue::from_static("nosniff"),
            ),
        ],
        Bytes::from(asset.bytes),
    )
        .into_response())
}

fn parse_project_id(raw: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(raw).map_err(|_| ApiError::BadRequest("无效的 Project ID".into()))
}

fn content_type_for_path(path: &str) -> &'static str {
    let lower = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
    if lower.ends_with(".html") || lower.ends_with(".htm") {
        "text/html; charset=utf-8"
    } else if lower.ends_with(".js") || lower.ends_with(".mjs") {
        "text/javascript; charset=utf-8"
    } else if lower.ends_with(".css") {
        "text/css; charset=utf-8"
    } else if lower.ends_with(".json") {
        "application/json; charset=utf-8"
    } else if lower.ends_with(".svg") {
        "image/svg+xml"
    } else if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else if lower.ends_with(".webp") {
        "image/webp"
    } else {
        "application/octet-stream"
    }
}

fn extension_package_error_to_api(error: ExtensionPackageArtifactUseCaseError) -> ApiError {
    match error {
        ExtensionPackageArtifactUseCaseError::Domain(error) => ApiError::from(error),
        ExtensionPackageArtifactUseCaseError::Storage(error) => {
            let context = DiagnosticErrorContext::new("extension_runtime.webview_asset", "storage");
            diag_error!(
                Error,
                Subsystem::Api,
                context = &context,
                error = &error,
                route = "/api/projects/{project_id}/extension-runtime/webviews/{extension_key}/{asset_path}",
                "extension package artifact storage error"
            );
            ApiError::Internal(String::from("扩展包存储错误"))
        }
        ExtensionPackageArtifactUseCaseError::BadRequest(error) => ApiError::BadRequest(error),
        ExtensionPackageArtifactUseCaseError::NotFound(error) => ApiError::NotFound(error),
        ExtensionPackageArtifactUseCaseError::Forbidden(error) => ApiError::Forbidden(error),
        ExtensionPackageArtifactUseCaseError::Conflict(error) => ApiError::Conflict(error),
        ExtensionPackageArtifactUseCaseError::Integrity(error) => ApiError::Internal(error),
    }
}
