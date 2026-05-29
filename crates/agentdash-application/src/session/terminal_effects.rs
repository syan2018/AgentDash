use std::io;
use std::sync::Arc;

use agentdash_agent_protocol::SourceInfo;
use agentdash_spi::hooks::{
    HookEffect, HookSessionRuntimeAccess, HookTraceTrigger, HookTrigger, SharedHookSessionRuntime,
};
use tokio::sync::RwLock;

use super::hub_support::TurnTerminalKind;
use super::persistence::SessionTerminalEffectStore;
use super::post_turn_handler::{
    DynPostTurnHandler, DynSessionTerminalCallback, DynTerminalHookEffectHandlerRegistry,
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
        terminal_state: String,
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
    pub hook_session: Option<SharedHookSessionRuntime>,
    pub post_turn_handler: Option<DynPostTurnHandler>,
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
        hook_session: &dyn HookSessionRuntimeAccess,
        input: TerminalHookTriggerRequest<'_>,
    ) -> Vec<HookEffect>;
}

#[async_trait::async_trait]
pub(crate) trait TerminalAutoResumePort: Send + Sync {
    async fn request_hook_auto_resume(&self, session_id: String) -> bool;
}

impl SessionTerminalEffectDispatcher {
    pub fn new(deps: TerminalEffectDeps) -> Self {
        Self { deps }
    }

    pub async fn enqueue_terminal_effects(
        &self,
        input: TerminalEffectDispatchInput,
    ) -> EnqueuedTerminalEffects {
        let mut batch = EnqueuedTerminalEffects::default();
        let terminal_state = input.terminal_kind.state_tag().to_string();

        if let Some(hook_session) = input.hook_session.as_ref() {
            let effects = self
                .deps
                .hook_trigger
                .emit_terminal_hook_trigger(
                    hook_session.as_ref(),
                    TerminalHookTriggerRequest {
                        session_id: &input.session_id,
                        turn_id: Some(&input.turn_id),
                        trigger: HookTrigger::SessionTerminal,
                        payload: Some(serde_json::json!({
                            "terminal_state": terminal_state,
                            "message": input.terminal_message,
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

        if let Some(callback) = self.deps.terminal_callback.read().await.clone() {
            self.enqueue(
                &mut batch,
                NewTerminalEffectRecord {
                    session_id: input.session_id.clone(),
                    turn_id: input.turn_id.clone(),
                    terminal_event_seq: input.terminal_event_seq,
                    effect_type: TerminalEffectType::SessionTerminalCallback,
                    payload: serde_json::json!({
                        "terminal_state": terminal_state,
                    }),
                },
                TerminalEffectExecutor::SessionTerminalCallback {
                    callback,
                    terminal_state: terminal_state.clone(),
                },
            )
            .await;
        }

        if should_auto_resume(input.terminal_kind, input.hook_session.as_ref()) {
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

    async fn enqueue(
        &self,
        batch: &mut EnqueuedTerminalEffects,
        effect: NewTerminalEffectRecord,
        executor: TerminalEffectExecutor,
    ) {
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
                tracing::error!(
                    error = %error,
                    "Terminal effect outbox 写入失败，终态事实已保留"
                );
            }
        }
    }

    pub async fn execute_enqueued(&self, batch: EnqueuedTerminalEffects) {
        for item in batch.effects {
            if let Err(error) = self.execute_one(item.clone()).await {
                tracing::warn!(
                    effect_id = %item.record.id,
                    effect_type = item.record.effect_type.as_str(),
                    error = %error,
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
            attempted += 1;
            if let Err(error) = self
                .execute_one(EnqueuedTerminalEffect { record, executor })
                .await
            {
                tracing::warn!(
                    error = %error,
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
                        terminal_state: record
                            .payload
                            .get("terminal_state")
                            .and_then(|value| value.as_str())
                            .unwrap_or("unknown")
                            .to_string(),
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
                terminal_state,
            } => {
                callback
                    .on_session_terminal(&item.record.session_id, terminal_state)
                    .await;
                Ok(())
            }
            TerminalEffectExecutor::HookAutoResume => {
                let _ = self
                    .deps
                    .auto_resume
                    .request_hook_auto_resume(item.record.session_id.clone())
                    .await;
                Ok(())
            }
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
                    tracing::warn!(
                        session_id = %record.session_id,
                        effect_kind = %effect.kind,
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
    hook_session: Option<&SharedHookSessionRuntime>,
) -> bool {
    matches!(terminal_kind, TurnTerminalKind::Completed)
        && hook_session.as_ref().is_some_and(|hook_session| {
            let trace = hook_session.trace();
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
    use crate::session::types::{ExecutionStatus, SessionBootstrapState, SessionMeta, TitleSource};
    use crate::session::{MemorySessionPersistence, SessionMetaStore};

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
        let persistence = Arc::new(MemorySessionPersistence::default());
        persistence
            .create_session(&SessionMeta {
                id: "sess-hook-replay".to_string(),
                title: "hook replay".to_string(),
                title_source: TitleSource::Auto,
                created_at: 1,
                updated_at: 1,
                last_event_seq: 0,
                last_execution_status: ExecutionStatus::Idle,
                last_turn_id: None,
                last_terminal_message: None,
                executor_config: None,
                executor_session_id: None,
                companion_context: None,
                tab_layout: None,
                visible_canvas_mount_ids: Vec::new(),
                bootstrap_state: SessionBootstrapState::Plain,
            })
            .await
            .expect("session should be created");

        let hub = SessionRuntimeInner::new_with_hooks_and_persistence(
            Arc::new(NoopConnector),
            None,
            persistence.clone(),
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
                            "kind": "task:set_status",
                            "payload": { "status": "done" }
                        }
                    ],
                    "handler": {
                        "kind": "recording"
                    },
                    "supported_effect_kinds": ["task:set_status"]
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
        assert_eq!(executed.lock().await.as_slice(), ["task:set_status"]);
        let succeeded = persistence
            .list_terminal_effects_by_status(&[TerminalEffectStatus::Succeeded], 10)
            .await
            .expect("succeeded effects should be queryable");
        assert_eq!(succeeded.len(), 1);
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
            &["task:set_status"]
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
