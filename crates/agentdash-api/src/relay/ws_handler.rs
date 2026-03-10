use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::response::IntoResponse;
use futures::{SinkExt, StreamExt};
use tokio::sync::mpsc;

use agentdash_relay::*;

use crate::app_state::AppState;
use crate::relay::registry::ConnectedBackend;

/// WebSocket 后端连接端点
pub async fn ws_backend_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let token = params.get("token").cloned().unwrap_or_default();
    ws.on_upgrade(move |socket| handle_backend_connection(socket, state, token))
}

async fn handle_backend_connection(socket: WebSocket, state: Arc<AppState>, _token: String) {
    let (mut ws_tx, mut ws_rx) = socket.split();

    // 第一步：等待 Register 消息
    let relay_msg = match tokio::time::timeout(
        std::time::Duration::from_secs(10),
        read_next_relay(&mut ws_rx),
    )
    .await
    {
        Ok(Some(msg)) => msg,
        Ok(None) => {
            tracing::error!("等待注册消息时连接关闭");
            return;
        }
        Err(_) => {
            tracing::error!("等待注册消息超时");
            return;
        }
    };

    let (reg_id, payload) = match relay_msg {
        RelayMessage::Register { id, payload } => (id, payload),
        other => {
            tracing::error!(msg_type = %other.id(), "首条消息必须是 register");
            return;
        }
    };

    let bid = payload.backend_id.clone();
    tracing::info!(
        backend_id = %bid,
        name = %payload.name,
        accessible_roots = ?payload.accessible_roots,
        "收到本机后端注册"
    );

    // 创建命令发送通道
    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<RelayMessage>();

    let connected = ConnectedBackend {
        backend_id: bid.clone(),
        name: payload.name.clone(),
        version: payload.version.clone(),
        capabilities: payload.capabilities.clone(),
        accessible_roots: payload.accessible_roots.clone(),
        sender: cmd_tx,
        connected_at: chrono::Utc::now(),
    };

    state.backend_registry.register(connected).await;

    // 发送 RegisterAck
    let ack = RelayMessage::RegisterAck {
        id: reg_id,
        payload: RegisterAckPayload {
            backend_id: bid.clone(),
            status: "ok".to_string(),
            server_time: chrono::Utc::now().timestamp_millis(),
        },
    };
    if send_relay(&mut ws_tx, &ack).await.is_err() {
        state.backend_registry.unregister(&bid).await;
        return;
    }

    tracing::info!(backend_id = %bid, "本机后端注册完成，进入消息循环");

    // 心跳定时器
    let mut ping_interval = tokio::time::interval(std::time::Duration::from_secs(30));
    ping_interval.tick().await;

    loop {
        tokio::select! {
            msg = ws_rx.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(relay_msg) = serde_json::from_str::<RelayMessage>(text.as_ref()) {
                            handle_backend_message(&state, &bid, relay_msg).await;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        tracing::info!(backend_id = %bid, "本机后端断开");
                        break;
                    }
                    Some(Err(e)) => {
                        tracing::error!(backend_id = %bid, error = %e, "WebSocket 读取错误");
                        break;
                    }
                    _ => {}
                }
            }
            cmd = cmd_rx.recv() => {
                if let Some(cmd) = cmd {
                    if send_relay(&mut ws_tx, &cmd).await.is_err() {
                        break;
                    }
                }
            }
            _ = ping_interval.tick() => {
                let ping = RelayMessage::Ping {
                    id: RelayMessage::new_id("ping"),
                    payload: PingPayload {
                        server_time: chrono::Utc::now().timestamp_millis(),
                    },
                };
                if send_relay(&mut ws_tx, &ping).await.is_err() {
                    break;
                }
            }
        }
    }

    state.backend_registry.unregister(&bid).await;
}

async fn handle_backend_message(state: &Arc<AppState>, backend_id: &str, msg: RelayMessage) {
    match &msg {
        RelayMessage::Pong { .. } => {
            tracing::debug!(backend_id = %backend_id, "收到 pong");
        }
        // 响应消息 → 分发到等待方
        RelayMessage::ResponsePrompt { .. }
        | RelayMessage::ResponseCancel { .. }
        | RelayMessage::ResponseDiscover { .. }
        | RelayMessage::ResponseWorkspaceFilesList { .. }
        | RelayMessage::ResponseWorkspaceFilesRead { .. }
        | RelayMessage::ResponseWorkspaceDetectGit { .. }
        | RelayMessage::ResponseToolFileRead { .. }
        | RelayMessage::ResponseToolFileWrite { .. }
        | RelayMessage::ResponseToolShellExec { .. }
        | RelayMessage::ResponseToolFileList { .. } => {
            if !state.backend_registry.resolve_response(&msg).await {
                tracing::warn!(
                    backend_id = %backend_id,
                    msg_id = %msg.id(),
                    "无匹配的挂起请求"
                );
            }
        }
        // 事件消息 → 转发到前端事件流（TODO: 集成 EventBus）
        RelayMessage::EventSessionNotification { payload, .. } => {
            tracing::info!(
                backend_id = %backend_id,
                session_id = %payload.session_id,
                "收到远程会话通知"
            );
        }
        RelayMessage::EventSessionStateChanged { payload, .. } => {
            tracing::info!(
                backend_id = %backend_id,
                session_id = %payload.session_id,
                state = ?payload.state,
                "收到远程会话状态变更"
            );
        }
        RelayMessage::EventCapabilitiesChanged { .. } => {
            tracing::info!(backend_id = %backend_id, "收到能力变更通知");
        }
        RelayMessage::EventDiscoverOptionsPatch { .. } => {
            tracing::debug!(backend_id = %backend_id, "收到选项发现 patch");
        }
        other => {
            tracing::debug!(
                backend_id = %backend_id,
                msg_id = %other.id(),
                "忽略意外消息"
            );
        }
    }
}

async fn read_next_relay(
    rx: &mut futures::stream::SplitStream<WebSocket>,
) -> Option<RelayMessage> {
    while let Some(msg) = rx.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                return serde_json::from_str::<RelayMessage>(text.as_ref()).ok();
            }
            Ok(Message::Close(_)) => return None,
            Err(_) => return None,
            _ => continue,
        }
    }
    None
}

async fn send_relay<S>(tx: &mut S, msg: &RelayMessage) -> Result<(), ()>
where
    S: SinkExt<Message> + Unpin,
{
    let json = serde_json::to_string(msg).map_err(|_| ())?;
    tx.send(Message::Text(json.into())).await.map_err(|_| ())?;
    Ok(())
}
