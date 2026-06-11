use std::sync::Arc;

use agentdash_application::workflow::{
    AgentRunMessageCommand, AgentRunMessageLaunchDeliveryPort, AgentRunMessageService,
    AgentRunSteeringCommand, AgentRunSteeringService, lifecycle_run_view_builder,
};
use agentdash_contracts::workflow::{
    AgentFrameRefDto, AgentRunAcceptedRefs, AgentRunCommandReceipt, AgentRunMessageRequest,
    AgentRunMessageResponse, AgentRunRefDto, AgentRunSteeringRequest, AgentRunSteeringResponse,
    AgentRunWorkspaceShell, AgentRunWorkspaceView, EnqueuePendingMessageRequest,
    EnqueuePendingMessageResponse, LifecycleRunRefDto, PendingMessageView,
    RuntimeSessionCommandStateDto, RuntimeSessionRefDto, RuntimeSessionTraceMeta,
    SessionRuntimeActionAvailabilityView, SessionRuntimeActionSetView,
    SessionRuntimeControlPlaneStatus, SessionRuntimeControlPlaneView,
};
use agentdash_domain::workflow::{LifecycleAgent, LifecycleRun};
use agentdash_spi::AgentConfig;
use axum::{
    Json,
    extract::{Path, State},
};
use uuid::Uuid;

use crate::{
    app_state::AppState,
    auth::{CurrentUser, ProjectPermission, load_project_with_permission},
    routes::{
        lifecycle_contracts::{agent_run_to_contract, subject_association_to_contract},
        lifecycle_views::{agent_frame_runtime_to_view, runtime_refs_for_agent},
    },
    rpc::ApiError,
};

struct AgentRunContext {
    run: LifecycleRun,
    agent: LifecycleAgent,
    delivery_runtime_session_id: Option<String>,
}

pub fn router() -> axum::Router<Arc<AppState>> {
    axum::Router::new()
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/workspace",
            axum::routing::get(get_agent_run_workspace),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/messages",
            axum::routing::post(send_agent_run_message),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/steering",
            axum::routing::post(steer_agent_run),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/pending-messages",
            axum::routing::get(list_agent_run_pending_messages)
                .post(enqueue_agent_run_pending_message),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/pending-messages/{message_id}",
            axum::routing::delete(delete_agent_run_pending_message),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/pending-messages/{message_id}/promote",
            axum::routing::post(promote_agent_run_pending_message),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/cancel",
            axum::routing::post(cancel_agent_run),
        )
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

pub async fn get_agent_run_workspace(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id)): Path<(String, String)>,
) -> Result<Json<AgentRunWorkspaceView>, ApiError> {
    let context = resolve_agent_run_context(
        &state,
        &current_user,
        &run_id,
        &agent_id,
        ProjectPermission::View,
    )
    .await?;
    Ok(Json(
        build_agent_run_workspace_view(&state, &context).await?,
    ))
}

pub async fn send_agent_run_message(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id)): Path<(String, String)>,
    Json(req): Json<AgentRunMessageRequest>,
) -> Result<Json<AgentRunMessageResponse>, ApiError> {
    if req.client_command_id.trim().is_empty() {
        return Err(ApiError::BadRequest(
            "client_command_id 不能为空".to_string(),
        ));
    }
    let context = resolve_agent_run_context(
        &state,
        &current_user,
        &run_id,
        &agent_id,
        ProjectPermission::Edit,
    )
    .await?;
    let runtime_session_id = context.delivery_runtime_session_id.clone().ok_or_else(|| {
        ApiError::Conflict(format!(
            "AgentRun {} / {} 缺少 delivery runtime",
            context.run.id, context.agent.id
        ))
    })?;
    dispatch_message_for_runtime(state, current_user, runtime_session_id, req).await
}

pub async fn steer_agent_run(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id)): Path<(String, String)>,
    Json(req): Json<AgentRunSteeringRequest>,
) -> Result<Json<AgentRunSteeringResponse>, ApiError> {
    if req.client_command_id.trim().is_empty() {
        return Err(ApiError::BadRequest(
            "client_command_id 不能为空".to_string(),
        ));
    }
    let context = resolve_agent_run_context(
        &state,
        &current_user,
        &run_id,
        &agent_id,
        ProjectPermission::Edit,
    )
    .await?;
    let runtime_session_id = context.delivery_runtime_session_id.clone().ok_or_else(|| {
        ApiError::Conflict(format!(
            "AgentRun {} / {} 缺少 delivery runtime",
            context.run.id, context.agent.id
        ))
    })?;
    steer_runtime_session(state, runtime_session_id, req).await
}

