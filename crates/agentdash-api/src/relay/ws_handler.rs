use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use futures::{SinkExt, StreamExt};
use tokio::sync::mpsc;

use agentdash_domain::DomainError;
use agentdash_domain::backend::{BackendConfig, BackendRepository, RuntimeHealthOnlineUpdate};
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
        Ok(Ok(Some(msg))) => msg,
        Ok(Ok(None)) => {
            tracing::error!("等待注册消息时连接关闭");
            return;
        }
        Ok(Err(error)) => {
            tracing::error!(authorized_backend_id = %authorized_backend.id, error = %error, "等待注册消息时收到非法 relay 消息");
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
        "收到本机后端注册"
    );

    // 创建命令发送通道
    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<RelayMessage>();

    let connected = ConnectedBackend {
        backend_id: bid.clone(),
        name: payload.name.clone(),
        version: payload.version.clone(),
        capabilities: payload.capabilities.clone(),
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

    let connected_at = chrono::Utc::now();
    if let Err(error) = state
        .repos
        .runtime_health_repo
        .upsert_online(&RuntimeHealthOnlineUpdate {
            backend_id: bid.clone(),
            profile_id: authorized_backend.profile_id.clone(),
            name: payload.name.clone(),
            version: payload.version.clone(),
            capabilities: serde_json::to_value(&payload.capabilities).unwrap_or_default(),
            device: authorized_backend.device.clone(),
            connected_at,
        })
        .await
    {
        tracing::error!(backend_id = %bid, error = %error, "写入 runtime health 在线状态失败");
        state.services.backend_registry.unregister(&bid).await;
        let _ = send_relay_error(
            &mut ws_tx,
            reg_id,
            RelayError::runtime_error("写入 runtime health 失败"),
        )
        .await;
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
        notify_backend_runtime_changed(&state, &bid);
        return;
    }
    notify_backend_runtime_changed(&state, &bid);

    tracing::info!(backend_id = %bid, "本机后端注册完成，进入消息循环");

    // 心跳定时器
    let mut ping_interval = tokio::time::interval(std::time::Duration::from_secs(30));
    ping_interval.tick().await;

    loop {
        tokio::select! {
            msg = ws_rx.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        match parse_relay_message_text(text.as_ref()) {
                            Ok(relay_msg) => handle_backend_message(&state, &bid, relay_msg).await,
                            Err(error) => {
                                tracing::error!(backend_id = %bid, error = %error, "收到非法 relay 消息，关闭连接");
                                break;
                            }
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

    let lost_session_count = state.services.backend_registry.feed_backend_terminal(
        &bid,
        agentdash_application_ports::backend_transport::RelayTerminalKind::Lost,
        Some("backend disconnected".to_string()),
    );
    if lost_session_count > 0 {
        tracing::warn!(
            backend_id = %bid,
            count = lost_session_count,
            "后端断连，已向 active relay session 投递 lost terminal"
        );
    }
    match state
        .repos
        .backend_execution_lease_repo
        .mark_lost_by_backend(
            &bid,
            Some("relay websocket disconnected".to_string()),
            chrono::Utc::now(),
        )
        .await
    {
        Ok(count) if count > 0 => {
            tracing::warn!(
                backend_id = %bid,
                count,
                "后端断连，已标记 active backend execution lease 为 lost"
            );
        }
        Ok(_) => {}
        Err(error) => {
            tracing::warn!(backend_id = %bid, error = %error, "标记 backend execution lease lost 失败");
        }
    }
    state.services.backend_registry.unregister(&bid).await;
    if let Err(error) = state
        .repos
        .runtime_health_repo
        .mark_offline(
            &bid,
            chrono::Utc::now(),
            Some("relay websocket disconnected".to_string()),
        )
        .await
    {
        tracing::warn!(backend_id = %bid, error = %error, "写入 runtime health 离线状态失败");
    }
    notify_backend_runtime_changed(&state, &bid);

    // 标记该后端名下的所有终端为 Lost 并推送 platform event
    let lost_terminal_ids = state
        .services
        .terminal_cache
        .handle_backend_disconnect(&bid);
    for terminal_id in &lost_terminal_ids {
        if let Some(term_state) = state.services.terminal_cache.get_terminal(terminal_id) {
            let source = agentdash_agent_protocol::SourceInfo {
                connector_id: "platform".to_string(),
                connector_type: "terminal".to_string(),
                executor_id: None,
            };
            let envelope = agentdash_agent_protocol::BackboneEnvelope::new(
                agentdash_agent_protocol::BackboneEvent::Platform(
                    agentdash_agent_protocol::PlatformEvent::TerminalStateChanged {
                        terminal_id: terminal_id.clone(),
                        state: "lost".to_string(),
                        exit_code: None,
                        message: Some("backend disconnected".to_string()),
                    },
                ),
                &term_state.session_id,
                source,
            );
            if let Err(error) = state
                .services
                .session_eventing
                .inject_notification(&term_state.session_id, envelope)
                .await
            {
                tracing::warn!(
                    backend_id = %bid,
                    terminal_id = %terminal_id,
                    session_id = %term_state.session_id,
                    error = %error,
                    "后端断连终端 lost 事件注入 session 失败"
                );
            }
        }
    }
    if !lost_terminal_ids.is_empty() {
        tracing::info!(
            backend_id = %bid,
            count = lost_terminal_ids.len(),
            "后端断连，已标记终端为 Lost"
        );
    }
}

async fn handle_backend_message(state: &Arc<AppState>, backend_id: &str, msg: RelayMessage) {
    match &msg {
        RelayMessage::Pong { .. } => {
            tracing::debug!(backend_id = %backend_id, "收到 pong");
            if let Err(error) = state
                .repos
                .runtime_health_repo
                .mark_seen(backend_id, chrono::Utc::now())
                .await
            {
                tracing::warn!(backend_id = %backend_id, error = %error, "更新 runtime health last_seen 失败");
            }
        }
        // 响应消息 → 分发到等待方
        response if is_pending_response_message(response) => {
            if !state.services.backend_registry.resolve_response(&msg).await {
                tracing::warn!(
                    backend_id = %backend_id,
                    msg_id = %msg.id(),
                    "无匹配的挂起请求"
                );
            }
        }
        RelayMessage::EventSessionNotification { payload, .. } => {
            use agentdash_application_ports::backend_transport::RelaySessionEvent;

            match serde_json::from_value::<agentdash_agent_protocol::BackboneEnvelope>(
                payload.notification.clone(),
            ) {
                Ok(envelope) => {
                    if !state.services.backend_registry.feed_session_event(
                        &payload.session_id,
                        RelaySessionEvent::Notification(Box::new(envelope)),
                    ) {
                        tracing::debug!(
                            backend_id = %backend_id,
                            session_id = %payload.session_id,
                            "relay notification 到达时 session sink 不存在（session 可能已结束）"
                        );
                    }
                }
                Err(err) => {
                    tracing::warn!(
                        backend_id = %backend_id,
                        session_id = %payload.session_id,
                        error = %err,
                        "反序列化远程 BackboneEnvelope 失败"
                    );
                }
            }
        }
        RelayMessage::EventSessionStateChanged { payload, .. } => {
            use agentdash_application_ports::backend_transport::{
                RelaySessionEvent, RelayTerminalKind,
            };

            tracing::info!(
                backend_id = %backend_id,
                session_id = %payload.session_id,
                state = ?payload.state,
                "收到远程会话状态变更"
            );

            if matches!(payload.state, agentdash_relay::SessionState::Started) {
                return;
            }

            let terminal_kind = match payload.state {
                agentdash_relay::SessionState::Completed => RelayTerminalKind::Completed,
                agentdash_relay::SessionState::Failed => RelayTerminalKind::Failed,
                agentdash_relay::SessionState::Cancelled => RelayTerminalKind::Interrupted,
                agentdash_relay::SessionState::Started => unreachable!(),
            };

            if !state.services.backend_registry.feed_session_event(
                &payload.session_id,
                RelaySessionEvent::Terminal {
                    kind: terminal_kind,
                    message: payload.message.clone(),
                },
            ) {
                tracing::debug!(
                    backend_id = %backend_id,
                    session_id = %payload.session_id,
                    "relay terminal 到达时 session sink 不存在（session 可能已结束）"
                );
            }
        }
        RelayMessage::EventCapabilitiesChanged { payload, .. } => {
            tracing::info!(backend_id = %backend_id, "收到能力变更通知");
            state
                .services
                .backend_registry
                .update_capabilities(backend_id, payload.clone())
                .await;
            if let Err(error) = state
                .repos
                .runtime_health_repo
                .update_capabilities(
                    backend_id,
                    serde_json::to_value(payload).unwrap_or_default(),
                )
                .await
            {
                tracing::warn!(backend_id = %backend_id, error = %error, "更新 runtime health capabilities 失败");
            }
            notify_backend_runtime_changed(state, backend_id);
        }
        RelayMessage::EventToolShellOutput { payload, .. } => {
            let payload = payload
                .clone()
                .bounded(agentdash_relay::LIVE_OUTPUT_EVENT_MAX_BYTES);
            if !state.services.shell_output_registry.route(&payload) {
                tracing::debug!(
                    backend_id = %backend_id,
                    call_id = %payload.call_id,
                    "shell output 到达时无匹配 sink（命令可能已结束）"
                );
            }
        }
        RelayMessage::EventTerminalOutput { payload, .. } => {
            let payload = payload
                .clone()
                .bounded(agentdash_relay::LIVE_OUTPUT_EVENT_MAX_BYTES);
            let terminal_id = &payload.terminal_id;
            tracing::info!(
                backend_id = %backend_id,
                terminal_id = %terminal_id,
                data_len = payload.data.len(),
                "收到终端输出事件"
            );
            if let Some(term_state) = state.services.terminal_cache.get_terminal(terminal_id) {
                let source = agentdash_agent_protocol::SourceInfo {
                    connector_id: "platform".to_string(),
                    connector_type: "terminal".to_string(),
                    executor_id: None,
                };
                let envelope = agentdash_agent_protocol::BackboneEnvelope::new(
                    agentdash_agent_protocol::BackboneEvent::Platform(
                        agentdash_agent_protocol::PlatformEvent::TerminalOutput {
                            terminal_id: terminal_id.clone(),
                            data: terminal_output_event_data(&payload),
                        },
                    ),
                    &term_state.session_id,
                    source,
                );
                if let Err(e) = state
                    .services
                    .session_eventing
                    .inject_notification(&term_state.session_id, envelope)
                    .await
                {
                    tracing::warn!(
                        terminal_id = %terminal_id,
                        session_id = %term_state.session_id,
                        error = %e,
                        "终端输出事件注入 session 失败"
                    );
                }
            } else {
                tracing::warn!(
                    terminal_id = %terminal_id,
                    "终端输出事件到达但 terminal_cache 中未找到"
                );
            }
        }
        RelayMessage::EventTerminalStateChanged { payload, .. } => {
            tracing::info!(
                backend_id = %backend_id,
                terminal_id = %payload.terminal_id,
                state = ?payload.state,
                "收到终端状态变更事件"
            );
            let state_str = match payload.state {
                agentdash_relay::TerminalProcessState::Running => "running",
                agentdash_relay::TerminalProcessState::Exited => "exited",
                agentdash_relay::TerminalProcessState::Lost => "lost",
                agentdash_relay::TerminalProcessState::Killed => "killed",
            };
            state.services.terminal_cache.update_state(
                &payload.terminal_id,
                state_str,
                payload.exit_code,
            );

            if let Some(term_state) = state
                .services
                .terminal_cache
                .get_terminal(&payload.terminal_id)
            {
                let source = agentdash_agent_protocol::SourceInfo {
                    connector_id: "platform".to_string(),
                    connector_type: "terminal".to_string(),
                    executor_id: None,
                };
                let envelope = agentdash_agent_protocol::BackboneEnvelope::new(
                    agentdash_agent_protocol::BackboneEvent::Platform(
                        agentdash_agent_protocol::PlatformEvent::TerminalStateChanged {
                            terminal_id: payload.terminal_id.clone(),
                            state: state_str.to_string(),
                            exit_code: payload.exit_code,
                            message: payload.message.clone(),
                        },
                    ),
                    &term_state.session_id,
                    source,
                );
                if let Err(e) = state
                    .services
                    .session_eventing
                    .inject_notification(&term_state.session_id, envelope)
                    .await
                {
                    tracing::warn!(
                        terminal_id = %payload.terminal_id,
                        error = %e,
                        "终端状态变更注入 session 失败"
                    );
                }
            }
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

fn terminal_output_event_data(payload: &agentdash_relay::TerminalOutputPayload) -> String {
    if !payload.truncation.truncated {
        return payload.data.clone();
    }
    let mut data = payload.data.clone();
    if !data.ends_with('\n') && !data.is_empty() {
        data.push('\n');
    }
    data.push_str(&format!(
        "[terminal output truncated: omitted_bytes={}]\n",
        payload.truncation.omitted_bytes
    ));
    data
}

fn notify_backend_runtime_changed(state: &AppState, backend_id: &str) {
    let _ = state
        .services
        .backend_runtime_events
        .send(backend_id.to_string());
}

fn is_pending_response_message(msg: &RelayMessage) -> bool {
    matches!(
        msg,
        RelayMessage::ResponsePrompt { .. }
            | RelayMessage::ResponseCancel { .. }
            | RelayMessage::ResponseDiscover { .. }
            | RelayMessage::ResponseWorkspaceDetect { .. }
            | RelayMessage::ResponseWorkspaceDetectGit { .. }
            | RelayMessage::ResponseWorkspaceDiscoverByIdentity { .. }
            | RelayMessage::ResponseToolFileRead { .. }
            | RelayMessage::ResponseToolFileReadBinary { .. }
            | RelayMessage::ResponseToolFileWrite { .. }
            | RelayMessage::ResponseToolFileDelete { .. }
            | RelayMessage::ResponseToolFileRename { .. }
            | RelayMessage::ResponseToolApplyPatch { .. }
            | RelayMessage::ResponseToolShellExec { .. }
            | RelayMessage::ResponseToolShellRead { .. }
            | RelayMessage::ResponseToolShellInput { .. }
            | RelayMessage::ResponseToolShellTerminate { .. }
            | RelayMessage::ResponseToolFileList { .. }
            | RelayMessage::ResponseToolSearch { .. }
            | RelayMessage::ResponseBrowseDirectory { .. }
            | RelayMessage::ResponseMcpListTools { .. }
            | RelayMessage::ResponseMcpCallTool { .. }
            | RelayMessage::ResponseMcpClose { .. }
            | RelayMessage::ResponseExtensionActionInvoke { .. }
            | RelayMessage::ResponseExtensionChannelInvoke { .. }
            | RelayMessage::ResponseVfsMaterialize { .. }
            | RelayMessage::ResponseTerminalSpawn { .. }
            | RelayMessage::ResponseTerminalInput { .. }
            | RelayMessage::ResponseTerminalResize { .. }
            | RelayMessage::ResponseTerminalKill { .. }
    )
}

async fn read_next_relay(
    rx: &mut futures::stream::SplitStream<WebSocket>,
) -> Result<Option<RelayMessage>, String> {
    while let Some(msg) = rx.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                return parse_relay_message_text(text.as_ref()).map(Some);
            }
            Ok(Message::Close(_)) => return Ok(None),
            Err(error) => return Err(error.to_string()),
            _ => continue,
        }
    }
    Ok(None)
}

fn parse_relay_message_text(text: &str) -> Result<RelayMessage, String> {
    serde_json::from_str::<RelayMessage>(text)
        .map_err(|error| format!("relay 消息不是合法 JSON 协议包: {error}"))
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
            tracing::error!(error = %err, error_debug = ?err, "relay 握手 token 查找失败");
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
    use agentdash_domain::backend::{
        BackendShareScopeKind, BackendType, BackendVisibility, LocalBackendClaim, UserPreferences,
        ViewConfig,
    };

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

        async fn ensure_local_backend(
            &self,
            _claim: &LocalBackendClaim,
        ) -> Result<BackendConfig, DomainError> {
            unreachable!("测试未使用");
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
            owner_user_id: None,
            profile_id: None,
            device_id: None,
            machine_id: None,
            machine_label: None,
            visibility: BackendVisibility::Private,
            share_scope_kind: BackendShareScopeKind::User,
            share_scope_id: None,
            capability_slot: "default".to_string(),
            device: serde_json::json!({}),
            last_claimed_at: None,
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
                mcp_servers: Vec::new(),
            },
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

    #[test]
    fn vfs_materialize_response_is_routed_to_pending_requests() {
        let response = RelayMessage::ResponseVfsMaterialize {
            id: "vfs-materialize-1".to_string(),
            payload: None,
            error: None,
        };

        assert!(is_pending_response_message(&response));
    }

    #[test]
    fn binary_file_read_response_is_routed_to_pending_requests() {
        let response = RelayMessage::ResponseToolFileReadBinary {
            id: "file-read-binary-1".to_string(),
            payload: None,
            error: None,
        };

        assert!(is_pending_response_message(&response));
    }

    #[test]
    fn extension_action_response_is_routed_to_pending_requests() {
        let response = RelayMessage::ResponseExtensionActionInvoke {
            id: "extension-action-1".to_string(),
            payload: None,
            error: None,
        };

        assert!(is_pending_response_message(&response));
    }

    #[test]
    fn shell_output_event_is_not_treated_as_pending_response() {
        let event = RelayMessage::EventToolShellOutput {
            id: "shell-out-1".to_string(),
            payload: ToolShellOutputPayload {
                call_id: "call-1".to_string(),
                delta: "ok\n".to_string(),
                stream: ShellOutputStream::Stdout,
                truncation: ToolShellTruncationInfo::default(),
            },
        };

        assert!(!is_pending_response_message(&event));
    }
}
