use std::convert::Infallible;
use std::io;
use std::sync::Arc;
use std::time::Duration;

use axum::{
    Json,
    body::{Body, Bytes},
    extract::{Path, Query, State},
    http::HeaderMap,
    response::IntoResponse,
};
use tokio::time::MissedTickBehavior;
use uuid::Uuid;

use crate::routes::lifecycle_contracts::{
    agent_run_to_contract, lifecycle_run_view_to_contract, subject_association_to_contract,
};
use crate::routes::lifecycle_views::{agent_frame_runtime_to_view, runtime_refs_for_agent};
use crate::{app_state::AppState, rpc::ApiError};
use agentdash_application::lifecycle::run_view_builder;
use agentdash_application::session::{
    ExecutionStatus, SessionExecutionState, SessionForkRequest, SessionMeta,
    SessionProjectionRollbackRequest as ApplicationProjectionRollbackRequest, TitleSource,
};
use agentdash_contracts::session::{
    ApproveToolCallResponse, CreateSessionForkRequest, DeleteSessionResponse,
    RejectToolCallResponse, RollbackSessionProjectionRequest, SessionEventResponse,
    SessionEventsPageResponse, SessionForkChildSessionResponse, SessionForkResponse,
    SessionLineageViewResponse, SessionNdjsonEnvelope, SessionProjectionRollbackResponse,
    SessionProjectionViewResponse,
};
use agentdash_contracts::workflow::{
    RuntimeSessionExecutionAnchorDto, RuntimeSessionRefDto, SessionRuntimeControlPlaneStatus,
    SessionRuntimeControlPlaneView, SessionRuntimeControlView, SessionShellDto,
};
use agentdash_domain::workflow::{LifecycleRun, RuntimeSessionExecutionAnchor};

use crate::auth::{CurrentUser, ProjectPermission, load_project_with_permission};
use crate::dto::{
    ContextAuditEventDto, ContextAuditQuery, NdjsonStreamQuery, RejectToolApprovalRequest,
    SessionEventsQuery, SessionExecutionStateResponse, UpdateSessionMetaRequest,
};

/// Session trace 权限检查通过 RuntimeSessionExecutionAnchor 进入 LifecycleRun project。
pub async fn ensure_session_permission(
    state: &AppState,
    user: &agentdash_integration_api::AuthIdentity,
    session_id: &str,
    permission: ProjectPermission,
) -> Result<(), ApiError> {
    let _meta = state
        .services
        .session_core
        .get_session_meta(session_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("会话 {session_id} 不存在")))?;
    let anchor = match state
        .repos
        .execution_anchor_repo
        .find_by_session(session_id)
        .await?
    {
        Some(anchor) => anchor,
        None => {
            return Err(ApiError::BadRequest(format!(
                "runtime session 缺少 RuntimeSessionExecutionAnchor: {session_id}"
            )));
        }
    };
    let run = load_lifecycle_run_for_session(state, anchor.run_id).await?;
    load_project_with_permission(state, user, run.project_id, permission).await?;
    Ok(())
}

const RUNTIME_TRACE_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(20);

pub fn router() -> axum::Router<std::sync::Arc<crate::app_state::AppState>> {
    axum::Router::new()
        .route(
            "/sessions/{id}",
            axum::routing::get(get_session).delete(delete_session),
        )
        .route(
            "/sessions/{id}/runtime-control",
            axum::routing::get(get_session_runtime_control),
        )
        .route(
            "/sessions/{id}/meta",
            axum::routing::get(get_session_meta).patch(update_session_meta),
        )
        .route(
            "/sessions/{id}/state",
            axum::routing::get(get_session_state),
        )
        .route(
            "/sessions/{id}/events",
            axum::routing::get(list_session_events),
        )
        .route(
            "/sessions/{id}/context/projection",
            axum::routing::get(get_session_context_projection),
        )
        .route(
            "/sessions/{id}/lineage",
            axum::routing::get(get_session_lineage),
        )
        .route("/sessions/{id}/fork", axum::routing::post(fork_session))
        .route(
            "/sessions/{id}/projection/rollback",
            axum::routing::post(rollback_session_projection),
        )
        .route(
            "/sessions/{id}/context/audit",
            axum::routing::get(get_session_context_audit),
        )
        .route(
            "/sessions/{id}/tool-approvals/{tool_call_id}/approve",
            axum::routing::post(approve_tool_call),
        )
        .route(
            "/sessions/{id}/tool-approvals/{tool_call_id}/reject",
            axum::routing::post(reject_tool_call),
        )
        .route(
            "/sessions/{id}/stream/ndjson",
            axum::routing::get(session_stream_ndjson),
        )
}

