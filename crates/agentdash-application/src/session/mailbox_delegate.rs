use std::sync::Arc;

use agentdash_domain::workflow::{
    AgentFrameRepository, AgentRunCommandReceiptRepository, AgentRunMailboxRepository,
    ConsumptionBarrier, LifecycleAgentRepository, LifecycleRunRepository, MailboxDrainMode,
    MailboxMessageSource, RuntimeSessionExecutionAnchorRepository, SteeringStopEffect,
};
use agentdash_spi::{
    AfterToolCallEffects, AfterToolCallInput, AfterTurnInput, AgentMessage, AgentRuntimeDelegate,
    AgentRuntimeError, BeforeProviderRequestInput, BeforeStopInput, BeforeToolCallInput,
    CompactionFailureInput, CompactionParams, CompactionResult, EvaluateCompactionInput,
    StopDecision, ToolCallDecision, TransformContextInput, TransformContextOutput,
    TurnControlDecision,
};
use async_trait::async_trait;
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;

use crate::session::{
    SessionControlService, SessionCoreService, SessionEventingService, SessionLaunchService,
};
use crate::workflow::{
    AgentRunMailboxScheduleTrigger, AgentRunMailboxService, WorkflowApplicationError,
};

#[derive(Clone)]
pub(crate) struct AgentRunMailboxRuntimeBoundaryDeps {
    pub lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
    pub lifecycle_agent_repo: Arc<dyn LifecycleAgentRepository>,
    pub agent_frame_repo: Arc<dyn AgentFrameRepository>,
    pub execution_anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository>,
    pub command_receipt_repo: Arc<dyn AgentRunCommandReceiptRepository>,
    pub mailbox_repo: Arc<dyn AgentRunMailboxRepository>,
    pub session_core: SessionCoreService,
    pub session_control: SessionControlService,
    pub session_eventing: SessionEventingService,
    pub session_launch: Arc<SessionLaunchService>,
}

pub(crate) struct AgentRunMailboxRuntimeDelegate {
    runtime_session_id: String,
    inner: Option<Arc<dyn AgentRuntimeDelegate>>,
    deps: AgentRunMailboxRuntimeBoundaryDeps,
}

impl AgentRunMailboxRuntimeDelegate {
    pub(crate) fn new(
        runtime_session_id: String,
        inner: Option<Arc<dyn AgentRuntimeDelegate>>,
        deps: AgentRunMailboxRuntimeBoundaryDeps,
    ) -> Self {
        Self {
            runtime_session_id,
            inner,
            deps,
        }
    }

