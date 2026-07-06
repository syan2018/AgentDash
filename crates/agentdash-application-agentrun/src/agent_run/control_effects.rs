use std::sync::Arc;

use agentdash_agent_protocol::text_user_input_blocks;
use agentdash_application_ports::agent_run_control_effect::{
    AgentRunControlEffectPort, AgentRunControlEffectReplayPort,
    AgentRunLifecycleTerminalConvergencePort, AgentRunPostTurnHandler,
    AgentRunTerminalControlInput, AgentRunTerminalHookEffects,
    AgentRunWaitProducerTerminalConvergencePort, AgentRunWaitProducerTerminalEvent,
    DynAgentRunHookEffectHandlerRegistry,
};
use agentdash_application_ports::frame_launch_envelope::TerminalHookEffectBinding;
use agentdash_diagnostics::{DiagnosticErrorContext, Subsystem, diag, diag_error};
use agentdash_spi::hooks::HookEffect;
use agentdash_spi::session_persistence::{
    AgentRunControlEffectKind, AgentRunControlEffectRecord, AgentRunControlEffectStatus,
    AgentRunControlEffectStore, NewAgentRunControlEffectRecord,
};
use async_trait::async_trait;
use chrono::Utc;
use tokio::sync::RwLock;
use uuid::Uuid;

use super::{
    AgentRunMailboxAutoResumeRequest, AgentRunMailboxRuntimeAdapter,
    AgentRunRuntimeTerminalCommand, AgentRunTerminalConvergenceDeps,
    AgentRunTerminalConvergenceService, SessionControlService, SessionCoreService,
    SessionEventingService, SessionLaunchService,
};

const MAX_AGENT_RUN_CONTROL_EFFECT_ATTEMPTS: u32 = 3;
const AUTO_RESUME_PROMPT: &str =
    "继续推进当前 workflow step，直接执行未完成的动作或补齐证据。不要重复总结已发生的内容。";

#[derive(Clone)]
pub struct AgentRunControlEffectService {
    deps: Arc<AgentRunControlEffectDeps>,
}

#[derive(Clone)]
pub struct AgentRunControlEffectDeps {
    pub control_effect_store: Arc<dyn AgentRunControlEffectStore>,
    pub terminal_convergence_deps: AgentRunTerminalConvergenceDeps,
    pub session_core: SessionCoreService,
    pub session_control: SessionControlService,
    pub session_eventing: SessionEventingService,
    pub session_launch: SessionLaunchService,
    pub mailbox_runtime: AgentRunMailboxRuntimeAdapter,
    pub wait_producer_terminal_port: Arc<dyn AgentRunWaitProducerTerminalConvergencePort>,
    pub lifecycle_terminal_port: Arc<dyn AgentRunLifecycleTerminalConvergencePort>,
    pub hook_effect_handler_registry: Arc<RwLock<Option<DynAgentRunHookEffectHandlerRegistry>>>,
}

#[derive(Clone)]
enum ControlEffectExecutor {
    AgentRunDeliveryConvergence,
    WaitProducerTerminalConvergence,
    LifecycleTerminalConvergence,
    HookEffects {
        handler: Option<Arc<dyn AgentRunPostTurnHandler>>,
    },
    HookAutoResumeDelivery,
    Noop,
}

impl AgentRunControlEffectService {
    pub fn new(deps: AgentRunControlEffectDeps) -> Self {
        Self {
            deps: Arc::new(deps),
        }
    }

    pub async fn set_hook_effect_handler_registry(
        &self,
        registry: DynAgentRunHookEffectHandlerRegistry,
    ) {
        *self.deps.hook_effect_handler_registry.write().await = Some(registry);
    }

    async fn enqueue_effect(
        &self,
        effect: NewAgentRunControlEffectRecord,
    ) -> Result<AgentRunControlEffectRecord, String> {
        self.deps
            .control_effect_store
            .insert_control_effect(effect)
            .await
            .map_err(|error| error.to_string())
    }

    async fn owner_from_delivery(
        &self,
        delivery_runtime_session_id: &str,
    ) -> (Option<Uuid>, Option<Uuid>, Option<Uuid>) {
        match self
            .deps
            .terminal_convergence_deps
            .execution_anchor_repo
            .find_by_session(delivery_runtime_session_id)
            .await
        {
            Ok(Some(anchor)) => (
                Some(anchor.run_id),
                Some(anchor.agent_id),
                Some(anchor.launch_frame_id),
            ),
            _ => (None, None, None),
        }
    }

