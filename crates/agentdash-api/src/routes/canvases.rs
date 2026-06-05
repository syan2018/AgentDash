use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use uuid::Uuid;

use agentdash_application::canvas::{
    CanvasExtensionPackageInput, CanvasMutationInput, CanvasRuntimeBridgeSnapshot,
    CreateCanvasInput, build_canvas_extension_package, build_runtime_snapshot_with_bindings,
    create_project_canvas, delete_canvas_record,
    list_project_canvases as list_project_canvases_use_case, load_canvas_by_ref,
    update_canvas_record,
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
use agentdash_contracts::core::DeletedIdResponse;
use agentdash_contracts::extension_package::ExtensionPackageInstallationResponse;

use crate::app_state::AppState;
use crate::auth::{CurrentUser, ProjectPermission, load_project_with_permission};
use crate::dto::{
    CanvasResponse, CanvasRuntimeInvokeRequest, CanvasRuntimeSnapshotQuery, CreateCanvasRequest,
    ListProjectCanvasesPath, PromoteCanvasToExtensionRequest, UpdateCanvasRequest,
};
use crate::rpc::ApiError;
use crate::session_construction::resolve_session_frame_vfs;

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

    let canvases = list_project_canvases_use_case(&state.repos, project_id).await?;
    Ok(Json(
        canvases.into_iter().map(CanvasResponse::from).collect(),
    ))
}

pub fn router() -> axum::Router<std::sync::Arc<crate::app_state::AppState>> {
    axum::Router::new()
        .route(
            "/projects/{project_id}/canvases",
            axum::routing::get(list_project_canvases).post(create_canvas),
        )
        .route(
            "/canvases/{id}",
            axum::routing::get(get_canvas)
                .put(update_canvas)
                .delete(delete_canvas),
        )
        .route(
            "/canvases/{id}/runtime-snapshot",
            axum::routing::get(get_canvas_runtime_snapshot),
        )
        .route(
            "/canvases/{id}/runtime-invoke",
            axum::routing::post(invoke_canvas_runtime_action),
        )
        .route(
            "/canvases/{id}/promote-extension",
            axum::routing::post(promote_canvas_to_extension),
        )
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

    let canvas = create_project_canvas(
        &state.repos,
        CreateCanvasInput {
            project_id,
            mount_id: req.mount_id,
            title: req.title,
            description: req.description,
            mutation: CanvasMutationInput {
                entry_file: req.entry_file,
                sandbox_config: req.sandbox_config,
                files: req.files,
                bindings: req.bindings,
                ..CanvasMutationInput::default()
            },
        },
    )
    .await?;

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
    let canvas =
        load_canvas_with_permission(state.as_ref(), &current_user, &id, ProjectPermission::Edit)
            .await?;

    let canvas = update_canvas_record(
        &state.repos,
        canvas,
        CanvasMutationInput {
            title: req.title,
            description: req.description,
            entry_file: req.entry_file,
            sandbox_config: req.sandbox_config,
            files: req.files,
            bindings: req.bindings,
        },
    )
    .await?;

    Ok(Json(CanvasResponse::from(canvas)))
}

pub async fn delete_canvas(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<DeletedIdResponse>, ApiError> {
    let canvas =
        load_canvas_with_permission(state.as_ref(), &current_user, &id, ProjectPermission::Edit)
            .await?;
    delete_canvas_record(&state.repos, &canvas).await?;

    Ok(Json(DeletedIdResponse { deleted: id }))
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

    let vfs =
        resolve_canvas_runtime_vfs(&state, &current_user, query.session_id.as_deref()).await?;
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
    current_user: &agentdash_integration_api::AuthIdentity,
    raw_canvas_id: &str,
    permission: ProjectPermission,
) -> Result<agentdash_domain::canvas::Canvas, ApiError> {
    let canvas = load_canvas_by_ref(&state.repos, raw_canvas_id).await?;

    load_project_with_permission(state, current_user, canvas.project_id, permission).await?;
    Ok(canvas)
}

fn parse_project_id(raw_project_id: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(raw_project_id).map_err(|_| ApiError::BadRequest("无效的 Project ID".into()))
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

async fn resolve_canvas_runtime_vfs(
    state: &Arc<AppState>,
    current_user: &agentdash_integration_api::AuthIdentity,
    session_id: Option<&str>,
) -> Result<Option<agentdash_spi::Vfs>, ApiError> {
    let Some(session_id) = session_id else {
        return Ok(None);
    };

    Ok(resolve_session_frame_vfs(state, current_user, session_id)
        .await?
        .vfs)
}
