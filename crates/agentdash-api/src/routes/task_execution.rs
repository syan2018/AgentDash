use std::sync::Arc;

use agentdash_application::task_execution::TaskExecutionError;
use agentdash_domain::task::{Task, TaskStatus};
use axum::{
    Json,
    extract::{Path, State},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    app_state::AppState,
    bootstrap::task_execution_gateway::{
        execute_cancel_task, execute_continue_task, execute_get_task_session, execute_start_task,
    },
    rpc::ApiError,
};

#[derive(Debug, Deserialize, Default)]
pub struct StartTaskRequest {
    #[serde(default)]
    pub override_prompt: Option<String>,
    #[serde(default)]
    pub executor_config: Option<executors::profile::ExecutorConfig>,
}

#[derive(Debug, Serialize)]
pub struct StartTaskResponse {
    pub task_id: Uuid,
    pub session_id: String,
    pub executor_session_id: Option<String>,
    pub turn_id: String,
    pub status: TaskStatus,
    pub context_sources: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct ContinueTaskRequest {
    #[serde(default)]
    pub additional_prompt: Option<String>,
    #[serde(default)]
    pub executor_config: Option<executors::profile::ExecutorConfig>,
}

#[derive(Debug, Serialize)]
pub struct ContinueTaskResponse {
    pub task_id: Uuid,
    pub session_id: String,
    pub executor_session_id: Option<String>,
    pub turn_id: String,
    pub status: TaskStatus,
    pub context_sources: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct TaskSessionResponse {
    pub task_id: Uuid,
    pub session_id: Option<String>,
    pub executor_session_id: Option<String>,
    pub task_status: TaskStatus,
    pub agent_binding: agentdash_domain::task::AgentBinding,
    pub session_title: Option<String>,
    pub last_activity: Option<i64>,
}

pub async fn start_task(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<StartTaskRequest>,
) -> Result<Json<StartTaskResponse>, ApiError> {
    let task_id = parse_task_id(&id)?;
    let result = execute_start_task(state, task_id, req.override_prompt, req.executor_config)
        .await
        .map_err(map_task_execution_error)?;

    Ok(Json(StartTaskResponse {
        task_id: result.task_id,
        session_id: result.session_id,
        executor_session_id: result.executor_session_id,
        turn_id: result.turn_id,
        status: result.status,
        context_sources: result.context_sources,
    }))
}

pub async fn continue_task(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<ContinueTaskRequest>,
) -> Result<Json<ContinueTaskResponse>, ApiError> {
    let task_id = parse_task_id(&id)?;
    let result = execute_continue_task(state, task_id, req.additional_prompt, req.executor_config)
        .await
        .map_err(map_task_execution_error)?;

    Ok(Json(ContinueTaskResponse {
        task_id: result.task_id,
        session_id: result.session_id,
        executor_session_id: result.executor_session_id,
        turn_id: result.turn_id,
        status: result.status,
        context_sources: result.context_sources,
    }))
}

pub async fn cancel_task(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Task>, ApiError> {
    let task_id = parse_task_id(&id)?;
    let task = execute_cancel_task(state, task_id)
        .await
        .map_err(map_task_execution_error)?;
    Ok(Json(task))
}

pub async fn get_task_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<TaskSessionResponse>, ApiError> {
    let task_id = parse_task_id(&id)?;
    let result = execute_get_task_session(state, task_id)
        .await
        .map_err(map_task_execution_error)?;

    Ok(Json(TaskSessionResponse {
        task_id: result.task_id,
        session_id: result.session_id,
        executor_session_id: result.executor_session_id,
        task_status: result.task_status,
        agent_binding: result.agent_binding,
        session_title: result.session_title,
        last_activity: result.last_activity,
    }))
}

fn parse_task_id(id: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(id).map_err(|_| ApiError::BadRequest("无效的 Task ID".into()))
}

fn map_task_execution_error(err: TaskExecutionError) -> ApiError {
    match err {
        TaskExecutionError::BadRequest(message) => ApiError::BadRequest(message),
        TaskExecutionError::NotFound(message) => ApiError::NotFound(message),
        TaskExecutionError::Conflict(message) => ApiError::Conflict(message),
        TaskExecutionError::UnprocessableEntity(message) => ApiError::UnprocessableEntity(message),
        TaskExecutionError::Internal(message) => ApiError::Internal(message),
    }
}
