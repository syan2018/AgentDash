use std::sync::Arc;
use std::time::Duration;

use axum::Json;
use axum::body::{Body, Bytes};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, header};
use axum::response::IntoResponse;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use tokio::time::MissedTickBehavior;
use uuid::Uuid;

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

/// Project 级事件流（SSE）
pub async fn event_stream(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Query(query): Query<EventStreamQuery>,
    headers: HeaderMap,
) -> Result<Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>>, ApiError> {
    let project_id = parse_project_id(&query.project_id)?;
    let project = load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::View,
    )
    .await?;
    let resume_from = parse_last_event_id(&headers)?;
    let initial_latest_event_id = state
        .repos
        .state_change_repo
        .latest_event_id_by_project(project.id)
        .await?;

    tracing::info!(
        project_id = %project.id,
        last_event_id = ?resume_from,
        has_resume_cursor = resume_from.is_some(),
        "Project 事件流连接建立（SSE）"
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
                        if let Some(event) =
                            build_sse_event(&StreamEvent::StateChanged(change), Some(cursor))
                        {
                            yield Ok(event);
                        }
                    }
                }
                Err(err) => {
                    tracing::error!(project_id = %project.id, error = %err, "Project 事件流补发失败");
                }
            }
        }

        cursor = cursor.max(initial_latest_event_id);
        if let Some(event) =
            build_sse_event(&StreamEvent::Connected { last_event_id: cursor }, Some(cursor))
        {
            yield Ok(event);
        }
        tracing::info!(
            project_id = %project.id,
            replayed_count = replayed,
            cursor = cursor,
            "Project 事件流补发完成（SSE）"
        );

        let mut poll_tick = tokio::time::interval(STREAM_POLL_INTERVAL);
        poll_tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let mut heartbeat_tick = tokio::time::interval(STREAM_HEARTBEAT_INTERVAL);
        heartbeat_tick.set_missed_tick_behavior(MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                _ = poll_tick.tick() => {
                    match load_state_changes_since(&state, project.id, cursor).await {
                        Ok(changes) => {
                            for change in changes {
                                cursor = change.id;
                                if let Some(event) = build_sse_event(&StreamEvent::StateChanged(change), Some(cursor)) {
                                    yield Ok(event);
                                }
                            }
                        }
                        Err(err) => {
                            tracing::error!(project_id = %project.id, error = %err, "Project 事件流轮询 state_changes 失败");
                        }
                    }
                }
                _ = heartbeat_tick.tick() => {
                    let heartbeat = StreamEvent::Heartbeat {
                        timestamp: chrono::Utc::now().timestamp_millis(),
                    };
                    if let Some(event) = build_sse_event(&heartbeat, None) {
                        yield Ok(event);
                    }
                }
            }
        }
    };

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(STREAM_HEARTBEAT_INTERVAL)
            .text("keep-alive"),
    ))
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
    let resume_from = parse_last_event_id(&headers)?;
    let initial_latest_event_id = state
        .repos
        .state_change_repo
        .latest_event_id_by_project(project.id)
        .await?;

    tracing::info!(
        project_id = %project.id,
        last_event_id = ?resume_from,
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
                        if let Some(line) = to_ndjson_line(&StreamEvent::StateChanged(change)) {
                            yield Ok::<Bytes, std::convert::Infallible>(line);
                        }
                    }
                }
                Err(err) => {
                    tracing::error!(project_id = %project.id, error = %err, "Project NDJSON 事件流补发失败");
                }
            }
        }

        cursor = cursor.max(initial_latest_event_id);
        if let Some(line) = to_ndjson_line(&StreamEvent::Connected { last_event_id: cursor }) {
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

        loop {
            tokio::select! {
                _ = poll_tick.tick() => {
                    match load_state_changes_since(&state, project.id, cursor).await {
                        Ok(changes) => {
                            for change in changes {
                                cursor = change.id;
                                if let Some(line) = to_ndjson_line(&StreamEvent::StateChanged(change)) {
                                    yield Ok::<Bytes, std::convert::Infallible>(line);
                                }
                            }
                        }
                        Err(err) => {
                            tracing::error!(project_id = %project.id, error = %err, "Project NDJSON 事件流轮询 state_changes 失败");
                        }
                    }
                }
                _ = heartbeat_tick.tick() => {
                    let heartbeat = StreamEvent::Heartbeat {
                        timestamp: chrono::Utc::now().timestamp_millis(),
                    };
                    if let Some(line) = to_ndjson_line(&heartbeat) {
                        yield Ok::<Bytes, std::convert::Infallible>(line);
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

/// Resume 端点
pub async fn get_events_since(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Query(query): Query<EventStreamQuery>,
    Path(since_id): Path<i64>,
) -> Result<Json<Vec<StateChange>>, ApiError> {
    let project_id = parse_project_id(&query.project_id)?;
    let project = load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::View,
    )
    .await?;
    let changes = state
        .repos
        .state_change_repo
        .get_changes_since_by_project(project.id, since_id, 1000)
        .await?;
    Ok(Json(changes))
}

fn parse_project_id(project_id: &str) -> Result<Uuid, ApiError> {
    let trimmed = project_id.trim();
    if trimmed.is_empty() {
        return Err(ApiError::BadRequest("project_id 不能为空".into()));
    }
    Uuid::parse_str(trimmed).map_err(|_| ApiError::BadRequest("无效的 project_id".into()))
}

fn parse_last_event_id(headers: &HeaderMap) -> Result<Option<i64>, ApiError> {
    let Some(value) = headers.get("last-event-id") else {
        return Ok(None);
    };
    let raw = value
        .to_str()
        .map_err(|_| ApiError::BadRequest("last-event-id 不是有效 UTF-8".into()))?;
    let parsed = raw
        .parse::<i64>()
        .map_err(|_| ApiError::BadRequest("last-event-id 不是有效整数".into()))?;
    if parsed < 0 {
        return Err(ApiError::BadRequest("last-event-id 不能为负数".into()));
    }
    Ok(Some(parsed))
}

fn build_sse_event(event: &StreamEvent, id: Option<i64>) -> Option<Event> {
    match serde_json::to_string(event) {
        Ok(data) => {
            let event = match id {
                Some(event_id) => Event::default().id(event_id.to_string()).data(data),
                None => Event::default().data(data),
            };
            Some(event)
        }
        Err(err) => {
            tracing::error!(error = %err, "序列化 SSE 事件失败");
            None
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
