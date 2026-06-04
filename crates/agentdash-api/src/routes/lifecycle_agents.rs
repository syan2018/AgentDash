use std::sync::Arc;

use agentdash_application::workflow::{
    LifecycleAgentMessageCommand, LifecycleAgentMessageService, LifecycleAgentSteeringCommand,
    LifecycleAgentSteeringService, SessionLaunchLifecycleAgentMessageDeliveryPort,
};
use agentdash_contracts::workflow::{
    AgentFrameRefDto, LifecycleAgentMessageRequest, LifecycleAgentMessageResponse,
    LifecycleAgentRefDto, LifecycleAgentSteeringRequest, LifecycleAgentSteeringResponse,
    LifecycleRunRefDto, RuntimeSessionCommandStateDto,
};
use agentdash_spi::AgentConfig;
use axum::{
    Json,
    extract::{Path, State},
};

use crate::{
    app_state::AppState,
    auth::{CurrentUser, ProjectPermission, load_project_with_permission},
    rpc::ApiError,
};

pub fn router() -> axum::Router<Arc<AppState>> {
    axum::Router::new()
        .route(
            "/lifecycle-agents/by-runtime-session/{runtime_session_id}/messages",
            axum::routing::post(send_lifecycle_agent_message),
        )
        .route(
            "/lifecycle-agents/by-runtime-session/{runtime_session_id}/steering-messages",
            axum::routing::post(steer_lifecycle_agent_message),
        )
}

pub async fn send_lifecycle_agent_message(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(runtime_session_id): Path<String>,
    Json(req): Json<LifecycleAgentMessageRequest>,
) -> Result<Json<LifecycleAgentMessageResponse>, ApiError> {
    if req.input.is_empty() {
        return Err(ApiError::BadRequest("input 不能为空".to_string()));
    }

    let anchor = state
        .repos
        .execution_anchor_repo
        .find_by_session(&runtime_session_id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| {
            ApiError::NotFound(format!(
                "RuntimeSession 缺少控制面锚点: {runtime_session_id}"
            ))
        })?;

    let agent = state
        .repos
        .lifecycle_agent_repo
        .get(anchor.agent_id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::NotFound(format!("LifecycleAgent 不存在: {}", anchor.agent_id)))?;

    let run = state
        .repos
        .lifecycle_run_repo
        .get_by_id(anchor.run_id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::NotFound(format!("LifecycleRun 不存在: {}", anchor.run_id)))?;

    if agent.run_id != run.id {
        return Err(ApiError::Conflict(format!(
            "RuntimeSession anchor agent 与 run 不一致: {runtime_session_id}"
        )));
    }

    load_project_with_permission(
        state.as_ref(),
        &current_user,
        run.project_id,
        ProjectPermission::Edit,
    )
    .await?;

    let executor_config = req
        .executor_config
        .map(serde_json::from_value::<AgentConfig>)
        .transpose()
        .map_err(|error| ApiError::BadRequest(format!("executor_config 非法: {error}")))?;

    let delivery =
        SessionLaunchLifecycleAgentMessageDeliveryPort::new(state.services.session_launch.clone());
    let service = LifecycleAgentMessageService::new(
        state.repos.lifecycle_run_repo.as_ref(),
        state.repos.lifecycle_agent_repo.as_ref(),
        state.repos.agent_frame_repo.as_ref(),
        state.repos.execution_anchor_repo.as_ref(),
        delivery,
    );

    let dispatch = service
        .dispatch_user_message(LifecycleAgentMessageCommand {
            delivery_runtime_session_id: runtime_session_id.clone(),
            input: req.input,
            executor_config,
            identity: Some(current_user.clone()),
        })
        .await
        .map_err(ApiError::from)?;

    Ok(Json(LifecycleAgentMessageResponse {
        runtime_session_id: dispatch.runtime_session_id,
        turn_id: dispatch.turn_id,
        run_ref: LifecycleRunRefDto {
            run_id: dispatch.run_id.to_string(),
        },
        agent_ref: LifecycleAgentRefDto {
            run_id: dispatch.run_id.to_string(),
            agent_id: dispatch.agent_id.to_string(),
        },
        frame_ref: AgentFrameRefDto {
            agent_id: dispatch.agent_id.to_string(),
            frame_id: dispatch.frame_id.to_string(),
            revision: Some(dispatch.frame_revision),
        },
    }))
}

pub async fn steer_lifecycle_agent_message(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(runtime_session_id): Path<String>,
    Json(req): Json<LifecycleAgentSteeringRequest>,
) -> Result<Json<LifecycleAgentSteeringResponse>, ApiError> {
    if req.input.is_empty() {
        return Err(ApiError::BadRequest("input 不能为空".to_string()));
    }

    let anchor = state
        .repos
        .execution_anchor_repo
        .find_by_session(&runtime_session_id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| {
            ApiError::NotFound(format!(
                "RuntimeSession 缺少控制面锚点: {runtime_session_id}"
            ))
        })?;
    let run = state
        .repos
        .lifecycle_run_repo
        .get_by_id(anchor.run_id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::NotFound(format!("LifecycleRun 不存在: {}", anchor.run_id)))?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        run.project_id,
        ProjectPermission::Edit,
    )
    .await?;

    let service = LifecycleAgentSteeringService::new(
        state.repos.lifecycle_run_repo.as_ref(),
        state.repos.lifecycle_agent_repo.as_ref(),
        state.repos.agent_frame_repo.as_ref(),
        state.repos.execution_anchor_repo.as_ref(),
        state.services.session_core.clone(),
        state.services.session_control.clone(),
        state.services.session_eventing.clone(),
    );
    let dispatch = service
        .steer(LifecycleAgentSteeringCommand {
            delivery_runtime_session_id: runtime_session_id.clone(),
            input: req.input,
        })
        .await
        .map_err(ApiError::from)?;
    let execution_state = state
        .services
        .session_core
        .inspect_session_execution_state(&runtime_session_id)
        .await?;

    Ok(Json(LifecycleAgentSteeringResponse {
        runtime_session_id: dispatch.runtime_session_id,
        accepted: true,
        state: runtime_command_state_dto(execution_state),
    }))
}

fn runtime_command_state_dto(
    execution_state: agentdash_application::session::SessionExecutionState,
) -> RuntimeSessionCommandStateDto {
    match execution_state {
        agentdash_application::session::SessionExecutionState::Idle => {
            RuntimeSessionCommandStateDto {
                status: "idle".to_string(),
                turn_id: None,
                message: None,
            }
        }
        agentdash_application::session::SessionExecutionState::Running { turn_id } => {
            RuntimeSessionCommandStateDto {
                status: "running".to_string(),
                turn_id,
                message: None,
            }
        }
        agentdash_application::session::SessionExecutionState::Completed { turn_id } => {
            RuntimeSessionCommandStateDto {
                status: "completed".to_string(),
                turn_id: Some(turn_id),
                message: None,
            }
        }
        agentdash_application::session::SessionExecutionState::Failed { turn_id, message } => {
            RuntimeSessionCommandStateDto {
                status: "failed".to_string(),
                turn_id: Some(turn_id),
                message,
            }
        }
        agentdash_application::session::SessionExecutionState::Interrupted { turn_id, message } => {
            RuntimeSessionCommandStateDto {
                status: "interrupted".to_string(),
                turn_id,
                message,
            }
        }
    }
}
