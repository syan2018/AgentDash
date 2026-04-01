use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use serde::Deserialize;
use uuid::Uuid;

use agentdash_application::canvas::{
    CanvasMutationInput, apply_canvas_mutation, build_canvas, build_runtime_snapshot_with_bindings,
};
use agentdash_domain::canvas::{CanvasDataBinding, CanvasFile, CanvasSandboxConfig};
use agentdash_domain::session_binding::{SessionBinding, SessionOwnerType};

use crate::app_state::AppState;
use crate::auth::{CurrentUser, ProjectPermission, load_project_with_permission};
use crate::dto::CanvasResponse;
use crate::routes::{project_sessions, story_sessions};
use crate::rpc::ApiError;

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

pub async fn get_canvas_runtime_snapshot(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
    Query(query): Query<CanvasRuntimeSnapshotQuery>,
) -> Result<Json<agentdash_application::canvas::CanvasRuntimeSnapshot>, ApiError> {
    let canvas =
        load_canvas_with_permission(state.as_ref(), &current_user, &id, ProjectPermission::View)
            .await?;

    let address_space = resolve_canvas_runtime_address_space(
        &state,
        query.session_id.as_deref(),
        canvas.project_id,
    )
    .await?;
    let snapshot = build_runtime_snapshot_with_bindings(
        &canvas,
        query.session_id,
        address_space.as_ref(),
        state.services.address_space_service.as_ref(),
    )
    .await;

    Ok(Json(snapshot))
}

async fn load_canvas_with_permission(
    state: &AppState,
    current_user: &agentdash_plugin_api::AuthIdentity,
    raw_canvas_id: &str,
    permission: ProjectPermission,
) -> Result<agentdash_domain::canvas::Canvas, ApiError> {
    let canvas_id = Uuid::parse_str(raw_canvas_id)
        .map_err(|_| ApiError::BadRequest("无效的 Canvas ID".into()))?;
    let canvas = state
        .repos
        .canvas_repo
        .get_by_id(canvas_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Canvas {raw_canvas_id} 不存在")))?;

    load_project_with_permission(state, current_user, canvas.project_id, permission).await?;
    Ok(canvas)
}

fn parse_project_id(raw_project_id: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(raw_project_id).map_err(|_| ApiError::BadRequest("无效的 Project ID".into()))
}

async fn resolve_canvas_runtime_address_space(
    state: &Arc<AppState>,
    session_id: Option<&str>,
    expected_project_id: Uuid,
) -> Result<Option<agentdash_spi::AddressSpace>, ApiError> {
    let Some(session_id) = session_id else {
        return Ok(None);
    };

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

    let Some(binding) = pick_primary_binding_for_project(&bindings, expected_project_id) else {
        return Ok(None);
    };
    let session_meta = state
        .services
        .session_hub
        .get_session_meta(session_id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;

    match binding.owner_type {
        SessionOwnerType::Task => {
            let built_context =
                agentdash_application::task::context_builder::build_task_session_context(
                    &state.repos,
                    &state.services.address_space_service,
                    state.config.mcp_base_url.as_deref(),
                    binding.owner_id,
                    session_meta.as_ref(),
                )
                .await;
            Ok(built_context.and_then(|context| context.address_space))
        }
        SessionOwnerType::Story => {
            let story = state
                .repos
                .story_repo
                .get_by_id(binding.owner_id)
                .await?
                .ok_or_else(|| ApiError::NotFound(format!("Story {} 不存在", binding.owner_id)))?;
            let built_context =
                story_sessions::build_story_session_context_response(state, &story, session_id)
                    .await?;
            Ok(built_context.and_then(|context| context.address_space))
        }
        SessionOwnerType::Project => {
            let project = state
                .repos
                .project_repo
                .get_by_id(binding.owner_id)
                .await?
                .ok_or_else(|| {
                    ApiError::NotFound(format!("Project {} 不存在", binding.owner_id))
                })?;
            let built_context = project_sessions::build_project_session_context_response(
                state,
                &project,
                session_id,
                &binding.label,
            )
            .await?;
            Ok(built_context.address_space)
        }
    }
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
