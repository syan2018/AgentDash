use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use futures::{SinkExt, StreamExt};
use tokio::sync::mpsc;

use agentdash_domain::DomainError;
use agentdash_domain::backend::{BackendConfig, BackendRepository};
use agentdash_relay::*;

use crate::app_state::AppState;
use crate::relay::registry::{ConnectedBackend, RegisterBackendError};

/// WebSocket 后端连接端点
pub async fn ws_backend_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let token = params
        .get("token")
        .map(|value| value.trim().to_string())
        .unwrap_or_default();

    let authorized_backend =
        match authorize_backend_token(state.repos.backend_repo.as_ref(), &token).await {
            Ok(config) => config,
            Err(err) => return err.into_response(),
        };

    ws.on_upgrade(move |socket| handle_backend_connection(socket, state, authorized_backend))
}

async fn handle_backend_connection(
    socket: WebSocket,
    state: Arc<AppState>,
    authorized_backend: BackendConfig,
) {
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
            tracing::warn!(
                authorized_backend_id = %authorized_backend.id,
                msg_type = %other.id(),
                "首条消息不是 register，拒绝建立 relay 注册"
            );
            let _ = send_relay_error(
                &mut ws_tx,
                other.id().to_string(),
                RelayError::invalid_message("首条消息必须是 register"),
            )
            .await;
            return;
        }
    };

    let bid = payload.backend_id.clone();
    if let Err(error) = validate_register_payload(&authorized_backend, &payload) {
        tracing::warn!(
            authorized_backend_id = %authorized_backend.id,
            claimed_backend_id = %payload.backend_id,
            error_code = error.code.as_str(),
            error = %error.message.as_str(),
            "本机后端注册校验失败"
        );
        let _ = send_relay_error(&mut ws_tx, reg_id, error).await;
        return;
    }

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

    if let Err(err) = state
        .services
        .backend_registry
        .try_register(connected)
        .await
    {
        let error = match err {
            RegisterBackendError::AlreadyOnline { backend_id } => {
                RelayError::conflict(format!("backend `{backend_id}` 已在线，拒绝重复注册"))
            }
        };
        tracing::warn!(
            backend_id = %bid,
            error_code = error.code.as_str(),
            error = %error.message.as_str(),
            "本机后端重复注册被拒绝"
        );
        let _ = send_relay_error(&mut ws_tx, reg_id, error).await;
        return;
    }

    // 发送 RegisterAck
    let ack = RelayMessage::RegisterAck {
        id: reg_id,
        payload: RegisterAckPayload {
            backend_id: bid.clone(),
            status: "online".to_string(),
            server_time: chrono::Utc::now().timestamp_millis(),
        },
    };
    if send_relay(&mut ws_tx, &ack).await.is_err() {
        state.services.backend_registry.unregister(&bid).await;
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
                if let Some(cmd) = cmd
                    && send_relay(&mut ws_tx, &cmd).await.is_err() {
                        break;
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

    state.services.backend_registry.unregister(&bid).await;
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
        | RelayMessage::ResponseWorkspaceDetectGit { .. }
        | RelayMessage::ResponseToolFileRead { .. }
        | RelayMessage::ResponseToolFileWrite { .. }
        | RelayMessage::ResponseToolFileDelete { .. }
        | RelayMessage::ResponseToolFileRename { .. }
        | RelayMessage::ResponseToolApplyPatch { .. }
        | RelayMessage::ResponseToolShellExec { .. }
        | RelayMessage::ResponseToolFileList { .. }
        | RelayMessage::ResponseToolSearch { .. }
        | RelayMessage::ResponseBrowseDirectory { .. } => {
            if !state.services.backend_registry.resolve_response(&msg).await {
                tracing::warn!(
                    backend_id = %backend_id,
                    msg_id = %msg.id(),
                    "无匹配的挂起请求"
                );
            }
        }
        RelayMessage::EventSessionNotification { payload, .. } => {
            match serde_json::from_value::<agent_client_protocol::SessionNotification>(
                payload.notification.clone(),
            ) {
                Ok(notification) => {
                    if let Err(err) = state
                        .services
                        .session_hub
                        .inject_notification(&payload.session_id, notification)
                        .await
                    {
                        tracing::warn!(
                            backend_id = %backend_id,
                            session_id = %payload.session_id,
                            error = %err,
                            "注入远程 SessionNotification 失败"
                        );
                    }
                }
                Err(err) => {
                    tracing::warn!(
                        backend_id = %backend_id,
                        session_id = %payload.session_id,
                        error = %err,
                        "反序列化远程 SessionNotification 失败"
                    );
                }
            }
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

async fn read_next_relay(rx: &mut futures::stream::SplitStream<WebSocket>) -> Option<RelayMessage> {
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

async fn send_relay_error<S>(tx: &mut S, id: String, error: RelayError) -> Result<(), ()>
where
    S: SinkExt<Message> + Unpin,
{
    send_relay(tx, &RelayMessage::Error { id, error }).await
}

async fn authorize_backend_token(
    backend_repo: &dyn BackendRepository,
    token: &str,
) -> Result<BackendConfig, AuthResponseError> {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        tracing::warn!("relay 握手缺少 token");
        return Err(AuthResponseError::unauthorized(
            "缺少 relay token",
            "未携带 token，拒绝升级 WebSocket",
        ));
    }

    match backend_repo.get_backend_by_auth_token(trimmed).await {
        Ok(config) => Ok(config),
        Err(DomainError::NotFound { .. }) => {
            tracing::warn!("relay 握手 token 无效");
            Err(AuthResponseError::unauthorized(
                "relay token 无效",
                "token 无效或未绑定 backend",
            ))
        }
        Err(err) => {
            tracing::error!(error = %err, "relay 握手 token 查找失败");
            Err(AuthResponseError::internal(
                "relay token 校验失败",
                "服务端无法完成 token 校验",
            ))
        }
    }
}

fn validate_register_payload(
    authorized_backend: &BackendConfig,
    payload: &RegisterPayload,
) -> Result<(), RelayError> {
    if payload.backend_id != authorized_backend.id {
        return Err(RelayError::new(
            RelayErrorCode::Forbidden,
            format!(
                "token 绑定 backend `{}`，不能注册为 `{}`",
                authorized_backend.id, payload.backend_id
            ),
        ));
    }

    if !authorized_backend.enabled {
        return Err(RelayError::new(
            RelayErrorCode::Forbidden,
            format!("backend `{}` 已禁用，不能注册上线", authorized_backend.id),
        ));
    }

    Ok(())
}

struct AuthResponseError {
    status: StatusCode,
    response_message: &'static str,
}

impl AuthResponseError {
    fn unauthorized(_log_message: &'static str, response_message: &'static str) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            response_message,
        }
    }

    fn internal(_log_message: &'static str, response_message: &'static str) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            response_message,
        }
    }
}

