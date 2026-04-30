//! SessionTurnProcessor — per-turn 事件处理器。
//!
//! 统一 cloud-native 和 relay 两条路径的 notification 处理逻辑。
//! 所有 turn 生命周期内的 notification（无论来自 connector stream 还是 relay 注入）
//! 都经由此处处理：on_event → persist → broadcast → terminal hook → effects。

use std::sync::Arc;

use agent_client_protocol::SessionNotification;
use tokio::sync::mpsc;

use agentdash_acp_meta::AgentDashSourceV1;
use agentdash_spi::hooks::{HookTrigger, SharedHookSessionRuntime};

use super::hub::HookTriggerInput;
use super::hub::SessionHub;
use super::hub_support::*;
use super::persistence::SessionPersistence;
use super::persistence_listener;
use super::post_turn_handler::DynPostTurnHandler;

/// Processor 消费的事件类型。
pub enum TurnEvent {
    /// 一条 session notification（来自 connector stream 或 relay 注入）。
    Notification(SessionNotification),
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
    pub source: AgentDashSourceV1,
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

        let persistence = hub.persistence.clone();
        let sessions = hub.sessions.clone();

        let mut terminal_kind = TurnTerminalKind::Completed;
        let mut terminal_message: Option<String> = None;
        let mut received_terminal = false;
        let mut last_executor_session_id: Option<String> = None;

        while let Some(event) = rx.recv().await {
            match event {
                TurnEvent::Notification(notification) => {
                    Self::handle_notification(
                        &hub,
                        &persistence,
                        &session_id,
                        &turn_id,
                        &notification,
                        &post_turn_handler,
                        &mut last_executor_session_id,
                    )
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
            let (cancel_requested, live_turn_matches) = {
                let guard = sessions.lock().await;
                match guard.get(&session_id).and_then(|rt| rt.current_turn.as_ref()) {
                    Some(turn) => (
                        turn.cancel_requested,
                        turn.turn_id.as_str() == turn_id.as_str(),
                    ),
                    None => (false, false),
                }
            };
            if cancel_requested && live_turn_matches {
                terminal_kind = TurnTerminalKind::Interrupted;
                terminal_message = Some("执行已取消".to_string());
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
        let _ = hub
            .persist_notification(&session_id, terminal_notification)
            .await;

        // Hook 评估（SessionTerminal trigger）
        let terminal_effects = if let Some(hs) = hook_session.as_ref() {
            hub.emit_session_hook_trigger(
                hs.as_ref(),
                &HookTriggerInput {
                    session_id: &session_id,
                    turn_id: Some(&turn_id),
                    trigger: HookTrigger::SessionTerminal,
                    payload: Some(serde_json::json!({
                        "terminal_state": terminal_kind.state_tag(),
                        "message": terminal_message,
                    })),
                    refresh_reason: "trigger:session_terminal",
                    source: source.clone(),
                },
            )
            .await
        } else {
            Vec::new()
        };

        // PostTurnHandler effect 执行
        if let Some(handler) = post_turn_handler.as_ref() {
            if !terminal_effects.is_empty() {
                let supported = handler.supported_effect_kinds();
                if !supported.is_empty() {
                    for eff in &terminal_effects {
                        if !supported.contains(&eff.kind.as_str()) {
                            tracing::warn!(
                                session_id = %session_id,
                                effect_kind = %eff.kind,
                                supported = ?supported,
                                "Hook 产出了 handler 未声明支持的 effect kind"
                            );
                        }
                    }
                }
                handler
                    .execute_effects(&session_id, &turn_id, &terminal_effects)
                    .await;
            }
        }

        // Hook auto-resume 逻辑
        const MAX_HOOK_AUTO_RESUMES: u32 = 2;
        let should_auto_resume = matches!(terminal_kind, TurnTerminalKind::Completed)
            && hook_session.as_ref().is_some_and(|hs| {
                let trace = hs.trace();
                trace
                    .iter()
                    .rev()
                    .find(|t| matches!(t.trigger, HookTrigger::BeforeStop))
                    .is_some_and(|t| t.decision == "continue")
            });

        let can_auto_resume = should_auto_resume && {
            let guard = sessions.lock().await;
            guard
                .get(&session_id)
                .is_some_and(|rt| rt.hook_auto_resume_count < MAX_HOOK_AUTO_RESUMES)
        };

        // 清理 running 状态 — 整个 current_turn 一起退位
        {
            let mut guard = sessions.lock().await;
            if let Some(runtime) = guard.get_mut(&session_id) {
                runtime.running = false;
                runtime.current_turn = None;
                if can_auto_resume {
                    runtime.hook_auto_resume_count += 1;
                }
            }
        }

        // SessionTerminalCallback — 平台级 session 终态回调（如 LifecycleOrchestrator）
        {
            let cb_guard = hub.terminal_callback.read().await;
            if let Some(cb) = cb_guard.as_ref() {
                let state_tag = terminal_kind.state_tag();
                cb.on_session_terminal(&session_id, state_tag).await;
            }
        }

        if can_auto_resume {
            tracing::info!(
                session_id = %session_id,
                "Hook auto-resume: stop gate unsatisfied, scheduling retry"
            );
            hub.schedule_hook_auto_resume(session_id);
        }
    }

    /// 处理单条 notification：委托 persistence_listener 同步 meta → on_event → persist。
    ///
    /// PR 7：`SessionMeta` 写入不再在 processor 里直接做，而是外包给
    /// `persistence_listener::sync_executor_session_id`；processor 只持有
    /// per-turn 去重状态 (`last_executor_session_id`) 并传入。
    async fn handle_notification(
        hub: &SessionHub,
        persistence: &Arc<dyn SessionPersistence>,
        session_id: &str,
        turn_id: &str,
        notification: &SessionNotification,
        post_turn_handler: &Option<DynPostTurnHandler>,
        last_executor_session_id: &mut Option<String>,
    ) {
        persistence_listener::sync_executor_session_id(
            persistence,
            session_id,
            turn_id,
            notification,
            last_executor_session_id,
        )
        .await;

        if let Some(handler) = post_turn_handler.as_ref() {
            handler.on_event(session_id, notification).await;
        }
        let _ = hub
            .persist_notification(session_id, notification.clone())
            .await;
    }
}
