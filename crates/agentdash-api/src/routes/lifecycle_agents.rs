use std::sync::Arc;

use agentdash_application::workflow::{
    AgentRunMessageCommand, AgentRunMessageLaunchDeliveryPort, AgentRunMessageService,
    AgentRunSteeringCommand, AgentRunSteeringService,
};
use agentdash_contracts::workflow::{
    AgentFrameRefDto, AgentRunMessageRequest, AgentRunMessageResponse, AgentRunRefDto,
    AgentRunSteeringRequest, AgentRunSteeringResponse, EnqueuePendingMessageRequest,
    EnqueuePendingMessageResponse, LifecycleRunRefDto, PendingMessageView,
    RuntimeSessionCommandStateDto,
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
            "/sessions/{runtime_session_id}/messages",
            axum::routing::post(send_session_message),
        )
        .route(
            "/sessions/{runtime_session_id}/steering",
            axum::routing::post(steer_session),
        )
        .route(
            "/sessions/{runtime_session_id}/pending-messages",
            axum::routing::get(list_pending_messages).post(enqueue_pending_message),
        )
        .route(
            "/sessions/{runtime_session_id}/pending-messages/{message_id}",
            axum::routing::delete(delete_pending_message),
        )
        .route(
            "/sessions/{runtime_session_id}/pending-messages/{message_id}/promote",
            axum::routing::post(promote_pending_message),
        )
}

pub async fn send_session_message(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(runtime_session_id): Path<String>,
    Json(req): Json<AgentRunMessageRequest>,
) -> Result<Json<AgentRunMessageResponse>, ApiError> {
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

    let delivery = AgentRunMessageLaunchDeliveryPort::new(state.services.session_launch.clone());
    let service = AgentRunMessageService::new(
        state.repos.lifecycle_run_repo.as_ref(),
        state.repos.lifecycle_agent_repo.as_ref(),
        state.repos.agent_frame_repo.as_ref(),
        state.repos.execution_anchor_repo.as_ref(),
        delivery,
    );

    let dispatch = service
        .dispatch_user_message(AgentRunMessageCommand {
            delivery_runtime_session_id: runtime_session_id.clone(),
            input: req.input,
            executor_config,
            identity: Some(current_user.clone()),
        })
        .await
        .map_err(ApiError::from)?;

    Ok(Json(AgentRunMessageResponse {
        runtime_session_id: dispatch.runtime_session_id,
        turn_id: dispatch.turn_id,
        run_ref: LifecycleRunRefDto {
            run_id: dispatch.run_id.to_string(),
        },
        agent_ref: AgentRunRefDto {
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

pub async fn steer_session(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(runtime_session_id): Path<String>,
    Json(req): Json<AgentRunSteeringRequest>,
) -> Result<Json<AgentRunSteeringResponse>, ApiError> {
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

    let service = AgentRunSteeringService::new(
        state.repos.lifecycle_run_repo.as_ref(),
        state.repos.lifecycle_agent_repo.as_ref(),
        state.repos.agent_frame_repo.as_ref(),
        state.repos.execution_anchor_repo.as_ref(),
        state.services.session_core.clone(),
        state.services.session_control.clone(),
        state.services.session_eventing.clone(),
    );
    let dispatch = service
        .steer(AgentRunSteeringCommand {
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

    Ok(Json(AgentRunSteeringResponse {
        runtime_session_id: dispatch.runtime_session_id,
        accepted: true,
        state: runtime_command_state_dto(execution_state),
    }))
}

/// Resolve runtime_session_id → anchor → run → project permission check.
async fn ensure_runtime_session_permission(
    state: &AppState,
    user: &agentdash_integration_api::AuthIdentity,
    runtime_session_id: &str,
) -> Result<(), ApiError> {
    let anchor = state
        .repos
        .execution_anchor_repo
        .find_by_session(runtime_session_id)
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
    load_project_with_permission(state, user, run.project_id, ProjectPermission::Edit).await?;
    Ok(())
}

async fn list_pending_messages(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(runtime_session_id): Path<String>,
) -> Result<Json<Vec<PendingMessageView>>, ApiError> {
    ensure_runtime_session_permission(&state, &current_user, &runtime_session_id).await?;
    let previews = state.services.pending_queue.list(&runtime_session_id).await;
    let views: Vec<PendingMessageView> = previews
        .into_iter()
        .map(|p| PendingMessageView {
            id: p.id,
            preview: p.preview,
            has_images: p.has_images,
            created_at: p.created_at.to_rfc3339(),
        })
        .collect();
    Ok(Json(views))
}

async fn enqueue_pending_message(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(runtime_session_id): Path<String>,
    Json(body): Json<EnqueuePendingMessageRequest>,
) -> Result<Json<EnqueuePendingMessageResponse>, ApiError> {
    if body.input.is_empty() {
        return Err(ApiError::BadRequest("input 不能为空".to_string()));
    }
    ensure_runtime_session_permission(&state, &current_user, &runtime_session_id).await?;

    let input_blocks: Vec<agentdash_agent_protocol::UserInputBlock> = body.input;
    let executor_config = body
        .executor_config
        .map(serde_json::from_value::<AgentConfig>)
        .transpose()
        .map_err(|e| ApiError::BadRequest(format!("executor_config 格式错误: {e}")))?;

    let preview = state
        .services
        .pending_queue
        .enqueue(&runtime_session_id, input_blocks, executor_config)
        .await;
    Ok(Json(EnqueuePendingMessageResponse {
        message: PendingMessageView {
            id: preview.id,
            preview: preview.preview,
            has_images: preview.has_images,
            created_at: preview.created_at.to_rfc3339(),
        },
    }))
}

async fn delete_pending_message(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((runtime_session_id, message_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    ensure_runtime_session_permission(&state, &current_user, &runtime_session_id).await?;
    let deleted = state
        .services
        .pending_queue
        .delete(&runtime_session_id, &message_id)
        .await;
    if !deleted {
        return Err(ApiError::NotFound(format!(
            "pending message {} 不存在",
            message_id
        )));
    }
    Ok(Json(serde_json::json!({ "deleted": true })))
}

async fn promote_pending_message(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((runtime_session_id, message_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    ensure_runtime_session_permission(&state, &current_user, &runtime_session_id).await?;
    let msg = state
        .services
        .pending_queue
        .take(&runtime_session_id, &message_id)
        .await
        .ok_or_else(|| ApiError::NotFound(format!("pending message {} 不存在", message_id)))?;

    let service = AgentRunSteeringService::new(
        state.repos.lifecycle_run_repo.as_ref(),
        state.repos.lifecycle_agent_repo.as_ref(),
        state.repos.agent_frame_repo.as_ref(),
        state.repos.execution_anchor_repo.as_ref(),
        state.services.session_core.clone(),
        state.services.session_control.clone(),
        state.services.session_eventing.clone(),
    );
    let dispatch = service
        .steer(AgentRunSteeringCommand {
            delivery_runtime_session_id: runtime_session_id.clone(),
            input: msg.input,
        })
        .await
        .map_err(|e| ApiError::Internal(format!("promote-to-steer 失败: {e}")))?;

    Ok(Json(serde_json::json!({
        "promoted": true,
        "turn_id": dispatch.active_turn_id,
    })))
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
