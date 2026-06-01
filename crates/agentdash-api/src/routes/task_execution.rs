use std::sync::Arc;

use agentdash_application::task::execution::{ExecutionPhase, TaskExecutionCommand};
use axum::{
    Json,
    extract::{Path, State},
};
use uuid::Uuid;

use crate::{
    app_state::AppState,
    auth::{CurrentUser, ProjectPermission, load_task_story_project_with_permission},
    dto::{
        ContinueTaskRequest, ContinueTaskResponse, StartTaskRequest, StartTaskResponse,
        TaskExecutionViewResponse,
    },
    rpc::ApiError,
};

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
        run_ref: result.run_ref,
        agent_ref: result.agent_ref,
        frame_ref: result.frame_ref,
        assignment_ref: result.assignment_ref,
        subject_execution_ref: result.subject_execution_ref.association_id,
        trace_ref: result.trace_ref,
        status: result.status,
    }))
}

pub fn router() -> axum::Router<std::sync::Arc<crate::app_state::AppState>> {
    axum::Router::new()
        .route("/tasks/{id}/start", axum::routing::post(start_task))
        .route("/tasks/{id}/continue", axum::routing::post(continue_task))
        .route("/tasks/{id}/cancel", axum::routing::post(cancel_task))
        .route(
            "/tasks/{id}/execution",
            axum::routing::get(get_task_execution_view),
        )
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
        run_ref: result.run_ref,
        agent_ref: result.agent_ref,
        frame_ref: result.frame_ref,
        assignment_ref: result.assignment_ref,
        subject_execution_ref: result.subject_execution_ref.association_id,
        trace_ref: result.trace_ref,
        status: result.status,
    }))
}

pub async fn cancel_task(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<crate::dto::TaskResponse>, ApiError> {
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
    Ok(Json(crate::dto::TaskResponse::from(task)))
}

pub async fn get_task_execution_view(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<TaskExecutionViewResponse>, ApiError> {
    let task_id = parse_task_id(&id)?;
    load_task_story_project_with_permission(
        state.as_ref(),
        &current_user,
        task_id,
        ProjectPermission::View,
    )
    .await?;
    let view = state
        .services
        .story_step_activation_service
        .get_task_execution_view(task_id)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(TaskExecutionViewResponse {
        task_id: view.task_id,
        execution_status: view.execution_status,
        agent_ref: view.agent_ref,
        run_ref: view.run_ref,
        frame_ref: view.frame_ref,
        trace_ref: view.trace_ref,
        task_status: view.task_status,
    }))
}

fn parse_task_id(id: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(id).map_err(|_| ApiError::BadRequest("无效的 Task ID".into()))
}
