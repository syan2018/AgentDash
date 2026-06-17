use std::path::PathBuf;
use std::time::Duration;

use futures::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

use std::sync::Arc;

use agentdash_relay::*;

use crate::LocalExtensionHostManager;
use crate::handlers::LocalCommandRouter;
use crate::local_backend_config::WorkspaceContractRuntimeConfig;
use crate::mcp_client_manager::McpClientManager;
use crate::tool_executor::ToolExecutor;
use agentdash_application::session::SessionRuntimeServices;
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
}

/// 主循环：连接 → 注册 → 消息处理 → 断线 → 重连
pub async fn run(config: Config) -> anyhow::Result<()> {
    let (_shutdown_tx, shutdown_rx) = watch::channel(false);
    run_until_shutdown(config, shutdown_rx).await
}

/// 主循环：连接 → 注册 → 消息处理 → 断线 → 重连，直到收到 shutdown 信号。
pub async fn run_until_shutdown(
    config: Config,
    mut shutdown_rx: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let mut retry_count: u32 = 0;

    loop {
        if *shutdown_rx.borrow() {
            tracing::info!("收到 shutdown 信号，本机 relay 主循环停止");
            return Ok(());
        }

        let url = format!("{}?token={}", config.cloud_url, config.token);

        tracing::info!(retry = retry_count, "连接云端 WebSocket...");

        match connect_async(&url).await {
            Ok((ws_stream, _response)) => {
                retry_count = 0;
                tracing::info!("WebSocket 连接成功");

                if let Err(e) = run_session(ws_stream, &config, shutdown_rx.clone()).await {
                    tracing::error!(error = %e, "会话异常终止");
                }
            }
            Err(e) => {
                tracing::error!(error = %e, "WebSocket 连接失败");
            }
        }

        let delay = reconnect_delay(retry_count);
        tracing::info!(delay_secs = delay.as_secs(), "等待重连...");
        tokio::select! {
            _ = tokio::time::sleep(delay) => {}
            changed = shutdown_rx.changed() => {
                if changed.is_err() || *shutdown_rx.borrow() {
                    tracing::info!("等待重连时收到 shutdown 信号");
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
    let register_msg = RelayMessage::Register {
        id: RelayMessage::new_id("reg"),
        payload: RegisterPayload {
            backend_id: config.backend_id.clone(),
            name: config.name.clone(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            capabilities: build_capabilities(&handler, &config.mcp_manager),
            workspace_roots: config
                .workspace_roots
                .iter()
                .map(|p| p.to_string_lossy().to_string())
                .collect(),
        },
    };

    send_message(&mut write, &register_msg).await?;
    tracing::info!("已发送注册消息");

    // 等待 register_ack
    let ack_timeout = loop {
        tokio::select! {
            result = tokio::time::timeout(Duration::from_secs(10), read.next()) => break result,
            changed = shutdown_rx.changed() => {
                if changed.is_err() || *shutdown_rx.borrow() {
                    tracing::info!("等待注册响应时收到 shutdown 信号");
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
                        tracing::info!(
                            backend_id = %payload.backend_id,
                            status = %payload.status,
                            "注册成功"
                        );
                    }
                    RelayMessage::Error { error, .. } => {
                        anyhow::bail!("注册失败: {}", error);
                    }
                    other => {
                        tracing::warn!("期望 register_ack，收到: {:?}", other.id());
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

    loop {
        tokio::select! {
            msg = read.next() => {
                match msg {
                    Some(Ok(ws_msg)) => {
                        if let Some(relay_msg) = parse_ws_message(&ws_msg) {
                            if should_handle_in_background(&relay_msg) {
                                let handler = handler.clone();
                                let outbound_tx = outbound_tx.clone();
                                tokio::spawn(async move {
                                    let msg_id = relay_msg.id().to_string();
                                    let responses = handler.handle(relay_msg).await;
                                    for resp in responses {
                                        if outbound_tx.send(resp).is_err() {
                                            tracing::debug!(
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
                                        tracing::debug!("relay response 写出通道已关闭");
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    Some(Err(e)) => {
                        tracing::error!(error = %e, "WebSocket 读取错误");
                        break;
                    }
                    None => {
                        tracing::info!("WebSocket 连接关闭");
                        break;
                    }
                }
            }
            event = event_rx.recv() => {
                match event {
                    Some(relay_msg) => {
                        if outbound_tx.send(relay_msg).is_err() {
                            tracing::warn!("relay event 写出通道已关闭");
                            break;
                        }
                    }
                    None => {
                        tracing::warn!("事件通道关闭");
                        break;
                    }
                }
            }
            _ = ping_interval.tick() => {
                // 本机侧不主动发 ping，只响应云端的 ping
            }
            changed = shutdown_rx.changed() => {
                if changed.is_err() || *shutdown_rx.borrow() {
                    tracing::info!("消息循环收到 shutdown 信号");
                    break;
                }
            }
            writer_result = &mut writer_task => {
                match writer_result {
                    Ok(Ok(())) => tracing::info!("WebSocket 写出任务结束"),
                    Ok(Err(e)) => tracing::error!(error = %e, "WebSocket 写出错误"),
                    Err(e) => tracing::error!(error = %e, "WebSocket 写出任务异常结束"),
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

fn should_handle_in_background(msg: &RelayMessage) -> bool {
    matches!(msg, RelayMessage::CommandToolShellExec { .. })
}

fn build_capabilities(
    handler: &LocalCommandRouter,
    mcp_manager: &Option<Arc<McpClientManager>>,
) -> CapabilitiesPayload {
    let executors = handler.list_executors();
    let mcp_servers = mcp_manager
        .as_ref()
        .map(|m| m.capability_entries())
        .unwrap_or_default();
    CapabilitiesPayload {
        executors,
        supports_cancel: true,
        supports_discover_options: false,
        mcp_servers,
    }
}

fn parse_ws_message(msg: &Message) -> Option<RelayMessage> {
    match msg {
        Message::Text(text) => match serde_json::from_str::<RelayMessage>(text.as_ref()) {
            Ok(relay_msg) => Some(relay_msg),
            Err(e) => {
                tracing::warn!(error = %e, "无法解析 WebSocket 消息");
                None
            }
        },
        Message::Close(_) => {
            tracing::info!("收到关闭帧");
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
            },
        };

        assert!(should_handle_in_background(&msg));
    }

    #[test]
    fn ordinary_relay_messages_keep_inline_ordering() {
        let msg = RelayMessage::Ping {
            id: "ping-1".to_string(),
            payload: PingPayload { server_time: 1 },
        };

        assert!(!should_handle_in_background(&msg));
    }
}
