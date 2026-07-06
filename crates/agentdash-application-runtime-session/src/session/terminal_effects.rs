use agentdash_diagnostics::{DiagnosticErrorContext, Subsystem, diag, diag_error};
use std::io;
use std::sync::Arc;

use agentdash_agent_protocol::SourceInfo;
use agentdash_spi::hooks::{
    HookEffect, HookRuntimeAccess, HookTraceTrigger, HookTrigger, SharedHookRuntime,
};
use tokio::sync::RwLock;

use super::hub_support::TurnTerminalKind;
use super::persistence::SessionTerminalEffectStore;
use super::post_turn_handler::{
    DynPostTurnHandler, DynSessionTerminalCallback, DynTerminalHookEffectHandlerRegistry,
    SessionTerminalNotification,
};
pub use agentdash_spi::session_persistence::{
    NewTerminalEffectRecord, TerminalEffectRecord, TerminalEffectStatus, TerminalEffectType,
};

#[derive(Clone)]
enum TerminalEffectExecutor {
    HookEffects {
        handler: Option<DynPostTurnHandler>,
        effects: Vec<HookEffect>,
    },
    SessionTerminalCallback {
        callback: DynSessionTerminalCallback,
        notification: SessionTerminalNotification,
    },
    HookAutoResume,
    Unavailable {
        reason: String,
    },
}

#[derive(Clone)]
struct EnqueuedTerminalEffect {
    record: TerminalEffectRecord,
    executor: TerminalEffectExecutor,
}

#[derive(Default)]
pub(crate) struct EnqueuedTerminalEffects {
    effects: Vec<EnqueuedTerminalEffect>,
}

pub(crate) struct TerminalEffectDispatchInput {
    pub session_id: String,
    pub turn_id: String,
    pub terminal_event_seq: u64,
    pub terminal_kind: TurnTerminalKind,
    pub terminal_message: Option<String>,
    pub source: SourceInfo,
    pub hook_runtime: Option<SharedHookRuntime>,
    pub post_turn_handler: Option<DynPostTurnHandler>,
}

pub(crate) struct TerminalCallbackDispatchInput {
    pub session_id: String,
    pub turn_id: String,
    pub terminal_event_seq: u64,
    pub terminal_kind: TurnTerminalKind,
    pub terminal_message: Option<String>,
}

pub(crate) struct SessionTerminalEffectDispatcher {
    deps: TerminalEffectDeps,
}

#[derive(Clone)]
pub(crate) struct TerminalEffectDeps {
    pub terminal_effects: Arc<dyn SessionTerminalEffectStore>,
    pub hook_trigger: Arc<dyn TerminalHookTriggerPort>,
    pub terminal_callback: Arc<RwLock<Option<DynSessionTerminalCallback>>>,
    pub hook_effect_handler_registry: Arc<RwLock<Option<DynTerminalHookEffectHandlerRegistry>>>,
    pub auto_resume: Arc<dyn TerminalAutoResumePort>,
}

pub(crate) struct TerminalHookTriggerRequest<'a> {
    pub session_id: &'a str,
    pub turn_id: Option<&'a str>,
    pub trigger: HookTrigger,
    pub payload: Option<serde_json::Value>,
    pub refresh_reason: &'static str,
    pub source: SourceInfo,
}

#[async_trait::async_trait]
pub(crate) trait TerminalHookTriggerPort: Send + Sync {
    async fn emit_terminal_hook_trigger(
        &self,
        hook_runtime: &dyn HookRuntimeAccess,
        input: TerminalHookTriggerRequest<'_>,
    ) -> Vec<HookEffect>;
}

#[async_trait::async_trait]
pub(crate) trait TerminalAutoResumePort: Send + Sync {
    async fn request_hook_auto_resume(
        &self,
        request: TerminalAutoResumeRequest,
    ) -> Result<bool, String>;
}

#[derive(Debug, Clone)]
pub(crate) struct TerminalAutoResumeRequest {
    pub effect_id: uuid::Uuid,
    pub session_id: String,
    pub turn_id: String,
    pub terminal_event_seq: u64,
    pub payload: serde_json::Value,
}

impl SessionTerminalEffectDispatcher {
    pub fn new(deps: TerminalEffectDeps) -> Self {
        Self { deps }
    }

