//! Project Extension Runtime HTTP 路由。

use std::sync::Arc;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderValue, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use uuid::Uuid;

use crate::agent_run_runtime_surface::resolve_current_runtime_surface_with_backend_for_project_for_api;
use crate::app_state::AppState;
use crate::auth::{CurrentUser, ProjectPermission, load_project_with_permission};
use crate::dto::{ExtensionRuntimeProjectionResponse, extension_runtime_projection_response};
use crate::routes::backend_access::ensure_project_backend_access;
use crate::rpc::ApiError;
use agentdash_application::agent_run::RuntimeSurfaceQueryPurpose;
use agentdash_application::extension_package::{
    ExtensionPackageArtifactUseCaseError, ReadExtensionPackageWebviewAssetInput,
    read_extension_package_webview_asset,
};
use agentdash_application::extension_runtime::{
    UninstallExtensionInstallationInput, extension_runtime_projection_from_installations,
    uninstall_extension_installation,
};
use agentdash_application::runtime_gateway::{
    ExtensionInvocationWorkspaceContext, ExtensionRuntimeChannelConsumer,
    ExtensionRuntimeChannelInvokeRequest, ExtensionRuntimeChannelInvokeResult, RuntimeActionKey,
    RuntimeActor, RuntimeContext, RuntimeInvocationRequest, RuntimeInvocationResult, RuntimeTarget,
    RuntimeTrace, attach_extension_invocation_workspace,
};
use agentdash_contracts::extension_runtime::{
    ExtensionRuntimeInvocationOutputResponse, ExtensionRuntimeInvokeActionRequest,
    ExtensionRuntimeInvokeActionResponse,
    ExtensionRuntimeInvokeChannelRequest as ExtensionRuntimeInvokeChannelRequestDto,
    ExtensionRuntimeInvokeChannelResponse, ExtensionRuntimeTraceResponse,
    UninstallExtensionInstallationResponse,
};
use agentdash_domain::DomainError;
use agentdash_spi::{RuntimeBackendAnchor, Vfs};

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
            "/projects/{project_id}/extension-runtime/invoke-action",
            axum::routing::post(invoke_project_extension_runtime_action),
        )
        .route(
            "/projects/{project_id}/extension-runtime/invoke-channel",
            axum::routing::post(invoke_project_extension_runtime_channel),
        )
        .route(
            "/projects/{project_id}/extension-runtime/webviews/{extension_key}/{*asset_path}",
            axum::routing::get(get_project_extension_webview_asset),
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
        .map_err(ApiError::from)?;
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
    let runtime_surface = resolve_current_runtime_surface_with_backend_for_project_for_api(
        &state,
        &current_user,
        session_id,
        project_id,
        RuntimeSurfaceQueryPurpose::new("extension_runtime"),
        "Extension runtime action",
    )
    .await?;
    let backend_anchor = &runtime_surface.runtime_backend_anchor;
    let backend_id = backend_anchor.backend_id().to_string();
    ensure_project_backend_access(&state, project_id, &backend_id).await?;
    let workspace =
        resolve_extension_invocation_workspace(&runtime_surface.surface.vfs, backend_anchor);

    let action_key = RuntimeActionKey::parse(req.action_key)
        .map_err(|error| ApiError::BadRequest(error.to_string()))?;
    let mut request = RuntimeInvocationRequest::new(
        action_key,
        RuntimeActor::SessionUser {
            session_id: session_id.to_string(),
            user_id: Some(current_user.user_id.clone()),
        },
        RuntimeContext::Session {
            session_id: session_id.to_string(),
            project_id: Some(project_id),
            workspace_id: None,
        },
        req.input,
    );
    request.target = Some(RuntimeTarget::Backend {
        backend_id: backend_id.clone(),
    });
    attach_extension_invocation_workspace(&mut request, workspace);

    let result = state.services.runtime_gateway.invoke(request).await?;
    Ok(Json(extension_runtime_invoke_response(result)))
}

/// POST `/api/projects/:project_id/extension-runtime/invoke-channel`
pub async fn invoke_project_extension_runtime_channel(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<ProjectExtensionRuntimePath>,
    Json(req): Json<ExtensionRuntimeInvokeChannelRequestDto>,
) -> Result<Json<ExtensionRuntimeInvokeChannelResponse>, ApiError> {
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
            "extension channel invoke 缺少 session_id".into(),
        ));
    }
    let runtime_surface = resolve_current_runtime_surface_with_backend_for_project_for_api(
        &state,
        &current_user,
        session_id,
        project_id,
        RuntimeSurfaceQueryPurpose::new("extension_runtime"),
        "Extension runtime channel",
    )
    .await?;
    let backend_anchor = &runtime_surface.runtime_backend_anchor;
    let backend_id = backend_anchor.backend_id().to_string();
    if req.channel_key.trim().is_empty() {
        return Err(ApiError::BadRequest(
            "extension channel invoke 缺少 channel_key".into(),
        ));
    }
    if req.method.trim().is_empty() {
        return Err(ApiError::BadRequest(
            "extension channel invoke 缺少 method".into(),
        ));
    }
    ensure_project_backend_access(&state, project_id, &backend_id).await?;
    let workspace =
        resolve_extension_invocation_workspace(&runtime_surface.surface.vfs, backend_anchor);

    let consumer = req
        .consumer_extension_key
        .as_ref()
        .map(
            |extension_key| ExtensionRuntimeChannelConsumer::ExtensionPanel {
                extension_key: extension_key.trim().to_string(),
            },
        )
        .unwrap_or(ExtensionRuntimeChannelConsumer::SessionUser);
    let result = state
        .services
        .extension_runtime_channel_invoker
        .invoke(ExtensionRuntimeChannelInvokeRequest {
            project_id,
            session_id: session_id.to_string(),
            backend_id,
            workspace,
            consumer,
            channel_key: req.channel_key,
            dependency_alias: req.dependency_alias,
            method: req.method,
            input: req.input,
            trace: RuntimeTrace::new(),
        })
        .await?;
    Ok(Json(extension_runtime_channel_invoke_response(result)))
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

