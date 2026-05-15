//! SessionTurnProcessor — per-turn 事件处理器。
//!
//! 统一 cloud-native 和 relay 两条路径的 notification 处理逻辑。
//! 所有 turn 生命周期内的 notification（无论来自 connector stream 还是 relay 注入）
//! 都经由此处处理：on_event → persist → broadcast → terminal hook → effects。

use agentdash_agent_protocol::BackboneEnvelope;
use tokio::sync::mpsc;

use agentdash_agent_protocol::SourceInfo;
use agentdash_spi::hooks::SharedHookSessionRuntime;

use super::hub::SessionHub;
use super::hub_support::*;
use super::post_turn_handler::DynPostTurnHandler;
use super::terminal_effects::{SessionTerminalEffectDispatcher, TerminalEffectDispatchInput};

/// Processor 消费的事件类型。
pub enum TurnEvent {
    /// 一条 BackboneEnvelope（来自 connector stream 或 relay 注入）。
    Notification(Box<BackboneEnvelope>),
    /// Turn 已结束（来自 connector stream 关闭或 relay event.session_state_changed）。
    Terminal {
        kind: TurnTerminalKind,
        message: Option<String>,
    },
}

/// 创建 processor 所需的配置。
pub struct SessionTurnProcessorConfig {
    pub session_id: String,
    pub turn_id: String,
    pub source: SourceInfo,
    pub hook_session: Option<SharedHookSessionRuntime>,
    pub post_turn_handler: Option<DynPostTurnHandler>,
}

/// Per-turn 事件处理器句柄。
///
/// 持有发送端和后台任务 handle，调用方通过 `tx()` 向 channel 推送事件。
/// 后台任务在收到 `Terminal` 或 channel 关闭时完成 hook 评估 + effects 执行后退出。
pub struct SessionTurnProcessor {
    tx: mpsc::UnboundedSender<TurnEvent>,
    _join_handle: tokio::task::JoinHandle<()>,
}

impl SessionTurnProcessor {
    /// 启动 processor 后台任务，返回句柄。
    pub fn spawn(hub: SessionHub, config: SessionTurnProcessorConfig) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let join_handle = tokio::spawn(Self::run(hub, config, rx));
        Self {
            tx,
            _join_handle: join_handle,
        }
    }

    /// 获取事件发送端（clone 语义）。
    pub fn tx(&self) -> mpsc::UnboundedSender<TurnEvent> {
        self.tx.clone()
    }

    /// 后台主循环：消费 TurnEvent channel，处理 notification 和 terminal。
    async fn run(
        hub: SessionHub,
        config: SessionTurnProcessorConfig,
        mut rx: mpsc::UnboundedReceiver<TurnEvent>,
    ) {
        let SessionTurnProcessorConfig {
            session_id,
            turn_id,
            source,
            hook_session,
            post_turn_handler,
        } = config;

        let mut terminal_kind = TurnTerminalKind::Completed;
        let mut terminal_message: Option<String> = None;
        let mut received_terminal = false;

        while let Some(event) = rx.recv().await {
            match event {
                TurnEvent::Notification(notification) => {
                    Self::handle_notification(&hub, &session_id, &notification, &post_turn_handler)
                        .await;
                }
                TurnEvent::Terminal { kind, message } => {
                    terminal_kind = kind;
                    terminal_message = message;
                    received_terminal = true;
                    break;
                }
            }
        }

        // channel 关闭但没收到显式 Terminal → 检测 cancel 状态
        if !received_terminal {
            if let Some((kind, message)) = hub
                .turn_supervisor
                .cancel_interrupted_terminal(&session_id, &turn_id)
                .await
            {
                terminal_kind = kind;
                terminal_message = message;
            }
        }

        // 生成并持久化终态 notification
        let terminal_notification = build_turn_terminal_notification(
            &session_id,
            &source,
            &turn_id,
            terminal_kind,
            terminal_message.clone(),
        );
        let terminal_event = hub
            .persist_notification(&session_id, terminal_notification)
            .await;
        let terminal_event_seq = match terminal_event {
            Ok(event) => event.event_seq,
            Err(error) => {
                tracing::error!(
                    session_id = %session_id,
                    turn_id = %turn_id,
                    error = %error,
                    "Turn terminal event 持久化失败，跳过 terminal effect outbox"
                );
                return;
            }
        };

        let dispatcher = SessionTerminalEffectDispatcher::new(&hub);
        let terminal_effects = dispatcher
            .enqueue_terminal_effects(TerminalEffectDispatchInput {
                session_id: session_id.clone(),
                turn_id: turn_id.clone(),
                terminal_event_seq,
                terminal_kind,
                terminal_message: terminal_message.clone(),
                source,
                hook_session,
                post_turn_handler,
            })
            .await;

        // 清理 turn 状态 — 回到 Idle。
        // 这里必须早于 auto-resume effect 执行，否则下一轮 prompt reservation
        // 会被当前 terminal turn 拦住。
        hub.turn_supervisor.clear_active_turn(&session_id).await;

        dispatcher.execute_enqueued(terminal_effects).await;
    }

    /// 处理单条 notification：on_event → persist。
    ///
    /// `executor_session_id` 同步已由 `append_event` 的事件投影统一处理，
    /// processor 不再需要额外的直接 meta 写入路径。
    async fn handle_notification(
        hub: &SessionHub,
        session_id: &str,
        envelope: &BackboneEnvelope,
        post_turn_handler: &Option<DynPostTurnHandler>,
    ) {
        if let Some(handler) = post_turn_handler.as_ref() {
            handler.on_event(session_id, envelope).await;
        }
        let _ = hub.persist_notification(session_id, envelope.clone()).await;
    }
}
