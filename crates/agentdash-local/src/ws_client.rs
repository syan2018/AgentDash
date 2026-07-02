use agentdash_diagnostics::{Subsystem, diag};
use std::path::PathBuf;
use std::time::Duration;

use futures::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

use std::sync::Arc;

use agentdash_relay::*;
use chrono::Utc;
use serde::Serialize;

use crate::LocalExtensionHostManager;
use crate::handlers::{CommandExecutionMode, LocalCommandRouter};
use crate::local_backend_config::WorkspaceContractRuntimeConfig;
use crate::mcp_client_manager::McpClientManager;
use crate::runner_redaction::redact_secret;
use crate::runner_status::RunnerStatusReporter;
use crate::tool_executor::ToolExecutor;
use agentdash_application_runtime_session::session::SessionRuntimeServices;
use agentdash_infrastructure::postgres_runtime::PostgresRuntime;
use agentdash_spi::AgentConnector;

#[derive(Clone)]
pub struct Config {
    pub cloud_url: String,
    pub token: String,
    pub api_base_url: String,
    pub backend_id: String,
    pub name: String,
    pub workspace_roots: Vec<PathBuf>,
    pub tool_executor: ToolExecutor,
    pub session_runtime: Option<SessionRuntimeServices>,
    pub _session_db_runtime: Option<Arc<PostgresRuntime>>,
    pub connector: Option<Arc<dyn AgentConnector>>,
    pub mcp_manager: Option<Arc<McpClientManager>>,
    pub workspace_contract_config: WorkspaceContractRuntimeConfig,
    pub extension_host: LocalExtensionHostManager,
    pub extension_artifact_cache_root: PathBuf,
    pub runner_status: Option<RunnerStatusReporter>,
    pub relay_status_tx: Option<watch::Sender<RelayConnectionStatus>>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RelayConnectionState {
    NotConfigured,
    Connecting,
    Registered,
    Reconnecting,
    Disconnected,
    Error,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RelayConnectionStatus {
    pub state: RelayConnectionState,
    pub target: Option<String>,
    pub last_connected_at: Option<String>,
    pub last_disconnected_at: Option<String>,
    pub last_error: Option<String>,
    pub retry_count: Option<u32>,
    pub next_retry_at: Option<String>,
    pub registered_backend_id: Option<String>,
}

impl RelayConnectionStatus {
    pub fn not_configured(target: Option<String>) -> Self {
        Self {
            state: RelayConnectionState::NotConfigured,
            target,
            last_connected_at: None,
            last_disconnected_at: None,
            last_error: None,
            retry_count: None,
            next_retry_at: None,
            registered_backend_id: None,
        }
    }

    fn for_config(config: &Config, state: RelayConnectionState) -> Self {
        Self {
            state,
            target: Some(redact_secret(&config.cloud_url)),
            ..Self::not_configured(Some(redact_secret(&config.cloud_url)))
        }
    }

    fn connecting(config: &Config, retry_count: u32) -> Self {
        Self {
            retry_count: Some(retry_count),
            ..Self::for_config(config, RelayConnectionState::Connecting)
        }
    }

    fn registered(config: &Config) -> Self {
        Self {
            last_connected_at: Some(Utc::now().to_rfc3339()),
            registered_backend_id: Some(config.backend_id.clone()),
            ..Self::for_config(config, RelayConnectionState::Registered)
        }
    }

    fn reconnecting(config: &Config, retry_count: u32, error: impl AsRef<str>) -> Self {
        let next_retry_at = chrono::Duration::from_std(reconnect_delay(retry_count))
            .ok()
            .map(|delay| (Utc::now() + delay).to_rfc3339());
        Self {
            last_disconnected_at: Some(Utc::now().to_rfc3339()),
            last_error: Some(redact_secret(error.as_ref())),
            retry_count: Some(retry_count),
            next_retry_at,
            ..Self::for_config(config, RelayConnectionState::Reconnecting)
        }
    }

    fn disconnected(config: &Config, message: impl AsRef<str>) -> Self {
        Self {
            last_disconnected_at: Some(Utc::now().to_rfc3339()),
            last_error: Some(redact_secret(message.as_ref())),
            ..Self::for_config(config, RelayConnectionState::Disconnected)
        }
    }

    fn with_previous_connection(mut self, previous: &Self) -> Self {
        if self.last_connected_at.is_none() {
            self.last_connected_at = previous.last_connected_at.clone();
        }
        if self.registered_backend_id.is_none() {
            self.registered_backend_id = previous.registered_backend_id.clone();
        }
        self
    }
}

/// 主循环：连接 → 注册 → 消息处理 → 断线 → 重连，直到收到 shutdown 信号。
pub async fn run_until_shutdown(
    config: Config,
    mut shutdown_rx: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let mut retry_count: u32 = 0;

    loop {
        if *shutdown_rx.borrow() {
            diag!(
                Info,
                Subsystem::Relay,
                "收到 shutdown 信号，本机 relay 主循环停止"
            );
            report_status(&config.runner_status, "stopped", None, "runner 已停止").await;
            report_relay_status(
                &config,
                RelayConnectionStatus::disconnected(&config, "runner 已停止"),
            );
            return Ok(());
        }

        let url = format!("{}?token={}", config.cloud_url, config.token);
        report_relay_status(
            &config,
            RelayConnectionStatus::connecting(&config, retry_count),
        );
        report_status(
            &config.runner_status,
            "connecting",
            None,
            "连接云端 WebSocket",
        )
        .await;

        diag!(
            Info,
            Subsystem::Relay,
            retry = retry_count,
            "连接云端 WebSocket..."
        );

        match connect_async(&url).await {
            Ok((ws_stream, _response)) => {
                retry_count = 0;
                diag!(Info, Subsystem::Relay, "WebSocket 连接成功");

                if let Err(e) = run_session(ws_stream, &config, shutdown_rx.clone()).await {
                    report_relay_status(
                        &config,
                        RelayConnectionStatus::reconnecting(&config, retry_count, e.to_string()),
                    );
                    report_status(
                        &config.runner_status,
                        "retrying",
                        Some("session_error"),
                        &e.to_string(),
                    )
                    .await;
                    diag!(Error, Subsystem::Relay,
        error = %redact_secret(&e.to_string()), "会话异常终止");
                } else if *shutdown_rx.borrow() {
                    report_status(&config.runner_status, "stopped", None, "runner 已停止").await;
                    report_relay_status(
                        &config,
                        RelayConnectionStatus::disconnected(&config, "runner 已停止"),
                    );
                } else {
                    report_relay_status(
                        &config,
                        RelayConnectionStatus::reconnecting(
                            &config,
                            retry_count,
                            "WebSocket 连接关闭",
                        ),
                    );
                    report_status(
                        &config.runner_status,
                        "retrying",
                        Some("disconnected"),
                        "WebSocket 连接关闭",
                    )
                    .await;
                }
            }
            Err(e) => {
                report_relay_status(
                    &config,
                    RelayConnectionStatus::reconnecting(&config, retry_count, e.to_string()),
                );
                report_status(
                    &config.runner_status,
                    "retrying",
                    Some("connect_failed"),
                    &e.to_string(),
                )
                .await;
                diag!(Error, Subsystem::Relay,
        error = %redact_secret(&e.to_string()), "WebSocket 连接失败");
            }
        }

        let delay = reconnect_delay(retry_count);
        diag!(
            Info,
            Subsystem::Relay,
            delay_secs = delay.as_secs(),
            "等待重连..."
        );
        tokio::select! {
            _ = tokio::time::sleep(delay) => {}
            changed = shutdown_rx.changed() => {
                if changed.is_err() || *shutdown_rx.borrow() {
                    diag!(Info, Subsystem::Relay,
        "等待重连时收到 shutdown 信号");
                    report_status(&config.runner_status, "stopped", None, "runner 已停止").await;
                    report_relay_status(
                        &config,
                        RelayConnectionStatus::disconnected(&config, "runner 已停止"),
                    );
                    return Ok(());
                }
            }
        }
        retry_count += 1;
    }
}

/// 单次连接的完整会话
async fn run_session(
    ws_stream: WebSocketStream<MaybeTlsStream<TcpStream>>,
    config: &Config,
    mut shutdown_rx: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let (mut write, mut read) = ws_stream.split();

    // 创建事件通道（domain handlers 通过此通道推送异步事件）
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<RelayMessage>();

    let handler = LocalCommandRouter::new(crate::handlers::LocalCommandRouterConfig {
        backend_id: config.backend_id.clone(),
        workspace_roots: config.workspace_roots.clone(),
        tool_executor: config.tool_executor.clone(),
        session_runtime: config.session_runtime.clone(),
        connector: config.connector.clone(),
        mcp_manager: config.mcp_manager.clone(),
        workspace_contract_config: config.workspace_contract_config.clone(),
        extension_host: config.extension_host.clone(),
        extension_artifact_api_base_url: config.api_base_url.clone(),
        extension_artifact_access_token: config.token.clone(),
        extension_artifact_cache_root: config.extension_artifact_cache_root.clone(),
        event_tx,
    });

    // 第一步：发送注册消息
    let mut last_capabilities = build_capabilities(&handler, &config.mcp_manager).await;
    let register_msg = RelayMessage::Register {
        id: RelayMessage::new_id("reg"),
        payload: RegisterPayload {
            backend_id: config.backend_id.clone(),
            name: config.name.clone(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            capabilities: last_capabilities.clone(),
        },
    };

    send_message(&mut write, &register_msg).await?;
    diag!(Info, Subsystem::Relay, "已发送注册消息");

    // 等待 register_ack
    let ack_timeout = loop {
        tokio::select! {
            result = tokio::time::timeout(Duration::from_secs(10), read.next()) => break result,
            changed = shutdown_rx.changed() => {
                if changed.is_err() || *shutdown_rx.borrow() {
                    diag!(Info, Subsystem::Relay,
        "等待注册响应时收到 shutdown 信号");
                    return Ok(());
                }
            }
        }
    };
    match ack_timeout {
        Ok(Some(Ok(msg))) => {
            if let Some(relay_msg) = parse_ws_message(&msg) {
                match &relay_msg {
                    RelayMessage::RegisterAck { payload, .. } => {
                        report_relay_status(config, RelayConnectionStatus::registered(config));
                        report_status(&config.runner_status, "registered", None, "注册成功").await;
                        diag!(Info, Subsystem::Relay,

                            backend_id = %payload.backend_id,
                            status = %payload.status,
                            "注册成功"
                        );
                    }
                    RelayMessage::Error { error, .. } => {
                        anyhow::bail!("注册失败: {}", error);
                    }
                    other => {
                        diag!(
                            Warn,
                            Subsystem::Relay,
                            "期望 register_ack，收到: {:?}",
                            other.id()
                        );
                    }
                }
            }
        }
        Ok(Some(Err(e))) => anyhow::bail!("注册响应读取错误: {e}"),
        Ok(None) => anyhow::bail!("注册后连接关闭"),
        Err(_) => anyhow::bail!("等待注册响应超时"),
    }

    let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel::<RelayMessage>();
    let mut writer_task = tokio::spawn(async move {
        while let Some(relay_msg) = outbound_rx.recv().await {
            send_message(&mut write, &relay_msg).await?;
        }
        Ok::<(), anyhow::Error>(())
    });

    // 进入消息循环：WS 读取不直接执行命令，避免长耗时工具调用阻塞后续命令和事件写出。
    let mut ping_interval = tokio::time::interval(Duration::from_secs(30));
    ping_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    ping_interval.tick().await;
    let mut capability_interval = tokio::time::interval(Duration::from_secs(5));
    capability_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    capability_interval.tick().await;

    loop {
        tokio::select! {
            msg = read.next() => {
                match msg {
                    Some(Ok(ws_msg)) => {
                        if let Some(relay_msg) = parse_ws_message(&ws_msg) {
                            let dispatch_plan = handler.dispatch_plan(&relay_msg);
                            if dispatch_plan.execution_mode == CommandExecutionMode::Background {
                                let handler = handler.clone();
                                let outbound_tx = outbound_tx.clone();
                                tokio::spawn(async move {
                                    let msg_id = relay_msg.id().to_string();
                                    let responses = handler.handle(relay_msg).await;
                                    for resp in responses {
                                        if outbound_tx.send(resp).is_err() {
                                            diag!(Debug, Subsystem::Relay,

                                                msg_id = %msg_id,
                                                "relay response 写出通道已关闭"
                                            );
                                            break;
                                        }
                                    }
                                });
                            } else {
                                let responses = handler.handle(relay_msg).await;
                                for resp in responses {
                                    if outbound_tx.send(resp).is_err() {
                                        diag!(Debug, Subsystem::Relay,
        "relay response 写出通道已关闭");
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    Some(Err(e)) => {
                        diag!(Error, Subsystem::Relay,
        error = %e, "WebSocket 读取错误");
                        break;
                    }
                    None => {
                        diag!(Info, Subsystem::Relay,
        "WebSocket 连接关闭");
                        break;
                    }
                }
            }
            event = event_rx.recv() => {
                match event {
                    Some(relay_msg) => {
                        if outbound_tx.send(relay_msg).is_err() {
                            diag!(Warn, Subsystem::Relay,
        "relay event 写出通道已关闭");
                            break;
                        }
                    }
                    None => {
                        diag!(Warn, Subsystem::Relay,
        "事件通道关闭");
                        break;
                    }
                }
            }
            _ = ping_interval.tick() => {
                // 本机侧不主动发 ping，只响应云端的 ping
            }
            _ = capability_interval.tick() => {
                let next_capabilities = build_capabilities(&handler, &config.mcp_manager).await;
                if next_capabilities != last_capabilities {
                    last_capabilities = next_capabilities.clone();
                    let relay_msg = RelayMessage::EventCapabilitiesChanged {
                        id: RelayMessage::new_id("caps"),
                        payload: next_capabilities,
                    };
                    if outbound_tx.send(relay_msg).is_err() {
                        diag!(Warn, Subsystem::Relay,
        "capability changed event 写出通道已关闭");
                        break;
                    }
                }
            }
            changed = shutdown_rx.changed() => {
                if changed.is_err() || *shutdown_rx.borrow() {
                    diag!(Info, Subsystem::Relay,
        "消息循环收到 shutdown 信号");
                    break;
                }
            }
            writer_result = &mut writer_task => {
                match writer_result {
                    Ok(Ok(())) => diag!(Info, Subsystem::Relay,
        "WebSocket 写出任务结束"),
                    Ok(Err(e)) => diag!(Error, Subsystem::Relay,
        error = %e, "WebSocket 写出错误"),
                    Err(e) => diag!(Error, Subsystem::Relay,
        error = %e, "WebSocket 写出任务异常结束"),
                }
                break;
            }
        }
    }

    if !writer_task.is_finished() {
        writer_task.abort();
        let _ = writer_task.await;
    }

    Ok(())
}

async fn report_status(
    reporter: &Option<RunnerStatusReporter>,
    state: &str,
    code: Option<&str>,
    message: &str,
) {
    let Some(reporter) = reporter else {
        return;
    };
    let result = match state {
        "connecting" => reporter.mark_connecting().await,
        "registered" => reporter.mark_registered().await,
        "retrying" => {
            reporter
                .mark_retrying(code.unwrap_or("relay_retrying"), message)
                .await
        }
        "disconnected" => reporter.mark_disconnected(message).await,
        "stopped" => reporter.mark_stopped().await,
        _ => Ok(()),
    };
    if let Err(error) = result {
        diag!(Warn, Subsystem::Relay,
            error = %redact_secret(&error.to_string()),
            "写入 runner status snapshot 失败"
        );
    }
}

fn report_relay_status(config: &Config, status: RelayConnectionStatus) {
    if let Some(tx) = &config.relay_status_tx {
        let status = status.with_previous_connection(&tx.borrow());
        let _ = tx.send(status);
    }
}

async fn build_capabilities(
    handler: &LocalCommandRouter,
    mcp_manager: &Option<Arc<McpClientManager>>,
) -> CapabilitiesPayload {
    let executors = handler.list_executors();
    let mcp_servers = mcp_manager
        .as_ref()
        .map(|m| m.capability_entries())
        .unwrap_or_default();
    let capability_health = match mcp_manager.as_ref() {
        Some(m) => m
            .capability_health_snapshot()
            .await
            .into_iter()
            .map(|item| agentdash_relay::CapabilityHealthItemRelay {
                id: item.id,
                domain: item.domain,
                status: item.status,
                label: item.label,
                summary: item.summary,
                actions: item
                    .actions
                    .into_iter()
                    .map(|a| agentdash_relay::CapabilityHealthActionRelay {
                        kind: a.kind,
                        label: a.label,
                    })
                    .collect(),
            })
            .collect(),
        None => Vec::new(),
    };
    CapabilitiesPayload {
        executors,
        supports_cancel: true,
        supports_discover_options: false,
        mcp_servers,
        capability_health,
    }
}

fn parse_ws_message(msg: &Message) -> Option<RelayMessage> {
    match msg {
        Message::Text(text) => match serde_json::from_str::<RelayMessage>(text.as_ref()) {
            Ok(relay_msg) => Some(relay_msg),
            Err(e) => {
                diag!(Warn, Subsystem::Relay,
        error = %e, "无法解析 WebSocket 消息");
                None
            }
        },
        Message::Close(_) => {
            diag!(Info, Subsystem::Relay, "收到关闭帧");
            None
        }
        _ => None,
    }
}

async fn send_message(
    write: &mut futures::stream::SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>,
    msg: &RelayMessage,
) -> anyhow::Result<()> {
    let json = serde_json::to_string(msg)?;
    write.send(Message::Text(json.into())).await?;
    Ok(())
}

/// 指数退避重连延迟（1s → 2s → 4s → ... → 60s）
fn reconnect_delay(retry_count: u32) -> Duration {
    let secs = (1u64 << retry_count.min(6)).min(60);
    Duration::from_secs(secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_exec_is_handled_in_background() {
        let msg = RelayMessage::CommandToolShellExec {
            id: "shell-1".to_string(),
            payload: ToolShellExecPayload {
                call_id: "call-1".to_string(),
                command: "cargo check".to_string(),
                mount_root_ref: "D:/workspace".to_string(),
                cwd: None,
                timeout_ms: Some(30_000),
                yield_time_ms: Some(1_000),
                max_output_bytes: None,
                tty: false,
            },
        };

        assert_eq!(
            crate::handlers::dispatch_plan_for_message(&msg).execution_mode,
            CommandExecutionMode::Background
        );
    }

    #[test]
    fn ordinary_relay_messages_keep_inline_ordering() {
        let msg = RelayMessage::Ping {
            id: "ping-1".to_string(),
            payload: PingPayload { server_time: 1 },
        };

        assert_eq!(
            crate::handlers::dispatch_plan_for_message(&msg).execution_mode,
            CommandExecutionMode::Inline
        );
    }

    #[test]
    fn relay_status_preserves_registered_connection_facts() {
        let previous = RelayConnectionStatus {
            state: RelayConnectionState::Registered,
            target: Some("wss://example.test/ws/backend".to_string()),
            last_connected_at: Some("2026-06-26T00:00:00Z".to_string()),
            last_disconnected_at: None,
            last_error: None,
            retry_count: Some(0),
            next_retry_at: None,
            registered_backend_id: Some("backend-1".to_string()),
        };
        let reconnecting = RelayConnectionStatus {
            state: RelayConnectionState::Reconnecting,
            target: Some("wss://example.test/ws/backend".to_string()),
            last_connected_at: None,
            last_disconnected_at: Some("2026-06-26T00:01:00Z".to_string()),
            last_error: Some("WebSocket 连接关闭".to_string()),
            retry_count: Some(1),
            next_retry_at: None,
            registered_backend_id: None,
        }
        .with_previous_connection(&previous);

        assert_eq!(
            reconnecting.last_connected_at.as_deref(),
            Some("2026-06-26T00:00:00Z")
        );
        assert_eq!(
            reconnecting.registered_backend_id.as_deref(),
            Some("backend-1")
        );
    }
}
