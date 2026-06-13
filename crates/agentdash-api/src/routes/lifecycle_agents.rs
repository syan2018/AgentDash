use std::sync::Arc;

use agentdash_agent_protocol::UserInputBlock;
use agentdash_application::session::{SessionExecutionState, SessionMeta};
use agentdash_application::vfs::ResolvedVfsSurfaceSource as AppResolvedVfsSurfaceSource;
use agentdash_application::workflow::{
    AgentConversationSnapshotInput, AgentConversationSnapshotResolver, AgentFrameSurfaceExt,
    AgentRunMessageCommand, AgentRunMessageLaunchDeliveryPort, AgentRunMessageService,
    AgentRunSteeringCommand, AgentRunSteeringService, ConversationModelConfigInput,
    ConversationModelConfigResolver, conversation_snapshot_id, lifecycle_run_view_builder,
};
use agentdash_contracts::workflow::{
    AgentFrameRefDto, AgentRunAcceptedRefs, AgentRunCommandOnlyRequest,
    AgentRunCommandPreconditionView, AgentRunCommandReceipt, AgentRunComposerSubmitRequest,
    AgentRunComposerSubmitResponse, AgentRunRefDto, AgentRunWorkspaceActionAvailabilityView,
    AgentRunWorkspaceActionSetView, AgentRunWorkspaceControlPlaneStatus,
    AgentRunWorkspaceControlPlaneView, AgentRunWorkspaceListEntry, AgentRunWorkspaceListView,
    AgentRunWorkspaceShell, AgentRunWorkspaceView, ConversationCommandKind,
    ConversationDiagnosticView, LifecycleRunRefDto, LifecycleSubjectAssociationDto,
    PendingMessageView, PendingQueuePauseReasonDto, PendingQueueStateView,
    ResumePendingQueueResponse, RuntimeSessionCommandStateDto, RuntimeSessionRefDto,
    RuntimeSessionTraceMeta, ValidationSeverity,
};
use agentdash_domain::workflow::{LifecycleAgent, LifecycleRun};
use agentdash_spi::AgentConfig;
use axum::{
    Json,
    extract::{Path, State},
};
use uuid::Uuid;

use crate::agent_run_pending::AgentRunPendingDispatcher;
use crate::{
    app_state::AppState,
    auth::{CurrentUser, ProjectPermission, load_project_with_permission},
    routes::{
        lifecycle_contracts::{agent_run_to_contract, subject_association_to_contract},
        lifecycle_views::{agent_frame_runtime_to_view, runtime_refs_for_agent},
        vfs_surfaces::{
            dto as vfs_surface_dto,
            resolver::{build_surface_summary, resolve_agent_run_frame_vfs_for_agent},
        },
    },
    rpc::ApiError,
};