    async fn insert_terminal_delivery_effect(
        &self,
        input: &AgentRunTerminalControlInput,
    ) -> Result<AgentRunControlEffectRecord, String> {
        let (run_id, agent_id, frame_id) = self
            .owner_from_delivery(&input.delivery_runtime_session_id)
            .await;
        self.enqueue_effect(NewAgentRunControlEffectRecord {
            run_id,
            agent_id,
            frame_id,
            delivery_runtime_session_id: Some(input.delivery_runtime_session_id.clone()),
            turn_id: input.turn_id.clone(),
            terminal_event_seq: input.terminal_event_seq,
            effect_kind: AgentRunControlEffectKind::AgentRunDeliveryConvergence,
            payload: serde_json::json!({
                "terminal_state": input.terminal_state,
                "terminal_message": input.terminal_message,
            }),
        })
        .await
    }

    async fn insert_lifecycle_terminal_effect(
        &self,
        input: &AgentRunTerminalControlInput,
    ) -> Result<AgentRunControlEffectRecord, String> {
        let (run_id, agent_id, frame_id) = self
            .owner_from_delivery(&input.delivery_runtime_session_id)
            .await;
        self.enqueue_effect(NewAgentRunControlEffectRecord {
            run_id,
            agent_id,
            frame_id,
            delivery_runtime_session_id: Some(input.delivery_runtime_session_id.clone()),
            turn_id: input.turn_id.clone(),
            terminal_event_seq: input.terminal_event_seq,
            effect_kind: AgentRunControlEffectKind::LifecycleTerminalConvergence,
            payload: serde_json::json!({
                "terminal_state": input.terminal_state,
            }),
        })
        .await
    }

    async fn insert_hook_effects(
        &self,
        input: &AgentRunTerminalControlInput,
        hook_effects: &AgentRunTerminalHookEffects,
    ) -> Result<AgentRunControlEffectRecord, String> {
        let binding =
            hook_effects
                .durable_binding
                .clone()
                .unwrap_or_else(|| TerminalHookEffectBinding {
                    handler: serde_json::Value::Null,
                    supported_effect_kinds: Vec::new(),
                });
        let target = hook_effects.control_target.as_ref();
        self.enqueue_effect(NewAgentRunControlEffectRecord {
            run_id: target.map(|target| target.run_id),
            agent_id: target.map(|target| target.agent_id),
            frame_id: target.map(|target| target.frame_id),
            delivery_runtime_session_id: Some(input.delivery_runtime_session_id.clone()),
            turn_id: input.turn_id.clone(),
            terminal_event_seq: input.terminal_event_seq,
            effect_kind: AgentRunControlEffectKind::HookEffects,
            payload: serde_json::json!({
                "effects": hook_effects.effects,
                "handler": binding.handler,
                "supported_effect_kinds": binding.supported_effect_kinds,
            }),
        })
        .await
    }

    async fn insert_hook_auto_resume(
        &self,
        input: &AgentRunTerminalControlInput,
        hook_effects: Option<&AgentRunTerminalHookEffects>,
    ) -> Result<AgentRunControlEffectRecord, String> {
        let target = hook_effects.and_then(|effects| effects.control_target.as_ref());
        let (fallback_run_id, fallback_agent_id, fallback_frame_id) = self
            .owner_from_delivery(&input.delivery_runtime_session_id)
            .await;
        self.enqueue_effect(NewAgentRunControlEffectRecord {
            run_id: target.map(|target| target.run_id).or(fallback_run_id),
            agent_id: target.map(|target| target.agent_id).or(fallback_agent_id),
            frame_id: target.map(|target| target.frame_id).or(fallback_frame_id),
            delivery_runtime_session_id: Some(input.delivery_runtime_session_id.clone()),
            turn_id: input.turn_id.clone(),
            terminal_event_seq: input.terminal_event_seq,
            effect_kind: AgentRunControlEffectKind::HookAutoResumeDelivery,
            payload: serde_json::json!({
                "reason": "before_stop_continue",
            }),
        })
        .await
    }