    pub async fn execute_terminal_callback_control_plane(
        &self,
        input: TerminalCallbackDispatchInput,
    ) {
        let terminal_state = input.terminal_kind.state_tag().to_string();
        let Some(callback) = self.deps.terminal_callback.read().await.clone() else {
            return;
        };
        let notification = SessionTerminalNotification {
            session_id: input.session_id.clone(),
            turn_id: input.turn_id.clone(),
            terminal_event_seq: input.terminal_event_seq,
            terminal_state: terminal_state.clone(),
            terminal_message: input.terminal_message.clone(),
        };

        if let Err(error) = callback.on_session_terminal(notification).await {
            let context =
                DiagnosticErrorContext::new("session.terminal_effects", "terminal_callback");
            diag_error!(
                Warn,
                Subsystem::AgentRun,
                context = &context,
                error = &std::io::Error::other(error.clone()),
                session_id = %input.session_id,
                turn_id = %input.turn_id,
                terminal_event_seq = input.terminal_event_seq,
                terminal_state = %terminal_state,
                "Session terminal control-plane callback 失败，写入 durable replay outbox"
            );
            self.persist_terminal_callback_retry(&input, terminal_state)
                .await;
        }
    }

    pub async fn enqueue_terminal_effects(
        &self,
        input: TerminalEffectDispatchInput,
    ) -> EnqueuedTerminalEffects {
        let mut batch = EnqueuedTerminalEffects::default();
        let terminal_state = input.terminal_kind.state_tag().to_string();

        if let Some(hook_runtime) = input.hook_runtime.as_ref() {
            let effects = self
                .deps
                .hook_trigger
                .emit_terminal_hook_trigger(
                    hook_runtime.as_ref(),
                    TerminalHookTriggerRequest {
                        session_id: &input.session_id,
                        turn_id: Some(&input.turn_id),
                        trigger: HookTrigger::SessionTerminal,
                        payload: Some(serde_json::json!({
                            "terminal_state": terminal_state,
                            "message": input.terminal_message.clone(),
                        })),
                        refresh_reason: "trigger:session_terminal",
                        source: input.source.clone(),
                    },
                )
                .await;

            if !effects.is_empty() {
                let supported_effect_kinds = input
                    .post_turn_handler
                    .as_ref()
                    .map(|handler| handler.supported_effect_kinds().to_vec())
                    .unwrap_or_default();
                let handler = input
                    .post_turn_handler
                    .as_ref()
                    .and_then(|handler| handler.durable_effect_handler());
                let payload = serde_json::json!({
                    "effects": effects,
                    "handler": handler,
                    "supported_effect_kinds": supported_effect_kinds,
                });
                self.enqueue(
                    &mut batch,
                    NewTerminalEffectRecord {
                        session_id: input.session_id.clone(),
                        turn_id: input.turn_id.clone(),
                        terminal_event_seq: input.terminal_event_seq,
                        effect_type: TerminalEffectType::HookEffects,
                        payload,
                    },
                    TerminalEffectExecutor::HookEffects {
                        handler: input.post_turn_handler.clone(),
                        effects,
                    },
                )
                .await;
            }
        }

        if should_auto_resume(input.terminal_kind, input.hook_runtime.as_ref()) {
            self.enqueue(
                &mut batch,
                NewTerminalEffectRecord {
                    session_id: input.session_id,
                    turn_id: input.turn_id,
                    terminal_event_seq: input.terminal_event_seq,
                    effect_type: TerminalEffectType::HookAutoResume,
                    payload: serde_json::json!({
                        "reason": "before_stop_continue",
                    }),
                },
                TerminalEffectExecutor::HookAutoResume,
            )
            .await;
        }

        batch
    }