async fn list_agent_run_pending_messages(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id)): Path<(String, String)>,
) -> Result<Json<Vec<PendingMessageView>>, ApiError> {
    let context = resolve_agent_run_context(
        &state,
        &current_user,
        &run_id,
        &agent_id,
        ProjectPermission::View,
    )
    .await?;
    let Some(runtime_session_id) = context.delivery_runtime_session_id else {
        return Ok(Json(Vec::new()));
    };
    let views = state
        .services
        .pending_queue
        .list(&runtime_session_id)
        .await
        .into_iter()
        .map(pending_message_view)
        .collect();
    Ok(Json(views))
}

async fn enqueue_agent_run_pending_message(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id)): Path<(String, String)>,
    Json(body): Json<EnqueuePendingMessageRequest>,
) -> Result<Json<EnqueuePendingMessageResponse>, ApiError> {
    if body.client_command_id.trim().is_empty() {
        return Err(ApiError::BadRequest(
            "client_command_id 不能为空".to_string(),
        ));
    }
    if body.input.is_empty() {
        return Err(ApiError::BadRequest("input 不能为空".to_string()));
    }
    let context = resolve_agent_run_context(
        &state,
        &current_user,
        &run_id,
        &agent_id,
        ProjectPermission::Edit,
    )
    .await?;
    let runtime_session_id = context.delivery_runtime_session_id.ok_or_else(|| {
        ApiError::Conflict(format!(
            "AgentRun {} / {} 缺少 delivery runtime",
            context.run.id, context.agent.id
        ))
    })?;
    let executor_config = body
        .executor_config
        .map(serde_json::from_value::<AgentConfig>)
        .transpose()
        .map_err(|e| ApiError::BadRequest(format!("executor_config 格式错误: {e}")))?;
    let preview = state
        .services
        .pending_queue
        .enqueue(&runtime_session_id, body.input, executor_config)
        .await;
    Ok(Json(EnqueuePendingMessageResponse {
        command_receipt: accepted_receipt(body.client_command_id),
        message: pending_message_view(preview),
    }))
}

async fn delete_agent_run_pending_message(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id, message_id)): Path<(String, String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let context = resolve_agent_run_context(
        &state,
        &current_user,
        &run_id,
        &agent_id,
        ProjectPermission::Edit,
    )
    .await?;
    let runtime_session_id = context.delivery_runtime_session_id.ok_or_else(|| {
        ApiError::Conflict(format!(
            "AgentRun {} / {} 缺少 delivery runtime",
            context.run.id, context.agent.id
        ))
    })?;
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

async fn promote_agent_run_pending_message(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id, message_id)): Path<(String, String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let context = resolve_agent_run_context(
        &state,
        &current_user,
        &run_id,
        &agent_id,
        ProjectPermission::Edit,
    )
    .await?;
    let runtime_session_id = context.delivery_runtime_session_id.ok_or_else(|| {
        ApiError::Conflict(format!(
            "AgentRun {} / {} 缺少 delivery runtime",
            context.run.id, context.agent.id
        ))
    })?;
    promote_pending_message_for_runtime(state, runtime_session_id, message_id).await
}

async fn cancel_agent_run(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let context = resolve_agent_run_context(
        &state,
        &current_user,
        &run_id,
        &agent_id,
        ProjectPermission::Edit,
    )
    .await?;
    let runtime_session_id = context.delivery_runtime_session_id.ok_or_else(|| {
        ApiError::Conflict(format!(
            "AgentRun {} / {} 缺少 delivery runtime",
            context.run.id, context.agent.id
        ))
    })?;
    state
        .services
        .session_runtime
        .cancel(&runtime_session_id)
        .await?;
    Ok(Json(serde_json::json!({ "cancelled": true })))
}