    fn mailbox_service(&self) -> AgentRunMailboxService<'_> {
        AgentRunMailboxService::new(
            self.deps.lifecycle_run_repo.as_ref(),
            self.deps.lifecycle_agent_repo.as_ref(),
            self.deps.agent_frame_repo.as_ref(),
            self.deps.execution_anchor_repo.as_ref(),
            self.deps.command_receipt_repo.as_ref(),
            self.deps.mailbox_repo.as_ref(),
            self.deps.session_core.clone(),
            self.deps.session_control.clone(),
            self.deps.session_eventing.clone(),
            (*self.deps.session_launch).clone(),
        )
    }

    async fn schedule_agent_loop_turn_boundary(&self) {
        let result = self
            .mailbox_service()
            .schedule_for_runtime_session(
                &self.runtime_session_id,
                AgentRunMailboxScheduleTrigger::AgentLoopTurnBoundary,
            )
            .await;
        if let Err(error) = result {
            if matches!(error, WorkflowApplicationError::NotFound(_)) {
                return;
            }
            tracing::warn!(
                runtime_session_id = %self.runtime_session_id,
                "AgentRun mailbox AgentLoopTurnBoundary 调度失败: {error}"
            );
        }
    }

    async fn drain_agent_run_turn_boundary(&self, phase: &'static str) -> Vec<AgentMessage> {
        match self
            .mailbox_service()
            .drain_agent_run_turn_boundary_for_delegate(&self.runtime_session_id)
            .await
        {
            Ok(messages) => messages,
            Err(WorkflowApplicationError::NotFound(_)) => Vec::new(),
            Err(error) => {
                tracing::warn!(
                    runtime_session_id = %self.runtime_session_id,
                    phase,
                    error = %error,
                    "AgentRun mailbox delegate drain 失败，跳过本轮停止边界消费"
                );
                Vec::new()
            }
        }
    }

    async fn route_hook_delivery_messages(
        &self,
        source: MailboxMessageSource,
        barrier: ConsumptionBarrier,
        stop_effect: SteeringStopEffect,
        drain_mode: MailboxDrainMode,
        source_event_key: &str,
        messages: Vec<AgentMessage>,
    ) -> Result<HookDeliveryRouting, AgentRuntimeError> {
        if messages.is_empty() {
            return Ok(HookDeliveryRouting::default());
        }
        let direct_fallback = messages.clone();
        match self
            .mailbox_service()
            .accept_hook_steering_messages(
                &self.runtime_session_id,
                source,
                barrier,
                stop_effect,
                drain_mode,
                source_event_key,
                messages,
            )
            .await
        {
            Ok(_) => Ok(HookDeliveryRouting {
                direct_messages: Vec::new(),
            }),
            Err(WorkflowApplicationError::NotFound(_)) => Ok(HookDeliveryRouting {
                direct_messages: direct_fallback,
            }),
            Err(error) => Err(AgentRuntimeError::Runtime(error.to_string())),
        }
    }
}

#[derive(Default)]
struct HookDeliveryRouting {
    direct_messages: Vec<AgentMessage>,
}

#[async_trait]
impl AgentRuntimeDelegate for AgentRunMailboxRuntimeDelegate {
    async fn evaluate_compaction(
        &self,
        input: EvaluateCompactionInput,
        cancel: CancellationToken,
    ) -> Result<Option<CompactionParams>, AgentRuntimeError> {
        match &self.inner {
            Some(inner) => inner.evaluate_compaction(input, cancel).await,
            None => Ok(None),
        }
    }

    async fn after_compaction(
        &self,
        result: CompactionResult,
        cancel: CancellationToken,
    ) -> Result<(), AgentRuntimeError> {
        match &self.inner {
            Some(inner) => inner.after_compaction(result, cancel).await,
            None => Ok(()),
        }
    }

    async fn after_compaction_failed(
        &self,
        input: CompactionFailureInput,
        cancel: CancellationToken,
    ) -> Result<(), AgentRuntimeError> {
        match &self.inner {
            Some(inner) => inner.after_compaction_failed(input, cancel).await,
            None => Ok(()),
        }
    }

    async fn transform_context(
        &self,
        input: TransformContextInput,
        cancel: CancellationToken,
    ) -> Result<TransformContextOutput, AgentRuntimeError> {
        match &self.inner {
            Some(inner) => inner.transform_context(input, cancel).await,
            None => Ok(TransformContextOutput {
                steering_messages: Vec::new(),
                blocked: None,
            }),
        }
    }

    async fn before_tool_call(
        &self,
        input: BeforeToolCallInput,
        cancel: CancellationToken,
    ) -> Result<ToolCallDecision, AgentRuntimeError> {
        match &self.inner {
            Some(inner) => inner.before_tool_call(input, cancel).await,
            None => Ok(ToolCallDecision::Allow),
        }
    }

    async fn after_tool_call(
        &self,
        input: AfterToolCallInput,
        cancel: CancellationToken,
    ) -> Result<AfterToolCallEffects, AgentRuntimeError> {
        match &self.inner {
            Some(inner) => inner.after_tool_call(input, cancel).await,
            None => Ok(AfterToolCallEffects::default()),
        }
    }

