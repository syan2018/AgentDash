use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, State},
};
use uuid::Uuid;

use crate::{
    app_state::AppState,
    auth::{CurrentUser, ProjectPermission, load_task_story_project_with_permission},
    dto::TaskExecutionViewResponse,
    rpc::ApiError,
};

pub fn router() -> axum::Router<std::sync::Arc<crate::app_state::AppState>> {
    axum::Router::new().route(
        "/tasks/{id}/execution",
        axum::routing::get(get_task_execution_view),
    )
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
        .story_activity_activation_service
        .get_task_execution_view(task_id)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(TaskExecutionViewResponse {
        task_id: view.task_id,
        execution_status: view.execution_status,
        agent_ref: view.agent_ref,
        run_ref: view.run_ref,
        frame_ref: view.frame_ref,
        delivery_runtime_ref: view.delivery_runtime_ref,
        task_status: view.task_status,
    }))
}

fn parse_task_id(id: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(id).map_err(|_| ApiError::BadRequest("无效的 Task ID".into()))
}
