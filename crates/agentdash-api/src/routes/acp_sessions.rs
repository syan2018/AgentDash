use std::sync::Arc;
use std::time::Duration;

use axum::{
    Json,
    body::{Body, Bytes},
    extract::{Path, Query, State},
    http::HeaderMap,
    response::IntoResponse,
    response::sse::{Event, KeepAlive, Sse},
};
use futures::stream::Stream;
use serde::Deserialize;
use std::convert::Infallible;
use tokio::time::MissedTickBehavior;

use crate::{app_state::AppState, rpc::ApiError};
use agentdash_executor::{PromptSessionRequest, SessionMeta};

const ACP_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(20);

#[derive(Debug, Deserialize)]
pub struct NdjsonStreamQuery {
    pub since_id: Option<u64>,
}

pub async fn list_sessions(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<SessionMeta>>, ApiError> {
    let sessions = state
        .executor_hub
        .list_sessions()
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(sessions))
}

#[derive(Debug, Deserialize)]
pub struct CreateSessionRequest {
    #[serde(default)]
    pub title: Option<String>,
}

pub async fn create_session(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateSessionRequest>,
) -> Result<Json<SessionMeta>, ApiError> {
    let title = req.title.unwrap_or_else(|| "新会话".to_string());
    let meta = state
        .executor_hub
        .create_session(&title)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(meta))
}

pub async fn get_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> Result<Json<SessionMeta>, ApiError> {
    let meta = state
        .executor_hub
        .get_session_meta(&session_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("会话 {} 不存在", session_id)))?;
    Ok(Json(meta))
}

pub async fn delete_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state
        .executor_hub
        .delete_session(&session_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(
        serde_json::json!({ "deleted": true, "sessionId": session_id }),
    ))
}

pub async fn prompt_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(req): Json<PromptSessionRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let turn_id = state
        .executor_hub
        .start_prompt(&session_id, req)
        .await
        .map_err(|e| match &e {
            agentdash_executor::ConnectorError::InvalidConfig(_) => {
                ApiError::BadRequest(e.to_string())
            }
            _ => ApiError::Internal(e.to_string()),
        })?;

    Ok(Json(
        serde_json::json!({ "started": true, "sessionId": session_id, "turnId": turn_id }),
    ))
}

pub async fn cancel_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state
        .executor_hub
        .cancel(&session_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(
        serde_json::json!({ "cancelled": true, "sessionId": session_id }),
    ))
}

/// ACP 会话流（Streaming HTTP / SSE）
pub async fn acp_session_stream_sse(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    headers: HeaderMap,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let last_event_id = headers
        .get("last-event-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

    tracing::info!(
        session_id = %session_id,
        last_event_id = last_event_id,
        "ACP 会话流连接建立（SSE）"
    );

    let (history, mut rx) = state.executor_hub.subscribe_with_history(&session_id).await;
    let start_index = std::cmp::min(last_event_id as usize, history.len());
    let replayed = history.len().saturating_sub(start_index);
    tracing::info!(
        session_id = %session_id,
        replayed_count = replayed,
        history_total = history.len(),
        "ACP 会话流历史补发完成（SSE）"
    );

    let stream = async_stream::stream! {
        for (i, n) in history.iter().enumerate().skip(start_index) {
            let id = (i as u64) + 1;
            if let Ok(json) = serde_json::to_string(n) {
                yield Ok(Event::default().id(id.to_string()).data(json));
            }
        }

        let mut seq = history.len() as u64;
        loop {
            match rx.recv().await {
                Ok(n) => {
                    seq += 1;
                    if let Ok(json) = serde_json::to_string(&n) {
                        yield Ok(Event::default().id(seq.to_string()).data(json));
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(
                        session_id = %session_id,
                        lagged = n,
                        "ACP 会话流订阅落后，部分消息被跳过（SSE）"
                    );
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    tracing::info!(
                        session_id = %session_id,
                        last_seq = seq,
                        "ACP 会话流连接关闭：广播通道关闭（SSE）"
                    );
                    break;
                }
            }
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// ACP 会话流（Fetch Streaming / NDJSON）
pub async fn acp_session_stream_ndjson(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<NdjsonStreamQuery>,
) -> impl IntoResponse {
    let resume_from = parse_ndjson_resume_from(&headers, query.since_id);
    tracing::info!(
        session_id = %session_id,
        resume_from = resume_from,
        "ACP 会话流连接建立（NDJSON）"
    );

    let (history, mut rx) = state.executor_hub.subscribe_with_history(&session_id).await;
    let start_index = std::cmp::min(resume_from as usize, history.len());
    let replayed = history.len().saturating_sub(start_index);
    tracing::info!(
        session_id = %session_id,
        replayed_count = replayed,
        history_total = history.len(),
        "ACP 会话流历史补发完成（NDJSON）"
    );

    let stream = async_stream::stream! {
        let mut seq = history.len() as u64;
        if let Some(line) = to_ndjson_line(&serde_json::json!({
            "type": "connected",
            "last_event_id": seq,
        })) {
            yield Ok::<Bytes, Infallible>(line);
        }

        for (i, n) in history.iter().enumerate().skip(start_index) {
            let id = (i as u64) + 1;
            if let Some(line) = to_ndjson_line(&serde_json::json!({
                "type": "notification",
                "id": id,
                "notification": n,
            })) {
                yield Ok::<Bytes, Infallible>(line);
            }
        }

        let mut heartbeat_tick = tokio::time::interval(ACP_HEARTBEAT_INTERVAL);
        heartbeat_tick.set_missed_tick_behavior(MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                next = rx.recv() => {
                    match next {
                        Ok(n) => {
                            seq += 1;
                            if let Some(line) = to_ndjson_line(&serde_json::json!({
                                "type": "notification",
                                "id": seq,
                                "notification": n,
                            })) {
                                yield Ok::<Bytes, Infallible>(line);
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!(
                                session_id = %session_id,
                                lagged = n,
                                "ACP 会话流订阅落后，部分消息被跳过（NDJSON）"
                            );
                            continue;
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            tracing::info!(
                                session_id = %session_id,
                                last_seq = seq,
                                "ACP 会话流连接关闭：广播通道关闭（NDJSON）"
                            );
                            break;
                        }
                    }
                }
                _ = heartbeat_tick.tick() => {
                    if let Some(line) = to_ndjson_line(&serde_json::json!({
                        "type": "heartbeat",
                        "timestamp": chrono::Utc::now().timestamp_millis(),
                    })) {
                        yield Ok::<Bytes, Infallible>(line);
                    }
                }
            }
        }
    };

    (
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
}

fn parse_ndjson_resume_from(headers: &HeaderMap, query_since_id: Option<u64>) -> u64 {
    headers
        .get("x-stream-since-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .or(query_since_id)
        .unwrap_or(0)
}

fn to_ndjson_line(value: &serde_json::Value) -> Option<Bytes> {
    match serde_json::to_vec(value) {
        Ok(mut bytes) => {
            bytes.push(b'\n');
            Some(Bytes::from(bytes))
        }
        Err(err) => {
            tracing::error!(error = %err, "序列化 ACP NDJSON 消息失败");
            None
        }
    }
}
