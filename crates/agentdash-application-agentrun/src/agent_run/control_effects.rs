use std::sync::Arc;

use agentdash_agent_protocol::text_user_input_blocks;
use agentdash_application_ports::agent_run_control_effect::{
    AgentRunControlEffectPort, AgentRunControlEffectReplayPhase, AgentRunControlEffectReplayPort,
    AgentRunLifecycleTerminalConvergencePort, AgentRunPostTurnHandler,
    AgentRunTerminalControlEffectMode, AgentRunTerminalControlInput, AgentRunTerminalHookEffects,
    AgentRunTerminalHookTriggerInput, AgentRunTerminalHookTriggerPort,
    AgentRunWaitProducerTerminalConvergencePort, AgentRunWaitProducerTerminalEvent,
    DynAgentRunHookEffectHandlerRegistry,
};
use agentdash_application_ports::frame_launch_envelope::TerminalHookEffectBinding;
use agentdash_diagnostics::{DiagnosticErrorContext, Subsystem, diag, diag_error};
use agentdash_spi::hooks::{HookEffect, HookTraceTrigger};
use agentdash_spi::session_persistence::{
    AgentRunControlEffectKind, AgentRunControlEffectRecord, AgentRunControlEffectStore,
    ClaimAgentRunControlEffectsRequest, NewAgentRunControlEffectRecord,
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
const AGENT_RUN_CONTROL_EFFECT_LEASE_MS: i64 = 60_000;
const AUTO_RESUME_PROMPT: &str =
    "继续推进当前 workflow step，直接执行未完成的动作或补齐证据。不要重复总结已发生的内容。";

#[derive(Debug, Clone, Copy)]
struct ControlEffectOwnerRefs {
    run_id: Option<Uuid>,
    agent_id: Option<Uuid>,
    frame_id: Option<Uuid>,
}

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
    pub terminal_hook_trigger_port: Arc<dyn AgentRunTerminalHookTriggerPort>,
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
}