    async fn after_turn(
        &self,
        input: AfterTurnInput,
        cancel: CancellationToken,
    ) -> Result<TurnControlDecision, AgentRuntimeError> {
        let source_event_key = after_turn_source_key(&input);
        let mut decision = match &self.inner {
            Some(inner) => inner.after_turn(input, cancel).await?,
            None => TurnControlDecision::default(),
        };
        let steering = std::mem::take(&mut decision.steering);
        let follow_up = std::mem::take(&mut decision.follow_up);
        let steering_routing = self
            .route_hook_delivery_messages(
                MailboxMessageSource::HookAfterTurn,
                ConsumptionBarrier::AgentLoopTurnBoundary,
                SteeringStopEffect::None,
                MailboxDrainMode::All,
                &source_event_key,
                steering,
            )
            .await?;
        let follow_up_source_event_key = format!("after_turn_follow_up:{source_event_key}");
        let follow_up_routing = self
            .route_hook_delivery_messages(
                MailboxMessageSource::HookBeforeStop,
                ConsumptionBarrier::AgentRunTurnBoundary,
                SteeringStopEffect::ContinueOnStop,
                MailboxDrainMode::All,
                &follow_up_source_event_key,
                follow_up,
            )
            .await?;
        self.schedule_agent_loop_turn_boundary().await;
        Ok(TurnControlDecision {
            steering: steering_routing.direct_messages,
            follow_up: follow_up_routing.direct_messages,
            refresh_snapshot: decision.refresh_snapshot,
            diagnostics: decision.diagnostics,
        })
    }

    async fn before_stop(
        &self,
        input: BeforeStopInput,
        cancel: CancellationToken,
    ) -> Result<StopDecision, AgentRuntimeError> {
        let source_event_key = before_stop_source_key(&input);
        let inner_decision = match &self.inner {
            Some(inner) => inner.before_stop(input, cancel).await?,
            None => StopDecision::Stop,
        };
        match inner_decision {
            StopDecision::Stop => {
                let mailbox_messages = self.drain_agent_run_turn_boundary("before_stop_stop").await;
                if mailbox_messages.is_empty() {
                    Ok(StopDecision::Stop)
                } else {
                    Ok(StopDecision::Continue {
                        steering: mailbox_messages,
                        follow_up: Vec::new(),
                        reason: Some("agent_run_mailbox_boundary".to_string()),
                        allow_empty: false,
                    })
                }
            }
            StopDecision::Continue {
                mut steering,
                mut follow_up,
                reason,
                allow_empty,
            } => {
                steering.append(&mut follow_up);
                let routing = self
                    .route_hook_delivery_messages(
                        MailboxMessageSource::HookBeforeStop,
                        ConsumptionBarrier::AgentRunTurnBoundary,
                        SteeringStopEffect::ContinueOnStop,
                        MailboxDrainMode::All,
                        &source_event_key,
                        steering,
                    )
                    .await?;
                let mut steering = routing.direct_messages;
                let mut mailbox_messages = self
                    .drain_agent_run_turn_boundary("before_stop_continue")
                    .await;
                steering.append(&mut mailbox_messages);
                Ok(StopDecision::Continue {
                    steering,
                    follow_up: Vec::new(),
                    reason,
                    allow_empty,
                })
            }
        }
    }

    async fn on_before_provider_request(
        &self,
        input: BeforeProviderRequestInput,
        cancel: CancellationToken,
    ) -> Result<(), AgentRuntimeError> {
        match &self.inner {
            Some(inner) => inner.on_before_provider_request(input, cancel).await,
            None => Ok(()),
        }
    }
}

fn after_turn_source_key(input: &AfterTurnInput) -> String {
    stable_source_digest(serde_json::json!({
        "kind": "after_turn",
        "assistant_message": input.message,
        "tool_results": input.tool_results,
    }))
}

fn before_stop_source_key(input: &BeforeStopInput) -> String {
    stable_source_digest(serde_json::json!({
        "kind": "before_stop",
        "message_count": input.context.messages.len(),
        "last_message": input.context.messages.last(),
    }))
}

fn stable_source_digest(value: serde_json::Value) -> String {
    let bytes = serde_json::to_vec(&value).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}
