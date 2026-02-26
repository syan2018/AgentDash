use std::sync::Arc;

use axum::{
    extract::{
        Query,
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::IntoResponse,
};
use futures::{SinkExt, StreamExt};
use serde::Deserialize;

use crate::{app_state::AppState, executor::AgentConnector};

#[derive(Debug, Deserialize)]
pub struct DiscoveredOptionsQuery {
    /// 执行器 ID（例如 "CLAUDE_CODE"）
    pub executor: String,
    /// 可选变体（例如 "DEFAULT" / "PLAN"）
    pub variant: Option<String>,
    /// 可选工作目录（相对 workspace 根目录）
    pub working_dir: Option<String>,
}

/// WebSocket：执行器发现选项流（JSON Patch）
///
/// 对齐 vibe-kanban 消息格式：
/// - { "Ready": true }
/// - { "JsonPatch": [ ... RFC6902 ops ... ] }
/// - { "finished": true }
pub async fn discovered_options_ws(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    Query(q): Query<DiscoveredOptionsQuery>,
) -> impl IntoResponse {
    let connector = state.connector.clone();
    ws.on_upgrade(move |socket| handle_ws(connector, q, socket))
}

async fn handle_ws(connector: Arc<dyn AgentConnector>, q: DiscoveredOptionsQuery, socket: WebSocket) {
    let (mut sender, mut receiver) = socket.split();

    // Drain inbound messages to detect close (client may close without sending anything).
    let recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            if matches!(msg, Message::Close(_)) {
                break;
            }
        }
    });

    let ready = serde_json::json!({ "Ready": true }).to_string();
    if sender.send(Message::Text(ready.into())).await.is_err() {
        recv_task.abort();
        return;
    }

    let working_dir = q
        .working_dir
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .map(|rel| std::env::current_dir().unwrap_or_default().join(rel));

    let stream = connector
        .discover_options_stream(&q.executor, q.variant.as_deref(), working_dir)
        .await;

    match stream {
        Ok(mut patches) => {
            while let Some(patch) = patches.next().await {
                let msg = serde_json::json!({ "JsonPatch": patch }).to_string();
                if sender.send(Message::Text(msg.into())).await.is_err() {
                    break;
                }
            }
            let _ = sender
                .send(Message::Text(
                    serde_json::json!({ "finished": true }).to_string().into(),
                ))
                .await;
        }
        Err(e) => {
            let _ = sender
                .send(Message::Text(
                    serde_json::json!({ "Error": e.to_string() }).to_string().into(),
                ))
                .await;
        }
    }

    recv_task.abort();
}

