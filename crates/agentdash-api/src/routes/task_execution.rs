use std::sync::Arc;

use agentdash_application::session::context::SessionContextSnapshot;
use agentdash_application::task::execution::{ExecutionPhase, TaskExecutionCommand};
use agentdash_domain::task::TaskStatus;
use axum::{
    Json,
    extract::{Path, State},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    app_state::AppState,
    auth::{CurrentUser, ProjectPermission, load_task_story_project_with_permission},
    dto::TaskResponse,
    rpc::ApiError,
    session_use_cases::context_query::build_session_context_plan,
};

#[derive(Debug, Deserialize, Default)]
pub struct StartTaskRequest {
    #[serde(default)]
    pub override_prompt: Option<String>,
    #[serde(default)]
    pub executor_config: Option<agentdash_spi::AgentConfig>,
}

#[derive(Debug, Serialize)]
pub struct StartTaskResponse {
    pub task_id: Uuid,
    pub session_id: String,
    pub turn_id: String,
    pub status: TaskStatus,
    pub context_sources: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct ContinueTaskRequest {
    #[serde(default)]
    pub additional_prompt: Option<String>,
    #[serde(default)]
    pub executor_config: Option<agentdash_spi::AgentConfig>,
}

#[derive(Debug, Serialize)]
pub struct ContinueTaskResponse {
    pub task_id: Uuid,
    pub session_id: String,
    pub turn_id: String,
    pub status: TaskStatus,
    pub context_sources: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct TaskSessionResponse {
    pub task_id: Uuid,
    pub workspace_id: Option<Uuid>,
    pub session_id: Option<String>,
    pub task_status: TaskStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_execution_status: Option<String>,
    pub agent_binding: agentdash_domain::task::AgentBinding,
    pub session_title: Option<String>,
    pub last_activity: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vfs: Option<agentdash_spi::Vfs>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_surface: Option<agentdash_application::vfs::ResolvedVfsSurface>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_snapshot: Option<SessionContextSnapshot>,
}

pub async fn start_task(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<StartTaskRequest>,
) -> Result<Json<StartTaskResponse>, ApiError> {
    let task_id = parse_task_id(&id)?;
    load_task_story_project_with_permission(
        state.as_ref(),
        &current_user,
        task_id,
        ProjectPermission::Edit,
    )
    .await?;
    let result = state
        .services
        .story_step_activation_service
        .start_task(TaskExecutionCommand {
            task_id,
            phase: ExecutionPhase::Start,
            prompt: req.override_prompt,
            executor_config: req.executor_config,
            identity: Some(current_user),
        })
        .await
        .map_err(ApiError::from)?;

    Ok(Json(StartTaskResponse {
        task_id: result.task_id,
        session_id: result.session_id,
        turn_id: result.turn_id,
        status: result.status,
        context_sources: result.context_sources,
    }))
}

pub async fn continue_task(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<ContinueTaskRequest>,
) -> Result<Json<ContinueTaskResponse>, ApiError> {
    let task_id = parse_task_id(&id)?;
    load_task_story_project_with_permission(
        state.as_ref(),
        &current_user,
        task_id,
        ProjectPermission::Edit,
    )
    .await?;
    let result = state
        .services
        .story_step_activation_service
        .continue_task(TaskExecutionCommand {
            task_id,
            phase: ExecutionPhase::Continue,
            prompt: req.additional_prompt,
            executor_config: req.executor_config,
            identity: Some(current_user),
        })
        .await
        .map_err(ApiError::from)?;

    Ok(Json(ContinueTaskResponse {
        task_id: result.task_id,
        session_id: result.session_id,
        turn_id: result.turn_id,
        status: result.status,
        context_sources: result.context_sources,
    }))
}

pub async fn cancel_task(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<TaskResponse>, ApiError> {
    let task_id = parse_task_id(&id)?;
    load_task_story_project_with_permission(
        state.as_ref(),
        &current_user,
        task_id,
        ProjectPermission::Edit,
    )
    .await?;
    let task = state
        .services
        .story_step_activation_service
        .cancel_task(task_id)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(TaskResponse::from(task)))
}

pub async fn get_task_session(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<TaskSessionResponse>, ApiError> {
    let task_id = parse_task_id(&id)?;
    let (task, _, _) = load_task_story_project_with_permission(
        state.as_ref(),
        &current_user,
        task_id,
        ProjectPermission::View,
    )
    .await?;
    let result = state
        .services
        .story_step_activation_service
        .get_task_session(task_id)
        .await
        .map_err(ApiError::from)?;
    let context_projection = if let Some(session_id) = result.session_id.as_ref() {
        let bindings = state
            .repos
            .session_binding_repo
            .list_by_session(session_id)
            .await
            .map_err(ApiError::from)?;
        build_session_context_plan(&state, &current_user, session_id, &bindings)
            .await?
            .map(|plan| plan.context_projection)
    } else {
        None
    };

    let resolved_vfs = context_projection
        .as_ref()
        .and_then(|projection| projection.vfs.clone());
    let runtime_surface = context_projection
        .as_ref()
        .and_then(|projection| projection.runtime_surface.clone());
    let context_snapshot = context_projection.and_then(|projection| projection.context_snapshot);

    Ok(Json(TaskSessionResponse {
        task_id: result.task_id,
        workspace_id: task.workspace_id,
        session_id: result.session_id,
        task_status: result.task_status,
        session_execution_status: result.session_execution_status,
        agent_binding: result.agent_binding,
        session_title: result.session_title,
        last_activity: result.last_activity,
        vfs: resolved_vfs,
        runtime_surface,
        context_snapshot,
    }))
}

fn parse_task_id(id: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(id).map_err(|_| ApiError::BadRequest("无效的 Task ID".into()))
}
