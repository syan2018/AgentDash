use tokio::sync::{broadcast, mpsc};
use tokio::task::AbortHandle;

use super::hub_support::{SessionProfile, TurnExecution, TurnState, TurnTerminalKind};
use super::persistence::PersistedSessionEvent;
use super::runtime_registry::SessionRuntimeRegistry;
use super::turn_processor::TurnEvent;
use agentdash_spi::ConnectorError;

pub(crate) struct CancelTurnSnapshot {
    pub running: bool,
    pub current_turn_id: Option<String>,
    pub tx: broadcast::Sender<PersistedSessionEvent>,
    pub processor_tx: Option<mpsc::UnboundedSender<TurnEvent>>,
}

#[derive(Clone)]
pub(super) struct TurnSupervisor {
    registry: SessionRuntimeRegistry,
}

impl TurnSupervisor {
    pub(super) fn new(registry: SessionRuntimeRegistry) -> Self {
        Self { registry }
    }

    pub(super) async fn request_cancel(&self, session_id: &str) -> CancelTurnSnapshot {
        let runtimes = self.registry.shared_runtimes();
        let mut runtimes = runtimes.lock().await;
        let runtime = runtimes.entry(session_id.to_string()).or_insert_with(|| {
            let (tx, _rx) = broadcast::channel(1024);
            super::hub_support::build_session_runtime(tx)
        });
        if let Some(turn) = runtime.turn_state.active_turn_mut() {
            turn.cancel_requested = true;
        }
        let (current_turn_id, processor_tx) = runtime
            .turn_state
            .active_turn()
            .map(|turn| (Some(turn.turn_id.clone()), turn.processor_tx.clone()))
            .unwrap_or((None, None));
        CancelTurnSnapshot {
            running: runtime.is_running(),
            current_turn_id,
            tx: runtime.tx.clone(),
            processor_tx,
        }
    }

    pub(super) async fn claim_prompt(
        &self,
        session_id: &str,
    ) -> Result<Option<SessionProfile>, ConnectorError> {
        let runtimes = self.registry.shared_runtimes();
        let mut runtimes = runtimes.lock().await;
        let runtime = runtimes.entry(session_id.to_string()).or_insert_with(|| {
            let (tx, _rx) = broadcast::channel(1024);
            super::hub_support::build_session_runtime(tx)
        });
        if runtime.is_running() {
            return Err(ConnectorError::Runtime(
                "该会话有正在执行的 prompt，请等待完成或取消后再试".into(),
            ));
        }
        runtime.turn_state = TurnState::Claimed;
        Ok(runtime.session_profile.clone())
    }

    pub(super) async fn activate_turn(
        &self,
        session_id: &str,
        profile: SessionProfile,
        turn: TurnExecution,
    ) {
        self.registry
            .with_runtime_mut(session_id, |runtime| {
                if let Some(runtime) = runtime {
                    runtime.session_profile = Some(profile);
                    runtime.turn_state = TurnState::Active(Box::new(turn));
                }
            })
            .await;
    }

    pub(super) async fn clear_turn_and_hook(&self, session_id: &str) {
        self.registry
            .with_runtime_mut(session_id, |runtime| {
                if let Some(runtime) = runtime {
                    if let Some(turn) = runtime.turn_state.active_turn_mut() {
                        Self::abort_stream_adapter(turn);
                    }
                    runtime.turn_state = TurnState::Idle;
                    runtime.hook_runtime_delivery_binding = None;
                }
            })
            .await;
    }

    pub(super) async fn clear_active_turn(&self, session_id: &str) {
        self.registry
            .with_runtime_mut(session_id, |runtime| {
                if let Some(runtime) = runtime {
                    if let Some(turn) = runtime.turn_state.active_turn_mut() {
                        Self::abort_stream_adapter(turn);
                    }
                    runtime.turn_state = TurnState::Idle;
                }
            })
            .await;
    }

    pub(super) async fn cancel_interrupted_terminal(
        &self,
        session_id: &str,
        turn_id: &str,
    ) -> Option<(TurnTerminalKind, Option<String>)> {
        let cancel_matches = self
            .registry
            .with_runtime(session_id, |runtime| {
                runtime
                    .and_then(|runtime| runtime.turn_state.active_turn())
                    .is_some_and(|turn| turn.cancel_requested && turn.turn_id.as_str() == turn_id)
            })
            .await;

        cancel_matches.then(|| {
            (
                TurnTerminalKind::Interrupted,
                Some("执行已取消".to_string()),
            )
        })
    }

    pub(super) async fn register_processor_tx(
        &self,
        session_id: &str,
        processor_tx: mpsc::UnboundedSender<TurnEvent>,
    ) {
        self.registry
            .with_runtime_mut(session_id, |runtime| {
                if let Some(runtime) = runtime
                    && let Some(turn) = runtime.turn_state.active_turn_mut()
                {
                    turn.processor_tx = Some(processor_tx);
                }
            })
            .await;
    }

    pub(super) async fn register_stream_adapter_handle(
        &self,
        session_id: &str,
        abort_handle: AbortHandle,
    ) {
        self.registry
            .with_runtime_mut(session_id, |runtime| {
                if let Some(runtime) = runtime
                    && let Some(turn) = runtime.turn_state.active_turn_mut()
                {
                    turn.stream_adapter_abort = Some(abort_handle);
                }
            })
            .await;
    }

    fn abort_stream_adapter(turn: &mut TurnExecution) {
        if let Some(handle) = turn.stream_adapter_abort.take() {
            handle.abort();
        }
    }

    pub(super) async fn find_stalled_sessions(&self, stall_timeout_ms: u64) -> Vec<String> {
        self.registry
            .find_stalled_active_turns(stall_timeout_ms)
            .await
    }

