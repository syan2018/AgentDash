use agentdash_diagnostics::{Subsystem, diag};
use std::sync::Arc;

use agentdash_agent_protocol::{
    BackboneEnvelope, BackboneEvent, PlatformEvent, SourceInfo, UserInputBlock,
};
use agentdash_application_ports::runtime_session_live::{
    RuntimeSessionLivePortError, RuntimeSessionMailboxAutoResumeRequest,
    RuntimeSessionMailboxRuntimePort,
};
use agentdash_domain::agent_run_mailbox::{
    AgentRunMailboxRepository, ConsumptionBarrier, MailboxDrainMode, MailboxSourceIdentity,
    SteeringStopEffect,
};
use agentdash_domain::workflow::{
    AgentFrameRepository, AgentRunCommandReceiptRepository, LifecycleAgentRepository,
    LifecycleRunRepository, RuntimeSessionExecutionAnchorRepository,
};
use agentdash_spi::{
    AfterToolCallEffects, AfterToolCallInput, AfterTurnInput, AgentMessage, AgentRuntimeDelegate,
    AgentRuntimeError, BeforeProviderRequestInput, BeforeStopInput, BeforeToolCallInput,
    CompactionFailureInput, CompactionParams, CompactionResult, DynAgentRuntimeDelegate,
    EvaluateCompactionInput, StopDecision, ToolCallDecision, TransformContextInput,
    TransformContextOutput, TurnControlDecision,
};
use async_trait::async_trait;
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;

use crate::agent_run::runtime_session_boundary::{
    SessionControlService, SessionCoreService, SessionEventingService, SessionLaunchService,
};
use crate::error::WorkflowApplicationError;
use uuid::Uuid;

use super::mailbox::{AgentRunMailboxScheduleTrigger, AgentRunMailboxService};

#[derive(Clone)]
pub struct AgentRunMailboxRuntimeBoundaryDeps {
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

#[derive(Clone)]
pub struct AgentRunMailboxRuntimeAdapter {
    deps: Arc<AgentRunMailboxRuntimeBoundaryDeps>,
}

pub struct AgentRunMailboxAutoResumeRequest {
    pub session_id: String,
    pub effect_id: Uuid,
    pub source_turn_id: String,
    pub terminal_event_seq: u64,
    pub input: Vec<UserInputBlock>,
}

impl AgentRunMailboxRuntimeAdapter {
    pub fn new(deps: AgentRunMailboxRuntimeBoundaryDeps) -> Self {
        Self {
            deps: Arc::new(deps),
        }
    }

    pub fn runtime_delegate(
        &self,
        runtime_session_id: String,
        inner: Option<DynAgentRuntimeDelegate>,
    ) -> DynAgentRuntimeDelegate {
        Arc::new(AgentRunMailboxRuntimeDelegate::new(
            runtime_session_id,
            inner,
            self.deps.clone(),
        ))
    }

    fn mailbox_service(&self) -> AgentRunMailboxService<'_> {
        mailbox_service_from_deps(&self.deps)
    }

    pub async fn accept_hook_auto_resume_effect(
        &self,
        request: AgentRunMailboxAutoResumeRequest,
    ) -> Result<bool, String> {
        match self
            .mailbox_service()
            .accept_hook_auto_resume_effect(
                &request.session_id,
                request.effect_id,
                request.source_turn_id,
                request.terminal_event_seq,
                request.input,
            )
            .await
        {
            Ok(_) => Ok(true),
            Err(WorkflowApplicationError::NotFound(_)) => Ok(false),
            Err(error) => Err(error.to_string()),
        }
    }
}

pub fn mailbox_runtime_port(
    deps: AgentRunMailboxRuntimeBoundaryDeps,
) -> Arc<dyn RuntimeSessionMailboxRuntimePort> {
    Arc::new(AgentRunMailboxRuntimeAdapter::new(deps))
}

#[async_trait]
impl RuntimeSessionMailboxRuntimePort for AgentRunMailboxRuntimeAdapter {
    fn runtime_delegate(
        &self,
        runtime_session_id: String,
        inner: Option<DynAgentRuntimeDelegate>,
    ) -> DynAgentRuntimeDelegate {
        AgentRunMailboxRuntimeAdapter::runtime_delegate(self, runtime_session_id, inner)
    }