pub async fn get_session(
    State(state): State<Arc<AppState>>,
    CurrentUser(_current_user): CurrentUser,
    Path(session_id): Path<String>,
) -> Result<Json<SessionMeta>, ApiError> {
    let meta = state
        .services
        .session_core
        .get_session_meta(&session_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("会话 {} 不存在", session_id)))?;
    Ok(Json(meta))
}

pub async fn get_session_runtime_control(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(runtime_session_id): Path<String>,
) -> Result<Json<SessionRuntimeControlView>, ApiError> {
    let meta = state
        .services
        .session_core
        .get_session_meta(&runtime_session_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("会话 {} 不存在", runtime_session_id)))?;
    let Some(anchor) = state
        .repos
        .execution_anchor_repo
        .find_by_session(&runtime_session_id)
        .await?
    else {
        return Ok(Json(SessionRuntimeControlView {
            runtime_session_ref: RuntimeSessionRefDto { runtime_session_id },
            session_meta: session_shell_dto(&meta),
            control_plane: SessionRuntimeControlPlaneView {
                status: SessionRuntimeControlPlaneStatus::UnboundTrace,
                reason: Some(
                    "当前 Session 只有 runtime trace，没有绑定 Agent 控制面。".to_string(),
                ),
            },
            anchor: None,
            run: None,
            agent: None,
            frame_runtime: None,
            subject_associations: Vec::new(),
        }));
    };

    let run = load_lifecycle_run_for_session(state.as_ref(), anchor.run_id).await?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        run.project_id,
        ProjectPermission::View,
    )
    .await?;
    let agent = state
        .repos
        .lifecycle_agent_repo
        .get(anchor.agent_id)
        .await?
        .ok_or_else(|| {
            ApiError::NotFound(format!("lifecycle_agent 不存在: {}", anchor.agent_id))
        })?;
    if agent.run_id != run.id || agent.project_id != run.project_id {
        return Err(ApiError::BadRequest(format!(
            "runtime session anchor agent 与 run 不一致: {runtime_session_id}"
        )));
    }
    let frame = state
        .repos
        .agent_frame_repo
        .get_current(agent.id)
        .await?
        .or(state
            .repos
            .agent_frame_repo
            .get(anchor.launch_frame_id)
            .await?);
    let frame_runtime = match frame {
        Some(frame) => {
            let runtime_refs = runtime_refs_for_agent(state.as_ref(), agent.id).await?;
            Some(agent_frame_runtime_to_view(&frame, runtime_refs))
        }
        None => None,
    };
    let run_view = run_view_builder::build_lifecycle_run_view(&state.repos, &run).await?;
    let agent_view = run_view
        .agents
        .iter()
        .find(|view| view.agent_ref.agent_id == agent.id.to_string())
        .cloned();
    let agent_id_string = agent.id.to_string();
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
    let execution_state = state
        .services
        .session_core
        .inspect_session_execution_state(&runtime_session_id)
        .await?;
    let delivery_running = meta.last_delivery_status == ExecutionStatus::Running
        || matches!(execution_state, SessionExecutionState::Running { .. });
    let delivery_cancelling = matches!(execution_state, SessionExecutionState::Cancelling { .. });
    let terminal_agent = is_terminal_agent_status(&agent.status);
    let has_frame = frame_runtime.is_some();
    let control_plane = if terminal_agent {
        SessionRuntimeControlPlaneView {
            status: SessionRuntimeControlPlaneStatus::Terminal,
            reason: Some("当前 Agent 已结束。".to_string()),
        }
    } else if !has_frame {
        SessionRuntimeControlPlaneView {
            status: SessionRuntimeControlPlaneStatus::FrameMissing,
            reason: Some("当前 Agent 没有可投递的 runtime frame。".to_string()),
        }
    } else if delivery_cancelling {
        SessionRuntimeControlPlaneView {
            status: SessionRuntimeControlPlaneStatus::AnchoredCancelling,
            reason: Some("当前 Session 正在取消中，等待执行器收口。".to_string()),
        }
    } else if delivery_running {
        SessionRuntimeControlPlaneView {
            status: SessionRuntimeControlPlaneStatus::AnchoredRunning,
            reason: Some("当前 Session 正在执行中。".to_string()),
        }
    } else {
        SessionRuntimeControlPlaneView {
            status: SessionRuntimeControlPlaneStatus::AnchoredIdle,
            reason: None,
        }
    };
    Ok(Json(SessionRuntimeControlView {
        runtime_session_ref: RuntimeSessionRefDto { runtime_session_id },
        session_meta: session_shell_dto(&meta),
        control_plane,
        anchor: Some(anchor_dto(&anchor)),
        run: Some(lifecycle_run_view_to_contract(run_view)),
        agent: agent_view.map(agent_run_to_contract),
        frame_runtime,
        subject_associations,
    }))
}

