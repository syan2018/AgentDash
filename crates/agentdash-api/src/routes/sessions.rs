#![allow(clippy::items_after_test_module)]

use agentdash_diagnostics::{DiagnosticErrorContext, Subsystem, diag, diag_error};
use std::convert::Infallible;
use std::io;
use std::sync::Arc;
use std::time::Duration;

use axum::{
    Json,
    body::{Body, Bytes},
    extract::{Path, Query, State},
    http::HeaderMap,
    response::{IntoResponse, Response},
};
use tokio::time::MissedTickBehavior;
use uuid::Uuid;

use crate::{app_state::AppState, rpc::ApiError};
use agentdash_application_runtime_session::session::{
    SessionContextProjectionReadModel, SessionExecutionState, SessionMeta,
};
use agentdash_contracts::session::{
    SessionAttachmentContextContributionResponse, SessionContextUsageAnalysisResponse,
    SessionContextUsageCategoryResponse, SessionContextUsageItemResponse, SessionEventResponse,
    SessionEventsPageResponse, SessionLineageViewResponse, SessionMessageContextBreakdownResponse,
    SessionNdjsonEnvelope, SessionProjectionMessageRefResponse,
    SessionProjectionSegmentProvenanceResponse, SessionProjectionSegmentViewResponse,
    SessionProjectionSourceRangeResponse, SessionProjectionViewResponse,
    SessionToolContextContributionResponse,
};
use agentdash_domain::workflow::LifecycleRun;