    async fn insert_wait_terminal_effect(
        &self,
        source: &AgentRunControlEffectRecord,
        event: AgentRunWaitProducerTerminalEvent,
    ) -> Result<AgentRunControlEffectRecord, String> {
        self.enqueue_effect(NewAgentRunControlEffectRecord {
            run_id: Some(event.run_id),
            agent_id: Some(event.agent_id),
            frame_id: event.frame_id,
            delivery_runtime_session_id: source.delivery_runtime_session_id.clone(),
            turn_id: source.turn_id.clone(),
            terminal_event_seq: source.terminal_event_seq,
            effect_kind: AgentRunControlEffectKind::WaitProducerTerminalConvergence,
            payload: serde_json::to_value(event)
                .map_err(|error| format!("wait terminal effect payload 序列化失败: {error}"))?,
        })
        .await
    }

    async fn execute_record(
        &self,
        record: AgentRunControlEffectRecord,
        executor: ControlEffectExecutor,
    ) -> Result<(), String> {
        let next_attempt_count = record.attempt_count.saturating_add(1);
        self.deps
            .control_effect_store
            .mark_control_effect_running(record.id)
            .await
            .map_err(|error| error.to_string())?;

        let result = match executor {
            ControlEffectExecutor::AgentRunDeliveryConvergence => {
                self.execute_delivery_convergence(&record).await
            }
            ControlEffectExecutor::WaitProducerTerminalConvergence => {
                self.execute_wait_terminal_convergence(&record).await
            }
            ControlEffectExecutor::LifecycleTerminalConvergence => {
                self.execute_lifecycle_terminal_convergence(&record).await
            }
            ControlEffectExecutor::HookEffects { handler } => {
                self.execute_hook_effects(&record, handler).await
            }
            ControlEffectExecutor::HookAutoResumeDelivery => {
                self.execute_hook_auto_resume(&record).await
            }
            ControlEffectExecutor::Noop => Ok(()),
        };

        match result {
            Ok(()) => self
                .deps
                .control_effect_store
                .mark_control_effect_succeeded(record.id)
                .await
                .map_err(|error| error.to_string()),
            Err(error) => {
                if next_attempt_count >= MAX_AGENT_RUN_CONTROL_EFFECT_ATTEMPTS {
                    self.deps
                        .control_effect_store
                        .mark_control_effect_dead_letter(record.id, error.clone())
                        .await
                        .map_err(|store_error| store_error.to_string())?;
                } else {
                    self.deps
                        .control_effect_store
                        .mark_control_effect_failed(record.id, error.clone())
                        .await
                        .map_err(|store_error| store_error.to_string())?;
                }
                Err(error)
            }
        }
    }