fn resolve_extension_invocation_workspace(
    vfs: &Vfs,
    backend_anchor: &RuntimeBackendAnchor,
) -> Option<ExtensionInvocationWorkspaceContext> {
    select_extension_invocation_workspace(vfs, backend_anchor)
}

fn select_extension_invocation_workspace(
    vfs: &Vfs,
    backend_anchor: &RuntimeBackendAnchor,
) -> Option<ExtensionInvocationWorkspaceContext> {
    if let Some(root_ref) = backend_anchor
        .root_ref
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return vfs
            .mounts
            .iter()
            .find(|mount| mount.root_ref.trim() == root_ref && !mount.root_ref.trim().is_empty())
            .map(|mount| {
                ExtensionInvocationWorkspaceContext::new(
                    mount.id.clone(),
                    mount.root_ref.trim().to_string(),
                )
            });
    }
    vfs.default_mount()
        .filter(|mount| !mount.root_ref.trim().is_empty())
        .map(|mount| {
            ExtensionInvocationWorkspaceContext::new(
                mount.id.clone(),
                mount.root_ref.trim().to_string(),
            )
        })
}

#[cfg(test)]
mod tests {
    use agentdash_domain::common::{Mount, MountCapability};
    use agentdash_spi::RuntimeBackendAnchorSource;

    use super::*;

    #[test]
    fn extension_invocation_workspace_uses_backend_default_mount() {
        let vfs = Vfs {
            mounts: vec![mount("main", "backend-1", "D:/Workspaces/main")],
            default_mount_id: Some("main".to_string()),
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        };

        let workspace = select_extension_invocation_workspace(
            &vfs,
            &anchor("backend-1", Some("D:/Workspaces/main")),
        )
        .expect("workspace");

        assert_eq!(workspace.mount_id, "main");
        assert_eq!(workspace.root_ref, "D:/Workspaces/main");
    }

    #[test]
    fn extension_invocation_workspace_falls_back_to_matching_backend_mount() {
        let vfs = Vfs {
            mounts: vec![
                mount("main", "backend-2", "D:/Workspaces/other"),
                mount("local", "backend-1", "D:/Workspaces/local"),
            ],
            default_mount_id: Some("main".to_string()),
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        };

        let workspace = select_extension_invocation_workspace(
            &vfs,
            &anchor("backend-1", Some("D:/Workspaces/local")),
        )
        .expect("workspace");

        assert_eq!(workspace.mount_id, "local");
        assert_eq!(workspace.root_ref, "D:/Workspaces/local");
    }

    #[test]
    fn extension_invocation_workspace_returns_none_when_anchor_root_missing() {
        let vfs = Vfs {
            mounts: vec![mount("main", "backend-2", "D:/Workspaces/other")],
            default_mount_id: Some("main".to_string()),
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        };

        assert!(
            select_extension_invocation_workspace(
                &vfs,
                &anchor("backend-1", Some("D:/Workspaces/missing"))
            )
            .is_none()
        );
    }

    #[test]
    fn extension_invocation_workspace_uses_default_mount_when_anchor_has_no_root() {
        let vfs = Vfs {
            mounts: vec![mount("main", "backend-2", "D:/Workspaces/other")],
            default_mount_id: Some("main".to_string()),
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        };

        let workspace = select_extension_invocation_workspace(&vfs, &anchor("backend-1", None))
            .expect("workspace");

        assert_eq!(workspace.mount_id, "main");
        assert_eq!(workspace.root_ref, "D:/Workspaces/other");
    }

    fn anchor(backend_id: &str, root_ref: Option<&str>) -> RuntimeBackendAnchor {
        RuntimeBackendAnchor::new(backend_id, RuntimeBackendAnchorSource::System)
            .expect("anchor")
            .with_root_ref(root_ref.map(ToString::to_string))
    }

    fn mount(id: &str, backend_id: &str, root_ref: &str) -> Mount {
        Mount {
            id: id.to_string(),
            provider: "relay_fs".to_string(),
            backend_id: backend_id.to_string(),
            root_ref: root_ref.to_string(),
            capabilities: vec![MountCapability::Read, MountCapability::Write],
            default_write: true,
            display_name: id.to_string(),
            metadata: serde_json::Value::Null,
        }
    }
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

fn extension_runtime_channel_invoke_response(
    result: ExtensionRuntimeChannelInvokeResult,
) -> ExtensionRuntimeInvokeChannelResponse {
    ExtensionRuntimeInvokeChannelResponse {
        channel_key: result.channel_key,
        method: result.method,
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
            tracing::error!(error = %error, "extension package artifact storage error");
            ApiError::Internal(String::from("扩展包存储错误"))
        }
        ExtensionPackageArtifactUseCaseError::BadRequest(error) => ApiError::BadRequest(error),
        ExtensionPackageArtifactUseCaseError::NotFound(error) => ApiError::NotFound(error),
        ExtensionPackageArtifactUseCaseError::Forbidden(error) => ApiError::Forbidden(error),
        ExtensionPackageArtifactUseCaseError::Conflict(error) => ApiError::Conflict(error),
        ExtensionPackageArtifactUseCaseError::Integrity(error) => ApiError::Internal(error),
    }
}