    async fn accept_hook_auto_resume_effect(
        &self,
        request: RuntimeSessionMailboxAutoResumeRequest,
    ) -> Result<bool, RuntimeSessionLivePortError> {
        AgentRunMailboxRuntimeAdapter::accept_hook_auto_resume_effect(
            self,
            AgentRunMailboxAutoResumeRequest {
                session_id: request.session_id,
                effect_id: request.effect_id,
                source_turn_id: request.source_turn_id,
                terminal_event_seq: request.terminal_event_seq,
                input: request.input,
            },
        )
        .await
        .map_err(RuntimeSessionLivePortError::failed)
    }
}

struct AgentRunMailboxRuntimeDelegate {
    runtime_session_id: String,
    inner: Option<Arc<dyn AgentRuntimeDelegate>>,
    deps: Arc<AgentRunMailboxRuntimeBoundaryDeps>,
}

impl AgentRunMailboxRuntimeDelegate {
    fn new(
        runtime_session_id: String,
        inner: Option<Arc<dyn AgentRuntimeDelegate>>,
        deps: Arc<AgentRunMailboxRuntimeBoundaryDeps>,
    ) -> Self {
        Self {
            runtime_session_id,
            inner,
            deps,
        }
    }

    fn boundary_stage(&self) -> MailboxBoundaryStage<'_> {
        MailboxBoundaryStage {
            runtime_session_id: &self.runtime_session_id,
            deps: self.deps.as_ref(),
        }
    }

    fn hook_router(&self) -> HookDeliveryRouter<'_> {
        HookDeliveryRouter {
            runtime_session_id: &self.runtime_session_id,
            deps: self.deps.as_ref(),
        }
    }
}

struct MailboxBoundaryStage<'a> {
    runtime_session_id: &'a str,
    deps: &'a AgentRunMailboxRuntimeBoundaryDeps,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RuntimeSessionMailboxAdapterRef {
    runtime_session_id: String,
}

fn runtime_session_mailbox_adapter_ref(
    runtime_session_id: &str,
) -> RuntimeSessionMailboxAdapterRef {
    RuntimeSessionMailboxAdapterRef {
        runtime_session_id: runtime_session_id.to_string(),
    }
}

impl MailboxBoundaryStage<'_> {
    fn mailbox_service(&self) -> AgentRunMailboxService<'_> {
        mailbox_service_from_deps(self.deps)
    }

    async fn schedule_agent_loop_turn_boundary(&self) {
        let adapter_ref = runtime_session_mailbox_adapter_ref(self.runtime_session_id);
        let result = self
            .mailbox_service()
            .schedule_for_runtime_session(
                &adapter_ref.runtime_session_id,
                AgentRunMailboxScheduleTrigger::AgentLoopTurnBoundary,
            )
            .await;
        match result {
            Ok(outcomes) if !outcomes.is_empty() => {
                self.emit_mailbox_state_changed("steer_consumed").await;
            }
            Err(error) => {
                if !matches!(error, WorkflowApplicationError::NotFound(_)) {
                    diag!(Warn, Subsystem::AgentRun,

                        runtime_session_id = %self.runtime_session_id,
                        "AgentRun mailbox AgentLoopTurnBoundary 调度失败: {error}"
                    );
                }
            }
            _ => {}
        }
    }

    async fn emit_mailbox_state_changed(&self, reason: &str) {
        let envelope = BackboneEnvelope::new(
            BackboneEvent::Platform(PlatformEvent::MailboxStateChanged {
                reason: reason.to_string(),
            }),
            self.runtime_session_id,
            SourceInfo {
                connector_id: "mailbox".to_string(),
                connector_type: "platform".to_string(),
                executor_id: None,
            },
        );
        let _ = self
            .deps
            .session_eventing
            .persist_notification(self.runtime_session_id, envelope)
            .await;
    }

    async fn drain_agent_run_turn_boundary(&self) -> Result<Vec<AgentMessage>, AgentRuntimeError> {
        let adapter_ref = runtime_session_mailbox_adapter_ref(self.runtime_session_id);
        match self
            .mailbox_service()
            .drain_agent_run_turn_boundary_for_delegate(&adapter_ref.runtime_session_id)
            .await
        {
            Ok(messages) => Ok(messages),
            Err(WorkflowApplicationError::NotFound(_)) => Ok(Vec::new()),
            Err(error) => Err(AgentRuntimeError::Runtime(error.to_string())),
        }
    }
}

