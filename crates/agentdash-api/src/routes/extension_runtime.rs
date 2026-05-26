//! Project Extension Runtime HTTP 路由。

use std::path::{Component, Path as StdPath};
use std::sync::Arc;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderValue, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use uuid::Uuid;

use agentdash_application::extension_package::{digest_bytes, read_extension_package_archive_file};
use agentdash_application::extension_runtime::extension_runtime_projection_from_installations;
use agentdash_application::runtime_gateway::{
    RuntimeActionKey, RuntimeActor, RuntimeContext, RuntimeInvocationRequest,
    RuntimeInvocationResult, RuntimeTarget,
};
use agentdash_contracts::extension_runtime::{
    ExtensionRuntimeInvocationOutputResponse, ExtensionRuntimeInvokeActionRequest,
    ExtensionRuntimeInvokeActionResponse, ExtensionRuntimeTraceResponse,
};
use agentdash_domain::session_binding::SessionBinding;
use agentdash_domain::shared_library::{
    ExtensionTemplatePayload, ExtensionWorkspaceTabRendererDeclaration,
};

use crate::app_state::AppState;
use crate::auth::{CurrentUser, ProjectPermission, load_project_with_permission};
use crate::dto::{ExtensionRuntimeProjectionResponse, extension_runtime_projection_response};
use crate::routes::backend_access::ensure_project_backend_access;
use crate::routes::extension_package_artifacts::read_storage_object;
use crate::rpc::ApiError;

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
    let asset_path = normalize_webview_asset_path(&path.asset_path)?;
    let installation = state
        .repos
        .project_extension_installation_repo
        .get_by_project_and_key(project_id, &path.extension_key)
        .await?
        .ok_or_else(|| ApiError::NotFound("Extension installation 不存在".into()))?;
    if !installation.enabled {
        return Err(ApiError::NotFound("Extension installation 不存在".into()));
    }
    if !webview_asset_allowed(&installation.manifest, &asset_path) {
        return Err(ApiError::Forbidden(
            "Extension webview asset 不属于已声明 panel 目录".into(),
        ));
    }
    let artifact = installation
        .package_artifact
        .ok_or_else(|| ApiError::Conflict("Extension webview 需要 packaged artifact".into()))?;
    let archive_bytes = read_storage_object(&artifact.storage_ref).await?;
    let actual_digest = digest_bytes(&archive_bytes);
    if actual_digest != artifact.archive_digest {
        return Err(ApiError::Internal(format!(
            "extension package artifact 存储 digest 不匹配: expected {}, actual {}",
            artifact.archive_digest, actual_digest
        )));
    }
    let bytes = read_extension_package_archive_file(&archive_bytes, &asset_path)?
        .ok_or_else(|| ApiError::NotFound("Extension webview asset 不存在".into()))?;
    let content_type = HeaderValue::from_static(content_type_for_path(&asset_path));
    Ok((
        [
            (header::CONTENT_TYPE, content_type),
            (header::CACHE_CONTROL, HeaderValue::from_static("no-store")),
        ],
        Bytes::from(bytes),
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

fn normalize_webview_asset_path(raw: &str) -> Result<String, ApiError> {
    let mut parts = Vec::new();
    for component in StdPath::new(raw).components() {
        match component {
            Component::Normal(part) => {
                let Some(value) = part.to_str() else {
                    return Err(ApiError::BadRequest(
                        "Extension webview asset path 必须是 UTF-8".into(),
                    ));
                };
                if !value.is_empty() {
                    parts.push(value.to_string());
                }
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(ApiError::BadRequest(
                    "Extension webview asset path 非法".into(),
                ));
            }
        }
    }
    if parts.is_empty() {
        return Err(ApiError::BadRequest(
            "Extension webview asset path 不能为空".into(),
        ));
    }
    Ok(parts.join("/"))
}

fn webview_asset_allowed(manifest: &ExtensionTemplatePayload, asset_path: &str) -> bool {
    manifest.workspace_tabs.iter().any(|tab| {
        let entry = workspace_tab_renderer_entry(&tab.renderer);
        let Ok(entry_path) = normalize_webview_asset_path(entry) else {
            return false;
        };
        if asset_path == entry_path {
            return true;
        }
        let Some((dir, _file)) = entry_path.rsplit_once('/') else {
            return false;
        };
        asset_path.starts_with(&format!("{dir}/"))
    })
}

fn workspace_tab_renderer_entry(renderer: &ExtensionWorkspaceTabRendererDeclaration) -> &str {
    match renderer {
        ExtensionWorkspaceTabRendererDeclaration::Webview { entry }
        | ExtensionWorkspaceTabRendererDeclaration::CanvasPanel { entry } => entry,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webview_asset_path_rejects_traversal() {
        let err = normalize_webview_asset_path("../dist/panel/index.html")
            .expect_err("traversal should fail");
        assert!(err.to_string().contains("path 非法"));
    }

    #[test]
    fn webview_asset_allows_declared_panel_directory() {
        let manifest = serde_json::from_value::<ExtensionTemplatePayload>(serde_json::json!({
            "manifest_version": "2",
            "extension_id": "local-hello",
            "package": { "name": "@agentdash/local-hello", "version": "0.1.0" },
            "asset_version": "0.1.0",
            "workspace_tabs": [{
                "type_id": "local-hello.panel",
                "label": "Hello",
                "uri_scheme": "local-hello",
                "renderer": { "kind": "webview", "entry": "dist/panel/index.html" }
            }]
        }))
        .expect("manifest");

        assert!(webview_asset_allowed(&manifest, "dist/panel/index.html"));
        assert!(webview_asset_allowed(&manifest, "dist/panel/app.js"));
        assert!(!webview_asset_allowed(&manifest, "dist/extension.js"));
    }
}