    async fn persist_terminal_callback_retry(
        &self,
        input: &TerminalCallbackDispatchInput,
        terminal_state: String,
    ) {
        let effect = NewTerminalEffectRecord {
            session_id: input.session_id.clone(),
            turn_id: input.turn_id.clone(),
            terminal_event_seq: input.terminal_event_seq,
            effect_type: TerminalEffectType::SessionTerminalCallback,
            payload: serde_json::json!({
                "terminal_state": terminal_state,
                "terminal_message": input.terminal_message.clone(),
            }),
        };
        if let Err(error) = self
            .deps
            .terminal_effects
            .insert_terminal_effect(effect)
            .await
        {
            let context =
                DiagnosticErrorContext::new("session.terminal_effects", "persist_callback_retry");
            diag_error!(
                Error,
                Subsystem::AgentRun,
                context = &context,
                error = &error,
                session_id = %input.session_id,
                turn_id = %input.turn_id,
                terminal_event_seq = input.terminal_event_seq,
                "Session terminal callback retry outbox 写入失败"
            );
        }
    }

    async fn enqueue(
        &self,
        batch: &mut EnqueuedTerminalEffects,
        effect: NewTerminalEffectRecord,
        executor: TerminalEffectExecutor,
    ) {
        let session_id = effect.session_id.clone();
        let turn_id = effect.turn_id.clone();
        let terminal_event_seq = effect.terminal_event_seq;
        let effect_type = effect.effect_type.as_str().to_string();
        match self
            .deps
            .terminal_effects
            .insert_terminal_effect(effect)
            .await
        {
            Ok(record) => batch
                .effects
                .push(EnqueuedTerminalEffect { record, executor }),
            Err(error) => {
                let context =
                    DiagnosticErrorContext::new("session.terminal_effects", "enqueue_outbox");
                diag_error!(
                    Error,
                    Subsystem::AgentRun,
                    context = &context,
                    error = &error,
                    session_id = %session_id,
                    turn_id = %turn_id,
                    terminal_event_seq,
                    effect_type = %effect_type,
                    "Terminal effect outbox 写入失败，终态事实已保留"
                );
            }
        }
    }

    pub async fn execute_enqueued(&self, batch: EnqueuedTerminalEffects) {
        for item in batch.effects {
            if let Err(error) = self.execute_one(item.clone()).await {
                let context =
                    DiagnosticErrorContext::new("session.terminal_effects", "execute_enqueued");
                diag_error!(
                    Warn,
                    Subsystem::AgentRun,
                    context = &context,
                    error = &error,
                    session_id = %item.record.session_id,
                    turn_id = %item.record.turn_id,
                    effect_id = %item.record.id,
                    effect_type = item.record.effect_type.as_str(),
                    terminal_event_seq = item.record.terminal_event_seq,
                    attempt = item.record.attempt_count.saturating_add(1),
                    retry_count = item.record.attempt_count,
                    "Terminal effect 执行失败"
                );
            }
        }
    }

    pub async fn replay_durable_outbox(&self, limit: u32) -> io::Result<usize> {
        let records = self
            .deps
            .terminal_effects
            .list_terminal_effects_by_status(
                &[
                    TerminalEffectStatus::Pending,
                    TerminalEffectStatus::Running,
                    TerminalEffectStatus::Failed,
                ],
                limit,
            )
            .await?;
        let mut attempted = 0;
        for record in records {
            let executor = self.replay_executor_for(&record).await;
            let session_id = record.session_id.clone();
            let turn_id = record.turn_id.clone();
            let effect_id = record.id;
            let effect_type = record.effect_type.as_str().to_string();
            let terminal_event_seq = record.terminal_event_seq;
            let attempt = record.attempt_count.saturating_add(1);
            let retry_count = record.attempt_count;
            attempted += 1;
            if let Err(error) = self
                .execute_one(EnqueuedTerminalEffect { record, executor })
                .await
            {
                let context = DiagnosticErrorContext::new(
                    "session.terminal_effects",
                    "replay_durable_outbox",
                );
                diag_error!(
                    Warn,
                    Subsystem::AgentRun,
                    context = &context,
                    error = &error,
                    session_id = %session_id,
                    turn_id = %turn_id,
                    effect_id = %effect_id,
                    effect_type = %effect_type,
                    terminal_event_seq,
                    attempt,
                    retry_count,
                    "Terminal effect durable replay 失败"
                );
            }
        }
        Ok(attempted)
    }