async fn resolve_agent_run_context(
    state: &AppState,
    current_user: &agentdash_integration_api::AuthIdentity,
    run_id: &str,
    agent_id: &str,
    permission: ProjectPermission,
) -> Result<AgentRunContext, ApiError> {
    let run_id = parse_uuid(run_id, "run_id")?;
    let agent_id = parse_uuid(agent_id, "agent_id")?;
    let run = state
        .repos
        .lifecycle_run_repo
        .get_by_id(run_id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::NotFound(format!("LifecycleRun 不存在: {run_id}")))?;
    load_project_with_permission(state, current_user, run.project_id, permission).await?;
    let agent = state
        .repos
        .lifecycle_agent_repo
        .get(agent_id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::NotFound(format!("LifecycleAgent 不存在: {agent_id}")))?;
    if agent.run_id != run.id || agent.project_id != run.project_id {
        return Err(ApiError::Conflict(format!(
            "LifecycleAgent {agent_id} 不属于 LifecycleRun {run_id}"
        )));
    }
    let delivery_runtime_session_id = state
        .repos
        .execution_anchor_repo
        .latest_for_agent(agent.id)
        .await
        .map_err(ApiError::from)?
        .map(|anchor| anchor.runtime_session_id);
    Ok(AgentRunContext {
        run,
        agent,
        delivery_runtime_session_id,
    })
}

async fn build_agent_run_workspace_view(
    state: &AppState,
    context: &AgentRunContext,
) -> Result<AgentRunWorkspaceView, ApiError> {
    let runtime_session_id = context.delivery_runtime_session_id.clone();
    let meta = match runtime_session_id.as_deref() {
        Some(session_id) => {
            state
                .services
                .session_core
                .get_session_meta(session_id)
                .await?
        }
        None => None,
    };
    let anchor_frame = match runtime_session_id.as_deref() {
        Some(session_id) => state
            .repos
            .execution_anchor_repo
            .find_by_session(session_id)
            .await?
            .map(|anchor| anchor.launch_frame_id),
        None => None,
    };
    let frame = state
        .repos
        .agent_frame_repo
        .get_current(context.agent.id)
        .await?
        .or(match anchor_frame {
            Some(frame_id) => state.repos.agent_frame_repo.get(frame_id).await?,
            None => None,
        });
    let frame_runtime = match frame {
        Some(frame) => {
            let runtime_refs = runtime_refs_for_agent(state, context.agent.id).await?;
            Some(agent_frame_runtime_to_view(&frame, runtime_refs))
        }
        None => None,
    };
    let run_view =
        lifecycle_run_view_builder::build_lifecycle_run_view(&state.repos, &context.run).await?;
    let agent_view = run_view
        .agents
        .iter()
        .find(|view| view.agent_ref.agent_id == context.agent.id.to_string())
        .cloned();
    let agent_id_string = context.agent.id.to_string();
    let subject_associations = run_view
        .subject_associations
        .iter()
        .filter(|assoc| {
            assoc.anchor_agent_id.as_deref() == Some(agent_id_string.as_str())
                || assoc.anchor_agent_id.is_none()
        })
        .cloned()
        .map(subject_association_to_contract)
        .collect::<Vec<_>>();
    let execution_state = match runtime_session_id.as_deref() {
        Some(session_id) => {
            state
                .services
                .session_core
                .inspect_session_execution_state(session_id)
                .await?
        }
        None => agentdash_application::session::SessionExecutionState::Idle,
    };
    let delivery_running = matches!(
        execution_state,
        agentdash_application::session::SessionExecutionState::Running { .. }
    );
    let terminal_agent = is_terminal_agent_status(&context.agent.status);
    let has_frame = frame_runtime.is_some();
    let supports_steering = match runtime_session_id.as_deref() {
        Some(session_id) if delivery_running => {
            state
                .services
                .session_control
                .supports_session_steering(session_id)
                .await
        }
        _ => false,
    };
    let control_plane = if terminal_agent {
        SessionRuntimeControlPlaneView {
            status: SessionRuntimeControlPlaneStatus::Terminal,
            reason: Some("当前 AgentRun 已结束。".to_string()),
        }
    } else if !has_frame {
        SessionRuntimeControlPlaneView {
            status: SessionRuntimeControlPlaneStatus::FrameMissing,
            reason: Some("当前 AgentRun 没有可投递的 runtime frame。".to_string()),
        }
    } else if delivery_running {
        SessionRuntimeControlPlaneView {
            status: SessionRuntimeControlPlaneStatus::AnchoredRunning,
            reason: Some("当前 AgentRun 正在执行中。".to_string()),
        }
    } else {
        SessionRuntimeControlPlaneView {
            status: SessionRuntimeControlPlaneStatus::AnchoredIdle,
            reason: None,
        }
    };
    let actions = SessionRuntimeActionSetView {
        send_next: if has_frame && !terminal_agent && !delivery_running {
            enabled_action()
        } else if delivery_running {
            disabled_action("当前 AgentRun 正在执行中，不能并发发送下一轮消息。")
        } else if terminal_agent {
            disabled_action("当前 AgentRun 已结束，不能继续发送消息。")
        } else {
            disabled_action("当前 AgentRun 没有可投递的 runtime frame。")
        },
        enqueue: if has_frame && !terminal_agent && delivery_running {
            enabled_action()
        } else if !delivery_running {
            disabled_action("当前 AgentRun 未在执行中，直接发送即可。")
        } else if terminal_agent {
            disabled_action("当前 AgentRun 已结束。")
        } else {
            disabled_action("当前 AgentRun 没有可投递的 runtime frame。")
        },
        steer: if has_frame && !terminal_agent && delivery_running && supports_steering {
            enabled_action()
        } else if !delivery_running {
            disabled_action("当前 AgentRun 未在执行中，不需要运行中 steer。")
        } else if !supports_steering {
            disabled_action("当前执行器不支持对该运行中 AgentRun steer。")
        } else if terminal_agent {
            disabled_action("当前 AgentRun 已结束，不能运行中 steer。")
        } else {
            disabled_action("当前 AgentRun 没有可投递的 runtime frame。")
        },
        cancel: if delivery_running {
            enabled_action()
        } else {
            disabled_action("当前 AgentRun 没有正在执行的 turn。")
        },
    };
    let pending_messages = match runtime_session_id.as_deref() {
        Some(session_id) => state
            .services
            .pending_queue
            .list(session_id)
            .await
            .into_iter()
            .map(pending_message_view)
            .collect(),
        None => Vec::new(),
    };
    let display_title = resolve_workspace_title(state, context).await?;
    let delivery_status = workspace_delivery_status(&execution_state, &context.agent.status);
    Ok(AgentRunWorkspaceView {
        run_ref: LifecycleRunRefDto {
            run_id: context.run.id.to_string(),
        },
        agent_ref: AgentRunRefDto {
            run_id: context.run.id.to_string(),
            agent_id: context.agent.id.to_string(),
        },
        project_id: context.run.project_id.to_string(),
        shell: AgentRunWorkspaceShell {
            display_title,
            title_source: "agentrun_workspace".to_string(),
            workspace_status: context.agent.status.clone(),
            delivery_status,
            last_turn_id: execution_state_turn_id(&execution_state),
            last_activity_at: context.agent.updated_at.to_rfc3339(),
        },
        delivery_runtime_ref: runtime_session_id
            .map(|runtime_session_id| RuntimeSessionRefDto { runtime_session_id }),
        delivery_trace_meta: meta.as_ref().map(runtime_trace_meta),
        control_plane,
        agent: agent_view.map(agent_run_to_contract),
        frame_runtime,
        subject_associations,
        actions,
        pending_messages,
    })
}