fn map_session_event(
    event: agentdash_application::session::PersistedSessionEvent,
) -> SessionEventResponse {
    event.into()
}

fn stream_event_payload(
    event: agentdash_application::session::PersistedSessionEvent,
) -> SessionNdjsonEnvelope {
    SessionNdjsonEnvelope::event(event)
}

pub async fn get_session_state(
    State(state): State<Arc<AppState>>,
    CurrentUser(_current_user): CurrentUser,
    Path(session_id): Path<String>,
) -> Result<Json<SessionExecutionStateResponse>, ApiError> {
    state
        .services
        .session_core
        .get_session_meta(&session_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("会话 {} 不存在", session_id)))?;

    let execution_state = state
        .services
        .session_core
        .inspect_session_execution_state(&session_id)
        .await?;

    let response = match execution_state {
        SessionExecutionState::Idle => SessionExecutionStateResponse {
            session_id,
            status: "idle".to_string(),
            turn_id: None,
            message: None,
        },
        SessionExecutionState::Running { turn_id } => SessionExecutionStateResponse {
            session_id,
            status: "running".to_string(),
            turn_id,
            message: None,
        },
        SessionExecutionState::Cancelling { turn_id } => SessionExecutionStateResponse {
            session_id,
            status: "cancelling".to_string(),
            turn_id,
            message: Some("当前执行正在取消中。".to_string()),
        },
        SessionExecutionState::Completed { turn_id } => SessionExecutionStateResponse {
            session_id,
            status: "completed".to_string(),
            turn_id: Some(turn_id),
            message: None,
        },
        SessionExecutionState::Failed { turn_id, message } => SessionExecutionStateResponse {
            session_id,
            status: "failed".to_string(),
            turn_id: Some(turn_id),
            message,
        },
        SessionExecutionState::Interrupted { turn_id, message } => SessionExecutionStateResponse {
            session_id,
            status: "interrupted".to_string(),
            turn_id,
            message,
        },
    };

    Ok(Json(response))
}

async fn load_lifecycle_run_for_session(
    state: &AppState,
    run_id: Uuid,
) -> Result<LifecycleRun, ApiError> {
    state
        .repos
        .lifecycle_run_repo
        .get_by_id(run_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("lifecycle_run 不存在: {run_id}")))
}

