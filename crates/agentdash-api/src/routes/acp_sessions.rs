use std::sync::Arc;

use axum::{
    Json,
    extract::{
        Path,
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

use crate::{
    app_state::AppState,
    executor::PromptSessionRequest,
    rpc::ApiError,
};

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum WsClientMessage {
    Execute { #[serde(flatten)] req: PromptSessionRequest },
    Cancel,
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

    let (history, mut rx) = state.executor_hub.subscribe_with_history(&session_id).await;
    let start_index = std::cmp::min(last_event_id as usize, history.len());

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
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::default())
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

