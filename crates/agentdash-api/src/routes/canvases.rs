use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

use agentdash_application::canvas::{
    CanvasExtensionPackageInput, CanvasMutationInput, CanvasRuntimeBridgeSnapshot,
    apply_canvas_mutation, build_canvas, build_canvas_extension_package,
    build_runtime_snapshot_with_bindings,
};
use agentdash_application::extension_package::{
    ExtensionPackageArtifactUseCaseError, InstallExtensionPackageArtifactInput,
    StoreExtensionPackageArchiveInput, install_extension_package_artifact,
    store_extension_package_archive,
};
use agentdash_application::runtime_gateway::{
    RuntimeActionKey, RuntimeActor, RuntimeContext, RuntimeInvocationRequest,
    RuntimeInvocationResult,
};
use agentdash_contracts::extension_package::ExtensionPackageInstallationResponse;
use agentdash_domain::canvas::{CanvasDataBinding, CanvasFile, CanvasSandboxConfig};
use agentdash_domain::session_binding::{SessionBinding, SessionOwnerType};

use crate::app_state::AppState;
use crate::auth::{CurrentUser, ProjectPermission, load_project_with_permission};
use crate::dto::CanvasResponse;
use crate::rpc::ApiError;
use crate::session_use_cases::context_query::build_session_context_plan;

#[derive(Debug, Deserialize)]
pub struct ListProjectCanvasesPath {
    pub project_id: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateCanvasRequest {
    pub mount_id: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub entry_file: Option<String>,
    pub sandbox_config: Option<CanvasSandboxConfig>,
    pub files: Option<Vec<CanvasFile>>,
    pub bindings: Option<Vec<CanvasDataBinding>>,
}

#[derive(Debug, Deserialize, Default)]
pub struct UpdateCanvasRequest {
    pub title: Option<String>,
    pub description: Option<String>,
    pub entry_file: Option<String>,
    pub sandbox_config: Option<CanvasSandboxConfig>,
    pub files: Option<Vec<CanvasFile>>,
    pub bindings: Option<Vec<CanvasDataBinding>>,
}

#[derive(Debug, Deserialize, Default)]
pub struct CanvasRuntimeSnapshotQuery {
    pub session_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CanvasRuntimeInvokeRequest {
    pub session_id: String,
    pub action_key: String,
    #[serde(default)]
    pub input: Value,
}

#[derive(Debug, Deserialize)]
pub struct PromoteCanvasToExtensionRequest {
    pub extension_key: Option<String>,
    pub display_name: Option<String>,
    pub package_version: Option<String>,
    pub asset_version: Option<String>,
    #[serde(default = "default_promote_overwrite")]
    pub overwrite: bool,
}

pub async fn list_project_canvases(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<ListProjectCanvasesPath>,
) -> Result<Json<Vec<CanvasResponse>>, ApiError> {
    let project_id = parse_project_id(&path.project_id)?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::View,
    )
    .await?;

    let canvases = state.repos.canvas_repo.list_by_project(project_id).await?;
    Ok(Json(
        canvases.into_iter().map(CanvasResponse::from).collect(),
    ))
}

pub async fn create_canvas(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<ListProjectCanvasesPath>,
    Json(req): Json<CreateCanvasRequest>,
) -> Result<Json<CanvasResponse>, ApiError> {
    let project_id = parse_project_id(&path.project_id)?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Edit,
    )
    .await?;

    let title = req.title.trim();
    if title.is_empty() {
        return Err(ApiError::BadRequest("Canvas 标题不能为空".into()));
    }

    let canvas = build_canvas(
        project_id,
        req.mount_id,
        title.to_string(),
        req.description.unwrap_or_default(),
        CanvasMutationInput {
            entry_file: req.entry_file,
            sandbox_config: req.sandbox_config,
            files: req.files,
            bindings: req.bindings,
            ..CanvasMutationInput::default()
        },
    )?;
    state.repos.canvas_repo.create(&canvas).await?;

    Ok(Json(CanvasResponse::from(canvas)))
}

