use std::io;
use std::sync::Arc;

use agentdash_agent_protocol::SourceInfo;
use agentdash_spi::hooks::{HookEffect, HookTraceTrigger, HookTrigger, SharedHookSessionRuntime};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::hub::{HookTriggerInput, SessionHub};
use super::hub_support::TurnTerminalKind;
use super::post_turn_handler::{DynPostTurnHandler, DynSessionTerminalCallback};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalEffectType {
    HookEffects,
    SessionTerminalCallback,
    HookAutoResume,
}

impl TerminalEffectType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::HookEffects => "hook_effects",
            Self::SessionTerminalCallback => "session_terminal_callback",
            Self::HookAutoResume => "hook_auto_resume",
        }
    }
}

impl TryFrom<&str> for TerminalEffectType {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "hook_effects" => Ok(Self::HookEffects),
            "session_terminal_callback" => Ok(Self::SessionTerminalCallback),
            "hook_auto_resume" => Ok(Self::HookAutoResume),
            other => Err(format!("unknown terminal effect type: {other}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalEffectStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
    DeadLetter,
}

impl TerminalEffectStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::DeadLetter => "dead_letter",
        }
    }
}

impl TryFrom<&str> for TerminalEffectStatus {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "pending" => Ok(Self::Pending),
            "running" => Ok(Self::Running),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            "dead_letter" => Ok(Self::DeadLetter),
            other => Err(format!("unknown terminal effect status: {other}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TerminalEffectRecord {
    pub id: Uuid,
    pub session_id: String,
    pub turn_id: String,
    pub terminal_event_seq: u64,
    pub effect_type: TerminalEffectType,
    pub payload: serde_json::Value,
    pub status: TerminalEffectStatus,
    pub attempt_count: u32,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NewTerminalEffectRecord {
    pub session_id: String,
    pub turn_id: String,
    pub terminal_event_seq: u64,
    pub effect_type: TerminalEffectType,
    pub payload: serde_json::Value,
}

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
pub(super) struct EnqueuedTerminalEffects {
    effects: Vec<EnqueuedTerminalEffect>,
}

pub(super) struct TerminalEffectDispatchInput {
    pub session_id: String,
    pub turn_id: String,
    pub terminal_event_seq: u64,
    pub terminal_kind: TurnTerminalKind,
    pub terminal_message: Option<String>,
    pub source: SourceInfo,
    pub hook_session: Option<SharedHookSessionRuntime>,
    pub post_turn_handler: Option<DynPostTurnHandler>,
}

pub(super) struct SessionTerminalEffectDispatcher<'a> {
    hub: &'a SessionHub,
}

impl<'a> SessionTerminalEffectDispatcher<'a> {
    pub fn new(hub: &'a SessionHub) -> Self {
        Self { hub }
    }

    pub async fn enqueue_terminal_effects(
        &self,
        input: TerminalEffectDispatchInput,
    ) -> EnqueuedTerminalEffects {
        let mut batch = EnqueuedTerminalEffects::default();
        let terminal_state = input.terminal_kind.state_tag().to_string();

        if let Some(hook_session) = input.hook_session.as_ref() {
            let effects = self
                .hub
                .emit_session_hook_trigger(
                    hook_session.as_ref(),
                    &HookTriggerInput {
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
                .await
                .effects;

            if !effects.is_empty() {
                let supported_effect_kinds = input
                    .post_turn_handler
                    .as_ref()
                    .map(|handler| handler.supported_effect_kinds().to_vec())
                    .unwrap_or_default();
                let payload = serde_json::json!({
                    "effects": effects,
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

        if let Some(callback) = self.hub.terminal_callback.read().await.clone() {
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
            .hub
            .stores
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
            .hub
            .stores
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
                match self.hub.terminal_callback.read().await.clone() {
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
            TerminalEffectType::HookEffects => TerminalEffectExecutor::Unavailable {
                reason: "hook_effects 依赖原 turn 的 post-turn handler，当前尚未具备 durable handler registry"
                    .to_string(),
            },
        }
    }

    async fn execute_one(&self, item: EnqueuedTerminalEffect) -> io::Result<()> {
        let next_attempt_count = item.record.attempt_count.saturating_add(1);
        self.hub
            .stores
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
                    .hub
                    .request_hook_auto_resume(item.record.session_id.clone())
                    .await;
                Ok(())
            }
            TerminalEffectExecutor::Unavailable { reason } => Err(reason.clone()),
        };

        match result {
            Ok(()) => {
                self.hub
                    .stores
                    .terminal_effects
                    .mark_terminal_effect_succeeded(item.record.id)
                    .await
            }
            Err(error) => {
                if next_attempt_count >= MAX_TERMINAL_EFFECT_ATTEMPTS {
                    self.hub
                        .stores
                        .terminal_effects
                        .mark_terminal_effect_dead_letter(item.record.id, error.clone())
                        .await?;
                } else {
                    self.hub
                        .stores
                        .terminal_effects
                        .mark_terminal_effect_failed(item.record.id, error.clone())
                        .await?;
                }
                Err(io::Error::new(io::ErrorKind::Other, error))
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

impl SessionHub {
    pub async fn replay_terminal_effect_outbox(&self, limit: u32) -> io::Result<usize> {
        SessionTerminalEffectDispatcher::new(self)
            .replay_durable_outbox(limit)
            .await
    }
}

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
    use super::*;

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
}