    async fn replay_executor_for(&self, record: &TerminalEffectRecord) -> TerminalEffectExecutor {
        match record.effect_type {
            TerminalEffectType::HookAutoResume => TerminalEffectExecutor::HookAutoResume,
            TerminalEffectType::SessionTerminalCallback => {
                match self.deps.terminal_callback.read().await.clone() {
                    Some(callback) => TerminalEffectExecutor::SessionTerminalCallback {
                        callback,
                        notification: SessionTerminalNotification {
                            session_id: record.session_id.clone(),
                            turn_id: record.turn_id.clone(),
                            terminal_event_seq: record.terminal_event_seq,
                            terminal_state: record
                                .payload
                                .get("terminal_state")
                                .and_then(|value| value.as_str())
                                .unwrap_or("unknown")
                                .to_string(),
                            terminal_message: record
                                .payload
                                .get("terminal_message")
                                .and_then(|value| value.as_str())
                                .map(ToOwned::to_owned),
                        },
                    },
                    None => TerminalEffectExecutor::Unavailable {
                        reason: "terminal callback 未注入，无法 replay session_terminal_callback"
                            .to_string(),
                    },
                }
            }
            TerminalEffectType::HookEffects => self.replay_hook_effect_executor(record).await,
        }
    }

    async fn replay_hook_effect_executor(
        &self,
        record: &TerminalEffectRecord,
    ) -> TerminalEffectExecutor {
        let effects = match record.payload.get("effects").cloned() {
            Some(value) => match serde_json::from_value::<Vec<HookEffect>>(value) {
                Ok(effects) => effects,
                Err(error) => {
                    return TerminalEffectExecutor::Unavailable {
                        reason: format!("hook_effects payload 无法反序列化: {error}"),
                    };
                }
            },
            None => {
                return TerminalEffectExecutor::Unavailable {
                    reason: "hook_effects payload 缺少 effects".to_string(),
                };
            }
        };
        let Some(registry) = self.deps.hook_effect_handler_registry.read().await.clone() else {
            return TerminalEffectExecutor::Unavailable {
                reason: "hook_effects 缺少 durable handler registry".to_string(),
            };
        };
        match registry
            .handler_for(&record.session_id, &record.payload)
            .await
        {
            Ok(Some(handler)) => TerminalEffectExecutor::HookEffects {
                handler: Some(handler),
                effects,
            },
            Ok(None) => TerminalEffectExecutor::Unavailable {
                reason: "hook_effects durable handler registry 无匹配 handler".to_string(),
            },
            Err(error) => TerminalEffectExecutor::Unavailable { reason: error },
        }
    }

    async fn execute_one(&self, item: EnqueuedTerminalEffect) -> io::Result<()> {
        let next_attempt_count = item.record.attempt_count.saturating_add(1);
        self.deps
            .terminal_effects
            .mark_terminal_effect_running(item.record.id)
            .await?;
        let result = match &item.executor {
            TerminalEffectExecutor::HookEffects { handler, effects } => {
                self.execute_hook_effects(&item.record, handler.as_ref(), effects)
                    .await
            }
            TerminalEffectExecutor::SessionTerminalCallback {
                callback,
                notification,
            } => callback.on_session_terminal(notification.clone()).await,
            TerminalEffectExecutor::HookAutoResume => self
                .deps
                .auto_resume
                .request_hook_auto_resume(TerminalAutoResumeRequest {
                    effect_id: item.record.id,
                    session_id: item.record.session_id.clone(),
                    turn_id: item.record.turn_id.clone(),
                    terminal_event_seq: item.record.terminal_event_seq,
                    payload: item.record.payload.clone(),
                })
                .await
                .map(|_| ()),
            TerminalEffectExecutor::Unavailable { reason } => Err(reason.clone()),
        };

        match result {
            Ok(()) => self
                .deps
                .terminal_effects
                .mark_terminal_effect_succeeded(item.record.id)
                .await
                .map_err(Into::into),
            Err(error) => {
                if next_attempt_count >= MAX_TERMINAL_EFFECT_ATTEMPTS {
                    self.deps
                        .terminal_effects
                        .mark_terminal_effect_dead_letter(item.record.id, error.clone())
                        .await?;
                } else {
                    self.deps
                        .terminal_effects
                        .mark_terminal_effect_failed(item.record.id, error.clone())
                        .await?;
                }
                Err(io::Error::other(error))
            }
        }
    }