pub async fn get_canvas(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<CanvasResponse>, ApiError> {
    let canvas =
        load_canvas_with_permission(state.as_ref(), &current_user, &id, ProjectPermission::View)
            .await?;

    Ok(Json(CanvasResponse::from(canvas)))
}

pub async fn update_canvas(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<UpdateCanvasRequest>,
) -> Result<Json<CanvasResponse>, ApiError> {
    let mut canvas =
        load_canvas_with_permission(state.as_ref(), &current_user, &id, ProjectPermission::Edit)
            .await?;

    apply_canvas_mutation(
        &mut canvas,
        CanvasMutationInput {
            title: req.title,
            description: req.description,
            entry_file: req.entry_file,
            sandbox_config: req.sandbox_config,
            files: req.files,
            bindings: req.bindings,
        },
    )?;
    state.repos.canvas_repo.update(&canvas).await?;

    Ok(Json(CanvasResponse::from(canvas)))
}

pub async fn delete_canvas(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let canvas =
        load_canvas_with_permission(state.as_ref(), &current_user, &id, ProjectPermission::Edit)
            .await?;
    state.repos.canvas_repo.delete(canvas.id).await?;

    Ok(Json(serde_json::json!({ "deleted": id })))
}

pub async fn promote_canvas_to_extension(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<PromoteCanvasToExtensionRequest>,
) -> Result<Json<ExtensionPackageInstallationResponse>, ApiError> {
    let canvas =
        load_canvas_with_permission(state.as_ref(), &current_user, &id, ProjectPermission::Edit)
            .await?;
    let package = build_canvas_extension_package(
        &canvas,
        CanvasExtensionPackageInput {
            package_version: req.package_version,
            asset_version: req.asset_version,
        },
    )?;
    let artifact = store_extension_package_archive(
        &state.repos,
        state.services.extension_package_artifact_storage.as_ref(),
        StoreExtensionPackageArchiveInput {
            project_id: canvas.project_id,
            archive_bytes: package.archive_bytes,
            expected_archive_digest: Some(package.archive_digest),
        },
    )
    .await
    .map_err(extension_package_error_to_api)?;
    let installation = install_extension_package_artifact(
        &state.repos,
        InstallExtensionPackageArtifactInput {
            project_id: canvas.project_id,
            artifact_id: artifact.id,
            extension_key: req.extension_key,
            display_name: req
                .display_name
                .or_else(|| Some(canvas.title.trim().to_string())),
            overwrite: req.overwrite,
        },
    )
    .await?;
    let artifact = installation.package_artifact.ok_or_else(|| {
        ApiError::Internal("Canvas promoted extension installation 缺少 artifact 引用".into())
    })?;

    Ok(Json(ExtensionPackageInstallationResponse {
        installation_id: installation.id.to_string(),
        extension_key: installation.extension_key,
        extension_id: installation.manifest.extension_id,
        package_artifact_id: artifact.artifact_id.to_string(),
        archive_digest: artifact.archive_digest,
    }))
}

pub async fn get_canvas_runtime_snapshot(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
    Query(query): Query<CanvasRuntimeSnapshotQuery>,
) -> Result<Json<agentdash_application::canvas::CanvasRuntimeSnapshot>, ApiError> {
    let canvas =
        load_canvas_with_permission(state.as_ref(), &current_user, &id, ProjectPermission::View)
            .await?;

    let vfs = resolve_canvas_runtime_vfs(
        &state,
        &current_user,
        query.session_id.as_deref(),
        canvas.project_id,
    )
    .await?;
    let mut snapshot = build_runtime_snapshot_with_bindings(
        &canvas,
        query.session_id.clone(),
        vfs.as_ref(),
        state.services.vfs_service.as_ref(),
    )
    .await;
    if let Some(session_id) = query.session_id.as_deref() {
        snapshot.runtime_bridge =
            build_canvas_runtime_bridge_surface(state.as_ref(), &canvas, session_id)?;
    }

    Ok(Json(snapshot))
}

pub async fn invoke_canvas_runtime_action(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<CanvasRuntimeInvokeRequest>,
) -> Result<Json<RuntimeInvocationResult>, ApiError> {
    let canvas =
        load_canvas_with_permission(state.as_ref(), &current_user, &id, ProjectPermission::View)
            .await?;
    let session_id = req.session_id.trim();
    if session_id.is_empty() {
        return Err(ApiError::BadRequest(
            "Canvas runtime invoke 缺少 session_id".to_string(),
        ));
    }

    ensure_canvas_session_scope(state.as_ref(), session_id, canvas.project_id).await?;

    let action_key = RuntimeActionKey::parse(req.action_key)
        .map_err(|error| ApiError::BadRequest(error.to_string()))?;
    let request = RuntimeInvocationRequest::new(
        action_key,
        RuntimeActor::UserCanvas {
            session_id: session_id.to_string(),
            canvas_id: Some(canvas.id),
        },
        RuntimeContext::Session {
            session_id: session_id.to_string(),
            project_id: Some(canvas.project_id),
            workspace_id: None,
        },
        req.input,
    );

    let result = state.services.runtime_gateway.invoke(request).await?;
    Ok(Json(result))
}

fn build_canvas_runtime_bridge_surface(
    state: &AppState,
    canvas: &agentdash_domain::canvas::Canvas,
    session_id: &str,
) -> Result<CanvasRuntimeBridgeSnapshot, ApiError> {
    let surface = state.services.runtime_gateway.surface_for_actor(
        RuntimeActor::UserCanvas {
            session_id: session_id.to_string(),
            canvas_id: Some(canvas.id),
        },
        RuntimeContext::Session {
            session_id: session_id.to_string(),
            project_id: Some(canvas.project_id),
            workspace_id: None,
        },
    )?;

    Ok(CanvasRuntimeBridgeSnapshot::enabled(surface))
}

async fn load_canvas_with_permission(
    state: &AppState,
    current_user: &agentdash_plugin_api::AuthIdentity,
    raw_canvas_id: &str,
    permission: ProjectPermission,
) -> Result<agentdash_domain::canvas::Canvas, ApiError> {
    let canvas = if let Ok(uuid) = Uuid::parse_str(raw_canvas_id) {
        state.repos.canvas_repo.get_by_id(uuid).await?
    } else {
        state
            .repos
            .canvas_repo
            .find_by_mount_id(raw_canvas_id)
            .await?
    };
    let canvas =
        canvas.ok_or_else(|| ApiError::NotFound(format!("Canvas {raw_canvas_id} 不存在")))?;

    load_project_with_permission(state, current_user, canvas.project_id, permission).await?;
    Ok(canvas)
}

fn parse_project_id(raw_project_id: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(raw_project_id).map_err(|_| ApiError::BadRequest("无效的 Project ID".into()))
}

fn default_promote_overwrite() -> bool {
    true
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

async fn resolve_canvas_runtime_vfs(
    state: &Arc<AppState>,
    current_user: &agentdash_plugin_api::AuthIdentity,
    session_id: Option<&str>,
    expected_project_id: Uuid,
) -> Result<Option<agentdash_spi::Vfs>, ApiError> {
    let Some(session_id) = session_id else {
        return Ok(None);
    };

    let bindings =
        ensure_canvas_session_scope(state.as_ref(), session_id, expected_project_id).await?;

    if pick_primary_binding_for_project(&bindings, expected_project_id).is_none() {
        return Ok(None);
    }
    Ok(
        build_session_context_plan(state, current_user, session_id, &bindings)
            .await?
            .and_then(|plan| plan.context_projection.vfs),
    )
}

async fn ensure_canvas_session_scope(
    state: &AppState,
    session_id: &str,
    expected_project_id: Uuid,
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
        .any(|binding| binding.project_id == expected_project_id)
    {
        return Err(ApiError::Forbidden(
            "当前 session 与目标 Canvas 不属于同一 Project".to_string(),
        ));
    }

    Ok(bindings)
}

fn pick_primary_binding_for_project(
    bindings: &[SessionBinding],
    project_id: Uuid,
) -> Option<&SessionBinding> {
    bindings
        .iter()
        .filter(|binding| binding.project_id == project_id)
        .find(|binding| binding.owner_type == SessionOwnerType::Project)
        .or_else(|| {
            bindings
                .iter()
                .filter(|binding| binding.project_id == project_id)
                .find(|binding| binding.owner_type == SessionOwnerType::Story)
        })
        .or_else(|| {
            bindings
                .iter()
                .filter(|binding| binding.project_id == project_id)
                .find(|binding| binding.owner_type == SessionOwnerType::Task)
        })
        .or_else(|| {
            bindings
                .iter()
                .find(|binding| binding.project_id == project_id)
        })
}