struct HookDeliveryRouter<'a> {
    runtime_session_id: &'a str,
    deps: &'a AgentRunMailboxRuntimeBoundaryDeps,
}

impl HookDeliveryRouter<'_> {
    fn mailbox_service(&self) -> AgentRunMailboxService<'_> {
        mailbox_service_from_deps(self.deps)
    }

    async fn route_hook_delivery_messages(
        &self,
        source: MailboxSourceIdentity,
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
                self.runtime_session_id,
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
            Err(WorkflowApplicationError::NotFound(message)) => {
                route_hook_delivery_not_found(
                    self.deps.execution_anchor_repo.as_ref(),
                    self.runtime_session_id,
                    direct_fallback,
                    message,
                )
                .await
            }
            Err(error) => Err(AgentRuntimeError::Runtime(error.to_string())),
        }
    }
}

fn mailbox_service_from_deps(
    deps: &AgentRunMailboxRuntimeBoundaryDeps,
) -> AgentRunMailboxService<'_> {
    AgentRunMailboxService::new(
        deps.lifecycle_run_repo.as_ref(),
        deps.lifecycle_agent_repo.as_ref(),
        deps.agent_frame_repo.as_ref(),
        deps.execution_anchor_repo.as_ref(),
        deps.command_receipt_repo.as_ref(),
        deps.mailbox_repo.as_ref(),
        deps.session_core.clone(),
        deps.session_control.clone(),
        deps.session_eventing.clone(),
        (*deps.session_launch).clone(),
    )
}

#[derive(Debug, Default)]
struct HookDeliveryRouting {
    direct_messages: Vec<AgentMessage>,
}