    async fn execute_hook_effects(
        &self,
        record: &TerminalEffectRecord,
        handler: Option<&Arc<dyn super::post_turn_handler::PostTurnHandler>>,
        effects: &[HookEffect],
    ) -> Result<(), String> {
        let Some(handler) = handler else {
            return Err("SessionTerminal hook effects 缺少 post-turn handler".to_string());
        };

        let supported = handler.supported_effect_kinds();
        if !supported.is_empty() {
            for effect in effects {
                if !supported.contains(&effect.kind.as_str()) {
                    diag!(Warn, Subsystem::AgentRun,

                        operation = "session.terminal_effects",
                        stage = "validate_hook_effect_kind",
                        session_id = %record.session_id,
                        turn_id = %record.turn_id,
                        effect_id = %record.id,
                        effect_kind = %effect.kind,
                        supported_count = supported.len(),
                        supported = ?supported,
                        "Hook 产出了 handler 未声明支持的 effect kind"
                    );
                }
            }
        }
        handler
            .execute_effects(&record.session_id, &record.turn_id, effects)
            .await;
        Ok(())
    }
}

const MAX_TERMINAL_EFFECT_ATTEMPTS: u32 = 3;

fn should_auto_resume(
    terminal_kind: TurnTerminalKind,
    hook_runtime: Option<&SharedHookRuntime>,
) -> bool {
    matches!(terminal_kind, TurnTerminalKind::Completed)
        && hook_runtime.as_ref().is_some_and(|hook_runtime| {
            let trace = hook_runtime.trace();
            trace
                .iter()
                .rev()
                .find(|entry| matches!(entry.trigger, HookTraceTrigger::BeforeStop))
                .is_some_and(|entry| entry.decision == "continue")
        })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use agentdash_agent_protocol::BackboneEnvelope;
    use agentdash_spi::{
        AgentConnector, ConnectorCapabilities, ConnectorError, ConnectorType, ExecutionContext,
        ExecutionStream, PromptPayload,
    };
    use futures::stream;
    use tokio::sync::Mutex;

    use crate::session::hub::SessionRuntimeInner;
    use crate::session::post_turn_handler::TerminalHookEffectHandlerRegistry;
    use crate::session::types::{ExecutionStatus, SessionMeta};
    use crate::session::{MemoryRuntimeTraceStore, SessionMetaStore, SessionStoreSet};

    #[test]
    fn terminal_effect_status_round_trips_wire_values() {
        assert_eq!(
            TerminalEffectStatus::try_from("pending"),
            Ok(TerminalEffectStatus::Pending)
        );
        assert_eq!(TerminalEffectStatus::Failed.as_str(), "failed");
        assert_eq!(TerminalEffectStatus::DeadLetter.as_str(), "dead_letter");
        assert!(TerminalEffectStatus::try_from("unknown").is_err());
    }

    #[test]
    fn terminal_effect_type_round_trips_wire_values() {
        assert_eq!(
            TerminalEffectType::try_from("hook_auto_resume"),
            Ok(TerminalEffectType::HookAutoResume)
        );
        assert_eq!(
            TerminalEffectType::SessionTerminalCallback.as_str(),
            "session_terminal_callback"
        );
        assert!(TerminalEffectType::try_from("unknown").is_err());
    }

    #[tokio::test]
    async fn replay_hook_effects_uses_durable_handler_registry() {
        let persistence = Arc::new(MemoryRuntimeTraceStore::default());
        persistence
            .create_session(&SessionMeta {
                id: "sess-hook-replay".to_string(),
                created_at: 1,
                updated_at: 1,
                last_event_seq: 0,
                last_delivery_status: ExecutionStatus::Idle,
                last_turn_id: None,
                last_terminal_message: None,
                executor_session_id: None,
            })
            .await
            .expect("session should be created");

        let hub = SessionRuntimeInner::new_with_hooks_and_stores(
            Arc::new(NoopConnector),
            None,
            SessionStoreSet::from_runtime_trace_test_store(persistence.clone()),
        );
        let executed = Arc::new(Mutex::new(Vec::<String>::new()));
        hub.set_hook_effect_handler_registry(Arc::new(RecordingHookEffectRegistry {
            executed: executed.clone(),
        }))
        .await;
        hub.stores
            .terminal_effects
            .insert_terminal_effect(NewTerminalEffectRecord {
                session_id: "sess-hook-replay".to_string(),
                turn_id: "turn-1".to_string(),
                terminal_event_seq: 1,
                effect_type: TerminalEffectType::HookEffects,
                payload: serde_json::json!({
                    "effects": [
                        {
                            "kind": "record:note",
                            "payload": { "message": "done" }
                        }
                    ],
                    "handler": {
                        "kind": "recording"
                    },
                    "supported_effect_kinds": ["record:note"]
                }),
            })
            .await
            .expect("terminal effect should be inserted");

        let attempted = hub
            .effects_service()
            .replay_terminal_effect_outbox(10)
            .await
            .expect("replay should not fail at store level");

        assert_eq!(attempted, 1);
        assert_eq!(executed.lock().await.as_slice(), ["record:note"]);
        let succeeded = persistence
            .list_terminal_effects_by_status(&[TerminalEffectStatus::Succeeded], 10)
            .await
            .expect("succeeded effects should be queryable");
        assert_eq!(succeeded.len(), 1);
    }

    #[tokio::test]
    async fn hook_auto_resume_failure_keeps_terminal_effect_replayable() {
        let persistence = Arc::new(MemoryRuntimeTraceStore::default());
        persistence
            .create_session(&SessionMeta {
                id: "sess-auto-resume-failure".to_string(),
                created_at: 1,
                updated_at: 1,
                last_event_seq: 0,
                last_delivery_status: ExecutionStatus::Idle,
                last_turn_id: None,
                last_terminal_message: None,
                executor_session_id: None,
            })
            .await
            .expect("session should be created");
        let attempts = Arc::new(AtomicUsize::new(0));
        let dispatcher = SessionTerminalEffectDispatcher::new(TerminalEffectDeps {
            terminal_effects: persistence.clone(),
            hook_trigger: Arc::new(NoopTerminalHookTrigger),
            terminal_callback: Arc::new(RwLock::new(None)),
            hook_effect_handler_registry: Arc::new(RwLock::new(None)),
            auto_resume: Arc::new(FailingAutoResume {
                attempts: attempts.clone(),
            }),
        });
        let record = persistence
            .insert_terminal_effect(NewTerminalEffectRecord {
                session_id: "sess-auto-resume-failure".to_string(),
                turn_id: "turn-1".to_string(),
                terminal_event_seq: 7,
                effect_type: TerminalEffectType::HookAutoResume,
                payload: serde_json::json!({
                    "reason": "before_stop_continue",
                }),
            })
            .await
            .expect("terminal effect should be inserted");

        assert!(
            dispatcher
                .execute_one(EnqueuedTerminalEffect {
                    record,
                    executor: TerminalEffectExecutor::HookAutoResume,
                })
                .await
                .is_err()
        );

        let failed = persistence
            .list_terminal_effects_by_status(&[TerminalEffectStatus::Failed], 10)
            .await
            .expect("failed effects should be queryable");
        assert_eq!(failed.len(), 1);
        assert_eq!(attempts.load(Ordering::SeqCst), 1);

        let replayed = dispatcher
            .replay_durable_outbox(10)
            .await
            .expect("replay should query durable outbox");
        assert_eq!(replayed, 1);
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn terminal_callback_failure_keeps_terminal_effect_replayable() {
        let persistence = Arc::new(MemoryRuntimeTraceStore::default());
        persistence
            .create_session(&SessionMeta {
                id: "sess-callback-failure".to_string(),
                created_at: 1,
                updated_at: 1,
                last_event_seq: 0,
                last_delivery_status: ExecutionStatus::Idle,
                last_turn_id: None,
                last_terminal_message: None,
                executor_session_id: None,
            })
            .await
            .expect("session should be created");
        let attempts = Arc::new(AtomicUsize::new(0));
        let callback = Arc::new(FailingTerminalCallback {
            attempts: attempts.clone(),
        });
        let dispatcher = SessionTerminalEffectDispatcher::new(TerminalEffectDeps {
            terminal_effects: persistence.clone(),
            hook_trigger: Arc::new(NoopTerminalHookTrigger),
            terminal_callback: Arc::new(RwLock::new(Some(callback))),
            hook_effect_handler_registry: Arc::new(RwLock::new(None)),
            auto_resume: Arc::new(NoopAutoResume),
        });
        let record = persistence
            .insert_terminal_effect(NewTerminalEffectRecord {
                session_id: "sess-callback-failure".to_string(),
                turn_id: "turn-1".to_string(),
                terminal_event_seq: 8,
                effect_type: TerminalEffectType::SessionTerminalCallback,
                payload: serde_json::json!({
                    "terminal_state": "failed",
                    "terminal_message": "provider failed",
                }),
            })
            .await
            .expect("terminal effect should be inserted");

        let executor = dispatcher.replay_executor_for(&record).await;
        assert!(
            dispatcher
                .execute_one(EnqueuedTerminalEffect { record, executor })
                .await
                .is_err()
        );

        let failed = persistence
            .list_terminal_effects_by_status(&[TerminalEffectStatus::Failed], 10)
            .await
            .expect("failed effects should be queryable");
        assert_eq!(failed.len(), 1);
        assert_eq!(attempts.load(Ordering::SeqCst), 1);

        let replayed = dispatcher
            .replay_durable_outbox(10)
            .await
            .expect("replay should query durable outbox");
        assert_eq!(replayed, 1);
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn terminal_callback_control_plane_success_does_not_create_replay_effect() {
        let persistence = Arc::new(MemoryRuntimeTraceStore::default());
        persistence
            .create_session(&SessionMeta {
                id: "sess-callback-success".to_string(),
                created_at: 1,
                updated_at: 1,
                last_event_seq: 0,
                last_delivery_status: ExecutionStatus::Idle,
                last_turn_id: None,
                last_terminal_message: None,
                executor_session_id: None,
            })
            .await
            .expect("session should be created");
        let attempts = Arc::new(AtomicUsize::new(0));
        let dispatcher = SessionTerminalEffectDispatcher::new(TerminalEffectDeps {
            terminal_effects: persistence.clone(),
            hook_trigger: Arc::new(NoopTerminalHookTrigger),
            terminal_callback: Arc::new(RwLock::new(Some(Arc::new(CountingTerminalCallback {
                attempts: attempts.clone(),
            })))),
            hook_effect_handler_registry: Arc::new(RwLock::new(None)),
            auto_resume: Arc::new(NoopAutoResume),
        });

        dispatcher
            .execute_terminal_callback_control_plane(TerminalCallbackDispatchInput {
                session_id: "sess-callback-success".to_string(),
                turn_id: "turn-1".to_string(),
                terminal_event_seq: 8,
                terminal_kind: TurnTerminalKind::Failed,
                terminal_message: Some("provider failed".to_string()),
            })
            .await;

        assert_eq!(attempts.load(Ordering::SeqCst), 1);
        let pending = persistence
            .list_terminal_effects_by_status(
                &[
                    TerminalEffectStatus::Pending,
                    TerminalEffectStatus::Failed,
                    TerminalEffectStatus::Succeeded,
                ],
                10,
            )
            .await
            .expect("effects should be queryable");
        assert!(pending.is_empty());
    }

    #[tokio::test]
    async fn terminal_callback_control_plane_failure_creates_replay_effect() {
        let persistence = Arc::new(MemoryRuntimeTraceStore::default());
        persistence
            .create_session(&SessionMeta {
                id: "sess-callback-direct-failure".to_string(),
                created_at: 1,
                updated_at: 1,
                last_event_seq: 0,
                last_delivery_status: ExecutionStatus::Idle,
                last_turn_id: None,
                last_terminal_message: None,
                executor_session_id: None,
            })
            .await
            .expect("session should be created");
        let attempts = Arc::new(AtomicUsize::new(0));
        let callback = Arc::new(FailingTerminalCallback {
            attempts: attempts.clone(),
        });
        let dispatcher = SessionTerminalEffectDispatcher::new(TerminalEffectDeps {
            terminal_effects: persistence.clone(),
            hook_trigger: Arc::new(NoopTerminalHookTrigger),
            terminal_callback: Arc::new(RwLock::new(Some(callback))),
            hook_effect_handler_registry: Arc::new(RwLock::new(None)),
            auto_resume: Arc::new(NoopAutoResume),
        });

        dispatcher
            .execute_terminal_callback_control_plane(TerminalCallbackDispatchInput {
                session_id: "sess-callback-direct-failure".to_string(),
                turn_id: "turn-1".to_string(),
                terminal_event_seq: 9,
                terminal_kind: TurnTerminalKind::Failed,
                terminal_message: Some("provider failed".to_string()),
            })
            .await;

        assert_eq!(attempts.load(Ordering::SeqCst), 1);
        let pending = persistence
            .list_terminal_effects_by_status(&[TerminalEffectStatus::Pending], 10)
            .await
            .expect("pending effects should be queryable");
        assert_eq!(pending.len(), 1);
        assert_eq!(
            pending[0].effect_type,
            TerminalEffectType::SessionTerminalCallback
        );

        let replayed = dispatcher
            .replay_durable_outbox(10)
            .await
            .expect("replay should query durable outbox");
        assert_eq!(replayed, 1);
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
    }

    struct NoopTerminalHookTrigger;

    #[async_trait::async_trait]
    impl TerminalHookTriggerPort for NoopTerminalHookTrigger {
        async fn emit_terminal_hook_trigger(
            &self,
            _hook_runtime: &dyn HookRuntimeAccess,
            _input: TerminalHookTriggerRequest<'_>,
        ) -> Vec<HookEffect> {
            Vec::new()
        }
    }

    struct FailingAutoResume {
        attempts: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl TerminalAutoResumePort for FailingAutoResume {
        async fn request_hook_auto_resume(
            &self,
            _request: TerminalAutoResumeRequest,
        ) -> Result<bool, String> {
            self.attempts.fetch_add(1, Ordering::SeqCst);
            Err("mailbox route failed".to_string())
        }
    }

    struct NoopAutoResume;

    #[async_trait::async_trait]
    impl TerminalAutoResumePort for NoopAutoResume {
        async fn request_hook_auto_resume(
            &self,
            _request: TerminalAutoResumeRequest,
        ) -> Result<bool, String> {
            Ok(false)
        }
    }

    struct FailingTerminalCallback {
        attempts: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl super::super::post_turn_handler::SessionTerminalCallback for FailingTerminalCallback {
        async fn on_session_terminal(
            &self,
            _notification: SessionTerminalNotification,
        ) -> Result<(), String> {
            self.attempts.fetch_add(1, Ordering::SeqCst);
            Err("control plane write failed".to_string())
        }
    }

    struct CountingTerminalCallback {
        attempts: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl super::super::post_turn_handler::SessionTerminalCallback for CountingTerminalCallback {
        async fn on_session_terminal(
            &self,
            _notification: SessionTerminalNotification,
        ) -> Result<(), String> {
            self.attempts.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    struct RecordingHookEffectRegistry {
        executed: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait::async_trait]
    impl TerminalHookEffectHandlerRegistry for RecordingHookEffectRegistry {
        async fn handler_for(
            &self,
            _session_id: &str,
            payload: &serde_json::Value,
        ) -> Result<Option<DynPostTurnHandler>, String> {
            let Some("recording") = payload
                .get("handler")
                .and_then(|handler| handler.get("kind"))
                .and_then(|kind| kind.as_str())
            else {
                return Ok(None);
            };
            Ok(Some(Arc::new(RecordingPostTurnHandler {
                executed: self.executed.clone(),
            }) as DynPostTurnHandler))
        }
    }

    struct RecordingPostTurnHandler {
        executed: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait::async_trait]
    impl super::super::post_turn_handler::PostTurnHandler for RecordingPostTurnHandler {
        async fn on_event(&self, _session_id: &str, _envelope: &BackboneEnvelope) {}

        async fn execute_effects(&self, _session_id: &str, _turn_id: &str, effects: &[HookEffect]) {
            self.executed
                .lock()
                .await
                .extend(effects.iter().map(|effect| effect.kind.clone()));
        }

        fn supported_effect_kinds(&self) -> &[&str] {
            &["record:note"]
        }
    }

    struct NoopConnector;

    #[async_trait::async_trait]
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