fn durable_hook_effect_binding(
    handler: &Arc<dyn AgentRunPostTurnHandler>,
) -> Option<TerminalHookEffectBinding> {
    let identity = handler.durable_effect_handler()?;
    if identity.is_null() {
        return None;
    }
    Some(TerminalHookEffectBinding {
        handler: identity,
        supported_effect_kinds: handler
            .supported_effect_kinds()
            .iter()
            .map(|kind| (*kind).to_string())
            .collect(),
    })
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

    async fn collect_terminal_hook_outputs(
        &self,
        input: &AgentRunTerminalControlInput,
    ) -> Option<AgentRunTerminalHookEffects> {
        let context = input.terminal_hook_context.as_ref()?;
        let effects = self
            .deps
            .terminal_hook_trigger_port
            .emit_agent_run_terminal_hook_trigger(
                context.hook_runtime.as_ref(),
                AgentRunTerminalHookTriggerInput {
                    delivery_runtime_session_id: input.delivery_runtime_session_id.clone(),
                    turn_id: input.turn_id.clone(),
                    terminal_state: input.terminal_state.clone(),
                    terminal_message: input.terminal_message.clone(),
                    terminal_diagnostic: input.terminal_diagnostic.clone(),
                    source: context.source.clone(),
                },
            )
            .await;
        if effects.is_empty() {
            return None;
        }

        let durable_binding = context
            .post_turn_handler
            .as_ref()
            .and_then(durable_hook_effect_binding);

        Some(AgentRunTerminalHookEffects {
            control_target: Some(context.hook_runtime.control_target()),
            effects,
            handler: context.post_turn_handler.clone(),
            durable_binding,
        })
    }

    fn should_deliver_hook_auto_resume(&self, input: &AgentRunTerminalControlInput) -> bool {
        input.terminal_state == "completed"
            && input.terminal_hook_context.as_ref().is_some_and(|context| {
                context
                    .hook_runtime
                    .trace()
                    .iter()
                    .rev()
                    .find(|entry| matches!(entry.trigger, HookTraceTrigger::BeforeStop))
                    .is_some_and(|entry| entry.decision == "continue")
            })
    }

    async fn enqueue_effect(
        &self,
        effect: NewAgentRunControlEffectRecord,
    ) -> Result<AgentRunControlEffectRecord, String> {
        self.deps
            .control_effect_store
            .insert_or_get_control_effect(effect)
            .await
            .map_err(|error| error.to_string())
    }

    async fn owner_from_delivery(
        &self,
        delivery_runtime_session_id: &str,
    ) -> ControlEffectOwnerRefs {
        match self
            .deps
            .terminal_convergence_deps
            .execution_anchor_repo
            .find_by_session(delivery_runtime_session_id)
            .await
        {
            Ok(Some(anchor)) => ControlEffectOwnerRefs {
                run_id: Some(anchor.run_id),
                agent_id: Some(anchor.agent_id),
                frame_id: Some(anchor.launch_frame_id),
            },
            _ => ControlEffectOwnerRefs {
                run_id: None,
                agent_id: None,
                frame_id: None,
            },
        }
    }

    fn terminal_dedup_key(
        input: &AgentRunTerminalControlInput,
        effect_kind: AgentRunControlEffectKind,
        discriminator: &str,
    ) -> String {
        format!(
            "runtime_terminal:{}:{}:{}:{}:{}",
            input.delivery_runtime_session_id,
            input.turn_id,
            input.terminal_event_seq,
            effect_kind.as_str(),
            discriminator
        )
    }

    async fn insert_terminal_delivery_effect(
        &self,
        input: &AgentRunTerminalControlInput,
        owner: ControlEffectOwnerRefs,
    ) -> Result<AgentRunControlEffectRecord, String> {
        self.enqueue_effect(NewAgentRunControlEffectRecord {
            dedup_key: Self::terminal_dedup_key(
                input,
                AgentRunControlEffectKind::AgentRunDeliveryConvergence,
                "delivery",
            ),
            run_id: owner.run_id,
            agent_id: owner.agent_id,
            frame_id: owner.frame_id,
            delivery_runtime_session_id: Some(input.delivery_runtime_session_id.clone()),
            turn_id: input.turn_id.clone(),
            terminal_event_seq: input.terminal_event_seq,
            effect_kind: AgentRunControlEffectKind::AgentRunDeliveryConvergence,
            payload: serde_json::json!({
                "terminal_state": input.terminal_state,
                "terminal_message": input.terminal_message,
                "terminal_diagnostic": input.terminal_diagnostic,
            }),
        })
        .await
    }

    async fn insert_lifecycle_terminal_effect(
        &self,
        input: &AgentRunTerminalControlInput,
        owner: ControlEffectOwnerRefs,
    ) -> Result<AgentRunControlEffectRecord, String> {
        self.enqueue_effect(NewAgentRunControlEffectRecord {
            dedup_key: Self::terminal_dedup_key(
                input,
                AgentRunControlEffectKind::LifecycleTerminalConvergence,
                "lifecycle",
            ),
            run_id: owner.run_id,
            agent_id: owner.agent_id,
            frame_id: owner.frame_id,
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
    ) -> Result<Option<AgentRunControlEffectRecord>, String> {
        let Some(binding) = hook_effects.durable_binding.clone() else {
            self.log_non_durable_hook_effects(
                input,
                hook_effects,
                "missing_durable_handler_identity",
            );
            return Ok(None);
        };
        let target = hook_effects.control_target.as_ref();
        self.enqueue_effect(NewAgentRunControlEffectRecord {
            dedup_key: Self::terminal_dedup_key(
                input,
                AgentRunControlEffectKind::HookEffects,
                "hook_effects",
            ),
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
        .map(Some)
    }

    fn log_non_durable_hook_effects(
        &self,
        input: &AgentRunTerminalControlInput,
        hook_effects: &AgentRunTerminalHookEffects,
        reason: &str,
    ) {
        let effect_kinds = hook_effects
            .effects
            .iter()
            .map(|effect| effect.kind.as_str())
            .collect::<Vec<_>>();
        diag!(
            Warn,
            Subsystem::AgentRun,
            operation = "agent_run.control_effects",
            stage = "skip_non_durable_hook_effects",
            delivery_runtime_session_id = %input.delivery_runtime_session_id,
            turn_id = %input.turn_id,
            terminal_event_seq = input.terminal_event_seq,
            reason = %reason,
            effect_count = hook_effects.effects.len(),
            effect_kinds = ?effect_kinds,
            "Hook effects 缺少可恢复 handler identity，未写入 durable outbox；仅能在当前 terminal intake 中即时执行"
        );
    }

    async fn execute_non_durable_hook_effects(
        &self,
        input: &AgentRunTerminalControlInput,
        hook_effects: &AgentRunTerminalHookEffects,
    ) -> Result<(), String> {
        if hook_effects.durable_binding.is_some() {
            return Ok(());
        }
        let Some(handler) = hook_effects.handler.as_ref() else {
            self.log_non_durable_hook_effects(input, hook_effects, "missing_live_handler");
            return Ok(());
        };
        handler
            .execute_effects(
                &input.delivery_runtime_session_id,
                &input.turn_id,
                &hook_effects.effects,
            )
            .await
    }

    async fn insert_hook_auto_resume(
        &self,
        input: &AgentRunTerminalControlInput,
        hook_effects: Option<&AgentRunTerminalHookEffects>,
        owner: ControlEffectOwnerRefs,
    ) -> Result<AgentRunControlEffectRecord, String> {
        let target = hook_effects.and_then(|effects| effects.control_target.as_ref());
        self.enqueue_effect(NewAgentRunControlEffectRecord {
            dedup_key: Self::terminal_dedup_key(
                input,
                AgentRunControlEffectKind::HookAutoResumeDelivery,
                "hook_auto_resume",
            ),
            run_id: target.map(|target| target.run_id).or(owner.run_id),
            agent_id: target.map(|target| target.agent_id).or(owner.agent_id),
            frame_id: target.map(|target| target.frame_id).or(owner.frame_id),
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
        input: &AgentRunTerminalControlInput,
        owner: ControlEffectOwnerRefs,
    ) -> Result<Option<AgentRunControlEffectRecord>, String> {
        let Some(run_id) = owner.run_id else {
            return Ok(None);
        };
        let Some(agent_id) = owner.agent_id else {
            return Ok(None);
        };
        let event = AgentRunWaitProducerTerminalEvent {
            run_id,
            agent_id,
            frame_id: owner.frame_id,
            terminal_state: input.terminal_state.clone(),
            terminal_message: input.terminal_message.clone(),
            terminal_diagnostic: input.terminal_diagnostic.clone(),
            producer_last_message: None,
            source_turn_id: Some(input.turn_id.clone()),
            delivery_trace_ref: Some(input.delivery_runtime_session_id.clone()),
        };
        self.enqueue_effect(NewAgentRunControlEffectRecord {
            dedup_key: Self::terminal_dedup_key(
                input,
                AgentRunControlEffectKind::WaitProducerTerminalConvergence,
                "wait_producer",
            ),
            run_id: Some(event.run_id),
            agent_id: Some(event.agent_id),
            frame_id: event.frame_id,
            delivery_runtime_session_id: Some(input.delivery_runtime_session_id.clone()),
            turn_id: input.turn_id.clone(),
            terminal_event_seq: input.terminal_event_seq,
            effect_kind: AgentRunControlEffectKind::WaitProducerTerminalConvergence,
            payload: serde_json::to_value(event)
                .map_err(|error| format!("wait terminal effect payload 序列化失败: {error}"))?,
        })
        .await
        .map(Some)
    }

    async fn execute_record(
        &self,
        record: AgentRunControlEffectRecord,
        executor: ControlEffectExecutor,
    ) -> Result<(), String> {
        let claim_token = record
            .claim_token
            .ok_or_else(|| format!("control effect {} 尚未 claim", record.id))?;
        if record.attempt_count > MAX_AGENT_RUN_CONTROL_EFFECT_ATTEMPTS {
            self.deps
                .control_effect_store
                .mark_control_effect_dead_letter(
                    record.id,
                    claim_token,
                    "control effect 尝试次数超限".to_string(),
                )
                .await
                .map_err(|store_error| store_error.to_string())?;
            return Err("control effect 尝试次数超限".to_string());
        }

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
        };

        match result {
            Ok(()) => self
                .deps
                .control_effect_store
                .mark_control_effect_succeeded(record.id, claim_token)
                .await
                .map_err(|error| error.to_string()),
            Err(error) => {
                if record.attempt_count >= MAX_AGENT_RUN_CONTROL_EFFECT_ATTEMPTS {
                    self.deps
                        .control_effect_store
                        .mark_control_effect_dead_letter(record.id, claim_token, error.clone())
                        .await
                        .map_err(|store_error| store_error.to_string())?;
                } else {
                    self.deps
                        .control_effect_store
                        .mark_control_effect_failed(record.id, claim_token, error.clone())
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
        let terminal_diagnostic = record
            .payload
            .get("terminal_diagnostic")
            .cloned()
            .and_then(|value| serde_json::from_value(value).ok());
        self.terminal_convergence_service()
            .converge_runtime_terminal(AgentRunRuntimeTerminalCommand {
                runtime_session_id: delivery_runtime_session_id.to_string(),
                turn_id: record.turn_id.clone(),
                terminal_state,
                terminal_message,
                terminal_diagnostic,
                observed_at: Utc::now(),
            })
            .await
            .map_err(|error| error.to_string())?;
        Ok(())
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
            .await
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
        }
    }

    fn phase_effect_kinds(
        phase: AgentRunControlEffectReplayPhase,
    ) -> Vec<AgentRunControlEffectKind> {
        match phase {
            AgentRunControlEffectReplayPhase::DeliveryConvergence => {
                vec![AgentRunControlEffectKind::AgentRunDeliveryConvergence]
            }
            AgentRunControlEffectReplayPhase::TerminalSideEffects => vec![
                AgentRunControlEffectKind::WaitProducerTerminalConvergence,
                AgentRunControlEffectKind::LifecycleTerminalConvergence,
                AgentRunControlEffectKind::HookEffects,
                AgentRunControlEffectKind::HookAutoResumeDelivery,
            ],
        }
    }

    fn phase_claim_owner(phase: AgentRunControlEffectReplayPhase) -> &'static str {
        match phase {
            AgentRunControlEffectReplayPhase::DeliveryConvergence => {
                "replay_control_effect_outbox.delivery_convergence"
            }
            AgentRunControlEffectReplayPhase::TerminalSideEffects => {
                "replay_control_effect_outbox.terminal_side_effects"
            }
        }
    }

    async fn claim_materialized_phase(
        &self,
        dedup_keys: &[String],
        phase: AgentRunControlEffectReplayPhase,
    ) -> Result<Vec<AgentRunControlEffectRecord>, String> {
        if dedup_keys.is_empty() {
            return Ok(Vec::new());
        }
        self.deps
            .control_effect_store
            .claim_control_effects(ClaimAgentRunControlEffectsRequest {
                claim_owner: format!("observe_runtime_terminal.{:?}", phase),
                lease_duration_ms: AGENT_RUN_CONTROL_EFFECT_LEASE_MS,
                limit: u32::try_from(dedup_keys.len()).unwrap_or(u32::MAX),
                dedup_keys: Some(dedup_keys.to_vec()),
                effect_kinds: Some(Self::phase_effect_kinds(phase)),
            })
            .await
            .map_err(|error| error.to_string())
    }

    async fn execute_claimed_records(
        &self,
        records: Vec<AgentRunControlEffectRecord>,
        terminal_hook_outputs: Option<&AgentRunTerminalHookEffects>,
    ) -> Vec<String> {
        let mut execute_errors = Vec::new();
        for record in records {
            let executor = match record.effect_kind {
                AgentRunControlEffectKind::HookEffects => ControlEffectExecutor::HookEffects {
                    handler: terminal_hook_outputs.and_then(|effects| effects.handler.clone()),
                },
                _ => self.executor_for_replay(&record),
            };
            if let Err(error) = self.execute_record(record, executor).await {
                execute_errors.push(error);
            }
        }
        execute_errors
    }
}

#[async_trait]
impl AgentRunControlEffectPort for AgentRunControlEffectService {
    async fn observe_runtime_terminal(
        &self,
        input: AgentRunTerminalControlInput,
    ) -> Result<(), String> {
        let terminal_hook_outputs = self.collect_terminal_hook_outputs(&input).await;
        let hook_auto_resume_requested = self.should_deliver_hook_auto_resume(&input);
        let owner = self
            .owner_from_delivery(&input.delivery_runtime_session_id)
            .await;

        let mut materialized = Vec::new();
        let mut materialize_errors = Vec::new();

        match self.insert_terminal_delivery_effect(&input, owner).await {
            Ok(record) => materialized.push(record),
            Err(error) => materialize_errors.push(error),
        }

        match self.insert_wait_terminal_effect(&input, owner).await {
            Ok(Some(record)) => materialized.push(record),
            Ok(None) => {}
            Err(error) => materialize_errors.push(error),
        }

        match self.insert_lifecycle_terminal_effect(&input, owner).await {
            Ok(record) => materialized.push(record),
            Err(error) => materialize_errors.push(error),
        }

        if let Some(hook_effects) = terminal_hook_outputs.as_ref() {
            match self.insert_hook_effects(&input, hook_effects).await {
                Ok(Some(record)) => materialized.push(record),
                Ok(None) => {}
                Err(error) => materialize_errors.push(error),
            }
        }

        if hook_auto_resume_requested {
            match self
                .insert_hook_auto_resume(&input, terminal_hook_outputs.as_ref(), owner)
                .await
            {
                Ok(record) => materialized.push(record),
                Err(error) => materialize_errors.push(error),
            }
        }

        if !materialize_errors.is_empty() {
            return Err(materialize_errors.join("; "));
        }
        if materialized.is_empty() {
            return Ok(());
        }

        let dedup_keys = materialized
            .iter()
            .map(|record| record.dedup_key.clone())
            .collect::<Vec<_>>();
        let mut execute_errors = Vec::new();
        let delivery_records = self
            .claim_materialized_phase(
                &dedup_keys,
                AgentRunControlEffectReplayPhase::DeliveryConvergence,
            )
            .await?;
        execute_errors.extend(
            self.execute_claimed_records(delivery_records, terminal_hook_outputs.as_ref())
                .await,
        );

        if input.effect_mode == AgentRunTerminalControlEffectMode::DeliveryConvergenceOnly {
            if !execute_errors.is_empty() {
                return Err(execute_errors.join("; "));
            }
            return Ok(());
        }

        if let Some(hook_effects) = terminal_hook_outputs.as_ref()
            && hook_effects.durable_binding.is_none()
            && let Err(error) = self
                .execute_non_durable_hook_effects(&input, hook_effects)
                .await
        {
            execute_errors.push(error);
        }
        let side_effect_records = self
            .claim_materialized_phase(
                &dedup_keys,
                AgentRunControlEffectReplayPhase::TerminalSideEffects,
            )
            .await?;
        execute_errors.extend(
            self.execute_claimed_records(side_effect_records, terminal_hook_outputs.as_ref())
                .await,
        );

        if !execute_errors.is_empty() {
            return Err(execute_errors.join("; "));
        }

        Ok(())
    }
}

#[async_trait]
impl AgentRunControlEffectReplayPort for AgentRunControlEffectService {
    async fn replay_control_effect_outbox_phase(
        &self,
        phase: AgentRunControlEffectReplayPhase,
        limit: u32,
    ) -> Result<usize, String> {
        let records = self
            .deps
            .control_effect_store
            .claim_control_effects(ClaimAgentRunControlEffectsRequest {
                claim_owner: Self::phase_claim_owner(phase).to_string(),
                lease_duration_ms: AGENT_RUN_CONTROL_EFFECT_LEASE_MS,
                limit,
                dedup_keys: None,
                effect_kinds: Some(Self::phase_effect_kinds(phase)),
            })
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

    async fn replay_control_effect_outbox(&self, limit: u32) -> Result<usize, String> {
        if limit == 0 {
            return Ok(0);
        }
        let delivery = self
            .replay_control_effect_outbox_phase(
                AgentRunControlEffectReplayPhase::DeliveryConvergence,
                limit,
            )
            .await?;
        if delivery >= limit as usize {
            return Ok(delivery);
        }
        let remaining = limit.saturating_sub(delivery as u32);
        let side_effects = self
            .replay_control_effect_outbox_phase(
                AgentRunControlEffectReplayPhase::TerminalSideEffects,
                remaining,
            )
            .await?;
        Ok(delivery + side_effects)
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

#[cfg(test)]
mod tests {
    use super::super::{
        AgentRunMailboxRuntimeBoundaryDeps, RuntimeSessionControlPort, RuntimeSessionCorePort,
        RuntimeSessionEventingPort, RuntimeSessionLaunchPort, SessionExecutionState, SessionMeta,
        SessionTurnSteerCommand,
    };
    use super::*;
    use crate::WorkflowApplicationError;
    use crate::test_support::{
        MemoryAgentFrameRepository, MemoryAgentRunCommandReceiptRepository,
        MemoryAgentRunDeliveryBindingRepository, MemoryAgentRunMailboxRepository,
        MemoryLifecycleAgentRepository, MemoryLifecycleRunRepository, MemoryProjectAgentRepository,
        MemoryProjectBackendAccessRepository, MemoryRuntimeSessionExecutionAnchorRepository,
    };
    use agentdash_agent_protocol::{
        BackboneEnvelope, RuntimeTerminalDiagnostic, SourceInfo, UserInputSubmissionKind,
    };
    use agentdash_application_ports::agent_run_control_effect::{
        AgentRunHookEffectHandlerRegistry, AgentRunTerminalHookContext,
        EmptyAgentRunHookEffectHandlerRegistry,
    };
    use agentdash_application_ports::launch::{LaunchCommand, LaunchPlanningInput};
    use agentdash_spi::ConnectorError;
    use agentdash_spi::hooks::{
        AgentFrameHookSnapshot, AgentFrameRuntimeSnapshot, ContextTokenStats, HookDiagnosticEntry,
        HookError, HookPendingAction, HookPendingActionResolutionKind, HookResolution,
        HookRuntimeAccess, HookRuntimeEvaluationQuery, HookRuntimeRefreshQuery, HookTraceEntry,
        HookTurnStartNotice, SetDelta,
    };
    use agentdash_spi::session_persistence::{
        AgentRunControlEffectStatus, SessionEventPage, SessionStoreError, SessionStoreResult,
    };
    use async_trait::async_trait;
    use std::collections::BTreeSet;
    use tokio::sync::{Mutex, RwLock, broadcast};

    static SUPPORTED_EFFECT_KINDS: [&str; 1] = ["test:effect"];

    #[derive(Default)]
    struct RecordingControlEffectStore {
        records: Mutex<Vec<AgentRunControlEffectRecord>>,
    }

    impl RecordingControlEffectStore {
        async fn records(&self) -> Vec<AgentRunControlEffectRecord> {
            self.records.lock().await.clone()
        }

        async fn records_by_kind(
            &self,
            effect_kind: AgentRunControlEffectKind,
        ) -> Vec<AgentRunControlEffectRecord> {
            self.records()
                .await
                .into_iter()
                .filter(|record| record.effect_kind == effect_kind)
                .collect()
        }

        async fn update_record(
            &self,
            effect_id: Uuid,
            claim_token: Uuid,
            update: impl FnOnce(&mut AgentRunControlEffectRecord, i64),
        ) -> SessionStoreResult<()> {
            let mut records = self.records.lock().await;
            let record = records
                .iter_mut()
                .find(|record| record.id == effect_id)
                .ok_or_else(|| {
                    SessionStoreError::NotFound(format!("control effect {effect_id} 不存在"))
                })?;
            if record.claim_token != Some(claim_token) {
                return Err(SessionStoreError::InvalidInput(format!(
                    "control effect {effect_id} claim token 不匹配"
                )));
            }
            update(record, Utc::now().timestamp_millis());
            Ok(())
        }
    }

    #[async_trait]
    impl AgentRunControlEffectStore for RecordingControlEffectStore {
        async fn insert_or_get_control_effect(
            &self,
            effect: NewAgentRunControlEffectRecord,
        ) -> SessionStoreResult<AgentRunControlEffectRecord> {
            let mut records = self.records.lock().await;
            if let Some(existing) = records
                .iter()
                .find(|record| record.dedup_key == effect.dedup_key)
                .cloned()
            {
                return Ok(existing);
            }
            let now = Utc::now().timestamp_millis();
            let record = AgentRunControlEffectRecord {
                id: Uuid::new_v4(),
                dedup_key: effect.dedup_key,
                run_id: effect.run_id,
                agent_id: effect.agent_id,
                frame_id: effect.frame_id,
                delivery_runtime_session_id: effect.delivery_runtime_session_id,
                turn_id: effect.turn_id,
                terminal_event_seq: effect.terminal_event_seq,
                effect_kind: effect.effect_kind,
                payload: effect.payload,
                status: AgentRunControlEffectStatus::Pending,
                attempt_count: 0,
                claim_token: None,
                claim_owner: None,
                claim_expires_at_ms: None,
                created_at_ms: now,
                updated_at_ms: now,
                last_error: None,
            };
            records.push(record.clone());
            Ok(record)
        }

        async fn claim_control_effects(
            &self,
            request: ClaimAgentRunControlEffectsRequest,
        ) -> SessionStoreResult<Vec<AgentRunControlEffectRecord>> {
            let mut records = self.records.lock().await;
            let now = Utc::now().timestamp_millis();
            let claim_token = Uuid::new_v4();
            let claim_expires_at_ms = now.saturating_add(request.lease_duration_ms);
            let limit = request.limit as usize;
            let mut claimed = Vec::new();
            for record in records.iter_mut() {
                if claimed.len() >= limit {
                    break;
                }
                if request
                    .dedup_keys
                    .as_ref()
                    .is_some_and(|keys| !keys.contains(&record.dedup_key))
                {
                    continue;
                }
                if request
                    .effect_kinds
                    .as_ref()
                    .is_some_and(|kinds| !kinds.contains(&record.effect_kind))
                {
                    continue;
                }
                let claimable = matches!(
                    record.status,
                    AgentRunControlEffectStatus::Pending | AgentRunControlEffectStatus::Failed
                ) || (record.status == AgentRunControlEffectStatus::Running
                    && record
                        .claim_expires_at_ms
                        .is_none_or(|expires_at| expires_at <= now));
                if !claimable {
                    continue;
                }
                record.status = AgentRunControlEffectStatus::Running;
                record.attempt_count = record.attempt_count.saturating_add(1);
                record.claim_token = Some(claim_token);
                record.claim_owner = Some(request.claim_owner.clone());
                record.claim_expires_at_ms = Some(claim_expires_at_ms);
                record.updated_at_ms = now;
                record.last_error = None;
                claimed.push(record.clone());
            }
            Ok(claimed)
        }

        async fn mark_control_effect_succeeded(
            &self,
            effect_id: Uuid,
            claim_token: Uuid,
        ) -> SessionStoreResult<()> {
            self.update_record(effect_id, claim_token, |record, now| {
                record.status = AgentRunControlEffectStatus::Succeeded;
                record.claim_token = None;
                record.claim_owner = None;
                record.claim_expires_at_ms = None;
                record.updated_at_ms = now;
                record.last_error = None;
            })
            .await
        }

        async fn mark_control_effect_failed(
            &self,
            effect_id: Uuid,
            claim_token: Uuid,
            error: String,
        ) -> SessionStoreResult<()> {
            self.update_record(effect_id, claim_token, |record, now| {
                record.status = AgentRunControlEffectStatus::Failed;
                record.claim_token = None;
                record.claim_owner = None;
                record.claim_expires_at_ms = None;
                record.updated_at_ms = now;
                record.last_error = Some(error);
            })
            .await
        }

        async fn mark_control_effect_dead_letter(
            &self,
            effect_id: Uuid,
            claim_token: Uuid,
            error: String,
        ) -> SessionStoreResult<()> {
            self.update_record(effect_id, claim_token, |record, now| {
                record.status = AgentRunControlEffectStatus::DeadLetter;
                record.claim_token = None;
                record.claim_owner = None;
                record.claim_expires_at_ms = None;
                record.updated_at_ms = now;
                record.last_error = Some(error);
            })
            .await
        }
    }

    #[derive(Debug)]
    struct TestHookRuntime {
        session_id: String,
        target: agentdash_spi::hooks::HookControlTarget,
    }

    #[async_trait]
    impl HookRuntimeAccess for TestHookRuntime {
        fn session_id(&self) -> &str {
            &self.session_id
        }

        fn control_target(&self) -> agentdash_spi::hooks::HookControlTarget {
            self.target.clone()
        }

        fn snapshot(&self) -> AgentFrameHookSnapshot {
            AgentFrameHookSnapshot::default()
        }

        fn diagnostics(&self) -> Vec<HookDiagnosticEntry> {
            Vec::new()
        }

        fn revision(&self) -> u64 {
            1
        }

        fn trace(&self) -> Vec<HookTraceEntry> {
            Vec::new()
        }

        fn pending_actions(&self) -> Vec<HookPendingAction> {
            Vec::new()
        }

        fn runtime_snapshot(&self) -> AgentFrameRuntimeSnapshot {
            AgentFrameRuntimeSnapshot::default()
        }

        async fn refresh_from_provenance(
            &self,
            _query: HookRuntimeRefreshQuery,
        ) -> Result<AgentFrameHookSnapshot, HookError> {
            Ok(AgentFrameHookSnapshot::default())
        }

        async fn evaluate_from_provenance(
            &self,
            _query: HookRuntimeEvaluationQuery,
        ) -> Result<HookResolution, HookError> {
            Ok(HookResolution::default())
        }

        fn replace_snapshot(&self, _snapshot: AgentFrameHookSnapshot) {}

        fn append_diagnostics_vec(&self, _entries: Vec<HookDiagnosticEntry>) {}

        fn append_trace(&self, _trace: HookTraceEntry) {}

        fn next_trace_sequence(&self) -> u64 {
            1
        }

        fn enqueue_pending_action(&self, _action: HookPendingAction) {}

        fn collect_pending_actions_for_injection(&self) -> Vec<HookPendingAction> {
            Vec::new()
        }

        fn enqueue_turn_start_notice(&self, _notice: HookTurnStartNotice) {}

        fn collect_turn_start_notices_for_injection(&self) -> Vec<HookTurnStartNotice> {
            Vec::new()
        }

        fn unresolved_pending_actions(&self) -> Vec<HookPendingAction> {
            Vec::new()
        }

        fn unresolved_blocking_actions(&self) -> Vec<HookPendingAction> {
            Vec::new()
        }

        fn resolve_pending_action(
            &self,
            _action_id: &str,
            _resolution_kind: HookPendingActionResolutionKind,
            _note: Option<String>,
            _turn_id: Option<String>,
        ) -> Option<HookPendingAction> {
            None
        }

        fn update_token_stats(&self, _stats: ContextTokenStats) {}

        fn token_stats(&self) -> ContextTokenStats {
            ContextTokenStats::default()
        }

        fn current_capabilities(&self) -> BTreeSet<String> {
            BTreeSet::new()
        }

        fn update_capabilities(&self, _new_caps: BTreeSet<String>) -> Option<SetDelta> {
            None
        }

        fn subscribe_traces(&self) -> Option<broadcast::Receiver<HookTraceEntry>> {
            None
        }
    }

    struct StaticTerminalHookTriggerPort {
        effects: Vec<HookEffect>,
    }

    #[async_trait]
    impl AgentRunTerminalHookTriggerPort for StaticTerminalHookTriggerPort {
        async fn emit_agent_run_terminal_hook_trigger(
            &self,
            _hook_runtime: &dyn HookRuntimeAccess,
            _input: AgentRunTerminalHookTriggerInput,
        ) -> Vec<HookEffect> {
            self.effects.clone()
        }
    }

    #[derive(Default)]
    struct TestPostTurnHandler {
        durable_identity: Option<serde_json::Value>,
        failures_remaining: Mutex<usize>,
        executed: Mutex<usize>,
    }

    #[async_trait]
    impl AgentRunPostTurnHandler for TestPostTurnHandler {
        async fn on_event(&self, _session_id: &str, _envelope: &BackboneEnvelope) {}

        async fn execute_effects(
            &self,
            _session_id: &str,
            _turn_id: &str,
            _effects: &[HookEffect],
        ) -> Result<(), String> {
            *self.executed.lock().await += 1;
            let mut failures = self.failures_remaining.lock().await;
            if *failures > 0 {
                *failures -= 1;
                return Err("forced hook effect failure".to_string());
            }
            Ok(())
        }

        fn supported_effect_kinds(&self) -> &[&str] {
            &SUPPORTED_EFFECT_KINDS
        }

        fn durable_effect_handler(&self) -> Option<serde_json::Value> {
            self.durable_identity.clone()
        }
    }

    struct StaticHookEffectHandlerRegistry {
        handler: Arc<dyn AgentRunPostTurnHandler>,
    }

    #[async_trait]
    impl AgentRunHookEffectHandlerRegistry for StaticHookEffectHandlerRegistry {
        async fn handler_for(
            &self,
            _delivery_runtime_session_id: &str,
            payload: &serde_json::Value,
        ) -> Result<Option<Arc<dyn AgentRunPostTurnHandler>>, String> {
            match payload.get("handler") {
                Some(value) if !value.is_null() => Ok(Some(self.handler.clone())),
                _ => Ok(None),
            }
        }
    }

    struct TestCorePort;

    #[async_trait]
    impl RuntimeSessionCorePort for TestCorePort {
        async fn inspect_session_execution_state(
            &self,
            _session_id: &str,
        ) -> Result<SessionExecutionState, WorkflowApplicationError> {
            Ok(SessionExecutionState::Idle)
        }

        async fn get_session_meta(
            &self,
            _session_id: &str,
        ) -> Result<Option<SessionMeta>, WorkflowApplicationError> {
            Ok(None)
        }

        async fn delete_session(&self, _session_id: &str) -> Result<(), WorkflowApplicationError> {
            Ok(())
        }
    }

    struct TestControlPort;

    #[async_trait]
    impl RuntimeSessionControlPort for TestControlPort {
        async fn supports_session_steering(&self, _session_id: &str) -> bool {
            false
        }

        async fn steer_session(
            &self,
            _command: SessionTurnSteerCommand,
        ) -> Result<(), ConnectorError> {
            Ok(())
        }
    }

    struct TestEventingPort;

    #[async_trait]
    impl RuntimeSessionEventingPort for TestEventingPort {
        async fn list_event_page(
            &self,
            _session_id: &str,
            _after_seq: u64,
            _limit: u32,
        ) -> std::io::Result<SessionEventPage> {
            Ok(SessionEventPage {
                snapshot_seq: 0,
                events: Vec::new(),
                has_more: false,
                next_after_seq: 0,
            })
        }

        async fn persist_notification(
            &self,
            _session_id: &str,
            _envelope: BackboneEnvelope,
        ) -> Result<(), WorkflowApplicationError> {
            Ok(())
        }

        async fn emit_user_input_submitted(
            &self,
            _session_id: &str,
            _turn_id: &str,
            _event_id: &str,
            _kind: UserInputSubmissionKind,
            _source: agentdash_agent_protocol::UserInputSource,
            _input: Vec<agentdash_agent_protocol::UserInputBlock>,
        ) -> Result<(), WorkflowApplicationError> {
            Ok(())
        }
    }

    struct TestLaunchPort;

    #[async_trait]
    impl RuntimeSessionLaunchPort for TestLaunchPort {
        async fn launch_command_in_task(
            &self,
            _session_id: String,
            _command: LaunchCommand,
            _planning_input: LaunchPlanningInput,
        ) -> Result<String, WorkflowApplicationError> {
            Ok("turn-launched".to_string())
        }
    }

    struct NoopWaitProducerTerminalPort;

    #[async_trait]
    impl AgentRunWaitProducerTerminalConvergencePort for NoopWaitProducerTerminalPort {
        async fn observe_agent_run_wait_producer_terminal(
            &self,
            _event: AgentRunWaitProducerTerminalEvent,
        ) -> Result<(), String> {
            Ok(())
        }
    }

    struct NoopLifecycleTerminalPort;

    #[async_trait]
    impl AgentRunLifecycleTerminalConvergencePort for NoopLifecycleTerminalPort {
        async fn observe_lifecycle_terminal(
            &self,
            _delivery_runtime_session_id: &str,
            _terminal_state: &str,
        ) -> Result<(), String> {
            Ok(())
        }
    }

    struct ControlEffectFixture {
        service: AgentRunControlEffectService,
        store: Arc<RecordingControlEffectStore>,
    }

    impl ControlEffectFixture {
        fn new(
            effects: Vec<HookEffect>,
            registry: Arc<dyn AgentRunHookEffectHandlerRegistry>,
        ) -> Self {
            let store = Arc::new(RecordingControlEffectStore::default());
            let lifecycle_run_repo = Arc::new(MemoryLifecycleRunRepository::default());
            let lifecycle_agent_repo = Arc::new(MemoryLifecycleAgentRepository::default());
            let project_agent_repo = Arc::new(MemoryProjectAgentRepository::default());
            let agent_frame_repo = Arc::new(MemoryAgentFrameRepository::default());
            let execution_anchor_repo =
                Arc::new(MemoryRuntimeSessionExecutionAnchorRepository::default());
            let delivery_binding_repo =
                Arc::new(MemoryAgentRunDeliveryBindingRepository::default());
            let project_backend_access_repo =
                Arc::new(MemoryProjectBackendAccessRepository::default());
            let command_receipt_repo = Arc::new(MemoryAgentRunCommandReceiptRepository::default());
            let mailbox_repo = Arc::new(MemoryAgentRunMailboxRepository::default());
            let session_core = SessionCoreService::new(Arc::new(TestCorePort));
            let session_control = SessionControlService::new(Arc::new(TestControlPort));
            let session_eventing = SessionEventingService::new(Arc::new(TestEventingPort));
            let session_launch = SessionLaunchService::new(Arc::new(TestLaunchPort));
            let terminal_convergence_deps = AgentRunTerminalConvergenceDeps {
                lifecycle_run_repo: lifecycle_run_repo.clone(),
                lifecycle_agent_repo: lifecycle_agent_repo.clone(),
                project_agent_repo: project_agent_repo.clone(),
                agent_frame_repo: agent_frame_repo.clone(),
                execution_anchor_repo: execution_anchor_repo.clone(),
                delivery_binding_repo: delivery_binding_repo.clone(),
                project_backend_access_repo: project_backend_access_repo.clone(),
                command_receipt_repo: command_receipt_repo.clone(),
                mailbox_repo: mailbox_repo.clone(),
                project_projection_notifications: None,
            };
            let mailbox_runtime =
                AgentRunMailboxRuntimeAdapter::new(AgentRunMailboxRuntimeBoundaryDeps {
                    lifecycle_run_repo,
                    lifecycle_agent_repo,
                    project_agent_repo,
                    agent_frame_repo,
                    execution_anchor_repo,
                    delivery_binding_repo,
                    project_backend_access_repo,
                    command_receipt_repo,
                    mailbox_repo,
                    session_core: session_core.clone(),
                    session_control: session_control.clone(),
                    session_eventing: session_eventing.clone(),
                    session_launch: Arc::new(session_launch.clone()),
                });
            let service = AgentRunControlEffectService::new(AgentRunControlEffectDeps {
                control_effect_store: store.clone(),
                terminal_convergence_deps,
                session_core,
                session_control,
                session_eventing,
                session_launch,
                mailbox_runtime,
                terminal_hook_trigger_port: Arc::new(StaticTerminalHookTriggerPort { effects }),
                wait_producer_terminal_port: Arc::new(NoopWaitProducerTerminalPort),
                lifecycle_terminal_port: Arc::new(NoopLifecycleTerminalPort),
                hook_effect_handler_registry: Arc::new(RwLock::new(Some(registry))),
            });
            Self { service, store }
        }
    }

    fn test_effect() -> HookEffect {
        HookEffect {
            kind: "test:effect".to_string(),
            payload: serde_json::json!({ "value": 1 }),
        }
    }

    fn terminal_input(
        post_turn_handler: Option<Arc<dyn AgentRunPostTurnHandler>>,
    ) -> AgentRunTerminalControlInput {
        AgentRunTerminalControlInput {
            delivery_runtime_session_id: "session-1".to_string(),
            turn_id: "turn-1".to_string(),
            terminal_event_seq: 42,
            terminal_state: "completed".to_string(),
            terminal_message: None,
            terminal_diagnostic: None,
            terminal_hook_context: Some(AgentRunTerminalHookContext {
                hook_runtime: Arc::new(TestHookRuntime {
                    session_id: "session-1".to_string(),
                    target: agentdash_spi::hooks::HookControlTarget {
                        run_id: Uuid::new_v4(),
                        agent_id: Uuid::new_v4(),
                        frame_id: Uuid::new_v4(),
                    },
                }),
                post_turn_handler,
                source: SourceInfo {
                    connector_id: "test".to_string(),
                    connector_type: "unit".to_string(),
                    executor_id: None,
                },
            }),
            effect_mode: AgentRunTerminalControlEffectMode::ImmediateAll,
        }
    }

    #[tokio::test]
    async fn terminal_delivery_effect_payload_preserves_runtime_provider_diagnostic() {
        let fixture =
            ControlEffectFixture::new(Vec::new(), Arc::new(EmptyAgentRunHookEffectHandlerRegistry));
        let mut input = terminal_input(None);
        input.terminal_state = "failed".to_string();
        input.terminal_message = Some("provider failed".to_string());
        input.terminal_diagnostic = Some(RuntimeTerminalDiagnostic {
            kind: "provider".to_string(),
            code: Some("invalid_request".to_string()),
            http_status: Some(400),
            provider: Some("Example LLM".to_string()),
            model: Some("example-chat-large".to_string()),
            message: "request rejected by provider".to_string(),
            retryable: false,
        });

        fixture
            .service
            .observe_runtime_terminal(input)
            .await
            .expect("runtime terminal control effects");

        let records = fixture
            .store
            .records_by_kind(AgentRunControlEffectKind::AgentRunDeliveryConvergence)
            .await;
        assert_eq!(records.len(), 1);
        assert_eq!(
            records[0].payload["terminal_diagnostic"]["kind"],
            serde_json::json!("provider")
        );
        assert_eq!(
            records[0].payload["terminal_diagnostic"]["code"],
            serde_json::json!("invalid_request")
        );
        assert_eq!(
            records[0].payload["terminal_diagnostic"]["http_status"],
            serde_json::json!(400)
        );
        assert_eq!(
            records[0].payload["terminal_diagnostic"]["provider"],
            serde_json::json!("Example LLM")
        );
        assert_eq!(
            records[0].payload["terminal_diagnostic"]["model"],
            serde_json::json!("example-chat-large")
        );
        assert_eq!(
            records[0].payload["terminal_diagnostic"]["retryable"],
            serde_json::json!(false)
        );
    }

    async fn insert_durable_hook_effect_row(
        store: &RecordingControlEffectStore,
    ) -> AgentRunControlEffectRecord {
        store
            .insert_or_get_control_effect(NewAgentRunControlEffectRecord {
                dedup_key: "runtime_terminal:session-1:turn-1:42:hook_effects:test".to_string(),
                run_id: None,
                agent_id: None,
                frame_id: None,
                delivery_runtime_session_id: Some("session-1".to_string()),
                turn_id: "turn-1".to_string(),
                terminal_event_seq: 42,
                effect_kind: AgentRunControlEffectKind::HookEffects,
                payload: serde_json::json!({
                    "effects": [test_effect()],
                    "handler": { "kind": "durable-test-handler" },
                    "supported_effect_kinds": ["test:effect"],
                }),
            })
            .await
            .expect("insert hook effect row")
    }

    async fn insert_control_effect_row(
        store: &RecordingControlEffectStore,
        effect_kind: AgentRunControlEffectKind,
        discriminator: &str,
        payload: serde_json::Value,
    ) -> AgentRunControlEffectRecord {
        store
            .insert_or_get_control_effect(NewAgentRunControlEffectRecord {
                dedup_key: format!(
                    "runtime_terminal:session-phase:turn-1:42:{}:{discriminator}",
                    effect_kind.as_str()
                ),
                run_id: None,
                agent_id: None,
                frame_id: None,
                delivery_runtime_session_id: Some("session-phase".to_string()),
                turn_id: "turn-1".to_string(),
                terminal_event_seq: 42,
                effect_kind,
                payload,
            })
            .await
            .expect("insert control effect row")
    }

    #[tokio::test]
    async fn hook_handler_error_marks_effect_failed_then_replay_succeeds() {
        let handler = Arc::new(TestPostTurnHandler {
            durable_identity: Some(serde_json::json!({ "kind": "durable-test-handler" })),
            failures_remaining: Mutex::new(1),
            executed: Mutex::new(0),
        });
        let registry = Arc::new(StaticHookEffectHandlerRegistry {
            handler: handler.clone(),
        });
        let fixture = ControlEffectFixture::new(vec![test_effect()], registry);

        let error = fixture
            .service
            .observe_runtime_terminal(terminal_input(Some(handler.clone())))
            .await
            .expect_err("first hook effect execution should fail");
        assert!(error.contains("forced hook effect failure"));

        let hook_records = fixture
            .store
            .records_by_kind(AgentRunControlEffectKind::HookEffects)
            .await;
        assert_eq!(hook_records.len(), 1);
        assert_eq!(hook_records[0].status, AgentRunControlEffectStatus::Failed);
        assert_eq!(
            hook_records[0].last_error.as_deref(),
            Some("forced hook effect failure")
        );

        let replayed = fixture
            .service
            .replay_control_effect_outbox(10)
            .await
            .expect("failed hook effect should replay");
        assert_eq!(replayed, 1);

        let hook_records = fixture
            .store
            .records_by_kind(AgentRunControlEffectKind::HookEffects)
            .await;
        assert_eq!(
            hook_records[0].status,
            AgentRunControlEffectStatus::Succeeded
        );
        assert_eq!(*handler.executed.lock().await, 2);
    }

    #[tokio::test]
    async fn unregistered_durable_handler_replay_fails_then_dead_letters() {
        let fixture =
            ControlEffectFixture::new(Vec::new(), Arc::new(EmptyAgentRunHookEffectHandlerRegistry));
        let record = insert_durable_hook_effect_row(&fixture.store).await;

        for _ in 0..3 {
            let replayed = fixture
                .service
                .replay_control_effect_outbox(10)
                .await
                .expect("replay should claim hook effect");
            assert_eq!(replayed, 1);
        }

        let hook_records = fixture
            .store
            .records_by_kind(AgentRunControlEffectKind::HookEffects)
            .await;
        assert_eq!(hook_records.len(), 1);
        assert_eq!(hook_records[0].id, record.id);
        assert_eq!(
            hook_records[0].status,
            AgentRunControlEffectStatus::DeadLetter
        );
        assert!(
            hook_records[0]
                .last_error
                .as_deref()
                .is_some_and(|error| error.contains("未注册 durable AgentRun hook effect handler"))
        );
    }

    #[tokio::test]
    async fn hook_output_without_durable_identity_does_not_write_hook_effect_outbox() {
        let handler = Arc::new(TestPostTurnHandler {
            durable_identity: None,
            failures_remaining: Mutex::new(0),
            executed: Mutex::new(0),
        });
        let fixture = ControlEffectFixture::new(
            vec![test_effect()],
            Arc::new(EmptyAgentRunHookEffectHandlerRegistry),
        );

        fixture
            .service
            .observe_runtime_terminal(terminal_input(Some(handler.clone())))
            .await
            .expect("non-durable hook effects may execute immediately");

        let hook_records = fixture
            .store
            .records_by_kind(AgentRunControlEffectKind::HookEffects)
            .await;
        assert!(hook_records.is_empty());
        assert_eq!(*handler.executed.lock().await, 1);
    }

    #[tokio::test]
    async fn phased_replay_claims_delivery_before_terminal_side_effects() {
        let fixture =
            ControlEffectFixture::new(Vec::new(), Arc::new(EmptyAgentRunHookEffectHandlerRegistry));
        let delivery = insert_control_effect_row(
            &fixture.store,
            AgentRunControlEffectKind::AgentRunDeliveryConvergence,
            "delivery",
            serde_json::json!({
                "terminal_state": "interrupted",
                "terminal_message": "startup recovery",
                "terminal_diagnostic": null,
            }),
        )
        .await;
        let wait = insert_control_effect_row(
            &fixture.store,
            AgentRunControlEffectKind::WaitProducerTerminalConvergence,
            "wait",
            serde_json::json!({
                "run_id": Uuid::new_v4(),
                "agent_id": Uuid::new_v4(),
                "frame_id": null,
                "terminal_state": "interrupted",
                "terminal_message": "startup recovery",
                "terminal_diagnostic": null,
                "producer_last_message": null,
                "source_turn_id": "turn-1",
                "delivery_trace_ref": "session-phase",
            }),
        )
        .await;

        let delivery_count = fixture
            .service
            .replay_control_effect_outbox_phase(
                AgentRunControlEffectReplayPhase::DeliveryConvergence,
                10,
            )
            .await
            .expect("delivery phase replay");
        assert_eq!(delivery_count, 1);

        let records = fixture.store.records().await;
        let delivery_record = records
            .iter()
            .find(|record| record.id == delivery.id)
            .expect("delivery record");
        let wait_record = records
            .iter()
            .find(|record| record.id == wait.id)
            .expect("wait record");
        assert_eq!(
            delivery_record.status,
            AgentRunControlEffectStatus::Succeeded
        );
        assert_eq!(wait_record.status, AgentRunControlEffectStatus::Pending);

        let side_effect_count = fixture
            .service
            .replay_control_effect_outbox_phase(
                AgentRunControlEffectReplayPhase::TerminalSideEffects,
                10,
            )
            .await
            .expect("side-effect phase replay");
        assert_eq!(side_effect_count, 1);
        let wait_record = fixture
            .store
            .records_by_kind(AgentRunControlEffectKind::WaitProducerTerminalConvergence)
            .await
            .into_iter()
            .next()
            .expect("wait record");
        assert_eq!(wait_record.status, AgentRunControlEffectStatus::Succeeded);
    }

    #[tokio::test]
    async fn unphased_replay_does_not_claim_side_effects_when_delivery_batch_is_full() {
        let fixture =
            ControlEffectFixture::new(Vec::new(), Arc::new(EmptyAgentRunHookEffectHandlerRegistry));
        let first_delivery = insert_control_effect_row(
            &fixture.store,
            AgentRunControlEffectKind::AgentRunDeliveryConvergence,
            "delivery-1",
            serde_json::json!({
                "terminal_state": "interrupted",
                "terminal_message": "startup recovery",
                "terminal_diagnostic": null,
            }),
        )
        .await;
        let second_delivery = insert_control_effect_row(
            &fixture.store,
            AgentRunControlEffectKind::AgentRunDeliveryConvergence,
            "delivery-2",
            serde_json::json!({
                "terminal_state": "interrupted",
                "terminal_message": "startup recovery",
                "terminal_diagnostic": null,
            }),
        )
        .await;
        let wait = insert_control_effect_row(
            &fixture.store,
            AgentRunControlEffectKind::WaitProducerTerminalConvergence,
            "wait",
            serde_json::json!({
                "run_id": Uuid::new_v4(),
                "agent_id": Uuid::new_v4(),
                "frame_id": null,
                "terminal_state": "interrupted",
                "terminal_message": "startup recovery",
                "terminal_diagnostic": null,
                "producer_last_message": null,
                "source_turn_id": "turn-1",
                "delivery_trace_ref": "session-phase",
            }),
        )
        .await;

        let attempted = fixture
            .service
            .replay_control_effect_outbox(1)
            .await
            .expect("unphased replay");

        assert_eq!(attempted, 1);
        let records = fixture.store.records().await;
        let first_delivery_status = records
            .iter()
            .find(|record| record.id == first_delivery.id)
            .map(|record| record.status)
            .expect("first delivery");
        let second_delivery_status = records
            .iter()
            .find(|record| record.id == second_delivery.id)
            .map(|record| record.status)
            .expect("second delivery");
        let wait_status = records
            .iter()
            .find(|record| record.id == wait.id)
            .map(|record| record.status)
            .expect("wait");

        assert_eq!(
            first_delivery_status,
            AgentRunControlEffectStatus::Succeeded
        );
        assert_eq!(second_delivery_status, AgentRunControlEffectStatus::Pending);
        assert_eq!(wait_status, AgentRunControlEffectStatus::Pending);
    }
}
