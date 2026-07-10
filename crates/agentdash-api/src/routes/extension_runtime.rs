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

use crate::agent_run_runtime_surface::resolve_current_runtime_surface_with_backend_for_agent_run_for_api;
use crate::app_state::AppState;
use crate::auth::{CurrentUser, ProjectPermission, load_project_with_permission};
use crate::dto::{ExtensionRuntimeProjectionResponse, extension_runtime_projection_response};
use crate::routes::backend_access::ensure_project_backend_access;
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
use agentdash_application_agentrun::agent_run::RuntimeSurfaceQueryPurpose;
use agentdash_application_ports::extension_runtime::{
    ExtensionBackendServiceInvokeRequest as BackendServiceTransportRequest,
    ExtensionBackendServiceInvokeResponse as BackendServiceTransportResponse,
    ExtensionBackendServiceReadinessPayload, ExtensionBackendServiceTransport,
    ExtensionInvocationWorkspacePayload as BackendServiceWorkspacePayload,
    ExtensionPackageArtifactPayload as BackendServicePackageArtifactPayload,
    ExtensionRuntimeActionTransportError,
};
use agentdash_application_runtime_gateway::{
    ExtensionRuntimeProtocolConsumer, ExtensionRuntimeProtocolInvokeRequest,
    ExtensionRuntimeProtocolInvokeResult, RuntimeActionKey, RuntimeActor, RuntimeContext,
    RuntimeInvocationRequest, RuntimeInvocationResult, RuntimeTarget, RuntimeTrace,
    attach_extension_invocation_workspace, resolve_extension_invocation_workspace,
};
use agentdash_contracts::extension_runtime::{
    ExtensionBackendServiceDiagnosticResponse, ExtensionBackendServiceHttpResponse,
    ExtensionBackendServiceInvokeMetadataResponse, ExtensionBackendServiceReadinessResponse,
    ExtensionRuntimeInvocationOutputResponse, ExtensionRuntimeInvokeActionRequest,
    ExtensionRuntimeInvokeActionResponse, ExtensionRuntimeInvokeBackendServiceRequest,
    ExtensionRuntimeInvokeBackendServiceResponse,
    ExtensionRuntimeInvokeProtocolRequest as ExtensionRuntimeInvokeProtocolRequestDto,
    ExtensionRuntimeInvokeProtocolResponse, ExtensionRuntimeTraceResponse,
    UninstallExtensionInstallationResponse,
};
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
            "/agent-runs/{run_id}/agents/{agent_id}/extension-runtime/invoke-action",
            axum::routing::post(invoke_agent_run_extension_runtime_action),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/extension-runtime/invoke-protocol",
            axum::routing::post(invoke_agent_run_extension_runtime_channel),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/extension-runtime/invoke-backend-service",
            axum::routing::post(invoke_agent_run_extension_backend_service),
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
pub struct AgentRunExtensionRuntimePath {
    pub run_id: String,
    pub agent_id: String,
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

/// POST `/api/agent-runs/:run_id/agents/:agent_id/extension-runtime/invoke-action`
pub async fn invoke_agent_run_extension_runtime_action(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<AgentRunExtensionRuntimePath>,
    Json(req): Json<ExtensionRuntimeInvokeActionRequest>,
) -> Result<Json<ExtensionRuntimeInvokeActionResponse>, ApiError> {
    let runtime_surface = resolve_current_runtime_surface_with_backend_for_agent_run_for_api(
        &state,
        &current_user,
        &path.run_id,
        &path.agent_id,
        ProjectPermission::Use,
        RuntimeSurfaceQueryPurpose::new("extension_runtime"),
        "Extension runtime action",
    )
    .await?;
    let project_id = runtime_surface.project_id;
    let session_id = runtime_surface.runtime_session_id.clone();
    let backend_anchor = &runtime_surface.runtime_backend_anchor;
    let backend_id = backend_anchor.backend_id().to_string();
    ensure_project_backend_access(&state, project_id, &backend_id).await?;
    let workspace =
        resolve_extension_invocation_workspace(&runtime_surface.surface.vfs, backend_anchor)
            .into_workspace();

    let action_key = RuntimeActionKey::parse(req.action_key)
        .map_err(|error| ApiError::BadRequest(error.to_string()))?;
    let mut request = RuntimeInvocationRequest::new(
        action_key,
        RuntimeActor::SessionUser {
            session_id: session_id.clone(),
            user_id: Some(current_user.user_id.clone()),
        },
        RuntimeContext::Session {
            session_id,
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

/// POST `/api/agent-runs/:run_id/agents/:agent_id/extension-runtime/invoke-protocol`
pub async fn invoke_agent_run_extension_runtime_channel(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<AgentRunExtensionRuntimePath>,
    Json(req): Json<ExtensionRuntimeInvokeProtocolRequestDto>,
) -> Result<Json<ExtensionRuntimeInvokeProtocolResponse>, ApiError> {
    let runtime_surface = resolve_current_runtime_surface_with_backend_for_agent_run_for_api(
        &state,
        &current_user,
        &path.run_id,
        &path.agent_id,
        ProjectPermission::Use,
        RuntimeSurfaceQueryPurpose::new("extension_runtime"),
        "Extension runtime channel",
    )
    .await?;
    let project_id = runtime_surface.project_id;
    let session_id = runtime_surface.runtime_session_id.clone();
    let backend_anchor = &runtime_surface.runtime_backend_anchor;
    let backend_id = backend_anchor.backend_id().to_string();
    if req.protocol_key.trim().is_empty() {
        return Err(ApiError::BadRequest(
            "extension protocol invoke 缺少 protocol_key".into(),
        ));
    }
    if req.method.trim().is_empty() {
        return Err(ApiError::BadRequest(
            "extension protocol invoke 缺少 method".into(),
        ));
    }
    ensure_project_backend_access(&state, project_id, &backend_id).await?;
    let workspace =
        resolve_extension_invocation_workspace(&runtime_surface.surface.vfs, backend_anchor)
            .into_workspace();

    let consumer = req
        .consumer_extension_key
        .as_ref()
        .map(
            |extension_key| ExtensionRuntimeProtocolConsumer::ExtensionPanel {
                extension_key: extension_key.trim().to_string(),
            },
        )
        .unwrap_or(ExtensionRuntimeProtocolConsumer::SessionUser);
    let result = state
        .services
        .extension_runtime_protocol_invoker
        .invoke(ExtensionRuntimeProtocolInvokeRequest {
            project_id,
            session_id,
            backend_id,
            workspace,
            consumer,
            provider_extension_key: req.provider_extension_key,
            protocol_key: req.protocol_key,
            protocol_version: req.protocol_version,
            dependency_alias: req.dependency_alias,
            method: req.method,
            input: req.input,
            trace: RuntimeTrace::new(),
        })
        .await?;
    Ok(Json(extension_runtime_channel_invoke_response(result)))
}

/// POST `/api/agent-runs/:run_id/agents/:agent_id/extension-runtime/invoke-backend-service`
pub async fn invoke_agent_run_extension_backend_service(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<AgentRunExtensionRuntimePath>,
    Json(req): Json<ExtensionRuntimeInvokeBackendServiceRequest>,
) -> Result<Json<ExtensionRuntimeInvokeBackendServiceResponse>, ApiError> {
    let runtime_surface = resolve_current_runtime_surface_with_backend_for_agent_run_for_api(
        &state,
        &current_user,
        &path.run_id,
        &path.agent_id,
        ProjectPermission::Use,
        RuntimeSurfaceQueryPurpose::new("extension_backend_service"),
        "Extension backend service",
    )
    .await?;
    let project_id = runtime_surface.project_id;
    let session_id = runtime_surface.runtime_session_id.clone();
    let backend_anchor = &runtime_surface.runtime_backend_anchor;
    let backend_id = backend_anchor.backend_id().to_string();
    ensure_project_backend_access(&state, project_id, &backend_id).await?;

    let extension_key = require_non_empty(req.extension_key, "extension_key")?;
    let service_key = require_non_empty(req.service_key, "service_key")?;
    let route = require_non_empty(req.route, "route")?;
    let method = require_non_empty(req.method, "method")?.to_ascii_uppercase();
    let trace = RuntimeTrace::new();

    let installations = state
        .repos
        .project_extension_installation_repo
        .list_enabled_by_project(project_id)
        .await
        .map_err(ApiError::from)?;
    let installation = installations
        .into_iter()
        .find(|installation| installation.extension_key == extension_key)
        .ok_or_else(|| ApiError::NotFound("Extension installation 不存在".into()))?;
    let extension_id = installation.manifest.extension_id.clone();
    let service = installation
        .manifest
        .backend_services
        .iter()
        .find(|service| service.service_key == service_key)
        .ok_or_else(|| ApiError::NotFound("Extension backend service 不存在".into()))?;
    if !backend_service_route_matches(&service.routes, &route) {
        return Err(ApiError::BadRequest(format!(
            "route `{route}` 不在 backend service `{service_key}` 声明范围内"
        )));
    }

    let workspace =
        resolve_extension_invocation_workspace(&runtime_surface.surface.vfs, backend_anchor)
            .into_workspace()
            .map(|workspace| BackendServiceWorkspacePayload {
                mount_id: workspace.mount_id,
                root_ref: workspace.root_ref,
            });

    let metadata = ExtensionBackendServiceInvokeMetadataResponse {
        project_id: project_id.to_string(),
        backend_id: backend_id.clone(),
        extension_key: extension_key.clone(),
        extension_id: extension_id.clone(),
        service_key: service_key.clone(),
        route: route.clone(),
        trace_id: trace.trace_id.clone(),
        invocation_id: trace.invocation_id.clone(),
    };

    let Some(artifact) = installation.package_artifact.as_ref() else {
        return Ok(Json(ExtensionRuntimeInvokeBackendServiceResponse {
            trace: extension_runtime_trace_response(&trace),
            metadata,
            response: None,
            diagnostic: Some(ExtensionBackendServiceDiagnosticResponse {
                readiness: ExtensionBackendServiceReadinessResponse::MissingArtifact,
                code: "missing_artifact".to_string(),
                message: "Extension backend service 缺少 package artifact".to_string(),
                retryable: false,
                details: Some(serde_json::json!({
                    "extension_key": extension_key,
                    "service_key": service_key,
                })),
            }),
        }));
    };

    let output = state
        .services
        .backend_registry
        .invoke_extension_backend_service(
            &backend_id,
            BackendServiceTransportRequest {
                extension_key,
                extension_id,
                service_key,
                route,
                project_id: project_id.to_string(),
                session_id,
                method,
                headers: req.headers,
                body: req.body,
                package_artifact: BackendServicePackageArtifactPayload {
                    artifact_id: artifact.artifact_id.to_string(),
                    archive_digest: artifact.archive_digest.clone(),
                },
                workspace,
                trace_id: trace.trace_id.clone(),
                invocation_id: trace.invocation_id.clone(),
            },
        )
        .await
        .map_err(extension_backend_service_transport_error_to_api)?;

    Ok(Json(extension_backend_service_invoke_response(
        trace, output,
    )))
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

fn require_non_empty(raw: String, field: &str) -> Result<String, ApiError> {
    let value = raw.trim().to_string();
    if value.is_empty() {
        Err(ApiError::BadRequest(format!("{field} 不能为空")))
    } else {
        Ok(value)
    }
}

fn backend_service_route_matches(patterns: &[String], route: &str) -> bool {
    let route = strip_query(route.trim());
    patterns.iter().any(|pattern| {
        let pattern = route_pattern_path(pattern);
        if pattern == route {
            return true;
        }
        if let Some(prefix) = pattern.strip_suffix("/**") {
            return route == prefix || route.starts_with(&format!("{prefix}/"));
        }
        if let Some(prefix) = pattern.strip_suffix('*') {
            return route.starts_with(prefix);
        }
        false
    })
}

fn route_pattern_path(pattern: &str) -> &str {
    let pattern = strip_query(pattern.trim());
    let Some(rest) = pattern
        .strip_prefix("http://")
        .or_else(|| pattern.strip_prefix("https://"))
    else {
        return pattern;
    };
    rest.find('/').map_or("/", |index| &rest[index..])
}

fn strip_query(value: &str) -> &str {
    value.split_once('?').map_or(value, |(path, _)| path)
}

fn extension_runtime_trace_response(trace: &RuntimeTrace) -> ExtensionRuntimeTraceResponse {
    ExtensionRuntimeTraceResponse {
        trace_id: trace.trace_id.clone(),
        invocation_id: trace.invocation_id.clone(),
        parent_trace_id: trace.parent_trace_id.clone(),
        created_at: trace.created_at.to_rfc3339(),
    }
}

fn extension_runtime_invoke_response(
    result: RuntimeInvocationResult,
) -> ExtensionRuntimeInvokeActionResponse {
    ExtensionRuntimeInvokeActionResponse {
        action_key: result.action_key.to_string(),
        trace: extension_runtime_trace_response(&result.trace),
        output: ExtensionRuntimeInvocationOutputResponse {
            output: result.output.output,
            metadata: result.output.metadata,
        },
    }
}

fn extension_runtime_channel_invoke_response(
    result: ExtensionRuntimeProtocolInvokeResult,
) -> ExtensionRuntimeInvokeProtocolResponse {
    ExtensionRuntimeInvokeProtocolResponse {
        provider_extension_key: result.provider_extension_key,
        provider_extension_id: result.provider_extension_id,
        protocol_key: result.protocol_key,
        protocol_version: result.protocol_version,
        method: result.method,
        trace: extension_runtime_trace_response(&result.trace),
        output: ExtensionRuntimeInvocationOutputResponse {
            output: result.output.output,
            metadata: result.output.metadata,
        },
    }
}

fn extension_backend_service_invoke_response(
    trace: RuntimeTrace,
    result: BackendServiceTransportResponse,
) -> ExtensionRuntimeInvokeBackendServiceResponse {
    ExtensionRuntimeInvokeBackendServiceResponse {
        trace: extension_runtime_trace_response(&trace),
        metadata: ExtensionBackendServiceInvokeMetadataResponse {
            project_id: result.metadata.project_id,
            backend_id: result.metadata.backend_id,
            extension_key: result.metadata.extension_key,
            extension_id: result.metadata.extension_id,
            service_key: result.metadata.service_key,
            route: result.metadata.route,
            trace_id: result.metadata.trace_id,
            invocation_id: result.metadata.invocation_id,
        },
        response: result
            .response
            .map(|response| ExtensionBackendServiceHttpResponse {
                status: response.status,
                headers: response.headers,
                body: response.body,
            }),
        diagnostic: result
            .diagnostic
            .map(|diagnostic| ExtensionBackendServiceDiagnosticResponse {
                readiness: backend_service_readiness_response(diagnostic.readiness),
                code: diagnostic.code,
                message: diagnostic.message,
                retryable: diagnostic.retryable,
                details: diagnostic.details,
            }),
    }
}

fn backend_service_readiness_response(
    readiness: ExtensionBackendServiceReadinessPayload,
) -> ExtensionBackendServiceReadinessResponse {
    match readiness {
        ExtensionBackendServiceReadinessPayload::Ready => {
            ExtensionBackendServiceReadinessResponse::Ready
        }
        ExtensionBackendServiceReadinessPayload::MissingArtifact => {
            ExtensionBackendServiceReadinessResponse::MissingArtifact
        }
        ExtensionBackendServiceReadinessPayload::MaterializeFailed => {
            ExtensionBackendServiceReadinessResponse::MaterializeFailed
        }
        ExtensionBackendServiceReadinessPayload::Starting => {
            ExtensionBackendServiceReadinessResponse::Starting
        }
        ExtensionBackendServiceReadinessPayload::HealthFailed => {
            ExtensionBackendServiceReadinessResponse::HealthFailed
        }
        ExtensionBackendServiceReadinessPayload::ProcessExited => {
            ExtensionBackendServiceReadinessResponse::ProcessExited
        }
        ExtensionBackendServiceReadinessPayload::UnsupportedRuntime => {
            ExtensionBackendServiceReadinessResponse::UnsupportedRuntime
        }
        ExtensionBackendServiceReadinessPayload::ServiceUnavailable => {
            ExtensionBackendServiceReadinessResponse::ServiceUnavailable
        }
    }
}

fn extension_backend_service_transport_error_to_api(
    error: ExtensionRuntimeActionTransportError,
) -> ApiError {
    match error {
        ExtensionRuntimeActionTransportError::Offline { backend_id } => {
            ApiError::ServiceUnavailable(format!("目标 Backend 当前不在线: {backend_id}"))
        }
        ExtensionRuntimeActionTransportError::Timeout { backend_id } => {
            ApiError::ServiceUnavailable(format!(
                "等待 Backend backendService response 超时: {backend_id}"
            ))
        }
        ExtensionRuntimeActionTransportError::ResponseDropped { backend_id } => {
            ApiError::ServiceUnavailable(format!(
                "Backend backendService response 通道已断开: {backend_id}"
            ))
        }
        ExtensionRuntimeActionTransportError::Failed(message) => {
            ApiError::ServiceUnavailable(message)
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_application_ports::extension_runtime::{
        ExtensionBackendServiceHttpResponsePayload, ExtensionBackendServiceInvokeMetadataPayload,
    };
    use std::collections::BTreeMap;

    #[test]
    fn backend_service_route_patterns_match_declared_routes() {
        let routes = vec![
            "/api/**".to_string(),
            "/health".to_string(),
            "/assets/*".to_string(),
        ];

        assert!(backend_service_route_matches(&routes, "/api"));
        assert!(backend_service_route_matches(&routes, "/api/search"));
        assert!(backend_service_route_matches(&routes, "/health"));
        assert!(backend_service_route_matches(&routes, "/assets/app.js"));
        assert!(!backend_service_route_matches(&routes, "/private/search"));

        let absolute_routes = vec!["http://localhost:4510/api/**".to_string()];
        assert!(backend_service_route_matches(
            &absolute_routes,
            "/api/search?q=abc"
        ));
        assert!(!backend_service_route_matches(
            &absolute_routes,
            "/other/search"
        ));
    }

    #[test]
    fn backend_service_transport_response_maps_metadata_and_diagnostic() {
        let trace = RuntimeTrace::new();
        let result = BackendServiceTransportResponse {
            metadata: ExtensionBackendServiceInvokeMetadataPayload {
                project_id: "project-1".to_string(),
                backend_id: "backend-1".to_string(),
                extension_key: "local-webapp".to_string(),
                extension_id: "local-webapp".to_string(),
                service_key: "local-webapp.api".to_string(),
                route: "/api/search".to_string(),
                trace_id: trace.trace_id.clone(),
                invocation_id: trace.invocation_id.clone(),
            },
            response: Some(ExtensionBackendServiceHttpResponsePayload {
                status: 204,
                headers: BTreeMap::new(),
                body: None,
            }),
            diagnostic: Some(
                agentdash_application_ports::extension_runtime::ExtensionBackendServiceInvokeDiagnosticPayload {
                    readiness: ExtensionBackendServiceReadinessPayload::ServiceUnavailable,
                    code: "service_unavailable".to_string(),
                    message: "service unavailable".to_string(),
                    retryable: true,
                    details: None,
                },
            ),
        };

        let response = extension_backend_service_invoke_response(trace, result);

        assert_eq!(response.metadata.backend_id, "backend-1");
        assert_eq!(response.metadata.service_key, "local-webapp.api");
        assert_eq!(response.response.expect("http response").status, 204);
        assert_eq!(
            response.diagnostic.expect("diagnostic").readiness,
            ExtensionBackendServiceReadinessResponse::ServiceUnavailable
        );
    }
}
