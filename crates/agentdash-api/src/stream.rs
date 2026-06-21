use std::sync::Arc;
use std::time::Duration;

use axum::body::{Body, Bytes};
use axum::extract::{Query, State};
use axum::http::{HeaderMap, header};
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast::error::RecvError;
use tokio::time::MissedTickBehavior;
use uuid::Uuid;

use agentdash_contracts::project::ProjectEventStreamEnvelope;
use agentdash_domain::common::events::StreamEvent;
use agentdash_domain::story::StateChange;

use crate::app_state::AppState;
use crate::auth::{CurrentUser, ProjectPermission, load_project_with_permission};
use crate::rpc::ApiError;

const STREAM_BATCH_LIMIT: i64 = 256;
const STREAM_POLL_INTERVAL: Duration = Duration::from_millis(400);
const STREAM_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(20);

#[derive(Debug, Deserialize)]
pub struct EventStreamQuery {
    pub project_id: String,
}

/// Project 级事件流（NDJSON）
pub async fn event_stream_ndjson(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Query(query): Query<EventStreamQuery>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    let project_id = parse_project_id(&query.project_id)?;
    let project = load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::View,
    )
    .await?;
    let resume_from = parse_stream_since_id(&headers)?;
    let initial_latest_event_id = state
        .repos
        .state_change_repo
        .latest_event_id_by_project(project.id)
        .await?;

    tracing::info!(
        project_id = %project.id,
        resume_from = ?resume_from,
        has_resume_cursor = resume_from.is_some(),
        "Project 事件流连接建立（NDJSON）"
    );

    let stream = async_stream::stream! {
        let mut cursor = match resume_from {
            Some(value) => value,
            None => initial_latest_event_id,
        };
        let mut replayed = 0usize;

        if resume_from.is_some() {
            match load_state_changes_since(&state, project.id, cursor).await {
                Ok(changes) => {
                    for change in changes {
                        cursor = change.id;
                        replayed += 1;
                        if let Some(event) = contract_stream_event(StreamEvent::StateChanged(change)) {
                            if let Some(line) = to_ndjson_line(&event) {
                                yield Ok::<Bytes, std::convert::Infallible>(line);
                            }
                        } else {
                            tracing::error!(project_id = %project.id, "Project NDJSON StateChanged payload is not a JSON object");
                        }
                    }
                }
                Err(err) => {
                    tracing::error!(project_id = %project.id, error = %err, "Project NDJSON 事件流补发失败");
                }
            }
        }

        cursor = cursor.max(initial_latest_event_id);
        if let Some(line) = contract_stream_event(StreamEvent::Connected { last_event_id: cursor })
            .and_then(|event| to_ndjson_line(&event))
        {
            yield Ok::<Bytes, std::convert::Infallible>(line);
        }
        tracing::info!(
            project_id = %project.id,
            replayed_count = replayed,
            cursor = cursor,
            "Project 事件流补发完成（NDJSON）"
        );

        let mut poll_tick = tokio::time::interval(STREAM_POLL_INTERVAL);
        poll_tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let mut heartbeat_tick = tokio::time::interval(STREAM_HEARTBEAT_INTERVAL);
        heartbeat_tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let mut backend_runtime_rx = state.services.backend_runtime_events.subscribe();

        loop {
            tokio::select! {
                _ = poll_tick.tick() => {
                    match load_state_changes_since(&state, project.id, cursor).await {
                        Ok(changes) => {
                            for change in changes {
                                cursor = change.id;
                                if let Some(event) = contract_stream_event(StreamEvent::StateChanged(change)) {
                                    if let Some(line) = to_ndjson_line(&event) {
                                        yield Ok::<Bytes, std::convert::Infallible>(line);
                                    }
                                } else {
                                    tracing::error!(project_id = %project.id, "Project NDJSON StateChanged payload is not a JSON object");
                                }
                            }
                        }
                        Err(err) => {
                            tracing::error!(project_id = %project.id, error = %err, "Project NDJSON 事件流轮询 state_changes 失败");
                        }
                    }
                }
                event = backend_runtime_rx.recv() => {
                    match event {
                        Ok(backend_id) => {
                            let runtime_event = StreamEvent::BackendRuntimeChanged { backend_id };
                            if let Some(line) = contract_stream_event(runtime_event).and_then(|event| to_ndjson_line(&event)) {
                                yield Ok(line);
                            }
                        }
                        Err(RecvError::Lagged(skipped)) => {
                            tracing::warn!(project_id = %project.id, skipped, "Project NDJSON 事件流 backend runtime 事件滞后");
                        }
                        Err(RecvError::Closed) => {
                            tracing::warn!(project_id = %project.id, "Project NDJSON 事件流 backend runtime 事件通道已关闭");
                            break;
                        }
                    }
                }
                _ = heartbeat_tick.tick() => {
                    let heartbeat = StreamEvent::Heartbeat {
                        timestamp: chrono::Utc::now().timestamp_millis(),
                    };
                    if let Some(line) = contract_stream_event(heartbeat).and_then(|event| to_ndjson_line(&event)) {
                        yield Ok(line);
                    }
                }
            }
        }
    };

    Ok((
        [
            (header::CONTENT_TYPE, "application/x-ndjson; charset=utf-8"),
            (header::CACHE_CONTROL, "no-cache, no-transform"),
            (header::CONNECTION, "keep-alive"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        Body::from_stream(stream),
    ))
}

fn parse_project_id(project_id: &str) -> Result<Uuid, ApiError> {
    let trimmed = project_id.trim();
    if trimmed.is_empty() {
        return Err(ApiError::BadRequest("project_id 不能为空".into()));
    }
    Uuid::parse_str(trimmed).map_err(|_| ApiError::BadRequest("无效的 project_id".into()))
}

fn parse_stream_since_id(headers: &HeaderMap) -> Result<Option<i64>, ApiError> {
    let Some(value) = headers.get("x-stream-since-id") else {
        return Ok(None);
    };
    let raw = value
        .to_str()
        .map_err(|_| ApiError::BadRequest("x-stream-since-id 不是有效 UTF-8".into()))?;
    let parsed = raw
        .parse::<i64>()
        .map_err(|_| ApiError::BadRequest("x-stream-since-id 不是有效整数".into()))?;
    if parsed < 0 {
        return Err(ApiError::BadRequest("x-stream-since-id 不能为负数".into()));
    }
    Ok(Some(parsed))
}

fn contract_stream_event(event: StreamEvent) -> Option<ProjectEventStreamEnvelope> {
    match event {
        StreamEvent::Connected { last_event_id } => {
            Some(ProjectEventStreamEnvelope::connected(last_event_id))
        }
        StreamEvent::StateChanged(change) => ProjectEventStreamEnvelope::state_changed(change),
        StreamEvent::BackendRuntimeChanged { backend_id } => Some(
            ProjectEventStreamEnvelope::backend_runtime_changed(backend_id),
        ),
        StreamEvent::Heartbeat { timestamp } => {
            Some(ProjectEventStreamEnvelope::heartbeat(timestamp))
        }
    }
}

fn to_ndjson_line<T: Serialize>(value: &T) -> Option<Bytes> {
    match serde_json::to_vec(value) {
        Ok(mut raw) => {
            raw.push(b'\n');
            Some(Bytes::from(raw))
        }
        Err(err) => {
            tracing::error!(error = %err, "序列化 NDJSON 事件失败");
            None
        }
    }
}

async fn load_state_changes_since(
    state: &Arc<AppState>,
    project_id: Uuid,
    since_id: i64,
) -> Result<Vec<StateChange>, agentdash_domain::DomainError> {
    let mut cursor = since_id;
    let mut all = Vec::new();

    loop {
        let batch = state
            .repos
            .state_change_repo
            .get_changes_since_by_project(project_id, cursor, STREAM_BATCH_LIMIT)
            .await?;
        if batch.is_empty() {
            break;
        }

        if let Some(last) = batch.last() {
            cursor = last.id;
        }
        let should_continue = batch.len() as i64 >= STREAM_BATCH_LIMIT;
        all.extend(batch);
        if !should_continue {
            break;
        }
    }

    Ok(all)
}