struct AgentRunContext {
    run: LifecycleRun,
    agent: LifecycleAgent,
    delivery_runtime_session_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WorkspaceShellTitle {
    display_title: String,
    title_source: String,
}

enum WorkspaceShellTitleCandidate<'a> {
    DeliveryMeta(&'a SessionMeta),
    WorkspaceFallback(String),
}

pub fn router() -> axum::Router<Arc<AppState>> {
    axum::Router::new()
        .route(
            "/projects/{project_id}/agent-runs",
            axum::routing::get(get_project_agent_runs),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/workspace",
            axum::routing::get(get_agent_run_workspace),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/composer-submit",
            axum::routing::post(submit_agent_run_composer_input),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/pending-messages",
            axum::routing::get(list_agent_run_pending_messages),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/pending-messages/resume",
            axum::routing::post(resume_agent_run_pending_queue),
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
}

pub async fn get_project_agent_runs(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(project_id): Path<String>,
) -> Result<Json<AgentRunWorkspaceListView>, ApiError> {
    let project_id = parse_uuid(&project_id, "project_id")?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::View,
    )
    .await?;

    let runs = state
        .repos
        .lifecycle_run_repo
        .list_by_project(project_id)
        .await
        .map_err(ApiError::from)?;
    let mut entries = Vec::new();
    for run in runs {
        let agents = state
            .repos
            .lifecycle_agent_repo
            .list_by_run(run.id)
            .await
            .map_err(ApiError::from)?;
        for agent in agents {
            let delivery_runtime_session_id = state
                .repos
                .execution_anchor_repo
                .latest_for_agent(agent.id)
                .await
                .map_err(ApiError::from)?
                .map(|anchor| anchor.runtime_session_id);
            let context = AgentRunContext {
                run: run.clone(),
                agent,
                delivery_runtime_session_id,
            };
            let workspace = build_agent_run_workspace_view(&state, &context).await?;
            entries.push(agent_run_workspace_list_entry(&context.run, workspace));
        }
    }
    entries.sort_by(|a, b| b.shell.last_activity_at.cmp(&a.shell.last_activity_at));

    Ok(Json(AgentRunWorkspaceListView {
        project_id: project_id.to_string(),
        agent_runs: entries,
    }))
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

pub async fn submit_agent_run_composer_input(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id)): Path<(String, String)>,
    Json(req): Json<AgentRunComposerSubmitRequest>,
) -> Result<Json<AgentRunComposerSubmitResponse>, ApiError> {
    if req.client_command_id.trim().is_empty() {
        return Err(ApiError::BadRequest(
            "client_command_id 不能为空".to_string(),
        ));
    }
    if req.input.is_empty() {
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
    let runtime_session_id = context.delivery_runtime_session_id.clone().ok_or_else(|| {
        ApiError::Conflict(format!(
            "AgentRun {} / {} 缺少 delivery runtime",
            context.run.id, context.agent.id
        ))
    })?;
    if is_terminal_agent_status(&context.agent.status) {
        return Err(ApiError::Conflict(
            "当前 AgentRun 已结束，不能继续发送消息。".to_string(),
        ));
    }

    let execution_state = state
        .services
        .session_core
        .inspect_session_execution_state(&runtime_session_id)
        .await?;
    ensure_composer_command_precondition_matches_agent_run(
        &req.command,
        &context,
        &runtime_session_id,
        &execution_state,
    )?;
    let supports_steering = match &execution_state {
        SessionExecutionState::Running { turn_id: Some(_) } => {
            state
                .services
                .session_control
                .supports_session_steering(&runtime_session_id)
                .await
        }
        _ => false,
    };
    let accepted_kind = classify_composer_submit_kind(
        &execution_state,
        req.command.command_kind,
        supports_steering,
    )
    .map_err(|code| {
        command_conflict(
            composer_submit_unavailable_message(code),
            code,
            replacement_command_for_state(&execution_state, false),
            serde_json::json!({
                "run_id": context.run.id.to_string(),
                "agent_id": context.agent.id.to_string(),
                "runtime_session_id": runtime_session_id,
                "state": conversation_state_code(&execution_state),
                "submitted_command_kind": req.command.command_kind,
            }),
        )
    })?;

    match accepted_kind {
        ConversationCommandKind::SendNext => {
            let response = dispatch_message_for_runtime(
                state,
                current_user,
                runtime_session_id,
                AgentRunMessageDispatchInput {
                    input: req.input,
                    client_command_id: req.client_command_id,
                    executor_config: req.executor_config,
                },
            )
            .await?;
            Ok(Json(AgentRunComposerSubmitResponse {
                accepted_kind,
                command_receipt: response.command_receipt,
                accepted_refs: Some(response.accepted_refs),
                pending_message: None,
                state: None,
            }))
        }
        ConversationCommandKind::Enqueue => {
            let executor_config = req
                .executor_config
                .map(serde_json::from_value::<AgentConfig>)
                .transpose()
                .map_err(|e| ApiError::BadRequest(format!("executor_config 格式错误: {e}")))?;
            let preview = state
                .services
                .pending_queue
                .enqueue(&runtime_session_id, req.input, executor_config)
                .await;
            let active_turn_id = execution_state_active_turn_id(&execution_state);
            Ok(Json(AgentRunComposerSubmitResponse {
                accepted_kind,
                command_receipt: accepted_receipt(req.client_command_id),
                accepted_refs: Some(accepted_refs(
                    context.run.id,
                    context.agent.id,
                    None,
                    None,
                    Some(runtime_session_id),
                    active_turn_id,
                )),
                pending_message: Some(pending_message_view(preview)),
                state: Some(runtime_command_state_dto(execution_state)),
            }))
        }
        ConversationCommandKind::Steer => {
            let response = steer_runtime_session(
                state,
                runtime_session_id,
                AgentRunSteeringDispatchInput {
                    input: req.input,
                    client_command_id: req.client_command_id,
                },
            )
            .await?;
            Ok(Json(AgentRunComposerSubmitResponse {
                accepted_kind,
                command_receipt: response.command_receipt,
                accepted_refs: Some(response.accepted_refs),
                pending_message: None,
                state: Some(response.state),
            }))
        }
        _ => Err(command_conflict(
            "当前输入提交无法映射到可执行的 AgentRun 文本命令。",
            "command_unavailable",
            replacement_command_for_state(&execution_state, false),
            serde_json::json!({
                "run_id": context.run.id.to_string(),
                "agent_id": context.agent.id.to_string(),
                "runtime_session_id": runtime_session_id,
                "state": conversation_state_code(&execution_state),
                "accepted_kind": accepted_kind,
            }),
        )),
    }
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
    let runtime_session_id = context.delivery_runtime_session_id.clone().ok_or_else(|| {
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

async fn resume_agent_run_pending_queue(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id)): Path<(String, String)>,
    Json(body): Json<AgentRunCommandOnlyRequest>,
) -> Result<Json<ResumePendingQueueResponse>, ApiError> {
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
    ensure_agent_run_command_allowed(
        state.as_ref(),
        &context,
        &runtime_session_id,
        AgentRunCommandPrecondition::ResumePendingQueue {
            command: body.command.clone(),
        },
    )
    .await?;
    let execution_state = state
        .services
        .session_core
        .inspect_session_execution_state(&runtime_session_id)
        .await?;
    if matches!(
        execution_state,
        SessionExecutionState::Running { .. } | SessionExecutionState::Cancelling { .. }
    ) {
        state
            .services
            .pending_queue
            .resume(&runtime_session_id)
            .await;
        return Ok(Json(ResumePendingQueueResponse {
            resumed: true,
            dispatched: false,
            accepted_refs: None,
        }));
    }
    if is_terminal_agent_status(&context.agent.status) {
        state
            .services
            .pending_queue
            .resume(&runtime_session_id)
            .await;
        return Ok(Json(ResumePendingQueueResponse {
            resumed: true,
            dispatched: false,
            accepted_refs: None,
        }));
    }

    let dispatcher = AgentRunPendingDispatcher::new(
        state.repos.clone(),
        state.services.pending_queue.clone(),
        state.services.session_launch.clone(),
    );
    let dispatch = dispatcher
        .resume_queue(&runtime_session_id)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(ResumePendingQueueResponse {
        resumed: true,
        dispatched: dispatch.is_some(),
        accepted_refs: dispatch.map(|dispatch| {
            accepted_refs(
                dispatch.run_id,
                dispatch.agent_id,
                Some(dispatch.frame_id),
                Some(dispatch.frame_revision),
                Some(dispatch.runtime_session_id),
                Some(dispatch.turn_id),
            )
        }),
    }))
}

async fn promote_agent_run_pending_message(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id, message_id)): Path<(String, String, String)>,
    Json(body): Json<AgentRunCommandOnlyRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
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
    ensure_agent_run_command_allowed(
        state.as_ref(),
        &context,
        &runtime_session_id,
        AgentRunCommandPrecondition::PromotePending {
            command: body.command.clone(),
        },
    )
    .await?;
    promote_pending_message_for_runtime(state, runtime_session_id, message_id).await
}

async fn cancel_agent_run(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id)): Path<(String, String)>,
    Json(body): Json<AgentRunCommandOnlyRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
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
    ensure_agent_run_command_allowed(
        state.as_ref(),
        &context,
        &runtime_session_id,
        AgentRunCommandPrecondition::Cancel {
            command: body.command.clone(),
        },
    )
    .await?;
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
    let delivery_runtime_session_id =
        delivery_runtime_session_for_agent_run(state, run.id, agent.id).await?;
    Ok(AgentRunContext {
        run,
        agent,
        delivery_runtime_session_id,
    })
}