    async fn execute_delivery_convergence(
        &self,
        record: &AgentRunControlEffectRecord,
    ) -> Result<(), String> {
        let delivery_runtime_session_id = record
            .delivery_runtime_session_id
            .as_deref()
            .ok_or_else(|| {
                "agent_run_delivery_convergence 缺少 delivery_runtime_session_id".to_string()
            })?;
        let terminal_state = payload_string(&record.payload, "terminal_state")?;
        let terminal_message = record
            .payload
            .get("terminal_message")
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned);
        let event = self
            .terminal_convergence_service()
            .converge_runtime_terminal(AgentRunRuntimeTerminalCommand {
                runtime_session_id: delivery_runtime_session_id.to_string(),
                turn_id: record.turn_id.clone(),
                terminal_state,
                terminal_message,
                observed_at: Utc::now(),
            })
            .await
            .map_err(|error| error.to_string())?;
        if let Some(event) = event {
            let wait_event = AgentRunWaitProducerTerminalEvent {
                run_id: event.run_id,
                agent_id: event.agent_id,
                frame_id: event.frame_id,
                terminal_state: event.terminal_state,
                terminal_message: event.terminal_message,
                source_turn_id: event.turn_id,
                delivery_trace_ref: event.delivery_trace_ref,
            };
            let wait_record = self.insert_wait_terminal_effect(record, wait_event).await?;
            self.execute_wait_terminal_record(wait_record).await?;
        }
        Ok(())
    }

    async fn execute_wait_terminal_record(
        &self,
        record: AgentRunControlEffectRecord,
    ) -> Result<(), String> {
        let next_attempt_count = record.attempt_count.saturating_add(1);
        self.deps
            .control_effect_store
            .mark_control_effect_running(record.id)
            .await
            .map_err(|error| error.to_string())?;
        match self.execute_wait_terminal_convergence(&record).await {
            Ok(()) => self
                .deps
                .control_effect_store
                .mark_control_effect_succeeded(record.id)
                .await
                .map_err(|error| error.to_string()),
            Err(error) => {
                if next_attempt_count >= MAX_AGENT_RUN_CONTROL_EFFECT_ATTEMPTS {
                    self.deps
                        .control_effect_store
                        .mark_control_effect_dead_letter(record.id, error.clone())
                        .await
                        .map_err(|store_error| store_error.to_string())?;
                } else {
                    self.deps
                        .control_effect_store
                        .mark_control_effect_failed(record.id, error.clone())
                        .await
                        .map_err(|store_error| store_error.to_string())?;
                }
                Err(error)
            }
        }
    }

    async fn execute_wait_terminal_convergence(
        &self,
        record: &AgentRunControlEffectRecord,
    ) -> Result<(), String> {
        let event =
            serde_json::from_value::<AgentRunWaitProducerTerminalEvent>(record.payload.clone())
                .map_err(|error| {
                    format!("wait_producer_terminal_convergence payload 无效: {error}")
                })?;
        self.deps
            .wait_producer_terminal_port
            .observe_agent_run_wait_producer_terminal(event)
            .await
    }

    async fn execute_lifecycle_terminal_convergence(
        &self,
        record: &AgentRunControlEffectRecord,
    ) -> Result<(), String> {
        let delivery_runtime_session_id = record
            .delivery_runtime_session_id
            .as_deref()
            .ok_or_else(|| {
                "lifecycle_terminal_convergence 缺少 delivery_runtime_session_id".to_string()
            })?;
        let terminal_state = payload_string(&record.payload, "terminal_state")?;
        self.deps
            .lifecycle_terminal_port
            .observe_lifecycle_terminal(delivery_runtime_session_id, &terminal_state)
            .await
    }

    async fn execute_hook_effects(
        &self,
        record: &AgentRunControlEffectRecord,
        immediate_handler: Option<Arc<dyn AgentRunPostTurnHandler>>,
    ) -> Result<(), String> {
        let effects = payload_hook_effects(&record.payload)?;
        let handler = match immediate_handler {
            Some(handler) => Some(handler),
            None => self.replay_hook_effect_handler(record).await?,
        };
        let Some(handler) = handler else {
            return Err("hook_effects 缺少 post-turn handler".to_string());
        };
        let supported = handler.supported_effect_kinds();
        if !supported.is_empty() {
            for effect in &effects {
                if !supported.contains(&effect.kind.as_str()) {
                    diag!(Warn, Subsystem::AgentRun,
                        operation = "agent_run.control_effects",
                        stage = "validate_hook_effect_kind",
                        delivery_runtime_session_id = %record.delivery_runtime_session_id.clone().unwrap_or_default(),
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
            .execute_effects(
                &record
                    .delivery_runtime_session_id
                    .clone()
                    .unwrap_or_default(),
                &record.turn_id,
                &effects,
            )
            .await;
        Ok(())
    }

    async fn replay_hook_effect_handler(
        &self,
        record: &AgentRunControlEffectRecord,
    ) -> Result<Option<Arc<dyn AgentRunPostTurnHandler>>, String> {
        let Some(registry) = self.deps.hook_effect_handler_registry.read().await.clone() else {
            return Ok(None);
        };
        registry
            .handler_for(
                &record
                    .delivery_runtime_session_id
                    .clone()
                    .unwrap_or_default(),
                &record.payload,
            )
            .await
    }

    async fn execute_hook_auto_resume(
        &self,
        record: &AgentRunControlEffectRecord,
    ) -> Result<(), String> {
        let delivery_runtime_session_id =
            record.delivery_runtime_session_id.clone().ok_or_else(|| {
                "hook_auto_resume_delivery 缺少 delivery_runtime_session_id".to_string()
            })?;
        self.deps
            .mailbox_runtime
            .accept_hook_auto_resume_effect(AgentRunMailboxAutoResumeRequest {
                session_id: delivery_runtime_session_id,
                effect_id: record.id,
                source_turn_id: record.turn_id.clone(),
                terminal_event_seq: record.terminal_event_seq,
                input: text_user_input_blocks(AUTO_RESUME_PROMPT),
            })
            .await
            .map(|_| ())
    }

    fn terminal_convergence_service(&self) -> AgentRunTerminalConvergenceService {
        AgentRunTerminalConvergenceService::new(
            self.deps.terminal_convergence_deps.clone(),
            self.deps.session_core.clone(),
            self.deps.session_control.clone(),
            self.deps.session_eventing.clone(),
            self.deps.session_launch.clone(),
        )
    }

    fn executor_for_replay(&self, record: &AgentRunControlEffectRecord) -> ControlEffectExecutor {
        match record.effect_kind {
            AgentRunControlEffectKind::AgentRunDeliveryConvergence => {
                ControlEffectExecutor::AgentRunDeliveryConvergence
            }
            AgentRunControlEffectKind::WaitProducerTerminalConvergence => {
                ControlEffectExecutor::WaitProducerTerminalConvergence
            }
            AgentRunControlEffectKind::LifecycleTerminalConvergence => {
                ControlEffectExecutor::LifecycleTerminalConvergence
            }
            AgentRunControlEffectKind::HookEffects => {
                ControlEffectExecutor::HookEffects { handler: None }
            }
            AgentRunControlEffectKind::HookAutoResumeDelivery => {
                ControlEffectExecutor::HookAutoResumeDelivery
            }
            AgentRunControlEffectKind::MailboxWakeDelivery
            | AgentRunControlEffectKind::HookRuntimeProjectionChanged => {
                ControlEffectExecutor::Noop
            }
        }
    }
}

#[async_trait]
impl AgentRunControlEffectPort for AgentRunControlEffectService {
    async fn observe_runtime_terminal(
        &self,
        input: AgentRunTerminalControlInput,
    ) -> Result<(), String> {
        let delivery = self.insert_terminal_delivery_effect(&input).await?;
        self.execute_record(delivery, ControlEffectExecutor::AgentRunDeliveryConvergence)
            .await?;

        let lifecycle = self.insert_lifecycle_terminal_effect(&input).await?;
        self.execute_record(
            lifecycle,
            ControlEffectExecutor::LifecycleTerminalConvergence,
        )
        .await?;

        if let Some(hook_effects) = input.terminal_hook_outputs.as_ref() {
            let record = self.insert_hook_effects(&input, hook_effects).await?;
            self.execute_record(
                record,
                ControlEffectExecutor::HookEffects {
                    handler: hook_effects.handler.clone(),
                },
            )
            .await?;
        }

        if input.before_stop_continue_observed {
            let record = self
                .insert_hook_auto_resume(&input, input.terminal_hook_outputs.as_ref())
                .await?;
            self.execute_record(record, ControlEffectExecutor::HookAutoResumeDelivery)
                .await?;
        }

        Ok(())
    }
}

#[async_trait]
impl AgentRunControlEffectReplayPort for AgentRunControlEffectService {
    async fn replay_control_effect_outbox(&self, limit: u32) -> Result<usize, String> {
        let records = self
            .deps
            .control_effect_store
            .list_control_effects_by_status(
                &[
                    AgentRunControlEffectStatus::Pending,
                    AgentRunControlEffectStatus::Running,
                    AgentRunControlEffectStatus::Failed,
                ],
                limit,
            )
            .await
            .map_err(|error| error.to_string())?;
        let mut attempted = 0;
        for record in records {
            attempted += 1;
            let delivery_runtime_session_id = record
                .delivery_runtime_session_id
                .clone()
                .unwrap_or_default();
            let effect_id = record.id;
            let effect_kind = record.effect_kind;
            let turn_id = record.turn_id.clone();
            let terminal_event_seq = record.terminal_event_seq;
            let executor = self.executor_for_replay(&record);
            if let Err(error) = self.execute_record(record, executor).await {
                let context = DiagnosticErrorContext::new(
                    "agent_run.control_effects",
                    "replay_control_effect_outbox",
                );
                diag_error!(
                    Warn,
                    Subsystem::AgentRun,
                    context = &context,
                    error = &std::io::Error::other(error),
                    delivery_runtime_session_id = %delivery_runtime_session_id,
                    turn_id = %turn_id,
                    effect_id = %effect_id,
                    effect_kind = effect_kind.as_str(),
                    terminal_event_seq,
                    "AgentRun control effect replay 失败"
                );
            }
        }
        Ok(attempted)
    }
}

fn payload_string(payload: &serde_json::Value, field: &str) -> Result<String, String> {
    payload
        .get(field)
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned)
        .ok_or_else(|| format!("control effect payload 缺少 `{field}`"))
}

fn payload_hook_effects(payload: &serde_json::Value) -> Result<Vec<HookEffect>, String> {
    let Some(value) = payload.get("effects").cloned() else {
        return Err("hook_effects payload 缺少 effects".to_string());
    };
    serde_json::from_value::<Vec<HookEffect>>(value)
        .map_err(|error| format!("hook_effects payload 无法反序列化: {error}"))
}
