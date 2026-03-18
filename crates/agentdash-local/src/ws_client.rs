use std::path::PathBuf;
use std::time::Duration;

use futures::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

use std::sync::Arc;

use agentdash_relay::*;

use crate::command_handler::CommandHandler;
use crate::tool_executor::ToolExecutor;
use agentdash_executor::{AgentConnector, ExecutorHub};

pub struct Config {
    pub cloud_url: String,
    pub token: String,
    pub backend_id: String,
    pub name: String,
    pub accessible_roots: Vec<PathBuf>,
    pub tool_executor: ToolExecutor,
    pub executor_hub: Option<ExecutorHub>,
    pub connector: Option<Arc<dyn AgentConnector>>,
}

/// 主循环：连接 → 注册 → 消息处理 → 断线 → 重连
pub async fn run(config: Config) -> anyhow::Result<()> {
    let mut retry_count: u32 = 0;

    loop {
        let url = format!("{}?token={}", config.cloud_url, config.token);

        tracing::info!(retry = retry_count, "连接云端 WebSocket...");

        match connect_async(&url).await {
            Ok((ws_stream, _response)) => {
                retry_count = 0;
                tracing::info!("WebSocket 连接成功");

                if let Err(e) = run_session(ws_stream, &config).await {
                    tracing::error!(error = %e, "会话异常终止");
                }
            }
            Err(e) => {
                tracing::error!(error = %e, "WebSocket 连接失败");
            }
        }

        let delay = reconnect_delay(retry_count);
        tracing::info!(delay_secs = delay.as_secs(), "等待重连...");
        tokio::time::sleep(delay).await;
        retry_count += 1;
    }
}

/// 单次连接的完整会话
async fn run_session(
    ws_stream: WebSocketStream<MaybeTlsStream<TcpStream>>,
    config: &Config,
) -> anyhow::Result<()> {
    let (mut write, mut read) = ws_stream.split();

    // 创建事件通道（CommandHandler 通过此通道推送异步事件）
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<RelayMessage>();

    let handler = CommandHandler::new(
        config.tool_executor.clone(),
        config.executor_hub.clone(),
        config.connector.clone(),
        event_tx,
    );

    // 第一步：发送注册消息
    let register_msg = RelayMessage::Register {
        id: RelayMessage::new_id("reg"),
        payload: RegisterPayload {
            backend_id: config.backend_id.clone(),
            name: config.name.clone(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            capabilities: build_capabilities(&handler),
            accessible_roots: config
                .accessible_roots
                .iter()
                .map(|p| p.to_string_lossy().to_string())
                .collect(),
        },
    };

    send_message(&mut write, &register_msg).await?;
    tracing::info!("已发送注册消息");

    // 等待 register_ack
    let ack_timeout = tokio::time::timeout(Duration::from_secs(10), read.next()).await;
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

    // 进入消息循环（三路复用：WS 读取、事件通道、心跳检测）
    let mut ping_interval = tokio::time::interval(Duration::from_secs(30));
    ping_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    ping_interval.tick().await;

    loop {
        tokio::select! {
            msg = read.next() => {
                match msg {
                    Some(Ok(ws_msg)) => {
                        if let Some(relay_msg) = parse_ws_message(&ws_msg) {
                            let responses = handler.handle(relay_msg).await;
                            for resp in responses {
                                send_message(&mut write, &resp).await?;
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
                        send_message(&mut write, &relay_msg).await?;
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
        }
    }

    Ok(())
}

fn build_capabilities(handler: &CommandHandler) -> CapabilitiesPayload {
    let executors = handler.list_executors();
    CapabilitiesPayload {
        executors,
        supports_cancel: true,
        supports_workspace_files: true,
        supports_discover_options: true,
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