use crate::auth::{CurrentUser, ProjectPermission, load_project_with_permission};
use crate::dto::{
    ContextAuditEventDto, ContextAuditQuery, NdjsonStreamQuery, SessionEventsQuery,
    SessionExecutionStateResponse,
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
        .ok_or_else(|| ApiError::NotFound(format!("RuntimeSession trace {session_id} 不存在")))?;
    let anchor = match state
        .repos
        .execution_anchor_repo
        .find_by_session(session_id)
        .await?
    {
        Some(anchor) => anchor,
        None => {
            return Err(ApiError::BadRequest(format!(
                "RuntimeSession trace 缺少 RuntimeSessionExecutionAnchor: {session_id}"
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
        .route("/sessions/{id}", axum::routing::get(get_session))
        .route("/sessions/{id}/meta", axum::routing::get(get_session_meta))
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
        .route(
            "/sessions/{id}/context/audit",
            axum::routing::get(get_session_context_audit),
        )
        .route(
            "/sessions/{id}/stream/ndjson",
            axum::routing::get(session_stream_ndjson),
        )
}

pub async fn get_session(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(session_id): Path<String>,
) -> Result<Json<SessionMeta>, ApiError> {
    ensure_session_permission(
        state.as_ref(),
        &current_user,
        &session_id,
        ProjectPermission::Use,
    )
    .await?;
    let meta = state
        .services
        .session_core
        .get_session_meta(&session_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("RuntimeSession trace {session_id} 不存在")))?;
    Ok(Json(meta))
}

fn map_session_event(
    event: agentdash_application_runtime_session::session::PersistedSessionEvent,
) -> SessionEventResponse {
    event.into()
}

fn stream_event_payload(
    event: agentdash_application_runtime_session::session::PersistedSessionEvent,
) -> SessionNdjsonEnvelope {
    SessionNdjsonEnvelope::event(event)
}

pub async fn get_session_state(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(session_id): Path<String>,
) -> Result<Json<SessionExecutionStateResponse>, ApiError> {
    ensure_session_permission(
        state.as_ref(),
        &current_user,
        &session_id,
        ProjectPermission::Use,
    )
    .await?;

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
        SessionExecutionState::Lost { turn_id, message } => SessionExecutionStateResponse {
            session_id,
            status: "lost".to_string(),
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

pub async fn list_session_events(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(session_id): Path<String>,
    Query(query): Query<SessionEventsQuery>,
) -> Result<Json<SessionEventsPageResponse>, ApiError> {
    ensure_session_permission(
        state.as_ref(),
        &current_user,
        &session_id,
        ProjectPermission::Use,
    )
    .await?;
    Ok(Json(
        load_runtime_session_events_page(state.as_ref(), &session_id, query).await?,
    ))
}

pub(crate) async fn load_runtime_session_events_page(
    state: &AppState,
    session_id: &str,
    query: SessionEventsQuery,
) -> Result<SessionEventsPageResponse, ApiError> {
    ensure_runtime_session_trace_exists(state, session_id).await?;
    let after_seq = query.after_seq.unwrap_or(0);
    let limit = query.limit.unwrap_or(500).clamp(1, 2_000);
    let page = state
        .services
        .session_eventing
        .list_event_page(session_id, after_seq, limit)
        .await
        .map_err(ApiError::from)?;

    Ok(SessionEventsPageResponse {
        snapshot_seq: page.snapshot_seq,
        events: page.events.into_iter().map(map_session_event).collect(),
        has_more: page.has_more,
        next_after_seq: page.next_after_seq,
    })
}

async fn ensure_runtime_session_trace_exists(
    state: &AppState,
    session_id: &str,
) -> Result<(), ApiError> {
    let _meta = state
        .services
        .session_core
        .get_session_meta(session_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("RuntimeSession trace {session_id} 不存在")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_application_runtime_session::session::{
        ExecutionStatus, PromptLaunchPath, RuntimeTraceLaunchState,
        SessionAttachmentContextContribution, SessionContextUsageCategory, SessionContextUsageItem,
        SessionContextUsageReadModel, SessionMessageContextBreakdown,
        SessionProjectionMessageRefReadModel, SessionProjectionSegmentProvenanceReadModel,
        SessionProjectionSegmentReadModel, SessionProjectionSourceRangeReadModel,
        SessionRepositoryRehydrateMode, SessionToolContextContribution, TitleSource,
        resolve_prompt_launch_path,
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
    fn prompt_launch_path_marks_pending_as_owner_bootstrap() {
        let meta = test_meta("sess-1", 0, None);
        assert_eq!(
            resolve_prompt_launch_path(&trace_state(&meta), false, false, true),
            PromptLaunchPath::OwnerBootstrap
        );
    }

    #[test]
    fn prompt_launch_path_requires_repository_rehydrate_after_cold_restart() {
        let meta = test_meta("sess-2", 12, None);
        assert_eq!(
            resolve_prompt_launch_path(&trace_state(&meta), false, false, false),
            PromptLaunchPath::RepositoryRehydrate(SessionRepositoryRehydrateMode::SystemContext,)
        );
        assert_eq!(
            resolve_prompt_launch_path(&trace_state(&meta), true, false, false),
            PromptLaunchPath::Plain
        );
    }

    #[test]
    fn prompt_launch_path_prefers_executor_follow_up_when_available() {
        let meta = test_meta("sess-3", 5, Some("exec-1"));
        assert_eq!(
            resolve_prompt_launch_path(&trace_state(&meta), false, true, false),
            PromptLaunchPath::Plain
        );
    }

    #[test]
    fn prompt_launch_path_uses_executor_state_restore_when_supported() {
        let meta = test_meta("sess-4", 7, None);
        assert_eq!(
            resolve_prompt_launch_path(&trace_state(&meta), false, true, false),
            PromptLaunchPath::RepositoryRehydrate(SessionRepositoryRehydrateMode::ExecutorState,)
        );
    }

    #[test]
    fn context_projection_mapper_preserves_usage_read_facts() {
        let response = session_context_projection_to_response(SessionContextProjectionReadModel {
            session_id: "sess-1".to_string(),
            projection_kind: "model_context".to_string(),
            projection_version: 2,
            head_event_seq: 42,
            active_compaction_id: Some("compaction-1".to_string()),
            token_estimate: Some(128),
            message_count: 1,
            segments: vec![SessionProjectionSegmentReadModel {
                id: "segment-1".to_string(),
                sort_order: 0,
                segment_type: "summary_chunk".to_string(),
                role: "compaction_summary".to_string(),
                origin: "projection".to_string(),
                synthetic: true,
                projection_kind: "compaction_summary".to_string(),
                message_ref: SessionProjectionMessageRefReadModel {
                    turn_id: "_projection:summary".to_string(),
                    entry_index: 0,
                },
                source_event_seq: None,
                source_range: Some(SessionProjectionSourceRangeReadModel {
                    start_event_seq: 1,
                    end_event_seq: 30,
                }),
                projection_segment_id: Some("segment-1".to_string()),
                preview: "summary".to_string(),
                token_estimate: Some(20),
                attachment_tokens: 0,
                attachment_names: Vec::new(),
                tool_names: vec!["read_file".to_string()],
                provenance: SessionProjectionSegmentProvenanceReadModel {
                    compaction_id: Some("compaction-1".to_string()),
                    projection_version: Some(2),
                    segment_type: Some("summary_chunk".to_string()),
                    strategy: Some("summary_prefix".to_string()),
                    trigger: Some("auto".to_string()),
                    phase: Some("pre_provider".to_string()),
                },
            }],
            context_usage: SessionContextUsageReadModel {
                categories: vec![SessionContextUsageCategory {
                    kind: "system_developer".to_string(),
                    label: "System / Developer".to_string(),
                    token_estimate: 12,
                    source: "context_frame".to_string(),
                    deferred: false,
                }],
                items: vec![SessionContextUsageItem {
                    kind: "system_developer".to_string(),
                    label: "System / Developer".to_string(),
                    name: "Identity".to_string(),
                    token_estimate: 12,
                    source: "context_frame".to_string(),
                    deferred: false,
                    source_event_seq: Some(8),
                    turn_id: Some("turn-1".to_string()),
                }],
                messages: SessionMessageContextBreakdown {
                    user_message_tokens: 1,
                    assistant_message_tokens: 2,
                    tool_call_tokens: 3,
                    tool_result_tokens: 4,
                    attachment_tokens: 5,
                },
                top_tools: vec![SessionToolContextContribution {
                    name: "read_file".to_string(),
                    call_tokens: 3,
                    result_tokens: 0,
                }],
                top_attachments: vec![SessionAttachmentContextContribution {
                    name: "image/png image #0".to_string(),
                    tokens: 5,
                }],
            },
        });

        assert_eq!(response.session_id, "sess-1");
        assert_eq!(
            response.segments[0]
                .source_range
                .as_ref()
                .unwrap()
                .end_event_seq,
            30
        );
        assert_eq!(
            response.segments[0].provenance.compaction_id.as_deref(),
            Some("compaction-1")
        );
        assert_eq!(
            response.context_usage.categories[0].kind,
            "system_developer"
        );
        assert_eq!(response.context_usage.items[0].source_event_seq, Some(8));
        assert_eq!(response.context_usage.messages.attachment_tokens, 5);
        assert_eq!(response.context_usage.top_tools[0].name, "read_file");
        assert_eq!(
            response.context_usage.top_attachments[0].name,
            "image/png image #0"
        );
    }
}

/// Internal diagnostics: GET /sessions/{id}/context/projection — 返回当前模型可见上下文投影。
pub async fn get_session_context_projection(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(session_id): Path<String>,
) -> Result<Json<SessionProjectionViewResponse>, ApiError> {
    ensure_session_permission(
        state.as_ref(),
        &current_user,
        &session_id,
        ProjectPermission::Use,
    )
    .await?;
    Ok(Json(
        load_runtime_session_context_projection(state.as_ref(), &session_id).await?,
    ))
}

pub(crate) async fn load_runtime_session_context_projection(
    state: &AppState,
    session_id: &str,
) -> Result<SessionProjectionViewResponse, ApiError> {
    ensure_runtime_session_trace_exists(state, session_id).await?;
    let projection = state
        .services
        .session_eventing
        .build_context_projection_read_model(session_id)
        .await
        .map_err(ApiError::from)?;

    Ok(session_context_projection_to_response(projection))
}

fn session_context_projection_to_response(
    projection: SessionContextProjectionReadModel,
) -> SessionProjectionViewResponse {
    SessionProjectionViewResponse {
        session_id: projection.session_id,
        projection_kind: projection.projection_kind,
        projection_version: projection.projection_version,
        head_event_seq: projection.head_event_seq,
        active_compaction_id: projection.active_compaction_id,
        token_estimate: projection.token_estimate,
        message_count: projection.message_count,
        segments: projection
            .segments
            .into_iter()
            .map(|segment| SessionProjectionSegmentViewResponse {
                id: segment.id,
                sort_order: segment.sort_order,
                segment_type: segment.segment_type,
                role: segment.role,
                origin: segment.origin,
                synthetic: segment.synthetic,
                projection_kind: segment.projection_kind,
                message_ref: SessionProjectionMessageRefResponse {
                    turn_id: segment.message_ref.turn_id,
                    entry_index: segment.message_ref.entry_index,
                },
                source_event_seq: segment.source_event_seq,
                source_range: segment.source_range.map(|range| {
                    SessionProjectionSourceRangeResponse {
                        start_event_seq: range.start_event_seq,
                        end_event_seq: range.end_event_seq,
                    }
                }),
                projection_segment_id: segment.projection_segment_id,
                preview: segment.preview,
                token_estimate: segment.token_estimate,
                attachment_tokens: segment.attachment_tokens,
                attachment_names: segment.attachment_names,
                tool_names: segment.tool_names,
                provenance: SessionProjectionSegmentProvenanceResponse {
                    compaction_id: segment.provenance.compaction_id,
                    projection_version: segment.provenance.projection_version,
                    segment_type: segment.provenance.segment_type,
                    strategy: segment.provenance.strategy,
                    trigger: segment.provenance.trigger,
                    phase: segment.provenance.phase,
                },
            })
            .collect(),
        context_usage: SessionContextUsageAnalysisResponse {
            categories: projection
                .context_usage
                .categories
                .into_iter()
                .map(|category| SessionContextUsageCategoryResponse {
                    kind: category.kind,
                    label: category.label,
                    token_estimate: category.token_estimate,
                    source: category.source,
                    deferred: category.deferred,
                })
                .collect(),
            items: projection
                .context_usage
                .items
                .into_iter()
                .map(|item| SessionContextUsageItemResponse {
                    kind: item.kind,
                    label: item.label,
                    name: item.name,
                    token_estimate: item.token_estimate,
                    source: item.source,
                    deferred: item.deferred,
                    source_event_seq: item.source_event_seq,
                    turn_id: item.turn_id,
                })
                .collect(),
            messages: SessionMessageContextBreakdownResponse {
                user_message_tokens: projection.context_usage.messages.user_message_tokens,
                assistant_message_tokens: projection
                    .context_usage
                    .messages
                    .assistant_message_tokens,
                tool_call_tokens: projection.context_usage.messages.tool_call_tokens,
                tool_result_tokens: projection.context_usage.messages.tool_result_tokens,
                attachment_tokens: projection.context_usage.messages.attachment_tokens,
            },
            top_tools: projection
                .context_usage
                .top_tools
                .into_iter()
                .map(|tool| SessionToolContextContributionResponse {
                    name: tool.name,
                    call_tokens: tool.call_tokens,
                    result_tokens: tool.result_tokens,
                })
                .collect(),
            top_attachments: projection
                .context_usage
                .top_attachments
                .into_iter()
                .map(|attachment| SessionAttachmentContextContributionResponse {
                    name: attachment.name,
                    tokens: attachment.tokens,
                })
                .collect(),
        },
    }
}

/// Internal diagnostics: GET /sessions/{id}/lineage — 返回 runtime trace 的父边、祖先与直接 children。
pub async fn get_session_lineage(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(session_id): Path<String>,
) -> Result<Json<SessionLineageViewResponse>, ApiError> {
    ensure_session_permission(
        state.as_ref(),
        &current_user,
        &session_id,
        ProjectPermission::Use,
    )
    .await?;
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

/// Internal diagnostics: GET /sessions/{id}/meta — 返回完整 runtime trace meta。
pub async fn get_session_meta(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(session_id): Path<String>,
) -> Result<Json<SessionMeta>, ApiError> {
    get_session(State(state), CurrentUser(current_user), Path(session_id)).await
}

/// Session trace stream（Fetch Streaming / NDJSON）
pub async fn session_stream_ndjson(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(session_id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<NdjsonStreamQuery>,
) -> Result<impl IntoResponse, ApiError> {
    ensure_session_permission(
        state.as_ref(),
        &current_user,
        &session_id,
        ProjectPermission::Use,
    )
    .await?;
    runtime_session_stream_ndjson(state.as_ref(), session_id, headers, query).await
}

pub(crate) async fn runtime_session_stream_ndjson(
    state: &AppState,
    session_id: String,
    headers: HeaderMap,
    query: NdjsonStreamQuery,
) -> Result<Response, ApiError> {
    ensure_runtime_session_trace_exists(state, &session_id).await?;
    let resume_from = parse_resume_from_header(&headers, "x-stream-since-id")?
        .or(query.since_id)
        .unwrap_or(0);
    diag!(Info, Subsystem::Api,

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
    diag!(Info, Subsystem::Api,

        session_id = %session_id,
        replayed_count = replayed,
        snapshot_seq = subscription.snapshot_seq,
        "Session trace stream 历史补发完成（NDJSON）"
    );

    let ephemeral_epoch = state.services.session_eventing.ephemeral_epoch();

    let stream = async_stream::stream! {
        let mut seq = resume_from;
        for event in subscription.backlog {
            seq = event.event_seq;
            if let Some(line) = to_ndjson_line(&stream_event_payload(event)) {
                yield Ok::<Bytes, Infallible>(line);
            }
        }

        if let Some(line) = to_ndjson_line(&SessionNdjsonEnvelope::connected(seq, ephemeral_epoch)) {
            yield Ok::<Bytes, Infallible>(line);
        }

        // durable backlog + connected 之后、live loop 之前补发 ephemeral 快照（in-flight 进度态）。
        // 这些事件 event_seq 承载 ephemeral_seq，不影响 durable `seq` 游标；前端按 ephemeral_seq 去重，
        // 与后续 live ephemeral 广播的重叠由去重消解。
        for event in subscription.ephemeral_backlog {
            if let Some(line) = to_ndjson_line(&SessionNdjsonEnvelope::ephemeral_event(event)) {
                yield Ok::<Bytes, Infallible>(line);
            }
        }

        let mut heartbeat_tick = tokio::time::interval(RUNTIME_TRACE_HEARTBEAT_INTERVAL);
        heartbeat_tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let mut rx = subscription.rx;

        loop {
            tokio::select! {
                next = rx.recv() => {
                    match next {
                        Ok(event) => {
                            if event.ephemeral {
                                // ephemeral 事件 event_seq=0，不参与 snapshot_seq 去重、不推进游标；
                                // 直接 emit 为 ephemeral envelope（live-only）。
                                if let Some(line) =
                                    to_ndjson_line(&SessionNdjsonEnvelope::ephemeral_event(event))
                                {
                                    yield Ok::<Bytes, Infallible>(line);
                                }
                                continue;
                            }
                            if event.event_seq <= subscription.snapshot_seq {
                                continue;
                            }
                            seq = event.event_seq;
                            if let Some(line) = to_ndjson_line(&stream_event_payload(event)) {
                                yield Ok::<Bytes, Infallible>(line);
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            diag!(Warn, Subsystem::Api,

                                session_id = %session_id,
                                lagged = n,
                                "Session trace stream 订阅落后，部分消息被跳过（NDJSON）"
                            );
                            continue;
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            diag!(Info, Subsystem::Api,

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
    )
        .into_response())
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
            let context = DiagnosticErrorContext::new("session_trace.ndjson", "serialize_event");
            diag_error!(
                Error,
                Subsystem::Api,
                context = &context,
                error = &err,
                route = "/api/sessions/{id}/trace.ndjson",
                "序列化 Session NDJSON 消息失败"
            );
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

/// Internal diagnostics: `GET /sessions/{id}/context/audit` —— 返回 runtime trace 的 Fragment 审计时间线。
///
/// 返回按 `at_ms` 升序的事件列表（审计总线内部已保持插入顺序）。
pub async fn get_session_context_audit(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(session_id): Path<String>,
    Query(query): Query<ContextAuditQuery>,
) -> Result<Json<Vec<ContextAuditEventDto>>, ApiError> {
    ensure_session_permission(
        state.as_ref(),
        &current_user,
        &session_id,
        ProjectPermission::Use,
    )
    .await?;
    Ok(Json(
        load_runtime_session_context_audit(state.as_ref(), &session_id, query).await?,
    ))
}

pub(crate) async fn load_runtime_session_context_audit(
    state: &AppState,
    session_id: &str,
    query: ContextAuditQuery,
) -> Result<Vec<ContextAuditEventDto>, ApiError> {
    ensure_runtime_session_trace_exists(state, session_id).await?;
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

    let events = state.services.audit_bus.query(session_id, &filter);
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

    Ok(dtos)
}
