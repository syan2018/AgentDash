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
        TaskResponse, TaskSessionResponse,
    },
    rpc::ApiError,
    session_construction::build_session_context_plan,
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
