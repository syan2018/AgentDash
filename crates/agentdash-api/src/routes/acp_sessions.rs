use std::sync::Arc;
use std::time::Duration;

use axum::{
    body::{Body, Bytes},
    Json,
    extract::{
        Path,
        Query,
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::HeaderMap,
    response::IntoResponse,
    response::sse::{Event, KeepAlive, Sse},
};
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use futures::stream::Stream;
use std::convert::Infallible;
use tokio::time::MissedTickBehavior;

use crate::{
    app_state::AppState,
    executor::PromptSessionRequest,
    rpc::ApiError,
};

const ACP_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(20);

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum WsClientMessage {
    Execute { #[serde(flatten)] req: PromptSessionRequest },
    Cancel,
}

#[derive(Debug, Deserialize)]
pub struct NdjsonStreamQuery {
    pub since_id: Option<u64>,
}

pub async fn prompt_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(req): Json<PromptSessionRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state
        .executor_hub
        .start_prompt(&session_id, req)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(serde_json::json!({ "started": true, "sessionId": session_id })))
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

    Ok(Json(serde_json::json!({ "cancelled": true, "sessionId": session_id })))
}

/// ACP 会话流（Streaming HTTP / SSE）
///
/// - 首次连接：先发送历史（jsonl），再持续发送新增通知
/// - 断线重连：浏览器会携带 `Last-Event-ID`，服务端会跳过已发送的历史，避免重复回放
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
        // 历史回放（带 id，支持浏览器自动 resume）
        for (i, n) in history.iter().enumerate().skip(start_index) {
            let id = (i as u64) + 1;
            if let Ok(json) = serde_json::to_string(n) {
                yield Ok(Event::default().id(id.to_string()).data(json));
            }
        }

        // 实时推送（继续递增 id）
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
///
/// Resume：
/// - 优先读取 `x-stream-since-id` 请求头
/// - 兼容读取 query `since_id`
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

/// 兼容：旧版 WebSocket 流
pub async fn acp_session_ws(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws(state, session_id, socket))
}

async fn handle_ws(state: Arc<AppState>, session_id: String, socket: WebSocket) {
    let (history, mut rx) = state.executor_hub.subscribe_with_history(&session_id).await;
    let (mut sender, mut receiver) = socket.split();

    for n in history {
        if let Ok(text) = serde_json::to_string(&n) {
            if sender.send(Message::Text(text.into())).await.is_err() {
                return;
            }
        }
    }

    let send_task = tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(n) => {
                    let Ok(text) = serde_json::to_string(&n) else {
                        continue;
                    };
                    if sender.send(Message::Text(text.into())).await.is_err() {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    // Optional: allow starting execution via WS message.
    while let Some(Ok(msg)) = receiver.next().await {
        match msg {
            Message::Text(text) => {
                let started = if let Ok(ws_msg) = serde_json::from_str::<WsClientMessage>(&text) {
                    match ws_msg {
                        WsClientMessage::Execute { req } => state.executor_hub.start_prompt(&session_id, req).await.ok(),
                        WsClientMessage::Cancel => state.executor_hub.cancel(&session_id).await.ok(),
                    };
                    true
                } else if let Ok(req) = serde_json::from_str::<PromptSessionRequest>(&text) {
                    state.executor_hub.start_prompt(&session_id, req).await.ok();
                    true
                } else {
                    false
                };

                if started {
                    // no-op
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    send_task.abort();
    let _ = send_task.await;
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

