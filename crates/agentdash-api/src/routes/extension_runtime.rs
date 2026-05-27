//! Project Extension Runtime HTTP 路由。

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
use crate::routes::backend_access::ensure_project_backend_access;
use crate::rpc::ApiError;
use agentdash_application::extension_package::{
    ExtensionPackageArtifactUseCaseError, ReadExtensionPackageWebviewAssetInput,
    read_extension_package_webview_asset,
};
use agentdash_application::extension_runtime::{
    UninstallExtensionInstallationInput, extension_runtime_projection_from_installations,
    uninstall_extension_installation,
};
use agentdash_application::runtime_gateway::{
    RuntimeActionKey, RuntimeActor, RuntimeContext, RuntimeInvocationRequest,
    RuntimeInvocationResult, RuntimeTarget,
};
use agentdash_contracts::extension_runtime::{
    ExtensionRuntimeInvocationOutputResponse, ExtensionRuntimeInvokeActionRequest,
    ExtensionRuntimeInvokeActionResponse, ExtensionRuntimeTraceResponse,
    UninstallExtensionInstallationResponse,
};
use agentdash_domain::DomainError;
use agentdash_domain::session_binding::SessionBinding;

#[derive(Debug, Deserialize)]
pub struct ProjectExtensionRuntimePath {
    pub project_id: String,
}

#[derive(Debug, Deserialize)]
pub struct ProjectExtensionRuntimeWebviewPath {
    pub project_id: String,
    pub extension_key: String,
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
        ProjectPermission::View,
    )
    .await?;
    let installations = state
        .repos
        .project_extension_installation_repo
        .list_enabled_by_project(project_id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    let projection = extension_runtime_projection_from_installations(installations)?;
    Ok(Json(extension_runtime_projection_response(projection)))
}

/// POST `/api/projects/:project_id/extension-runtime/invoke-action`
pub async fn invoke_project_extension_runtime_action(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<ProjectExtensionRuntimePath>,
    Json(req): Json<ExtensionRuntimeInvokeActionRequest>,
) -> Result<Json<ExtensionRuntimeInvokeActionResponse>, ApiError> {
    let project_id = parse_project_id(&path.project_id)?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::View,
    )
    .await?;

    let session_id = req.session_id.trim();
    if session_id.is_empty() {
        return Err(ApiError::BadRequest(
            "extension runtime invoke 缺少 session_id".into(),
        ));
    }
    let backend_id = req.backend_id.trim();
    if backend_id.is_empty() {
        return Err(ApiError::BadRequest(
            "extension runtime invoke 缺少 backend_id".into(),
        ));
    }
    ensure_project_session_scope(state.as_ref(), session_id, project_id).await?;
    ensure_project_backend_access(&state, project_id, backend_id).await?;

    let action_key = RuntimeActionKey::parse(req.action_key)
        .map_err(|error| ApiError::BadRequest(error.to_string()))?;
    let mut request = RuntimeInvocationRequest::new(
        action_key,
        RuntimeActor::SessionUser {
            session_id: session_id.to_string(),
            user_id: Some(current_user.user_id),
        },
        RuntimeContext::Session {
            session_id: session_id.to_string(),
            project_id: Some(project_id),
            workspace_id: None,
        },
        req.input,
    );
    request.target = Some(RuntimeTarget::Backend {
        backend_id: backend_id.to_string(),
    });

    let result = state.services.runtime_gateway.invoke(request).await?;
    Ok(Json(extension_runtime_invoke_response(result)))
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
        ProjectPermission::Edit,
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
        ProjectPermission::View,
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

fn parse_project_id(raw: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(raw).map_err(|_| ApiError::BadRequest("无效的 Project ID".into()))
}

async fn ensure_project_session_scope(
    state: &AppState,
    session_id: &str,
    project_id: Uuid,
) -> Result<Vec<SessionBinding>, ApiError> {
    let bindings = state
        .repos
        .session_binding_repo
        .list_by_session(session_id)
        .await?;
    if bindings.is_empty() {
        return Err(ApiError::NotFound(format!("Session {session_id} 不存在")));
    }
    if !bindings
        .iter()
        .any(|binding| binding.project_id == project_id)
    {
        return Err(ApiError::Forbidden(
            "当前 session 与目标 Extension Runtime 不属于同一 Project".into(),
        ));
    }
    Ok(bindings)
}

fn extension_runtime_invoke_response(
    result: RuntimeInvocationResult,
) -> ExtensionRuntimeInvokeActionResponse {
    ExtensionRuntimeInvokeActionResponse {
        action_key: result.action_key.to_string(),
        trace: ExtensionRuntimeTraceResponse {
            trace_id: result.trace.trace_id,
            invocation_id: result.trace.invocation_id,
            parent_trace_id: result.trace.parent_trace_id,
            created_at: result.trace.created_at.to_rfc3339(),
        },
        output: ExtensionRuntimeInvocationOutputResponse {
            output: result.output.output,
            metadata: result.output.metadata,
        },
    }
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
            ApiError::Internal(error.to_string())
        }
        ExtensionPackageArtifactUseCaseError::BadRequest(error) => ApiError::BadRequest(error),
        ExtensionPackageArtifactUseCaseError::NotFound(error) => ApiError::NotFound(error),
        ExtensionPackageArtifactUseCaseError::Forbidden(error) => ApiError::Forbidden(error),
        ExtensionPackageArtifactUseCaseError::Conflict(error) => ApiError::Conflict(error),
        ExtensionPackageArtifactUseCaseError::Integrity(error) => ApiError::Internal(error),
    }
}