fn session_shell_dto(meta: &SessionMeta) -> SessionShellDto {
    SessionShellDto {
        id: meta.id.clone(),
        title: meta.title.clone(),
        title_source: serialized_string(&meta.title_source),
        created_at: meta.created_at,
        updated_at: meta.updated_at,
        last_event_seq: meta.last_event_seq,
        last_turn_id: meta.last_turn_id.clone(),
        last_delivery_status: serialized_string(&meta.last_delivery_status),
    }
}

fn anchor_dto(anchor: &RuntimeSessionExecutionAnchor) -> RuntimeSessionExecutionAnchorDto {
    RuntimeSessionExecutionAnchorDto {
        runtime_session_id: anchor.runtime_session_id.clone(),
        run_id: anchor.run_id.to_string(),
        agent_id: anchor.agent_id.to_string(),
        launch_frame_id: anchor.launch_frame_id.to_string(),
        orchestration_id: anchor.orchestration_id.map(|id| id.to_string()),
        node_path: anchor.node_path.clone(),
        node_attempt: anchor.node_attempt,
        created_by_kind: anchor.created_by_kind.clone(),
        created_at: anchor.created_at.to_rfc3339(),
        updated_at: anchor.updated_at.to_rfc3339(),
    }
}

fn is_terminal_agent_status(status: &str) -> bool {
    matches!(status, "completed" | "failed" | "cancelled")
}

fn serialized_string<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| "unknown".to_string())
}