    pub(super) fn interrupted_event(message: impl Into<String>) -> TurnEvent {
        TurnEvent::Terminal {
            kind: TurnTerminalKind::Interrupted,
            message: Some(message.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::Arc;

    use agentdash_spi::{AgentConfig, CapabilityState, ExecutionSessionFrame};
    use tokio::sync::Mutex;
    use uuid::Uuid;

    use super::*;

    fn test_supervisor() -> (SessionRuntimeRegistry, TurnSupervisor) {
        let registry = SessionRuntimeRegistry::new(Arc::new(Mutex::new(HashMap::new())));
        let supervisor = TurnSupervisor::new(registry.clone());
        (registry, supervisor)
    }

    fn test_turn(turn_id: &str) -> TurnExecution {
        TurnExecution::new(
            turn_id.to_string(),
            ExecutionSessionFrame {
                turn_id: turn_id.to_string(),
                working_directory: PathBuf::from("."),
                environment_variables: HashMap::new(),
                executor_config: AgentConfig::new("PI_AGENT"),
                mcp_servers: Vec::new(),
                vfs: None,
                backend_execution: None,
                identity: None,
            },
            CapabilityState::default(),
            Uuid::new_v4(),
            Uuid::new_v4(),
        )
    }

    #[tokio::test]
    async fn claim_prompt_rejects_concurrent_running_turn() {
        let (registry, supervisor) = test_supervisor();

        let cached_profile = supervisor
            .claim_prompt("session-1")
            .await
            .expect("first claim should succeed");
        assert!(cached_profile.is_none());
        assert!(registry.has_runtime_entry("session-1").await);
        assert!(!registry.has_active_turn("session-1").await);

        let error = match supervisor.claim_prompt("session-1").await {
            Ok(_) => panic!("second claim should be rejected"),
            Err(error) => error,
        };
        assert!(matches!(error, ConnectorError::Runtime(_)));

        supervisor.clear_active_turn("session-1").await;
        supervisor
            .claim_prompt("session-1")
            .await
            .expect("cleared session can be claimed again");
    }

    #[tokio::test]
    async fn cancel_terminal_matches_only_current_active_turn() {
        let (registry, supervisor) = test_supervisor();

        supervisor
            .claim_prompt("session-1")
            .await
            .expect("claim should succeed");
        supervisor
            .activate_turn(
                "session-1",
                SessionProfile {
                    capability_state: CapabilityState::default(),
                },
                test_turn("turn-1"),
            )
            .await;
        assert!(registry.has_active_turn("session-1").await);

        let snapshot = supervisor.request_cancel("session-1").await;
        assert!(snapshot.running);
        assert_eq!(snapshot.current_turn_id.as_deref(), Some("turn-1"));

        assert_eq!(
            supervisor
                .cancel_interrupted_terminal("session-1", "turn-1")
                .await,
            Some((
                TurnTerminalKind::Interrupted,
                Some("执行已取消".to_string())
            ))
        );
        assert_eq!(
            supervisor
                .cancel_interrupted_terminal("session-1", "turn-2")
                .await,
            None
        );

        supervisor.clear_active_turn("session-1").await;
        assert!(!registry.has_active_turn("session-1").await);
    }

    #[tokio::test]
    async fn register_stream_adapter_handle_sets_active_turn_abort_handle() {
        let (_registry, supervisor) = test_supervisor();
        supervisor
            .claim_prompt("session-1")
            .await
            .expect("claim should succeed");
        supervisor
            .activate_turn(
                "session-1",
                SessionProfile {
                    capability_state: CapabilityState::default(),
                },
                test_turn("turn-1"),
            )
            .await;
        let handle = tokio::spawn(async { futures::future::pending::<()>().await });

        supervisor
            .register_stream_adapter_handle("session-1", handle.abort_handle())
            .await;

        let has_abort_handle = supervisor
            .registry
            .with_runtime("session-1", |runtime| {
                runtime
                    .and_then(|runtime| runtime.turn_state.active_turn())
                    .is_some_and(|turn| turn.stream_adapter_abort.is_some())
            })
            .await;
        assert!(has_abort_handle);
        handle.abort();
        let _ = handle.await;
    }

    #[tokio::test]
    async fn clear_active_turn_aborts_stream_adapter_task() {
        let (_registry, supervisor) = test_supervisor();
        supervisor
            .claim_prompt("session-1")
            .await
            .expect("claim should succeed");
        supervisor
            .activate_turn(
                "session-1",
                SessionProfile {
                    capability_state: CapabilityState::default(),
                },
                test_turn("turn-1"),
            )
            .await;
        let handle = tokio::spawn(async { futures::future::pending::<()>().await });
        supervisor
            .register_stream_adapter_handle("session-1", handle.abort_handle())
            .await;

        supervisor.clear_active_turn("session-1").await;

        let result = handle.await;
        assert!(result.is_err_and(|error| error.is_cancelled()));
    }

    #[tokio::test]
    async fn clear_turn_and_hook_aborts_stream_adapter_task() {
        let (_registry, supervisor) = test_supervisor();
        supervisor
            .claim_prompt("session-1")
            .await
            .expect("claim should succeed");
        supervisor
            .activate_turn(
                "session-1",
                SessionProfile {
                    capability_state: CapabilityState::default(),
                },
                test_turn("turn-1"),
            )
            .await;
        let handle = tokio::spawn(async { futures::future::pending::<()>().await });
        supervisor
            .register_stream_adapter_handle("session-1", handle.abort_handle())
            .await;

        supervisor.clear_turn_and_hook("session-1").await;

        let result = handle.await;
        assert!(result.is_err_and(|error| error.is_cancelled()));
    }
}