async fn resolve_workspace_title(
    state: &AppState,
    context: &AgentRunContext,
) -> Result<String, ApiError> {
    if let Some(project_agent_id) = context.agent.project_agent_id
        && let Some(project_agent) = state
            .repos
            .project_agent_repo
            .get_by_project_and_id(context.run.project_id, project_agent_id)
            .await
            .map_err(ApiError::from)?
    {
        return Ok(project_agent.name);
    }
    Ok(format!("AgentRun {}", context.agent.id))
}

fn runtime_trace_meta(
    meta: &agentdash_application::session::SessionMeta,
) -> RuntimeSessionTraceMeta {
    RuntimeSessionTraceMeta {
        runtime_session_ref: RuntimeSessionRefDto {
            runtime_session_id: meta.id.clone(),
        },
        last_event_seq: meta.last_event_seq,
        executor_session_id: meta.executor_session_id.clone(),
        trace_title: meta.title.clone(),
        trace_title_source: serialized_string(&meta.title_source),
        delivery_status: serialized_string(&meta.last_delivery_status),
        last_turn_id: meta.last_turn_id.clone(),
        terminal_summary: meta.last_terminal_message.clone(),
        updated_at: meta.updated_at,
    }
}

fn pending_message_view(
    preview: agentdash_application::session::PendingMessagePreview,
) -> PendingMessageView {
    PendingMessageView {
        id: preview.id,
        preview: preview.preview,
        has_images: preview.has_images,
        created_at: preview.created_at.to_rfc3339(),
    }
}