pub async fn list_session_events(
    State(state): State<Arc<AppState>>,
    CurrentUser(_current_user): CurrentUser,
    Path(session_id): Path<String>,
    Query(query): Query<SessionEventsQuery>,
) -> Result<Json<SessionEventsPageResponse>, ApiError> {
    let after_seq = query.after_seq.unwrap_or(0);
    let limit = query.limit.unwrap_or(500).clamp(1, 2_000);
    let page = state
        .services
        .session_eventing
        .list_event_page(&session_id, after_seq, limit)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(SessionEventsPageResponse {
        snapshot_seq: page.snapshot_seq,
        events: page.events.into_iter().map(map_session_event).collect(),
        has_more: page.has_more,
        next_after_seq: page.next_after_seq,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_application::session::{
        ExecutionStatus, RuntimeTraceLaunchState, SessionPromptLifecycle,
        SessionRepositoryRehydrateMode, TitleSource, resolve_session_prompt_lifecycle,
    };

    fn test_meta(id: &str, event_seq: u64, executor_session_id: Option<&str>) -> SessionMeta {
        SessionMeta {
            id: id.to_string(),
            title: "测试".to_string(),
            title_source: TitleSource::Auto,
            created_at: 1,
            updated_at: 1,
            last_event_seq: event_seq,
            last_delivery_status: if event_seq > 0 {
                ExecutionStatus::Completed
            } else {
                ExecutionStatus::Idle
            },
            last_turn_id: if event_seq > 0 {
                Some("t-last".to_string())
            } else {
                None
            },
            last_terminal_message: None,
            executor_session_id: executor_session_id.map(String::from),
        }
    }

    fn trace_state(meta: &SessionMeta) -> RuntimeTraceLaunchState {
        RuntimeTraceLaunchState::from(meta)
    }

    #[test]
    fn session_prompt_lifecycle_kind_marks_pending_as_owner_bootstrap() {
        let meta = test_meta("sess-1", 0, None);
        assert_eq!(
            resolve_session_prompt_lifecycle(&trace_state(&meta), false, false, true),
            SessionPromptLifecycle::OwnerBootstrap
        );
    }

    #[test]
    fn session_prompt_lifecycle_kind_requires_repository_rehydrate_after_cold_restart() {
        let meta = test_meta("sess-2", 12, None);
        assert_eq!(
            resolve_session_prompt_lifecycle(&trace_state(&meta), false, false, false),
            SessionPromptLifecycle::RepositoryRehydrate(
                SessionRepositoryRehydrateMode::SystemContext,
            )
        );
        assert_eq!(
            resolve_session_prompt_lifecycle(&trace_state(&meta), true, false, false),
            SessionPromptLifecycle::Plain
        );
    }

    #[test]
    fn session_prompt_lifecycle_prefers_executor_follow_up_when_available() {
        let meta = test_meta("sess-3", 5, Some("exec-1"));
        assert_eq!(
            resolve_session_prompt_lifecycle(&trace_state(&meta), false, true, false),
            SessionPromptLifecycle::Plain
        );
    }

    #[test]
    fn session_prompt_lifecycle_uses_executor_state_restore_when_supported() {
        let meta = test_meta("sess-4", 7, None);
        assert_eq!(
            resolve_session_prompt_lifecycle(&trace_state(&meta), false, true, false),
            SessionPromptLifecycle::RepositoryRehydrate(
                SessionRepositoryRehydrateMode::ExecutorState,
            )
        );
    }
}

/// GET /sessions/{id}/context/projection — 返回当前模型可见上下文投影。
pub async fn get_session_context_projection(
    State(state): State<Arc<AppState>>,
    CurrentUser(_current_user): CurrentUser,
    Path(session_id): Path<String>,
) -> Result<Json<SessionProjectionViewResponse>, ApiError> {
    let envelope = state
        .services
        .session_eventing
        .build_agent_context_envelope(&session_id)
        .await
        .map_err(ApiError::from)?;
    let context_items = state
        .services
        .session_eventing
        .build_context_usage_items(&session_id, envelope.head_event_seq)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(
        SessionProjectionViewResponse::from_envelope_and_context_items(envelope, context_items),
    ))
}

/// POST /sessions/{id}/fork — 基于当前模型投影创建可独立恢复的 child session。
pub async fn fork_session(
    State(state): State<Arc<AppState>>,
    CurrentUser(_current_user): CurrentUser,
    Path(session_id): Path<String>,
    Json(req): Json<CreateSessionForkRequest>,
) -> Result<Json<SessionForkResponse>, ApiError> {
    let result = state
        .services
        .session_branching
        .fork_session(SessionForkRequest {
            parent_session_id: session_id.clone(),
            title: req.title,
            fork_point_ref: req.fork_point_ref.map(Into::into),
            fork_point_compaction_id: req.fork_point_compaction_id,
            metadata_json: req.metadata_json.unwrap_or_else(|| serde_json::json!({})),
        })
        .await
        .map_err(api_error_from_io)?;

    Ok(Json(SessionForkResponse {
        parent_session_id: result.parent_session_id,
        child_session: SessionForkChildSessionResponse {
            id: result.child_session.id,
            title: result.child_session.title,
            created_at: result.child_session.created_at,
            updated_at: result.child_session.updated_at,
            last_event_seq: result.child_session.last_event_seq,
        },
        lineage: result.lineage.into(),
        child_initial_compaction_id: result.projection_commit.compaction.id,
        projection_version: result.projection_commit.head.projection_version,
        head_event_seq: result.projection_commit.head.head_event_seq,
    }))
}

/// GET /sessions/{id}/lineage — 返回当前 session 的父边、祖先与直接 children。
pub async fn get_session_lineage(
    State(state): State<Arc<AppState>>,
    CurrentUser(_current_user): CurrentUser,
    Path(session_id): Path<String>,
) -> Result<Json<SessionLineageViewResponse>, ApiError> {
    let view = state
        .services
        .session_branching
        .lineage_view(&session_id)
        .await
        .map_err(api_error_from_io)?;

    Ok(Json(SessionLineageViewResponse {
        session_id,
        lineage: view.lineage.map(Into::into),
        ancestors: view.ancestors.into_iter().map(Into::into).collect(),
        children: view.children.into_iter().map(Into::into).collect(),
    }))
}

/// POST /sessions/{id}/projection/rollback — 移动模型可见 projection head，不删除审计事件。
pub async fn rollback_session_projection(
    State(state): State<Arc<AppState>>,
    CurrentUser(_current_user): CurrentUser,
    Path(session_id): Path<String>,
    Json(req): Json<RollbackSessionProjectionRequest>,
) -> Result<Json<SessionProjectionRollbackResponse>, ApiError> {
    let result = state
        .services
        .session_branching
        .rollback_model_projection(ApplicationProjectionRollbackRequest {
            session_id: session_id.clone(),
            target_event_seq: req.target_event_seq,
            active_compaction_id: req.active_compaction_id,
            reason: req.reason,
        })
        .await
        .map_err(api_error_from_io)?;

    Ok(Json(SessionProjectionRollbackResponse {
        session_id,
        event: result.event.into(),
        head_event_seq: result.head.head_event_seq,
        active_compaction_id: result.head.active_compaction_id,
        projection_version: result.head.projection_version,
        updated_by_event_seq: result.head.updated_by_event_seq,
    }))
}

/// GET /sessions/{id}/meta — 返回完整 session meta。
pub async fn get_session_meta(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(session_id): Path<String>,
) -> Result<Json<SessionMeta>, ApiError> {
    get_session(State(state), CurrentUser(current_user), Path(session_id)).await
}

/// PATCH /sessions/{id}/meta — 用户手动修改会话 meta。
pub async fn update_session_meta(
    State(state): State<Arc<AppState>>,
    CurrentUser(_current_user): CurrentUser,
    Path(session_id): Path<String>,
    Json(req): Json<UpdateSessionMetaRequest>,
) -> Result<Json<SessionMeta>, ApiError> {
    if req.title.is_none() {
        return Err(ApiError::BadRequest("必须提供 title".to_string()));
    }
    if let Some(title) = req.title.as_deref()
        && title.trim().is_empty()
    {
        return Err(ApiError::BadRequest("标题不能为空".to_string()));
    }

    let meta = state
        .services
        .session_core
        .update_session_meta(&session_id, |meta| {
            if let Some(title) = req.title.as_deref() {
                let title = title.trim();
                if !title.is_empty() {
                    meta.title = title.to_string();
                    meta.title_source = TitleSource::User;
                }
            }
        })
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::NotFound(format!("会话 {} 不存在", session_id)))?;

    Ok(Json(meta))
}