impl IntoResponse for AuthResponseError {
    fn into_response(self) -> axum::response::Response {
        (self.status, self.response_message).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::backend::{BackendType, UserPreferences, ViewConfig};

    enum MockTokenResult {
        Ok(BackendConfig),
        NotFound,
    }

    struct MockBackendRepository {
        token_result: MockTokenResult,
    }

    #[async_trait::async_trait]
    impl BackendRepository for MockBackendRepository {
        async fn add_backend(&self, _config: &BackendConfig) -> Result<(), DomainError> {
            unreachable!("测试未使用");
        }

        async fn list_backends(&self) -> Result<Vec<BackendConfig>, DomainError> {
            unreachable!("测试未使用");
        }

        async fn get_backend(&self, _id: &str) -> Result<BackendConfig, DomainError> {
            unreachable!("测试未使用");
        }

        async fn get_backend_by_auth_token(
            &self,
            _token: &str,
        ) -> Result<BackendConfig, DomainError> {
            match &self.token_result {
                MockTokenResult::Ok(config) => Ok(config.clone()),
                MockTokenResult::NotFound => Err(DomainError::NotFound {
                    entity: "backend_auth_token",
                    id: "mock".to_string(),
                }),
            }
        }

        async fn remove_backend(&self, _id: &str) -> Result<(), DomainError> {
            unreachable!("测试未使用");
        }

        async fn list_views(&self) -> Result<Vec<ViewConfig>, DomainError> {
            unreachable!("测试未使用");
        }

        async fn save_view(&self, _view: &ViewConfig) -> Result<(), DomainError> {
            unreachable!("测试未使用");
        }

        async fn get_preferences(&self) -> Result<UserPreferences, DomainError> {
            unreachable!("测试未使用");
        }

        async fn save_preferences(&self, _prefs: &UserPreferences) -> Result<(), DomainError> {
            unreachable!("测试未使用");
        }
    }

    fn backend_config(id: &str, enabled: bool) -> BackendConfig {
        BackendConfig {
            id: id.to_string(),
            name: "测试后端".to_string(),
            endpoint: "ws://localhost".to_string(),
            auth_token: Some("secret".to_string()),
            enabled,
            backend_type: BackendType::Local,
        }
    }

    fn register_payload(backend_id: &str) -> RegisterPayload {
        RegisterPayload {
            backend_id: backend_id.to_string(),
            name: "本机-A".to_string(),
            version: "0.1.0".to_string(),
            capabilities: CapabilitiesPayload {
                executors: Vec::new(),
                supports_cancel: true,
                supports_discover_options: true,
            },
            accessible_roots: vec!["/tmp/project".to_string()],
        }
    }

    #[tokio::test]
    async fn authorize_backend_token_rejects_missing_token() {
        let repo = MockBackendRepository {
            token_result: MockTokenResult::Ok(backend_config("local-a", true)),
        };

        let err = authorize_backend_token(&repo, "")
            .await
            .expect_err("缺少 token 应被拒绝");

        assert_eq!(err.status, StatusCode::UNAUTHORIZED);
        assert_eq!(err.response_message, "未携带 token，拒绝升级 WebSocket");
    }

    #[tokio::test]
    async fn authorize_backend_token_rejects_invalid_token() {
        let repo = MockBackendRepository {
            token_result: MockTokenResult::NotFound,
        };

        let err = authorize_backend_token(&repo, "invalid")
            .await
            .expect_err("无效 token 应被拒绝");

        assert_eq!(err.status, StatusCode::UNAUTHORIZED);
        assert_eq!(err.response_message, "token 无效或未绑定 backend");
    }

    #[test]
    fn validate_register_payload_rejects_backend_id_mismatch() {
        let err = validate_register_payload(
            &backend_config("local-a", true),
            &register_payload("local-b"),
        )
        .expect_err("backend_id 不匹配应被拒绝");

        assert_eq!(err.code, RelayErrorCode::Forbidden);
        assert!(err.message.contains("不能注册为"));
    }

    #[test]
    fn validate_register_payload_rejects_disabled_backend() {
        let err = validate_register_payload(
            &backend_config("local-a", false),
            &register_payload("local-a"),
        )
        .expect_err("禁用 backend 不应注册成功");

        assert_eq!(err.code, RelayErrorCode::Forbidden);
        assert!(err.message.contains("已禁用"));
    }
}