fn enabled_action() -> SessionRuntimeActionAvailabilityView {
    SessionRuntimeActionAvailabilityView {
        enabled: true,
        unavailable_reason: None,
    }
}

fn disabled_action(reason: impl Into<String>) -> SessionRuntimeActionAvailabilityView {
    SessionRuntimeActionAvailabilityView {
        enabled: false,
        unavailable_reason: Some(reason.into()),
    }
}

async fn dispatch_message_for_runtime(
    state: Arc<AppState>,
    current_user: agentdash_integration_api::AuthIdentity,
    runtime_session_id: String,
    req: AgentRunMessageRequest,
) -> Result<Json<AgentRunMessageResponse>, ApiError> {
    if req.input.is_empty() {
        return Err(ApiError::BadRequest("input 不能为空".to_string()));
    }
    let executor_config = req
        .executor_config
        .map(serde_json::from_value::<AgentConfig>)
        .transpose()
        .map_err(|error| ApiError::BadRequest(format!("executor_config 非法: {error}")))?;
    let client_command_id = req.client_command_id;
    let delivery = AgentRunMessageLaunchDeliveryPort::new(state.services.session_launch.clone());
    let service = AgentRunMessageService::new(
        state.repos.lifecycle_run_repo.as_ref(),
        state.repos.lifecycle_agent_repo.as_ref(),
        state.repos.agent_frame_repo.as_ref(),
        state.repos.execution_anchor_repo.as_ref(),
        state.repos.agent_run_delivery_command_receipt_repo.as_ref(),
        delivery,
    );
    let dispatch = service
        .dispatch_user_message(AgentRunMessageCommand {
            delivery_runtime_session_id: runtime_session_id,
            input: req.input,
            client_command_id,
            executor_config,
            identity: Some(current_user),
        })
        .await
        .map_err(ApiError::from)?;

    Ok(Json(AgentRunMessageResponse {
        command_receipt: command_receipt_view(dispatch.command_receipt),
        accepted_refs: accepted_refs(
            dispatch.run_id,
            dispatch.agent_id,
            Some(dispatch.frame_id),
            Some(dispatch.frame_revision),
            Some(dispatch.runtime_session_id),
            Some(dispatch.turn_id),
        ),
    }))
}

async fn steer_runtime_session(
    state: Arc<AppState>,
    runtime_session_id: String,
    req: AgentRunSteeringRequest,
) -> Result<Json<AgentRunSteeringResponse>, ApiError> {
    if req.input.is_empty() {
        return Err(ApiError::BadRequest("input 不能为空".to_string()));
    }
    if let Some(expected_runtime_session_id) = req.expected_runtime_session_id.as_deref()
        && expected_runtime_session_id != runtime_session_id
    {
        return Err(ApiError::Conflict(format!(
            "expected_runtime_session_id 不匹配: {expected_runtime_session_id}"
        )));
    }
    if let Some(expected_turn_id) = req.expected_turn_id.as_deref() {
        match state
            .services
            .session_core
            .inspect_session_execution_state(&runtime_session_id)
            .await?
        {
            agentdash_application::session::SessionExecutionState::Running {
                turn_id: Some(active_turn_id),
            } if active_turn_id == expected_turn_id => {}
            _ => {
                return Err(ApiError::Conflict(format!(
                    "expected_turn_id 不匹配: {expected_turn_id}"
                )));
            }
        }
    }
    let client_command_id = req.client_command_id;
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
        command_receipt: accepted_receipt(client_command_id),
        accepted_refs: accepted_refs(
            dispatch.run_id,
            dispatch.agent_id,
            Some(dispatch.frame_id),
            None,
            Some(dispatch.runtime_session_id),
            Some(dispatch.active_turn_id),
        ),
        state: runtime_command_state_dto(execution_state),
    }))
}

fn accepted_receipt(client_command_id: String) -> AgentRunCommandReceipt {
    AgentRunCommandReceipt {
        client_command_id,
        status: "accepted".to_string(),
        duplicate: false,
        message: None,
    }
}

fn command_receipt_view(
    receipt: agentdash_application::workflow::AgentRunCommandReceiptView,
) -> AgentRunCommandReceipt {
    AgentRunCommandReceipt {
        client_command_id: receipt.client_command_id,
        status: receipt.status,
        duplicate: receipt.duplicate,
        message: receipt.message,
    }
}