pub async fn delete_session(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(session_id): Path<String>,
) -> Result<Json<DeleteSessionResponse>, ApiError> {
    ensure_session_permission(
        state.as_ref(),
        &current_user,
        &session_id,
        ProjectPermission::Edit,
    )
    .await?;
    state
        .services
        .session_core
        .delete_session(&session_id)
        .await?;
    Ok(Json(DeleteSessionResponse {
        deleted: true,
        session_id,
    }))
}

pub async fn approve_tool_call(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((session_id, tool_call_id)): Path<(String, String)>,
) -> Result<Json<ApproveToolCallResponse>, ApiError> {
    ensure_session_permission(
        state.as_ref(),
        &current_user,
        &session_id,
        ProjectPermission::Edit,
    )
    .await?;
    state
        .services
        .session_control
        .approve_tool_call(&session_id, &tool_call_id)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(ApproveToolCallResponse {
        approved: true,
        session_id,
        tool_call_id,
    }))
}

pub async fn reject_tool_call(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((session_id, tool_call_id)): Path<(String, String)>,
    Json(req): Json<RejectToolApprovalRequest>,
) -> Result<Json<RejectToolCallResponse>, ApiError> {
    ensure_session_permission(
        state.as_ref(),
        &current_user,
        &session_id,
        ProjectPermission::Edit,
    )
    .await?;
    state
        .services
        .session_control
        .reject_tool_call(&session_id, &tool_call_id, req.reason.clone())
        .await
        .map_err(ApiError::from)?;

    Ok(Json(RejectToolCallResponse {
        rejected: true,
        session_id,
        tool_call_id,
    }))
}