async fn route_hook_delivery_not_found(
    execution_anchor_repo: &dyn RuntimeSessionExecutionAnchorRepository,
    runtime_session_id: &str,
    direct_fallback: Vec<AgentMessage>,
    not_found_message: String,
) -> Result<HookDeliveryRouting, AgentRuntimeError> {
    match execution_anchor_repo
        .find_by_session(runtime_session_id)
        .await
    {
        Ok(Some(_)) => Err(AgentRuntimeError::Runtime(format!(
            "AgentRun mailbox hook delivery target missing for anchored runtime_session {runtime_session_id}: {not_found_message}"
        ))),
        Ok(None) => Ok(HookDeliveryRouting {
            direct_messages: direct_fallback,
        }),
        Err(error) => Err(AgentRuntimeError::Runtime(error.to_string())),
    }
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
            None => Ok(preserve_transform_context(input)),
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
        let hook_router = self.hook_router();
        let steering_routing = hook_router
            .route_hook_delivery_messages(
                MailboxSourceIdentity::hook_after_turn(),
                ConsumptionBarrier::AgentLoopTurnBoundary,
                SteeringStopEffect::None,
                MailboxDrainMode::All,
                &source_event_key,
                steering,
            )
            .await?;
        let follow_up_source_event_key = format!("after_turn_follow_up:{source_event_key}");
        let follow_up_routing = hook_router
            .route_hook_delivery_messages(
                MailboxSourceIdentity::hook_before_stop(),
                ConsumptionBarrier::AgentRunTurnBoundary,
                SteeringStopEffect::ContinueOnStop,
                MailboxDrainMode::All,
                &follow_up_source_event_key,
                follow_up,
            )
            .await?;
        self.boundary_stage()
            .schedule_agent_loop_turn_boundary()
            .await;
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
                let mailbox_messages = self
                    .boundary_stage()
                    .drain_agent_run_turn_boundary()
                    .await?;
                Ok(stop_after_mailbox_boundary_drain(mailbox_messages))
            }
            StopDecision::Continue {
                mut steering,
                mut follow_up,
                reason,
                allow_empty,
            } => {
                steering.append(&mut follow_up);
                let routing = self
                    .hook_router()
                    .route_hook_delivery_messages(
                        MailboxSourceIdentity::hook_before_stop(),
                        ConsumptionBarrier::AgentRunTurnBoundary,
                        SteeringStopEffect::ContinueOnStop,
                        MailboxDrainMode::All,
                        &source_event_key,
                        steering,
                    )
                    .await?;
                let mut steering = routing.direct_messages;
                let mut mailbox_messages = self
                    .boundary_stage()
                    .drain_agent_run_turn_boundary()
                    .await?;
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

fn preserve_transform_context(input: TransformContextInput) -> TransformContextOutput {
    TransformContextOutput {
        steering_messages: input.context.messages,
        blocked: None,
    }
}

fn stop_after_mailbox_boundary_drain(mailbox_messages: Vec<AgentMessage>) -> StopDecision {
    if mailbox_messages.is_empty() {
        StopDecision::Stop
    } else {
        StopDecision::Continue {
            steering: mailbox_messages,
            follow_up: Vec::new(),
            reason: Some("agent_run_mailbox_boundary".to_string()),
            allow_empty: false,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::MemoryRuntimeSessionExecutionAnchorRepository;
    use agentdash_domain::workflow::RuntimeSessionExecutionAnchor;
    use agentdash_spi::AgentContext;

    #[test]
    fn no_inner_transform_context_preserves_provider_visible_messages() {
        let messages = vec![
            AgentMessage::user("用户输入"),
            AgentMessage::assistant("已有上下文"),
        ];

        let output = preserve_transform_context(TransformContextInput {
            context: AgentContext {
                system_prompt: "system".to_string(),
                messages: messages.clone(),
                message_refs: vec![],
                tools: vec![],
            },
        });

        assert_eq!(output.steering_messages, messages);
        assert!(output.blocked.is_none());
    }

    #[test]
    fn mailbox_boundary_drain_requires_non_empty_continue() {
        assert!(matches!(
            stop_after_mailbox_boundary_drain(Vec::new()),
            StopDecision::Stop
        ));

        let drained = vec![AgentMessage::user("边界消息")];
        match stop_after_mailbox_boundary_drain(drained.clone()) {
            StopDecision::Continue {
                steering,
                follow_up,
                reason,
                allow_empty,
            } => {
                assert_eq!(steering, drained);
                assert!(follow_up.is_empty());
                assert_eq!(reason.as_deref(), Some("agent_run_mailbox_boundary"));
                assert!(!allow_empty);
            }
            StopDecision::Stop => panic!("边界消息不应被自然停止吞掉"),
        }
    }

    #[test]
    fn runtime_delegate_keeps_session_id_as_adapter_ref() {
        let adapter_ref = runtime_session_mailbox_adapter_ref("runtime-session-1");

        assert_eq!(
            adapter_ref,
            RuntimeSessionMailboxAdapterRef {
                runtime_session_id: "runtime-session-1".to_string(),
            }
        );
    }

    #[tokio::test]
    async fn hook_not_found_on_unbound_trace_keeps_direct_messages() {
        let anchor_repo = MemoryRuntimeSessionExecutionAnchorRepository::default();
        let messages = vec![AgentMessage::user("legacy direct")];

        let routing = route_hook_delivery_not_found(
            &anchor_repo,
            "unbound-runtime-session",
            messages.clone(),
            "runtime_session 缺少 RuntimeSessionExecutionAnchor".to_string(),
        )
        .await
        .expect("unbound trace can use direct fallback");

        assert_eq!(routing.direct_messages, messages);
    }

    #[tokio::test]
    async fn hook_not_found_on_anchored_runtime_errors_without_direct_messages() {
        let anchor_repo = MemoryRuntimeSessionExecutionAnchorRepository::default();
        let runtime_session_id = "anchored-runtime-session";
        anchor_repo
            .upsert(&RuntimeSessionExecutionAnchor::new_dispatch(
                runtime_session_id,
                uuid::Uuid::new_v4(),
                uuid::Uuid::new_v4(),
                uuid::Uuid::new_v4(),
            ))
            .await
            .expect("seed runtime anchor");

        let error = route_hook_delivery_not_found(
            &anchor_repo,
            runtime_session_id,
            vec![AgentMessage::user("must not inject")],
            "agent_frame 不存在".to_string(),
        )
        .await
        .expect_err("anchored AgentRun must not fall back to direct messages");

        match error {
            AgentRuntimeError::Runtime(message) => {
                assert!(message.contains("anchored runtime_session"));
                assert!(message.contains(runtime_session_id));
            }
        }
    }
}