fn accepted_refs(
    run_id: Uuid,
    agent_id: Uuid,
    frame_id: Option<Uuid>,
    frame_revision: Option<i32>,
    runtime_session_id: Option<String>,
    turn_id: Option<String>,
) -> AgentRunAcceptedRefs {
    AgentRunAcceptedRefs {
        run_ref: LifecycleRunRefDto {
            run_id: run_id.to_string(),
        },
        agent_ref: AgentRunRefDto {
            run_id: run_id.to_string(),
            agent_id: agent_id.to_string(),
        },
        frame_ref: frame_id.map(|frame_id| AgentFrameRefDto {
            agent_id: agent_id.to_string(),
            frame_id: frame_id.to_string(),
            revision: frame_revision,
        }),
        runtime_session_ref: runtime_session_id
            .map(|runtime_session_id| RuntimeSessionRefDto { runtime_session_id }),
        turn_id,
    }
}

pub async fn send_session_message(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(runtime_session_id): Path<String>,
    Json(req): Json<AgentRunMessageRequest>,
) -> Result<Json<AgentRunMessageResponse>, ApiError> {
    if req.client_command_id.trim().is_empty() {
        return Err(ApiError::BadRequest(
            "client_command_id 不能为空".to_string(),
        ));
    }
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

    dispatch_message_for_runtime(state, current_user, runtime_session_id, req).await
}

pub async fn steer_session(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(runtime_session_id): Path<String>,
    Json(req): Json<AgentRunSteeringRequest>,
) -> Result<Json<AgentRunSteeringResponse>, ApiError> {
    if req.client_command_id.trim().is_empty() {
        return Err(ApiError::BadRequest(
            "client_command_id 不能为空".to_string(),
        ));
    }
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

    steer_runtime_session(state, runtime_session_id, req).await
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
    let views: Vec<PendingMessageView> = previews.into_iter().map(pending_message_view).collect();
    Ok(Json(views))
}

async fn enqueue_pending_message(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(runtime_session_id): Path<String>,
    Json(body): Json<EnqueuePendingMessageRequest>,
) -> Result<Json<EnqueuePendingMessageResponse>, ApiError> {
    if body.client_command_id.trim().is_empty() {
        return Err(ApiError::BadRequest(
            "client_command_id 不能为空".to_string(),
        ));
    }
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
        command_receipt: accepted_receipt(body.client_command_id),
        message: pending_message_view(preview),
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
    promote_pending_message_for_runtime(state, runtime_session_id, message_id).await
}

async fn promote_pending_message_for_runtime(
    state: Arc<AppState>,
    runtime_session_id: String,
    message_id: String,
) -> Result<Json<serde_json::Value>, ApiError> {
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
        .map_err(ApiError::from)?;

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

fn parse_uuid(raw: &str, field: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(raw).map_err(|_| ApiError::BadRequest(format!("无效的 {field}: {raw}")))
}

fn serialized_string<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| "unknown".to_string())
}

fn is_terminal_agent_status(status: &str) -> bool {
    matches!(status, "completed" | "failed" | "cancelled")
}

fn workspace_delivery_status(
    execution_state: &agentdash_application::session::SessionExecutionState,
    agent_status: &str,
) -> String {
    match execution_state {
        agentdash_application::session::SessionExecutionState::Running { .. } => {
            "running".to_string()
        }
        agentdash_application::session::SessionExecutionState::Completed { .. } => {
            "completed".to_string()
        }
        agentdash_application::session::SessionExecutionState::Failed { .. } => {
            "failed".to_string()
        }
        agentdash_application::session::SessionExecutionState::Interrupted { .. } => {
            "interrupted".to_string()
        }
        agentdash_application::session::SessionExecutionState::Idle
            if is_terminal_agent_status(agent_status) =>
        {
            agent_status.to_string()
        }
        agentdash_application::session::SessionExecutionState::Idle => "idle".to_string(),
    }
}

fn execution_state_turn_id(
    execution_state: &agentdash_application::session::SessionExecutionState,
) -> Option<String> {
    match execution_state {
        agentdash_application::session::SessionExecutionState::Running { turn_id }
        | agentdash_application::session::SessionExecutionState::Interrupted { turn_id, .. } => {
            turn_id.clone()
        }
        agentdash_application::session::SessionExecutionState::Completed { turn_id }
        | agentdash_application::session::SessionExecutionState::Failed { turn_id, .. } => {
            Some(turn_id.clone())
        }
        agentdash_application::session::SessionExecutionState::Idle => None,
    }
}
