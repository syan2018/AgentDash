//! SessionTurnProcessor — per-turn 事件处理器。
//!
//! 统一 cloud-native 和 relay 两条路径的 notification 处理逻辑。
//! 所有 turn 生命周期内的 notification（无论来自 connector stream 还是 relay 注入）
//! 都经由此处处理：on_event → persist → terminal control-plane sync → broadcast → effects。

use agentdash_agent_protocol::{BackboneEnvelope, BackboneEvent};
use agentdash_diagnostics::{DiagnosticErrorContext, Subsystem, diag_error};
use tokio::sync::mpsc;

use agentdash_agent_protocol::SourceInfo;
use agentdash_spi::hooks::SharedHookRuntime;

use super::effects_service::SessionEffectsService;
use super::eventing::SessionEventingService;
use super::hub_support::*;
use super::post_turn_handler::DynPostTurnHandler;
use super::terminal_effects::{TerminalCallbackDispatchInput, TerminalEffectDispatchInput};
use super::turn_supervisor::TurnSupervisor;

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
    pub hook_runtime: Option<SharedHookRuntime>,
    pub post_turn_handler: Option<DynPostTurnHandler>,
}

#[derive(Clone)]
pub(in crate::session) struct SessionTurnProcessorDeps {
    pub(in crate::session) turn_supervisor: TurnSupervisor,
    pub(in crate::session) eventing: SessionEventingService,
    pub(in crate::session) effects: SessionEffectsService,
}