/// Session trace stream（Fetch Streaming / NDJSON）
pub async fn session_stream_ndjson(
    State(state): State<Arc<AppState>>,
    CurrentUser(_current_user): CurrentUser,
    Path(session_id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<NdjsonStreamQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let resume_from = parse_resume_from_header(&headers, "x-stream-since-id")?
        .or(query.since_id)
        .unwrap_or(0);
    tracing::info!(
        session_id = %session_id,
        resume_from = resume_from,
        "Session trace stream 连接建立（NDJSON）"
    );

    let subscription = state
        .services
        .session_eventing
        .subscribe_after(&session_id, resume_from)
        .await
        .map_err(ApiError::from)?;
    let replayed = subscription.backlog.len();
    tracing::info!(
        session_id = %session_id,
        replayed_count = replayed,
        snapshot_seq = subscription.snapshot_seq,
        "Session trace stream 历史补发完成（NDJSON）"
    );

    let stream = async_stream::stream! {
        let mut seq = resume_from;
        for event in subscription.backlog {
            seq = event.event_seq;
            if let Some(line) = to_ndjson_line(&stream_event_payload(event)) {
                yield Ok::<Bytes, Infallible>(line);
            }
        }

        if let Some(line) = to_ndjson_line(&SessionNdjsonEnvelope::connected(seq)) {
            yield Ok::<Bytes, Infallible>(line);
        }

        let mut heartbeat_tick = tokio::time::interval(RUNTIME_TRACE_HEARTBEAT_INTERVAL);
        heartbeat_tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let mut rx = subscription.rx;

        loop {
            tokio::select! {
                next = rx.recv() => {
                    match next {
                        Ok(event) => {
                            if event.event_seq <= subscription.snapshot_seq {
                                continue;
                            }
                            seq = event.event_seq;
                            if let Some(line) = to_ndjson_line(&stream_event_payload(event)) {
                                yield Ok::<Bytes, Infallible>(line);
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!(
                                session_id = %session_id,
                                lagged = n,
                                "Session trace stream 订阅落后，部分消息被跳过（NDJSON）"
                            );
                            continue;
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            tracing::info!(
                                session_id = %session_id,
                                last_seq = seq,
                                "Session trace stream 连接关闭：广播通道关闭（NDJSON）"
                            );
                            break;
                        }
                    }
                }
                _ = heartbeat_tick.tick() => {
                    if let Some(line) = to_ndjson_line(&SessionNdjsonEnvelope::heartbeat_now()) {
                        yield Ok::<Bytes, Infallible>(line);
                    }
                }
            }
        }
    };

    Ok((
        [
            (
                axum::http::header::CONTENT_TYPE,
                "application/x-ndjson; charset=utf-8",
            ),
            (axum::http::header::CACHE_CONTROL, "no-cache, no-transform"),
            (axum::http::header::CONNECTION, "keep-alive"),
            (axum::http::header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        Body::from_stream(stream),
    ))
}

fn parse_resume_from_header(
    headers: &HeaderMap,
    header_name: &'static str,
) -> Result<Option<u64>, ApiError> {
    let Some(value) = headers.get(header_name) else {
        return Ok(None);
    };
    let raw = value
        .to_str()
        .map_err(|_| ApiError::BadRequest(format!("{header_name} 不是有效 UTF-8")))?;
    let parsed = raw
        .parse::<i64>()
        .map_err(|_| ApiError::BadRequest(format!("{header_name} 不是有效整数")))?;
    if parsed < 0 {
        return Err(ApiError::BadRequest(format!("{header_name} 不能为负数")));
    }
    Ok(Some(parsed as u64))
}

fn to_ndjson_line(value: &SessionNdjsonEnvelope) -> Option<Bytes> {
    match serde_json::to_vec(value) {
        Ok(mut bytes) => {
            bytes.push(b'\n');
            Some(Bytes::from(bytes))
        }
        Err(err) => {
            tracing::error!(error = %err, "序列化 Session NDJSON 消息失败");
            None
        }
    }
}

fn api_error_from_io(error: io::Error) -> ApiError {
    match error.kind() {
        io::ErrorKind::InvalidInput | io::ErrorKind::InvalidData => {
            ApiError::BadRequest(error.to_string())
        }
        io::ErrorKind::NotFound => ApiError::NotFound(error.to_string()),
        io::ErrorKind::AlreadyExists => ApiError::Conflict(error.to_string()),
        _ => ApiError::Internal(String::from("内部 IO 错误")),
    }
}

// ═══════════════════════════════════════════════════════════════════
// Context Audit —— Bundle / Fragment 产出与消费的可观测轨迹（Step 10d）
// ═══════════════════════════════════════════════════════════════════

/// Content preview 的最大字节数（超过时截断）。
const CONTEXT_AUDIT_CONTENT_PREVIEW_MAX: usize = 2048;

fn parse_scope_tag(tag: &str) -> Option<agentdash_spi::FragmentScope> {
    match tag {
        "runtime_agent" => Some(agentdash_spi::FragmentScope::RuntimeAgent),
        "title_gen" => Some(agentdash_spi::FragmentScope::TitleGen),
        "summarizer" => Some(agentdash_spi::FragmentScope::Summarizer),
        "bridge_replay" => Some(agentdash_spi::FragmentScope::BridgeReplay),
        "audit" => Some(agentdash_spi::FragmentScope::Audit),
        _ => None,
    }
}

fn scope_set_to_tags(scope: agentdash_spi::FragmentScopeSet) -> Vec<String> {
    let mut tags = Vec::new();
    for (label, s) in [
        ("runtime_agent", agentdash_spi::FragmentScope::RuntimeAgent),
        ("title_gen", agentdash_spi::FragmentScope::TitleGen),
        ("summarizer", agentdash_spi::FragmentScope::Summarizer),
        ("bridge_replay", agentdash_spi::FragmentScope::BridgeReplay),
        ("audit", agentdash_spi::FragmentScope::Audit),
    ] {
        if scope.contains(s) {
            tags.push(label.to_string());
        }
    }
    tags
}

/// `GET /sessions/{id}/context/audit` —— 返回 session 的 Fragment 审计时间线。
///
/// 返回按 `at_ms` 升序的事件列表（审计总线内部已保持插入顺序）。
pub async fn get_session_context_audit(
    State(state): State<Arc<AppState>>,
    CurrentUser(_current_user): CurrentUser,
    Path(session_id): Path<String>,
    Query(query): Query<ContextAuditQuery>,
) -> Result<Json<Vec<ContextAuditEventDto>>, ApiError> {
    let scope = match query.scope.as_deref() {
        Some(raw) => match parse_scope_tag(raw) {
            Some(s) => Some(s),
            None => return Err(ApiError::BadRequest(format!("无效的 scope: {raw}"))),
        },
        None => None,
    };

    let filter = agentdash_application::context::AuditFilter {
        since_ms: query.since_ms,
        scope,
        slot: query.slot.clone(),
        source_prefix: query.source_prefix.clone(),
    };

    let events = state.services.audit_bus.query(&session_id, &filter);
    let dtos: Vec<ContextAuditEventDto> = events
        .into_iter()
        .map(|event| {
            let full_len = event.fragment.content.len();
            let truncated = full_len > CONTEXT_AUDIT_CONTENT_PREVIEW_MAX;
            let preview = if truncated {
                // 按字符边界截断，避免切断 UTF-8 多字节
                let mut end = CONTEXT_AUDIT_CONTENT_PREVIEW_MAX;
                while end > 0 && !event.fragment.content.is_char_boundary(end) {
                    end -= 1;
                }
                event.fragment.content[..end].to_string()
            } else {
                event.fragment.content.clone()
            };
            ContextAuditEventDto {
                event_id: event.event_id,
                bundle_id: event.bundle_id,
                session_id: event.session_id,
                bundle_session_uuid: event.bundle_session_uuid,
                at_ms: event.at_ms,
                trigger: event.trigger.as_tag(),
                slot: event.fragment.slot,
                label: event.fragment.label,
                source: event.fragment.source,
                order: event.fragment.order,
                scope: scope_set_to_tags(event.fragment.scope),
                content_preview: preview,
                content_hash: event.content_hash,
                full_content_available: truncated,
            }
        })
        .collect();

    Ok(Json(dtos))
}