async fn delivery_runtime_session_for_agent_run(
    state: &AppState,
    run_id: Uuid,
    agent_id: Uuid,
) -> Result<Option<String>, ApiError> {
    let anchors = state
        .repos
        .execution_anchor_repo
        .list_by_run(run_id)
        .await
        .map_err(ApiError::from)?;
    Ok(anchors
        .into_iter()
        .filter(|anchor| anchor.agent_id == agent_id)
        .max_by_key(|anchor| anchor.updated_at)
        .map(|anchor| anchor.runtime_session_id))
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
    let frame_resolution =
        resolve_agent_run_frame_vfs_for_agent(state, &context.run, &context.agent).await?;
    let frame = frame_resolution
        .as_ref()
        .map(|resolution| resolution.frame.clone());
    let frame_ref = frame.as_ref().map(|frame| (frame.id, frame.revision));
    let frame_execution_profile = frame
        .as_ref()
        .and_then(|frame| frame.typed_execution_profile());
    let resource_surface = match frame_resolution.as_ref() {
        Some(resolution) => {
            let source = AppResolvedVfsSurfaceSource::AgentRun {
                run_id: context.run.id,
                agent_id: context.agent.id,
            };
            let surface = build_surface_summary(state, &source, &resolution.vfs).await;
            Some(vfs_surface_dto::surface_from_application(surface))
        }
        None => None,
    };
    let frame_runtime = match frame.as_ref() {
        Some(frame) => {
            let runtime_refs = runtime_refs_for_agent(state, context.agent.id).await?;
            Some(agent_frame_runtime_to_view(frame, runtime_refs))
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
    let delivery_running_active = matches!(
        execution_state,
        agentdash_application::session::SessionExecutionState::Running { turn_id: Some(_) }
    );
    let delivery_starting_claimed = matches!(
        execution_state,
        agentdash_application::session::SessionExecutionState::Running { turn_id: None }
    );
    let delivery_cancelling = matches!(
        execution_state,
        agentdash_application::session::SessionExecutionState::Cancelling { .. }
    );
    let terminal_agent = is_terminal_agent_status(&context.agent.status);
    let has_frame = frame_runtime.is_some();
    let has_delivery_runtime = runtime_session_id.is_some();
    let supports_steering = match runtime_session_id.as_deref() {
        Some(session_id) if delivery_running_active => {
            state
                .services
                .session_control
                .supports_session_steering(session_id)
                .await
        }
        _ => false,
    };
    let control_plane = if terminal_agent {
        AgentRunWorkspaceControlPlaneView {
            status: AgentRunWorkspaceControlPlaneStatus::Terminal,
            reason: Some("当前 AgentRun 已结束。".to_string()),
        }
    } else if !has_delivery_runtime {
        AgentRunWorkspaceControlPlaneView {
            status: AgentRunWorkspaceControlPlaneStatus::DeliveryMissing,
            reason: Some("当前 AgentRun 缺少可投递的 runtime 通道。".to_string()),
        }
    } else if !has_frame {
        AgentRunWorkspaceControlPlaneView {
            status: AgentRunWorkspaceControlPlaneStatus::FrameMissing,
            reason: Some("当前 AgentRun 没有可投递的 runtime frame。".to_string()),
        }
    } else if delivery_cancelling {
        AgentRunWorkspaceControlPlaneView {
            status: AgentRunWorkspaceControlPlaneStatus::Cancelling,
            reason: Some("当前 AgentRun 正在取消中，等待执行器收口。".to_string()),
        }
    } else if delivery_running {
        AgentRunWorkspaceControlPlaneView {
            status: AgentRunWorkspaceControlPlaneStatus::Running,
            reason: Some(if delivery_starting_claimed {
                "当前 AgentRun 正在启动中，等待 active turn 建立。".to_string()
            } else {
                "当前 AgentRun 正在执行中。".to_string()
            }),
        }
    } else {
        AgentRunWorkspaceControlPlaneView {
            status: AgentRunWorkspaceControlPlaneStatus::Ready,
            reason: None,
        }
    };
    let actions = AgentRunWorkspaceActionSetView {
        send_next: if has_delivery_runtime
            && has_frame
            && !terminal_agent
            && !delivery_running
            && !delivery_cancelling
        {
            enabled_action()
        } else if !has_delivery_runtime {
            disabled_action("当前 AgentRun 缺少可投递的 runtime 通道。")
        } else if delivery_cancelling {
            disabled_action("当前 AgentRun 正在取消中，等待执行器收口后再发送下一轮消息。")
        } else if delivery_running {
            disabled_action("当前 AgentRun 正在执行中，不能并发发送下一轮消息。")
        } else if terminal_agent {
            disabled_action("当前 AgentRun 已结束，不能继续发送消息。")
        } else {
            disabled_action("当前 AgentRun 没有可投递的 runtime frame。")
        },
        enqueue: if has_delivery_runtime && has_frame && !terminal_agent && delivery_running_active
        {
            enabled_action()
        } else if !has_delivery_runtime {
            disabled_action("当前 AgentRun 缺少可投递的 runtime 通道。")
        } else if delivery_cancelling {
            disabled_action("当前 AgentRun 正在取消中，不能排队新消息。")
        } else if delivery_starting_claimed {
            disabled_action("当前 AgentRun 正在启动中，等待 active turn 建立后才能排队。")
        } else if !delivery_running {
            disabled_action("当前 AgentRun 未在执行中，直接发送即可。")
        } else if terminal_agent {
            disabled_action("当前 AgentRun 已结束。")
        } else {
            disabled_action("当前 AgentRun 没有可投递的 runtime frame。")
        },
        steer: if has_delivery_runtime
            && has_frame
            && !terminal_agent
            && delivery_running_active
            && !delivery_cancelling
            && supports_steering
        {
            enabled_action()
        } else if !has_delivery_runtime {
            disabled_action("当前 AgentRun 缺少可投递的 runtime 通道。")
        } else if delivery_cancelling {
            disabled_action("当前 AgentRun 正在取消中，不能运行中 steer。")
        } else if delivery_starting_claimed {
            disabled_action("当前 AgentRun 正在启动中，等待 active turn 建立后才能 steer。")
        } else if !delivery_running {
            disabled_action("当前 AgentRun 未在执行中，不需要运行中 steer。")
        } else if !supports_steering {
            disabled_action("当前执行器不支持对该运行中 AgentRun steer。")
        } else if terminal_agent {
            disabled_action("当前 AgentRun 已结束，不能运行中 steer。")
        } else {
            disabled_action("当前 AgentRun 没有可投递的 runtime frame。")
        },
        cancel: if delivery_running || delivery_cancelling {
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
    let pause_reason = match runtime_session_id.as_deref() {
        Some(session_id) => state.services.pending_queue.is_paused(session_id).await,
        None => None,
    };
    let pending_visible_message_count = pending_messages.len();
    let pending_queue = match runtime_session_id.as_deref() {
        Some(_) => pending_queue_state_view(
            pause_reason,
            has_delivery_runtime && !terminal_agent,
            pending_visible_message_count,
        ),
        None => pending_queue_state_view(None, false, 0),
    };
    let project_agent_preset_config = match context.agent.project_agent_id {
        Some(project_agent_id) => state
            .repos
            .project_agent_repo
            .get_by_project_and_id(context.run.project_id, project_agent_id)
            .await
            .map_err(ApiError::from)?
            .map(|project_agent| {
                project_agent
                    .preset_config()
                    .map(|preset| preset.to_agent_config(&project_agent.agent_type))
            })
            .transpose()
            .map_err(ApiError::from)?,
        None => None,
    };
    let model_config = ConversationModelConfigResolver::resolve(ConversationModelConfigInput {
        project_agent_preset: project_agent_preset_config.as_ref(),
        frame_execution_profile: frame_execution_profile.as_ref(),
        ..Default::default()
    })
    .view;
    let resource_diagnostics =
        workspace_resource_diagnostics(context.run.id, resource_surface.as_ref());
    let conversation = AgentConversationSnapshotResolver::resolve(AgentConversationSnapshotInput {
        project_id: context.run.project_id,
        run_id: context.run.id,
        agent_id: context.agent.id,
        frame_ref,
        delivery_runtime_session_id: runtime_session_id.clone(),
        subject_associations: subject_associations.clone(),
        execution_state: execution_state.clone(),
        terminal_agent,
        supports_steering,
        pending_paused: pause_reason.is_some(),
        pending_visible_message_count,
        resource_surface,
        resource_diagnostics,
        model_config,
    });
    let shell_title = match meta.as_ref() {
        Some(meta) => {
            select_workspace_shell_title(WorkspaceShellTitleCandidate::DeliveryMeta(meta))
        }
        None => {
            let display_title = resolve_workspace_title(state, context).await?;
            select_workspace_shell_title(WorkspaceShellTitleCandidate::WorkspaceFallback(
                display_title,
            ))
        }
    };
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
            display_title: shell_title.display_title,
            title_source: shell_title.title_source,
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
        pending_queue,
        pending_messages,
        resource_surface: conversation.resource_surface.clone(),
        conversation: Some(conversation),
    })
}

fn workspace_resource_diagnostics(
    run_id: Uuid,
    resource_surface: Option<&agentdash_contracts::vfs::ResolvedVfsSurface>,
) -> Vec<ConversationDiagnosticView> {
    lifecycle_resource_surface_diagnostics(run_id, resource_surface)
}

fn lifecycle_resource_surface_diagnostics(
    run_id: Uuid,
    resource_surface: Option<&agentdash_contracts::vfs::ResolvedVfsSurface>,
) -> Vec<ConversationDiagnosticView> {
    let has_lifecycle_mount = resource_surface
        .map(|surface| {
            surface
                .mounts
                .iter()
                .any(|mount| mount.id == "lifecycle" && mount.provider == "lifecycle_vfs")
        })
        .unwrap_or(false);
    if has_lifecycle_mount {
        return Vec::new();
    }

    vec![ConversationDiagnosticView {
        code: "resource_surface_lifecycle_mount_missing".to_string(),
        severity: ValidationSeverity::Error,
        message: "当前 AgentRun workspace resource_surface 缺少 lifecycle_vfs mount。".to_string(),
        detail: Some(serde_json::json!({
            "run_id": run_id,
        })),
    }]
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

fn select_workspace_shell_title(
    candidate: WorkspaceShellTitleCandidate<'_>,
) -> WorkspaceShellTitle {
    match candidate {
        WorkspaceShellTitleCandidate::DeliveryMeta(meta) => WorkspaceShellTitle {
            display_title: meta.title.clone(),
            title_source: serialized_string(&meta.title_source),
        },
        WorkspaceShellTitleCandidate::WorkspaceFallback(display_title) => WorkspaceShellTitle {
            display_title,
            title_source: "agentrun_workspace".to_string(),
        },
    }
}

fn runtime_trace_meta(meta: &SessionMeta) -> RuntimeSessionTraceMeta {
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

fn agent_run_workspace_list_entry(
    run: &LifecycleRun,
    workspace: AgentRunWorkspaceView,
) -> AgentRunWorkspaceListEntry {
    let subject_association = workspace.subject_associations.first();
    AgentRunWorkspaceListEntry {
        run_ref: workspace.run_ref,
        agent_ref: workspace.agent_ref,
        project_id: workspace.project_id,
        shell: workspace.shell,
        run_status: lifecycle_run_status_to_contract(run.status),
        delivery_runtime_ref: workspace.delivery_runtime_ref,
        delivery_trace_meta: workspace.delivery_trace_meta,
        frame_ref: workspace.frame_runtime.map(|frame| frame.frame_ref),
        subject_ref: subject_association.map(|association| association.subject_ref.clone()),
        subject_label: subject_association.and_then(subject_label_from_metadata),
    }
}

fn subject_label_from_metadata(association: &LifecycleSubjectAssociationDto) -> Option<String> {
    let metadata = association.metadata.as_ref()?;
    ["label", "title", "name"]
        .iter()
        .find_map(|key| metadata.get(key).and_then(|value| value.as_str()))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
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

pub(crate) fn pending_queue_state_view(
    pause_reason: Option<agentdash_application::session::QueuePauseReason>,
    can_resume: bool,
    visible_message_count: usize,
) -> PendingQueueStateView {
    let pause_reason_dto = pause_reason.map(|reason| match reason {
        agentdash_application::session::QueuePauseReason::TurnFailed => {
            PendingQueuePauseReasonDto::TurnFailed
        }
        agentdash_application::session::QueuePauseReason::TurnInterrupted => {
            PendingQueuePauseReasonDto::TurnInterrupted
        }
    });
    let message = pause_reason_dto.as_ref().map(|reason| match reason {
        PendingQueuePauseReasonDto::TurnFailed => "上一轮失败，pending 队列已暂停。",
        PendingQueuePauseReasonDto::TurnInterrupted => "上一轮已中断，pending 队列已暂停。",
    });
    PendingQueueStateView {
        paused: pause_reason_dto.is_some() && visible_message_count > 0,
        pause_reason: pause_reason_dto,
        message: message.map(str::to_string),
        can_resume: can_resume && pause_reason.is_some() && visible_message_count > 0,
    }
}

fn enabled_action() -> AgentRunWorkspaceActionAvailabilityView {
    AgentRunWorkspaceActionAvailabilityView {
        enabled: true,
        unavailable_reason: None,
    }
}

fn disabled_action(reason: impl Into<String>) -> AgentRunWorkspaceActionAvailabilityView {
    AgentRunWorkspaceActionAvailabilityView {
        enabled: false,
        unavailable_reason: Some(reason.into()),
    }
}

fn lifecycle_run_status_to_contract(
    status: agentdash_domain::workflow::LifecycleRunStatus,
) -> agentdash_contracts::workflow::LifecycleRunStatus {
    match status {
        agentdash_domain::workflow::LifecycleRunStatus::Draft => {
            agentdash_contracts::workflow::LifecycleRunStatus::Draft
        }
        agentdash_domain::workflow::LifecycleRunStatus::Ready => {
            agentdash_contracts::workflow::LifecycleRunStatus::Ready
        }
        agentdash_domain::workflow::LifecycleRunStatus::Running => {
            agentdash_contracts::workflow::LifecycleRunStatus::Running
        }
        agentdash_domain::workflow::LifecycleRunStatus::Blocked => {
            agentdash_contracts::workflow::LifecycleRunStatus::Blocked
        }
        agentdash_domain::workflow::LifecycleRunStatus::Completed => {
            agentdash_contracts::workflow::LifecycleRunStatus::Completed
        }
        agentdash_domain::workflow::LifecycleRunStatus::Failed => {
            agentdash_contracts::workflow::LifecycleRunStatus::Failed
        }
        agentdash_domain::workflow::LifecycleRunStatus::Cancelled => {
            agentdash_contracts::workflow::LifecycleRunStatus::Cancelled
        }
    }
}

async fn dispatch_message_for_runtime(
    state: Arc<AppState>,
    current_user: agentdash_integration_api::AuthIdentity,
    runtime_session_id: String,
    req: AgentRunMessageDispatchInput,
) -> Result<AgentRunMessageDispatchResponse, ApiError> {
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

    Ok(AgentRunMessageDispatchResponse {
        command_receipt: command_receipt_view(dispatch.command_receipt),
        accepted_refs: accepted_refs(
            dispatch.run_id,
            dispatch.agent_id,
            Some(dispatch.frame_id),
            Some(dispatch.frame_revision),
            Some(dispatch.runtime_session_id),
            Some(dispatch.turn_id),
        ),
    })
}

async fn steer_runtime_session(
    state: Arc<AppState>,
    runtime_session_id: String,
    req: AgentRunSteeringDispatchInput,
) -> Result<AgentRunSteeringDispatchResponse, ApiError> {
    if req.input.is_empty() {
        return Err(ApiError::BadRequest("input 不能为空".to_string()));
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

    Ok(AgentRunSteeringDispatchResponse {
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
    })
}

struct AgentRunMessageDispatchInput {
    input: Vec<UserInputBlock>,
    client_command_id: String,
    executor_config: Option<serde_json::Value>,
}

struct AgentRunMessageDispatchResponse {
    command_receipt: AgentRunCommandReceipt,
    accepted_refs: AgentRunAcceptedRefs,
}

struct AgentRunSteeringDispatchInput {
    input: Vec<UserInputBlock>,
    client_command_id: String,
}

struct AgentRunSteeringDispatchResponse {
    command_receipt: AgentRunCommandReceipt,
    accepted_refs: AgentRunAcceptedRefs,
    state: RuntimeSessionCommandStateDto,
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
    let dispatch_result = service
        .steer(AgentRunSteeringCommand {
            delivery_runtime_session_id: runtime_session_id.clone(),
            input: msg.input.clone(),
        })
        .await;
    let dispatch = match dispatch_result {
        Ok(dispatch) => dispatch,
        Err(error) => {
            state
                .services
                .pending_queue
                .requeue_front(&runtime_session_id, msg)
                .await;
            return Err(ApiError::from(error));
        }
    };

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
        agentdash_application::session::SessionExecutionState::Cancelling { turn_id } => {
            RuntimeSessionCommandStateDto {
                status: "cancelling".to_string(),
                turn_id,
                message: Some("当前执行正在取消中。".to_string()),
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

enum AgentRunCommandPrecondition {
    PromotePending {
        command: AgentRunCommandPreconditionView,
    },
    ResumePendingQueue {
        command: AgentRunCommandPreconditionView,
    },
    Cancel {
        command: AgentRunCommandPreconditionView,
    },
}

async fn ensure_agent_run_command_allowed(
    state: &AppState,
    context: &AgentRunContext,
    runtime_session_id: &str,
    command: AgentRunCommandPrecondition,
) -> Result<(), ApiError> {
    let execution_state = state
        .services
        .session_core
        .inspect_session_execution_state(runtime_session_id)
        .await?;
    let frame_resolution =
        resolve_agent_run_frame_vfs_for_agent(state, &context.run, &context.agent).await?;
    let frame_ref = frame_resolution
        .as_ref()
        .map(|resolution| (resolution.frame.id, resolution.frame.revision));
    let terminal_agent = is_terminal_agent_status(&context.agent.status);
    let state_code = conversation_state_code(&execution_state);
    let detail = || {
        serde_json::json!({
            "run_id": context.run.id.to_string(),
            "agent_id": context.agent.id.to_string(),
            "runtime_session_id": runtime_session_id,
            "state": state_code,
            "active_turn_id": execution_state_active_turn_id(&execution_state),
        })
    };
    let expected_kind = command.expected_kind();
    ensure_command_submission_matches_snapshot(
        command.command_precondition(),
        expected_kind,
        context,
        runtime_session_id,
        frame_ref,
        &execution_state,
        terminal_agent,
    )?;

    if terminal_agent && !matches!(&command, AgentRunCommandPrecondition::Cancel { .. }) {
        return Err(command_conflict(
            "当前 AgentRun 已结束，不能执行该命令。",
            "command_unavailable",
            None,
            detail(),
        ));
    }

    match command {
        AgentRunCommandPrecondition::PromotePending { .. } => {
            match &execution_state {
                SessionExecutionState::Running { turn_id: Some(_) } => {}
                SessionExecutionState::Running { turn_id: None } => {
                    return Err(command_conflict(
                        "当前 AgentRun 正在启动中，等待 active turn 建立后才能投递 pending 消息。",
                        "starting_claimed",
                        None,
                        detail(),
                    ));
                }
                _ => {
                    return Err(command_conflict(
                        "当前 AgentRun 不在可投递 pending 消息的运行状态。",
                        "command_unavailable",
                        None,
                        detail(),
                    ));
                }
            }
            if !state
                .services
                .session_control
                .supports_session_steering(runtime_session_id)
                .await
            {
                return Err(command_conflict(
                    "当前执行器不支持对该 AgentRun 投递 pending steer。",
                    "connector_steer_unsupported",
                    None,
                    detail(),
                ));
            }
            Ok(())
        }
        AgentRunCommandPrecondition::ResumePendingQueue { .. } => {
            let visible_message_count = state
                .services
                .pending_queue
                .list(runtime_session_id)
                .await
                .len();
            let paused = state
                .services
                .pending_queue
                .is_paused(runtime_session_id)
                .await;
            if paused.is_some() && visible_message_count > 0 {
                Ok(())
            } else {
                Err(command_conflict(
                    "当前没有需要用户恢复的 pending 队列。",
                    "command_unavailable",
                    None,
                    serde_json::json!({
                        "run_id": context.run.id.to_string(),
                        "agent_id": context.agent.id.to_string(),
                        "runtime_session_id": runtime_session_id,
                        "state": state_code,
                        "visible_message_count": visible_message_count,
                        "paused": paused.is_some(),
                    }),
                ))
            }
        }
        AgentRunCommandPrecondition::Cancel { .. } => match &execution_state {
            SessionExecutionState::Running { .. } | SessionExecutionState::Cancelling { .. } => {
                Ok(())
            }
            _ => Err(command_conflict(
                "当前 AgentRun 没有正在执行的 turn。",
                "command_unavailable",
                None,
                detail(),
            )),
        },
    }
}

impl AgentRunCommandPrecondition {
    fn expected_kind(&self) -> ConversationCommandKind {
        match self {
            AgentRunCommandPrecondition::PromotePending { .. } => {
                ConversationCommandKind::PromotePending
            }
            AgentRunCommandPrecondition::ResumePendingQueue { .. } => {
                ConversationCommandKind::ResumePendingQueue
            }
            AgentRunCommandPrecondition::Cancel { .. } => ConversationCommandKind::Cancel,
        }
    }

    fn command_precondition(&self) -> &AgentRunCommandPreconditionView {
        match self {
            AgentRunCommandPrecondition::PromotePending { command }
            | AgentRunCommandPrecondition::ResumePendingQueue { command }
            | AgentRunCommandPrecondition::Cancel { command } => command,
        }
    }
}

fn ensure_command_submission_matches_snapshot(
    command: &AgentRunCommandPreconditionView,
    expected_kind: ConversationCommandKind,
    context: &AgentRunContext,
    runtime_session_id: &str,
    frame_ref: Option<(Uuid, i32)>,
    execution_state: &SessionExecutionState,
    terminal_agent: bool,
) -> Result<(), ApiError> {
    let current_active_turn_id = execution_state_active_turn_id(execution_state);
    let current_frame_id = frame_ref.map(|(frame_id, _)| frame_id.to_string());
    let current_snapshot_id = conversation_snapshot_id(
        context.run.id,
        context.agent.id,
        frame_ref,
        Some(runtime_session_id),
        execution_state,
        terminal_agent,
    );
    let stale_detail = |reason: &str| {
        serde_json::json!({
            "reason": reason,
            "run_id": context.run.id.to_string(),
            "agent_id": context.agent.id.to_string(),
            "runtime_session_id": runtime_session_id,
            "state": conversation_state_code(execution_state),
            "expected_command_kind": expected_kind,
            "submitted_command_kind": command.command_kind,
            "expected_command_id": command_id_for_kind(expected_kind),
            "submitted_command_id": command.command_id,
            "expected_snapshot_id": current_snapshot_id,
            "submitted_snapshot_id": command.stale_guard.snapshot_id,
            "expected_frame_id": current_frame_id,
            "submitted_frame_id": command.stale_guard.frame_id,
            "expected_active_turn_id": current_active_turn_id,
            "submitted_active_turn_id": command.stale_guard.active_turn_id,
            "snapshot_refresh_required": true,
        })
    };

    if command.command_kind != expected_kind {
        return Err(stale_command_conflict(
            execution_state,
            terminal_agent,
            stale_detail("command_kind_mismatch"),
        ));
    }
    if command.command_id != command_id_for_kind(expected_kind) {
        return Err(stale_command_conflict(
            execution_state,
            terminal_agent,
            stale_detail("command_id_mismatch"),
        ));
    }
    if command.stale_guard.run_id != context.run.id.to_string()
        || command.stale_guard.agent_id != context.agent.id.to_string()
    {
        return Err(stale_command_conflict(
            execution_state,
            terminal_agent,
            stale_detail("agent_run_identity_mismatch"),
        ));
    }
    if command.stale_guard.runtime_session_id.as_deref() != Some(runtime_session_id) {
        return Err(stale_command_conflict(
            execution_state,
            terminal_agent,
            stale_detail("runtime_session_mismatch"),
        ));
    }
    if command.stale_guard.frame_id != current_frame_id {
        return Err(stale_command_conflict(
            execution_state,
            terminal_agent,
            stale_detail("frame_mismatch"),
        ));
    }
    if command.stale_guard.active_turn_id != current_active_turn_id {
        return Err(stale_command_conflict(
            execution_state,
            terminal_agent,
            stale_detail("active_turn_mismatch"),
        ));
    }
    if command.stale_guard.snapshot_id != current_snapshot_id {
        return Err(stale_command_conflict(
            execution_state,
            terminal_agent,
            stale_detail("snapshot_id_mismatch"),
        ));
    }

    Ok(())
}

fn stale_command_conflict(
    execution_state: &SessionExecutionState,
    terminal_agent: bool,
    detail: serde_json::Value,
) -> ApiError {
    command_conflict(
        "AgentRun command snapshot 已过期，请使用最新 workspace state 重试。",
        "stale_command",
        replacement_command_for_state(execution_state, terminal_agent),
        detail,
    )
}

fn ensure_composer_command_precondition_matches_agent_run(
    command: &AgentRunCommandPreconditionView,
    context: &AgentRunContext,
    runtime_session_id: &str,
    execution_state: &SessionExecutionState,
) -> Result<(), ApiError> {
    let detail = || {
        serde_json::json!({
            "run_id": context.run.id.to_string(),
            "agent_id": context.agent.id.to_string(),
            "runtime_session_id": runtime_session_id,
            "state": conversation_state_code(execution_state),
            "submitted_command_kind": command.command_kind,
            "submitted_command_id": command.command_id,
            "submitted_guard": &command.stale_guard,
        })
    };

    if command.stale_guard.run_id != context.run.id.to_string()
        || command.stale_guard.agent_id != context.agent.id.to_string()
    {
        return Err(stale_command_conflict(
            execution_state,
            false,
            serde_json::json!({
                "reason": "agent_run_identity_mismatch",
                "run_id": context.run.id.to_string(),
                "agent_id": context.agent.id.to_string(),
                "runtime_session_id": runtime_session_id,
                "state": conversation_state_code(execution_state),
                "submitted_run_id": &command.stale_guard.run_id,
                "submitted_agent_id": &command.stale_guard.agent_id,
                "snapshot_refresh_required": true,
            }),
        ));
    }

    if !matches!(
        command.command_kind,
        ConversationCommandKind::SendNext
            | ConversationCommandKind::Enqueue
            | ConversationCommandKind::Steer
    ) {
        return Err(command_conflict(
            "当前输入提交只能使用 send_next、enqueue 或 steer 命令意图。",
            "command_unavailable",
            replacement_command_for_state(execution_state, false),
            detail(),
        ));
    }

    Ok(())
}

fn classify_composer_submit_kind(
    execution_state: &SessionExecutionState,
    requested_kind: ConversationCommandKind,
    supports_steering: bool,
) -> Result<ConversationCommandKind, &'static str> {
    match execution_state {
        SessionExecutionState::Idle
        | SessionExecutionState::Completed { .. }
        | SessionExecutionState::Failed { .. }
        | SessionExecutionState::Interrupted { .. } => Ok(ConversationCommandKind::SendNext),
        SessionExecutionState::Running { turn_id: Some(_) } => {
            if requested_kind == ConversationCommandKind::Steer && supports_steering {
                Ok(ConversationCommandKind::Steer)
            } else {
                Ok(ConversationCommandKind::Enqueue)
            }
        }
        SessionExecutionState::Running { turn_id: None } => Err("starting_claimed"),
        SessionExecutionState::Cancelling { .. } => Err("cancelling"),
    }
}

fn composer_submit_unavailable_message(code: &str) -> &'static str {
    match code {
        "starting_claimed" => "当前 AgentRun 正在启动中，等待 active turn 建立。",
        "cancelling" => "当前 AgentRun 正在取消中，等待执行器收口。",
        _ => "当前 AgentRun 暂时不能接收新的输入。",
    }
}

fn replacement_command_for_state(
    execution_state: &SessionExecutionState,
    terminal_agent: bool,
) -> Option<&'static str> {
    if terminal_agent {
        return None;
    }
    match execution_state {
        SessionExecutionState::Idle
        | SessionExecutionState::Completed { .. }
        | SessionExecutionState::Failed { .. }
        | SessionExecutionState::Interrupted { .. } => Some("send_next"),
        SessionExecutionState::Running { turn_id: Some(_) } => Some("enqueue"),
        SessionExecutionState::Running { turn_id: None }
        | SessionExecutionState::Cancelling { .. } => None,
    }
}

fn command_id_for_kind(kind: ConversationCommandKind) -> &'static str {
    match kind {
        ConversationCommandKind::StartDraft => "start_draft",
        ConversationCommandKind::SendNext => "send_next",
        ConversationCommandKind::Enqueue => "enqueue",
        ConversationCommandKind::Steer => "steer",
        ConversationCommandKind::PromotePending => "promote_pending",
        ConversationCommandKind::ResumePendingQueue => "resume_pending_queue",
        ConversationCommandKind::Cancel => "cancel",
    }
}

fn command_conflict(
    message: impl Into<String>,
    error_code: impl Into<String>,
    replacement_command: Option<&str>,
    detail: serde_json::Value,
) -> ApiError {
    ApiError::ConflictWithCode {
        message: message.into(),
        error_code: error_code.into(),
        replacement_command: replacement_command.map(str::to_string),
        detail: Some(detail),
    }
}

fn conversation_state_code(execution_state: &SessionExecutionState) -> &'static str {
    match execution_state {
        SessionExecutionState::Idle => "ready",
        SessionExecutionState::Running { turn_id: None } => "starting_claimed",
        SessionExecutionState::Running { turn_id: Some(_) } => "running_active",
        SessionExecutionState::Cancelling { .. } => "cancelling",
        SessionExecutionState::Completed { .. } => "completed",
        SessionExecutionState::Failed { .. } => "failed",
        SessionExecutionState::Interrupted { .. } => "interrupted",
    }
}

fn execution_state_active_turn_id(execution_state: &SessionExecutionState) -> Option<String> {
    match execution_state {
        SessionExecutionState::Running {
            turn_id: Some(turn_id),
        }
        | SessionExecutionState::Cancelling {
            turn_id: Some(turn_id),
        } => Some(turn_id.clone()),
        _ => None,
    }
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
        agentdash_application::session::SessionExecutionState::Cancelling { .. } => {
            "cancelling".to_string()
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
        | agentdash_application::session::SessionExecutionState::Cancelling { turn_id }
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

#[cfg(test)]
mod tests {
    use agentdash_application::session::{ExecutionStatus, TitleSource};
    use agentdash_contracts::vfs::{
        ResolvedMountEditCapabilities, ResolvedMountPurpose, ResolvedMountSummary,
        ResolvedVfsSurface, ResolvedVfsSurfaceSource,
    };
    use agentdash_contracts::workflow::ConversationCommandStaleGuardView;
    use agentdash_domain::workflow::LifecycleRun;

    use super::*;

    fn test_session_meta(title: &str, title_source: TitleSource) -> SessionMeta {
        SessionMeta {
            id: "runtime-session-1".to_string(),
            title: title.to_string(),
            title_source,
            created_at: 1,
            updated_at: 2,
            last_event_seq: 3,
            last_delivery_status: ExecutionStatus::Idle,
            last_turn_id: None,
            last_terminal_message: None,
            executor_session_id: None,
        }
    }

    fn test_shell(display_title: &str, title_source: &str) -> AgentRunWorkspaceShell {
        AgentRunWorkspaceShell {
            display_title: display_title.to_string(),
            title_source: title_source.to_string(),
            workspace_status: "running".to_string(),
            delivery_status: "idle".to_string(),
            last_turn_id: None,
            last_activity_at: "2026-06-12T00:00:00Z".to_string(),
        }
    }

    fn test_surface(mounts: Vec<ResolvedMountSummary>) -> ResolvedVfsSurface {
        ResolvedVfsSurface {
            surface_ref: "agent-run:run-1:agent-1".to_string(),
            source: ResolvedVfsSurfaceSource::AgentRun {
                run_id: "run-1".to_string(),
                agent_id: "agent-1".to_string(),
            },
            mounts,
            default_mount_id: None,
        }
    }

    fn test_mount(id: &str, provider: &str, purpose: ResolvedMountPurpose) -> ResolvedMountSummary {
        ResolvedMountSummary {
            id: id.to_string(),
            display_name: id.to_string(),
            provider: provider.to_string(),
            backend_id: provider.to_string(),
            capabilities: vec!["read".to_string(), "list".to_string()],
            default_write: false,
            purpose,
            backend_online: None,
            file_count: None,
            edit_capabilities: ResolvedMountEditCapabilities::default(),
        }
    }

    fn test_agent_run_context() -> AgentRunContext {
        let run = LifecycleRun::new_graphless(Uuid::new_v4());
        let agent = LifecycleAgent::new_root(run.id, run.project_id, "PI_AGENT");
        AgentRunContext {
            run,
            agent,
            delivery_runtime_session_id: Some("session-1".to_string()),
        }
    }

    fn composer_precondition(
        kind: ConversationCommandKind,
        context: &AgentRunContext,
    ) -> AgentRunCommandPreconditionView {
        AgentRunCommandPreconditionView {
            command_id: command_id_for_kind(kind).to_string(),
            command_kind: kind,
            stale_guard: ConversationCommandStaleGuardView {
                snapshot_id: "stale-snapshot".to_string(),
                run_id: context.run.id.to_string(),
                agent_id: context.agent.id.to_string(),
                frame_id: Some(Uuid::new_v4().to_string()),
                runtime_session_id: Some("old-session".to_string()),
                active_turn_id: Some("old-turn".to_string()),
            },
        }
    }

    #[test]
    fn workspace_shell_title_uses_delivery_session_meta_when_present() {
        let meta = test_session_meta("Session meta title", TitleSource::Source);

        let shell_title =
            select_workspace_shell_title(WorkspaceShellTitleCandidate::DeliveryMeta(&meta));

        assert_eq!(shell_title.display_title, "Session meta title");
        assert_eq!(shell_title.title_source, "source");
    }

    #[test]
    fn composer_submit_reclassifies_stale_running_input_after_terminal() {
        let completed = SessionExecutionState::Completed {
            turn_id: "turn-1".to_string(),
        };
        let context = test_agent_run_context();
        let command = composer_precondition(ConversationCommandKind::Enqueue, &context);

        ensure_composer_command_precondition_matches_agent_run(
            &command,
            &context,
            "session-1",
            &completed,
        )
        .expect("composer input should not require stale frame or turn guard");

        let kind =
            classify_composer_submit_kind(&completed, ConversationCommandKind::Enqueue, true)
                .expect("terminal follow-up should start next turn");

        assert_eq!(kind, ConversationCommandKind::SendNext);
    }

    #[test]
    fn composer_submit_rejects_non_text_control_command_intent() {
        let running = SessionExecutionState::Running {
            turn_id: Some("turn-1".to_string()),
        };
        let context = test_agent_run_context();
        let command = composer_precondition(ConversationCommandKind::Cancel, &context);

        let error = ensure_composer_command_precondition_matches_agent_run(
            &command,
            &context,
            "session-1",
            &running,
        )
        .expect_err("cancel is not a composer input command");

        match error {
            ApiError::ConflictWithCode { error_code, .. } => {
                assert_eq!(error_code, "command_unavailable");
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn composer_submit_maps_running_input_to_current_runtime_capability() {
        let running = SessionExecutionState::Running {
            turn_id: Some("turn-1".to_string()),
        };

        assert_eq!(
            classify_composer_submit_kind(&running, ConversationCommandKind::SendNext, false),
            Ok(ConversationCommandKind::Enqueue)
        );
        assert_eq!(
            classify_composer_submit_kind(&running, ConversationCommandKind::Steer, true),
            Ok(ConversationCommandKind::Steer)
        );
        assert_eq!(
            classify_composer_submit_kind(&running, ConversationCommandKind::Steer, false),
            Ok(ConversationCommandKind::Enqueue)
        );
    }

    #[test]
    fn workspace_shell_title_uses_workspace_fallback_without_delivery_meta() {
        let shell_title = select_workspace_shell_title(
            WorkspaceShellTitleCandidate::WorkspaceFallback("AgentRun fallback".to_string()),
        );

        assert_eq!(shell_title.display_title, "AgentRun fallback");
        assert_eq!(shell_title.title_source, "agentrun_workspace");
    }

    #[test]
    fn list_entry_inherits_workspace_shell_title() {
        let run = LifecycleRun::new_graphless(Uuid::new_v4());
        let run_id = run.id.to_string();
        let agent_id = Uuid::new_v4().to_string();
        let project_id = run.project_id.to_string();
        let workspace = AgentRunWorkspaceView {
            run_ref: LifecycleRunRefDto {
                run_id: run_id.clone(),
            },
            agent_ref: AgentRunRefDto { run_id, agent_id },
            project_id,
            shell: test_shell("Session meta title", "source"),
            delivery_runtime_ref: None,
            delivery_trace_meta: None,
            control_plane: AgentRunWorkspaceControlPlaneView {
                status: AgentRunWorkspaceControlPlaneStatus::Ready,
                reason: None,
            },
            agent: None,
            frame_runtime: None,
            subject_associations: Vec::new(),
            actions: AgentRunWorkspaceActionSetView {
                send_next: enabled_action(),
                enqueue: disabled_action("not running"),
                steer: disabled_action("not running"),
                cancel: disabled_action("not running"),
            },
            pending_queue: pending_queue_state_view(None, true, 0),
            pending_messages: Vec::new(),
            resource_surface: None,
            conversation: None,
        };

        let entry = agent_run_workspace_list_entry(&run, workspace);

        assert_eq!(entry.shell.display_title, "Session meta title");
        assert_eq!(entry.shell.title_source, "source");
    }

    #[test]
    fn pending_queue_state_view_exposes_pause_reason_and_resume() {
        let view = pending_queue_state_view(
            Some(agentdash_application::session::QueuePauseReason::TurnInterrupted),
            true,
            1,
        );

        assert!(view.paused);
        assert_eq!(
            view.pause_reason,
            Some(PendingQueuePauseReasonDto::TurnInterrupted)
        );
        assert_eq!(
            view.message.as_deref(),
            Some("上一轮已中断，pending 队列已暂停。")
        );
        assert!(view.can_resume);
    }

    #[test]
    fn pending_queue_state_view_hides_empty_paused_prompt() {
        let view = pending_queue_state_view(
            Some(agentdash_application::session::QueuePauseReason::TurnInterrupted),
            true,
            0,
        );

        assert!(!view.paused);
        assert!(!view.can_resume);
        assert_eq!(
            view.pause_reason,
            Some(PendingQueuePauseReasonDto::TurnInterrupted)
        );
    }

    #[test]
    fn lifecycle_resource_diagnostic_reports_missing_mount() {
        let surface = test_surface(vec![test_mount(
            "main",
            "relay_fs",
            ResolvedMountPurpose::Workspace,
        )]);

        let run_id = Uuid::new_v4();
        let diagnostics = lifecycle_resource_surface_diagnostics(run_id, Some(&surface));

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "resource_surface_lifecycle_mount_missing"
                && diagnostic.severity == ValidationSeverity::Error
        }));
    }

    #[test]
    fn lifecycle_resource_diagnostic_accepts_lifecycle_mount() {
        let surface = test_surface(vec![
            test_mount("main", "relay_fs", ResolvedMountPurpose::Workspace),
            test_mount(
                "lifecycle",
                "lifecycle_vfs",
                ResolvedMountPurpose::Lifecycle,
            ),
        ]);

        let diagnostics = lifecycle_resource_surface_diagnostics(Uuid::new_v4(), Some(&surface));

        assert!(diagnostics.is_empty());
    }
}