pub(in crate::session) struct TurnTerminalDispatch {
    pub(in crate::session) session_id: String,
    pub(in crate::session) turn_id: String,
    pub(in crate::session) source: SourceInfo,
    pub(in crate::session) terminal_kind: TurnTerminalKind,
    pub(in crate::session) terminal_message: Option<String>,
    pub(in crate::session) hook_runtime: Option<SharedHookRuntime>,
    pub(in crate::session) post_turn_handler: Option<DynPostTurnHandler>,
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
    pub(super) fn spawn(
        deps: SessionTurnProcessorDeps,
        config: SessionTurnProcessorConfig,
    ) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let join_handle = tokio::spawn(Self::run(deps, config, rx));
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
        deps: SessionTurnProcessorDeps,
        config: SessionTurnProcessorConfig,
        mut rx: mpsc::UnboundedReceiver<TurnEvent>,
    ) {
        let SessionTurnProcessorConfig {
            session_id,
            turn_id,
            source,
            hook_runtime,
            post_turn_handler,
        } = config;

        let mut terminal_kind = TurnTerminalKind::Completed;
        let mut terminal_message: Option<String> = None;
        let mut received_terminal = false;

        while let Some(event) = rx.recv().await {
            match event {
                TurnEvent::Notification(notification) => {
                    Self::handle_notification(
                        &deps,
                        &session_id,
                        &notification,
                        &post_turn_handler,
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
        if !received_terminal
            && let Some((kind, message)) = deps
                .turn_supervisor
                .cancel_interrupted_terminal(&session_id, &turn_id)
                .await
        {
            terminal_kind = kind;
            terminal_message = message;
        }

        process_turn_terminal(
            &deps,
            TurnTerminalDispatch {
                session_id,
                turn_id,
                source,
                terminal_kind,
                terminal_message,
                hook_runtime,
                post_turn_handler,
            },
        )
        .await;
    }

    /// 处理单条 notification：on_event → persist。
    ///
    /// `executor_session_id` 同步已由 `append_event` 的事件投影统一处理，
    /// processor 不再需要额外的直接 meta 写入路径。
    async fn handle_notification(
        deps: &SessionTurnProcessorDeps,
        session_id: &str,
        envelope: &BackboneEnvelope,
        post_turn_handler: &Option<DynPostTurnHandler>,
    ) {
        if let Some(handler) = post_turn_handler.as_ref() {
            handler.on_event(session_id, envelope).await;
        }
        let notification = Self::notification_with_turn_duration(deps, session_id, envelope).await;
        let _ = deps
            .eventing
            .persist_notification(session_id, notification)
            .await;
    }

    async fn notification_with_turn_duration(
        deps: &SessionTurnProcessorDeps,
        session_id: &str,
        envelope: &BackboneEnvelope,
    ) -> BackboneEnvelope {
        let BackboneEvent::TurnCompleted(completed) = &envelope.event else {
            return envelope.clone();
        };
        let turn_id = envelope
            .trace
            .turn_id
            .as_deref()
            .unwrap_or(completed.turn.id.as_str());
        let Some(started_at_ms) = deps
            .turn_supervisor
            .active_turn_started_at_ms(session_id, turn_id)
            .await
        else {
            return envelope.clone();
        };
        let completed_at_ms = chrono::Utc::now().timestamp_millis();
        let timing = TurnTiming::complete(started_at_ms, completed_at_ms);
        let mut envelope = envelope.clone();
        if let BackboneEvent::TurnCompleted(completed) = &mut envelope.event {
            completed.turn.started_at = completed
                .turn
                .started_at
                .or(Some(timing.started_at_ms.div_euclid(1000)));
            completed.turn.completed_at = completed
                .turn
                .completed_at
                .or(Some(timing.completed_at_ms.div_euclid(1000)));
            completed.turn.duration_ms = completed.turn.duration_ms.or(Some(timing.duration_ms));
        }
        envelope
    }
}

impl TurnTerminalKind {
    fn requires_rewind_marker(self) -> bool {
        matches!(self, Self::Failed | Self::Interrupted | Self::Lost)
    }

    fn rewind_reason(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Failed => "runtime_failure",
            Self::Interrupted => "runtime_interrupted",
            Self::Lost => "runtime_lost",
        }
    }
}

pub(in crate::session) async fn process_turn_terminal(
    deps: &SessionTurnProcessorDeps,
    input: TurnTerminalDispatch,
) {
    let TurnTerminalDispatch {
        session_id,
        turn_id,
        source,
        terminal_kind,
        terminal_message,
        hook_runtime,
        post_turn_handler,
    } = input;

    let completed_at_ms = chrono::Utc::now().timestamp_millis();
    let terminal_timing = deps
        .turn_supervisor
        .active_turn_started_at_ms(&session_id, &turn_id)
        .await
        .map(|started_at_ms| TurnTiming::complete(started_at_ms, completed_at_ms));

    let terminal_notification = build_turn_terminal_notification_with_timing(
        &session_id,
        &source,
        &turn_id,
        terminal_kind,
        terminal_message.clone(),
        terminal_timing,
    );
    let terminal_event = deps
        .eventing
        .persist_notification_deferred_broadcast(&session_id, terminal_notification)
        .await;

    let (terminal_event_seq, terminal_event) = match terminal_event {
        Ok(event) => {
            let event_seq = event.event_seq;
            (event_seq, event)
        }
        Err(error) => {
            deps.turn_supervisor.clear_active_turn(&session_id).await;
            let context =
                DiagnosticErrorContext::new("session.turn_processor", "persist_terminal_event");
            diag_error!(
                Error,
                Subsystem::AgentRun,
                context = &context,
                error = &error,
                session_id = %session_id,
                turn_id = %turn_id,
                terminal_kind = ?terminal_kind,
                "Turn terminal event 持久化失败，跳过 terminal effect outbox"
            );
            return;
        }
    };

    let rewind_event = if terminal_kind.requires_rewind_marker() {
        match deps
            .eventing
            .persist_session_rewound_marker(
                &session_id,
                &source,
                &turn_id,
                terminal_kind.rewind_reason(),
                terminal_message.clone(),
                terminal_event_seq,
                false,
            )
            .await
        {
            Ok(event) => Some(event),
            Err(error) => {
                let context =
                    DiagnosticErrorContext::new("session.turn_processor", "persist_rewind_marker");
                diag_error!(
                    Error,
                    Subsystem::AgentRun,
                    context = &context,
                    error = &error,
                    session_id = %session_id,
                    turn_id = %turn_id,
                    terminal_event_seq,
                    terminal_kind = ?terminal_kind,
                    "Session rewind marker 持久化失败，仍释放 active turn"
                );
                None
            }
        }
    } else {
        None
    };

    deps.turn_supervisor.clear_active_turn(&session_id).await;

    let terminal_callback_input = TerminalCallbackDispatchInput {
        session_id: session_id.clone(),
        turn_id: turn_id.clone(),
        terminal_event_seq,
        terminal_kind,
        terminal_message: terminal_message.clone(),
    };
    deps.effects
        .dispatch_terminal_callback(terminal_callback_input)
        .await;

    deps.eventing
        .broadcast_persisted_event(&session_id, terminal_event)
        .await;
    if let Some(rewind_event) = rewind_event {
        deps.eventing
            .broadcast_persisted_event(&session_id, rewind_event)
            .await;
    }

    let terminal_effect_input = TerminalEffectDispatchInput {
        session_id: session_id.clone(),
        turn_id: turn_id.clone(),
        terminal_event_seq,
        terminal_kind,
        terminal_message: terminal_message.clone(),
        source,
        hook_runtime,
        post_turn_handler,
    };

    deps.effects
        .dispatch_terminal_effects(terminal_effect_input)
        .await;
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::Arc;

    use agentdash_agent_protocol::{PlatformEvent, SourceInfo};
    use agentdash_spi::hooks::{HookEffect, HookRuntimeAccess};
    use agentdash_spi::{
        AgentConfig, AgentConnector, CapabilityState, ConnectorCapabilities, ConnectorError,
        ConnectorType, ExecutionContext, ExecutionSessionFrame, ExecutionStream, PromptPayload,
    };
    use async_trait::async_trait;
    use futures::stream;
    use tokio::sync::{Mutex, RwLock, broadcast, mpsc};
    use uuid::Uuid;

    use super::super::MemoryRuntimeTraceStore;
    use super::super::effects_service::SessionEffectsService;
    use super::super::eventing::SessionEventingService;
    use super::super::hub_support::{SessionProfile, TurnExecution, TurnTerminalKind};
    use super::super::persistence::{
        PersistedSessionEvent, SessionEventBacklog, SessionEventPage, SessionEventStore,
        SessionStoreSet,
    };
    use super::super::post_turn_handler::{SessionTerminalCallback, SessionTerminalNotification};
    use super::super::runtime_registry::SessionRuntimeRegistry;
    use super::super::terminal_effects::{
        TerminalAutoResumePort, TerminalAutoResumeRequest, TerminalEffectDeps,
        TerminalHookTriggerPort, TerminalHookTriggerRequest,
    };
    use super::super::turn_supervisor::TurnSupervisor;
    use super::super::types::{ExecutionStatus, SessionMeta, TitleSource};
    use super::*;
    use crate::session::persistence::{SessionStoreError, SessionStoreResult};

    #[tokio::test]
    async fn terminal_persist_failure_still_clears_active_turn() {
        let session_id = "sess-terminal-persist-fails";
        let turn_id = "turn-1";
        let registry = SessionRuntimeRegistry::new(Arc::new(Mutex::new(HashMap::new())));
        let supervisor = TurnSupervisor::new(registry.clone());
        supervisor
            .claim_prompt(session_id)
            .await
            .expect("claim should succeed");
        supervisor
            .activate_turn(
                session_id,
                SessionProfile {
                    capability_state: CapabilityState::default(),
                },
                TurnExecution::new(
                    turn_id.to_string(),
                    ExecutionSessionFrame {
                        turn_id: turn_id.to_string(),
                        working_directory: PathBuf::from("."),
                        environment_variables: HashMap::new(),
                        executor_config: AgentConfig::new("PI_AGENT"),
                        mcp_servers: Vec::new(),
                        vfs: None,
                        vfs_access_policy: None,
                        backend_execution: None,
                        runtime_backend_anchor: None,
                        identity: None,
                    },
                    CapabilityState::default(),
                    Uuid::new_v4(),
                    Uuid::new_v4(),
                ),
            )
            .await;
        assert!(registry.has_active_turn(session_id).await);

        let persistence = Arc::new(MemoryRuntimeTraceStore::default());
        let base_stores = SessionStoreSet::from_runtime_trace_test_store(persistence);
        let stores = SessionStoreSet {
            events: Arc::new(FailingEventStore),
            ..base_stores
        };
        let deps = SessionTurnProcessorDeps {
            turn_supervisor: supervisor,
            eventing: SessionEventingService::new(
                stores.eventing_stores(),
                registry.clone(),
                Arc::new(NoopConnector),
            ),
            effects: SessionEffectsService::new(TerminalEffectDeps {
                terminal_effects: stores.terminal_effects.clone(),
                hook_trigger: Arc::new(NoopHookTrigger),
                terminal_callback: Arc::new(RwLock::new(None)),
                hook_effect_handler_registry: Arc::new(RwLock::new(None)),
                auto_resume: Arc::new(NoopAutoResume),
            }),
        };
        let config = SessionTurnProcessorConfig {
            session_id: session_id.to_string(),
            turn_id: turn_id.to_string(),
            source: SourceInfo {
                connector_id: "test".to_string(),
                connector_type: "unit".to_string(),
                executor_id: None,
            },
            hook_runtime: None,
            post_turn_handler: None,
        };
        let (tx, rx) = mpsc::unbounded_channel();
        tx.send(TurnEvent::Terminal {
            kind: TurnTerminalKind::Completed,
            message: None,
        })
        .expect("terminal event should be queued");
        drop(tx);

        SessionTurnProcessor::run(deps, config, rx).await;

        assert!(!registry.has_active_turn(session_id).await);
    }

    #[tokio::test]
    async fn failed_terminal_persists_duration_rewind_and_allows_next_prompt() {
        let session_id = "sess-terminal-failed-recovers";
        let turn_id = "turn-1";
        let registry = SessionRuntimeRegistry::new(Arc::new(Mutex::new(HashMap::new())));
        let supervisor = TurnSupervisor::new(registry.clone());
        supervisor
            .claim_prompt(session_id)
            .await
            .expect("claim should succeed");
        supervisor
            .activate_turn(
                session_id,
                SessionProfile {
                    capability_state: CapabilityState::default(),
                },
                TurnExecution::new_with_started_at(
                    turn_id.to_string(),
                    ExecutionSessionFrame {
                        turn_id: turn_id.to_string(),
                        working_directory: PathBuf::from("."),
                        environment_variables: HashMap::new(),
                        executor_config: AgentConfig::new("PI_AGENT"),
                        mcp_servers: Vec::new(),
                        vfs: None,
                        vfs_access_policy: None,
                        backend_execution: None,
                        runtime_backend_anchor: None,
                        identity: None,
                    },
                    CapabilityState::default(),
                    Uuid::new_v4(),
                    Uuid::new_v4(),
                    chrono::Utc::now().timestamp_millis().saturating_sub(1_500),
                ),
            )
            .await;
        assert!(registry.has_active_turn(session_id).await);

        let persistence = Arc::new(MemoryRuntimeTraceStore::default());
        let stores = SessionStoreSet::from_runtime_trace_test_store(persistence);
        stores
            .meta
            .create_session(&SessionMeta {
                id: session_id.to_string(),
                title: "Test".to_string(),
                title_source: TitleSource::Auto,
                created_at: 1,
                updated_at: 1,
                last_event_seq: 0,
                last_delivery_status: ExecutionStatus::Running,
                last_turn_id: Some(turn_id.to_string()),
                last_terminal_message: None,
                executor_session_id: None,
            })
            .await
            .expect("create session meta");
        let deps = SessionTurnProcessorDeps {
            turn_supervisor: supervisor.clone(),
            eventing: SessionEventingService::new(
                stores.eventing_stores(),
                registry.clone(),
                Arc::new(NoopConnector),
            ),
            effects: SessionEffectsService::new(TerminalEffectDeps {
                terminal_effects: stores.terminal_effects.clone(),
                hook_trigger: Arc::new(NoopHookTrigger),
                terminal_callback: Arc::new(RwLock::new(None)),
                hook_effect_handler_registry: Arc::new(RwLock::new(None)),
                auto_resume: Arc::new(NoopAutoResume),
            }),
        };
        let config = SessionTurnProcessorConfig {
            session_id: session_id.to_string(),
            turn_id: turn_id.to_string(),
            source: SourceInfo {
                connector_id: "test".to_string(),
                connector_type: "unit".to_string(),
                executor_id: None,
            },
            hook_runtime: None,
            post_turn_handler: None,
        };
        let (tx, rx) = mpsc::unbounded_channel();
        tx.send(TurnEvent::Terminal {
            kind: TurnTerminalKind::Failed,
            message: Some("provider disconnected".to_string()),
        })
        .expect("terminal event should be queued");
        drop(tx);

        SessionTurnProcessor::run(deps, config, rx).await;

        assert!(!registry.has_active_turn(session_id).await);
        supervisor
            .claim_prompt(session_id)
            .await
            .expect("failed terminal cleanup should allow next prompt");
        supervisor.clear_active_turn(session_id).await;

        let events = stores
            .events
            .list_all_events(session_id)
            .await
            .expect("read events");
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event_seq + 1, events[1].event_seq);

        match &events[0].notification.event {
            BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate { key, value }) => {
                assert_eq!(key, "turn_terminal");
                assert_eq!(
                    value
                        .get("terminal_type")
                        .and_then(serde_json::Value::as_str),
                    Some("turn_failed")
                );
                let duration_ms = value
                    .get("duration_ms")
                    .and_then(serde_json::Value::as_i64)
                    .expect("duration_ms should be present");
                assert!(duration_ms >= 1_000);
                assert!(value.get("started_at_ms").is_some());
                assert!(value.get("completed_at_ms").is_some());
            }
            event => panic!("expected turn_terminal event, got {event:?}"),
        }

        match &events[1].notification.event {
            BackboneEvent::Platform(PlatformEvent::SessionRewound(marker)) => {
                assert_eq!(marker.discarded_turn_id, turn_id);
                assert_eq!(marker.stable_event_seq, 0);
                assert_eq!(
                    marker.reason,
                    agentdash_agent_protocol::SessionRewindReason::RuntimeFailure
                );
            }
            event => panic!("expected session_rewound event, got {event:?}"),
        }
    }

    #[tokio::test]
    async fn process_turn_terminal_dispatches_session_terminal_callback() {
        let session_id = "sess-terminal-callback";
        let turn_id = "turn-1";
        let registry = SessionRuntimeRegistry::new(Arc::new(Mutex::new(HashMap::new())));
        let supervisor = TurnSupervisor::new(registry.clone());
        supervisor
            .claim_prompt(session_id)
            .await
            .expect("claim should succeed");
        supervisor
            .activate_turn(
                session_id,
                SessionProfile {
                    capability_state: CapabilityState::default(),
                },
                TurnExecution::new(
                    turn_id.to_string(),
                    ExecutionSessionFrame {
                        turn_id: turn_id.to_string(),
                        working_directory: PathBuf::from("."),
                        environment_variables: HashMap::new(),
                        executor_config: AgentConfig::new("PI_AGENT"),
                        mcp_servers: Vec::new(),
                        vfs: None,
                        vfs_access_policy: None,
                        backend_execution: None,
                        runtime_backend_anchor: None,
                        identity: None,
                    },
                    CapabilityState::default(),
                    Uuid::new_v4(),
                    Uuid::new_v4(),
                ),
            )
            .await;

        let persistence = Arc::new(MemoryRuntimeTraceStore::default());
        let stores = SessionStoreSet::from_runtime_trace_test_store(persistence);
        stores
            .meta
            .create_session(&SessionMeta {
                id: session_id.to_string(),
                title: "Test".to_string(),
                title_source: TitleSource::Auto,
                created_at: 1,
                updated_at: 1,
                last_event_seq: 0,
                last_delivery_status: ExecutionStatus::Running,
                last_turn_id: Some(turn_id.to_string()),
                last_terminal_message: None,
                executor_session_id: None,
            })
            .await
            .expect("create session meta");
        let notifications = Arc::new(Mutex::new(Vec::new()));
        let broadcast_rx = Arc::new(Mutex::new(registry.subscribe(session_id).await));
        let broadcast_seen_before_callback = Arc::new(Mutex::new(None));
        let deps = SessionTurnProcessorDeps {
            turn_supervisor: supervisor.clone(),
            eventing: SessionEventingService::new(
                stores.eventing_stores(),
                registry.clone(),
                Arc::new(NoopConnector),
            ),
            effects: SessionEffectsService::new(TerminalEffectDeps {
                terminal_effects: stores.terminal_effects.clone(),
                hook_trigger: Arc::new(NoopHookTrigger),
                terminal_callback: Arc::new(RwLock::new(Some(Arc::new(
                    RecordingTerminalCallback {
                        notifications: notifications.clone(),
                        broadcast_rx: Some(broadcast_rx),
                        broadcast_seen_before_callback: Some(
                            broadcast_seen_before_callback.clone(),
                        ),
                    },
                )))),
                hook_effect_handler_registry: Arc::new(RwLock::new(None)),
                auto_resume: Arc::new(NoopAutoResume),
            }),
        };

        process_turn_terminal(
            &deps,
            TurnTerminalDispatch {
                session_id: session_id.to_string(),
                turn_id: turn_id.to_string(),
                source: SourceInfo {
                    connector_id: "test".to_string(),
                    connector_type: "unit".to_string(),
                    executor_id: None,
                },
                terminal_kind: TurnTerminalKind::Failed,
                terminal_message: Some("connector start failed".to_string()),
                hook_runtime: None,
                post_turn_handler: None,
            },
        )
        .await;

        assert!(!registry.has_active_turn(session_id).await);
        let notifications = notifications.lock().await;
        assert_eq!(notifications.len(), 1);
        let notification = &notifications[0];
        assert_eq!(notification.session_id, session_id);
        assert_eq!(notification.turn_id, turn_id);
        assert_eq!(notification.terminal_state, "failed");
        assert_eq!(
            notification.terminal_message.as_deref(),
            Some("connector start failed")
        );
        assert!(notification.terminal_event_seq > 0);
        assert_eq!(*broadcast_seen_before_callback.lock().await, Some(false));
    }

    struct RecordingTerminalCallback {
        notifications: Arc<Mutex<Vec<SessionTerminalNotification>>>,
        broadcast_rx: Option<Arc<Mutex<broadcast::Receiver<PersistedSessionEvent>>>>,
        broadcast_seen_before_callback: Option<Arc<Mutex<Option<bool>>>>,
    }

    #[async_trait]
    impl SessionTerminalCallback for RecordingTerminalCallback {
        async fn on_session_terminal(
            &self,
            notification: SessionTerminalNotification,
        ) -> Result<(), String> {
            if let (Some(rx), Some(result)) =
                (&self.broadcast_rx, &self.broadcast_seen_before_callback)
            {
                let saw_broadcast = match rx.lock().await.try_recv() {
                    Ok(_) => true,
                    Err(broadcast::error::TryRecvError::Lagged(_)) => true,
                    Err(broadcast::error::TryRecvError::Empty) => false,
                    Err(broadcast::error::TryRecvError::Closed) => false,
                };
                *result.lock().await = Some(saw_broadcast);
            }
            self.notifications.lock().await.push(notification);
            Ok(())
        }
    }

    struct FailingEventStore;

    #[async_trait]
    impl SessionEventStore for FailingEventStore {
        async fn append_event(
            &self,
            _session_id: &str,
            _envelope: &agentdash_agent_protocol::BackboneEnvelope,
        ) -> SessionStoreResult<PersistedSessionEvent> {
            Err(SessionStoreError::Internal(
                "forced terminal persist failure".to_string(),
            ))
        }

        async fn read_backlog(
            &self,
            _session_id: &str,
            _after_seq: u64,
        ) -> SessionStoreResult<SessionEventBacklog> {
            Ok(SessionEventBacklog {
                snapshot_seq: 0,
                events: Vec::new(),
            })
        }

        async fn list_event_page(
            &self,
            _session_id: &str,
            _after_seq: u64,
            _limit: u32,
        ) -> SessionStoreResult<SessionEventPage> {
            Ok(SessionEventPage {
                snapshot_seq: 0,
                events: Vec::new(),
                has_more: false,
                next_after_seq: 0,
            })
        }

        async fn list_all_events(
            &self,
            _session_id: &str,
        ) -> SessionStoreResult<Vec<PersistedSessionEvent>> {
            Ok(Vec::new())
        }

        async fn list_events_from(
            &self,
            _session_id: &str,
            _from_seq: u64,
        ) -> SessionStoreResult<Vec<PersistedSessionEvent>> {
            Ok(Vec::new())
        }
    }

    struct NoopHookTrigger;

    #[async_trait]
    impl TerminalHookTriggerPort for NoopHookTrigger {
        async fn emit_terminal_hook_trigger(
            &self,
            _hook_runtime: &dyn HookRuntimeAccess,
            _input: TerminalHookTriggerRequest<'_>,
        ) -> Vec<HookEffect> {
            Vec::new()
        }
    }

    struct NoopAutoResume;

    #[async_trait]
    impl TerminalAutoResumePort for NoopAutoResume {
        async fn request_hook_auto_resume(
            &self,
            _request: TerminalAutoResumeRequest,
        ) -> Result<bool, String> {
            Ok(false)
        }
    }

    struct NoopConnector;

    #[async_trait]
    impl AgentConnector for NoopConnector {
        fn connector_id(&self) -> &'static str {
            "noop"
        }

        fn connector_type(&self) -> ConnectorType {
            ConnectorType::LocalExecutor
        }

        fn capabilities(&self) -> ConnectorCapabilities {
            ConnectorCapabilities::default()
        }

        fn list_executors(&self) -> Vec<agentdash_spi::AgentInfo> {
            Vec::new()
        }

        async fn discover_options_stream(
            &self,
            _executor: &str,
            _working_dir: Option<PathBuf>,
        ) -> Result<futures::stream::BoxStream<'static, json_patch::Patch>, ConnectorError>
        {
            Ok(Box::pin(stream::empty()))
        }

        async fn prompt(
            &self,
            _session_id: &str,
            _follow_up_session_id: Option<&str>,
            _prompt: &PromptPayload,
            _context: ExecutionContext,
        ) -> Result<ExecutionStream, ConnectorError> {
            Ok(Box::pin(stream::empty()))
        }

        async fn cancel(&self, _session_id: &str) -> Result<(), ConnectorError> {
            Ok(())
        }

        async fn approve_tool_call(
            &self,
            _session_id: &str,
            _tool_call_id: &str,
        ) -> Result<(), ConnectorError> {
            Ok(())
        }

        async fn reject_tool_call(
            &self,
            _session_id: &str,
            _tool_call_id: &str,
            _reason: Option<String>,
        ) -> Result<(), ConnectorError> {
            Ok(())
        }
    }
}
