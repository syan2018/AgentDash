use std::sync::Arc;

use agentdash_agent_protocol::{BackboneEnvelope, SourceInfo};
use agentdash_agent_runtime::{
    RuntimeStoreError, RuntimeTerminalApplicationEffectClaim,
    RuntimeTerminalApplicationEffectClaimRequest, RuntimeTerminalApplicationEffectOutbox,
    RuntimeWorkerId,
};
use agentdash_application::gate_wait_policy::{
    GateProducerTerminalConvergencePort, GateProducerTerminalConvergenceServiceAdapter,
};
use agentdash_application_agentrun::agent_run::{
    AgentRunJournalQuery, AgentRunJournalService, agent_run_journal_session_id,
};
use agentdash_application_lifecycle::lifecycle::surface::journey::{
    SessionItemContent, SessionItemProjection, SessionItemView, filter_session_items,
    item_summary_for_view, session_item_projections,
};
use agentdash_application_ports::agent_run_control_effect::{
    AgentRunControlEffectPort, AgentRunHookEffectHandlerRegistry, AgentRunTerminalControlInput,
    AgentRunTerminalHookEffectPort, AgentRunWaitProducerTerminalConvergencePort,
};
use agentdash_application_ports::agent_run_runtime::AgentRunRuntimeBindingRepository;
use agentdash_application_workflow::gate::{
    GateProducerTerminalEvent, RuntimeTerminalDiagnostic as GateRuntimeTerminalDiagnostic,
};
use agentdash_domain::workflow::WaitProducerRef;
use agentdash_spi::{HookRuntimeEvaluationQuery, HookTrigger, RuntimeAdapterProvenance};

use crate::bootstrap::agent_runtime_surface::CompiledAgentRunToolRegistry;

#[derive(Clone)]
pub(crate) struct RuntimeTerminalApplicationEffectWorker {
    outbox: Arc<dyn RuntimeTerminalApplicationEffectOutbox>,
    effects: Arc<dyn AgentRunControlEffectPort>,
    owner: RuntimeWorkerId,
    lease_duration_ms: u64,
    batch_limit: u32,
}

#[derive(Clone)]
pub(crate) struct RuntimeWaitProducerTerminalConvergence {
    bindings: Arc<dyn AgentRunRuntimeBindingRepository>,
    convergence: GateProducerTerminalConvergenceServiceAdapter,
    journal: Arc<AgentRunJournalService>,
}

#[derive(Clone)]
pub(crate) struct RuntimeTerminalHookEffects {
    registry: Arc<CompiledAgentRunToolRegistry>,
    handlers: Arc<dyn AgentRunHookEffectHandlerRegistry>,
}

impl RuntimeTerminalHookEffects {
    pub(crate) fn new(
        registry: Arc<CompiledAgentRunToolRegistry>,
        handlers: Arc<dyn AgentRunHookEffectHandlerRegistry>,
    ) -> Self {
        Self { registry, handlers }
    }
}

#[async_trait::async_trait]
impl AgentRunTerminalHookEffectPort for RuntimeTerminalHookEffects {
    async fn execute_terminal_hooks(
        &self,
        input: &AgentRunTerminalControlInput,
    ) -> Result<(), String> {
        let Some(effect_binding) = input.terminal_hook_effect_binding.as_ref() else {
            return Ok(());
        };
        let source_thread_id = input
            .source_thread_id
            .parse()
            .map_err(|error| format!("invalid terminal hook source thread: {error}"))?;
        let compiled = self
            .registry
            .get_applied_surface(
                &input.binding_id,
                input.driver_generation,
                &input.runtime_thread_id,
                &source_thread_id,
                input.surface_revision,
                &input.surface_digest,
            )
            .await
            .ok_or_else(|| {
                format!(
                    "terminal hook effect targets a stale Runtime surface {}@{}",
                    input.binding_id, input.surface_revision.0
                )
            })?;
        if compiled.terminal_hook_effect_binding.as_ref() != Some(effect_binding) {
            return Err("terminal hook effect binding does not match the compiled surface".into());
        }
        let handler = self
            .handlers
            .handler_for(&input.presentation_thread_id, &effect_binding.handler)
            .await?
            .ok_or_else(|| {
                format!(
                    "terminal hook handler {}:{}@{} is not registered",
                    effect_binding.handler.handler_type,
                    effect_binding.handler.handler_id,
                    effect_binding.handler.revision.0
                )
            })?;
        if handler.durable_effect_handler().as_ref() != Some(&effect_binding.handler) {
            return Err(
                "terminal hook handler durable identity does not match Runtime binding".into(),
            );
        }
        let resolution = compiled
            .hook_runtime
            .evaluate_from_provenance(HookRuntimeEvaluationQuery {
                provenance: RuntimeAdapterProvenance::runtime_session(
                    input.presentation_thread_id.to_string(),
                    input.source_turn_id.clone(),
                    "canonical_runtime_terminal_hook",
                ),
                trigger: HookTrigger::SessionTerminal,
                tool_name: None,
                tool_call_id: None,
                subagent_type: None,
                snapshot: None,
                payload: Some(serde_json::json!({
                    "terminal_state": terminal_state(input.terminal),
                    "message": input.message,
                    "diagnostic": input.diagnostic,
                    "terminal_event_sequence": input.terminal_event_sequence.0,
                })),
                token_stats: None,
            })
            .await
            .map_err(|error| error.to_string())?;
        for effect in &resolution.effects {
            let kind =
                agentdash_agent_runtime_contract::RuntimeHookEffectKind::new(effect.kind.clone())
                    .map_err(|error| error.to_string())?;
            if !effect_binding.supported_effect_kinds.contains(&kind)
                || !handler
                    .supported_effect_kinds()
                    .contains(&effect.kind.as_str())
            {
                return Err(format!(
                    "terminal hook effect kind {} is not owned by the bound handler",
                    effect.kind
                ));
            }
        }
        if resolution.effects.is_empty() {
            return Ok(());
        }
        let turn_id = input
            .source_turn_id
            .as_deref()
            .unwrap_or(input.presentation_turn_id.as_str());
        handler
            .execute_effects(
                &format!("{}:terminal_hook_effects", input.effect_id),
                input.presentation_thread_id.as_str(),
                turn_id,
                &resolution.effects,
            )
            .await
    }
}

impl RuntimeWaitProducerTerminalConvergence {
    pub(crate) fn new(
        bindings: Arc<dyn AgentRunRuntimeBindingRepository>,
        convergence: GateProducerTerminalConvergenceServiceAdapter,
        journal: Arc<AgentRunJournalService>,
    ) -> Self {
        Self {
            bindings,
            convergence,
            journal,
        }
    }
}

#[async_trait::async_trait]
impl AgentRunWaitProducerTerminalConvergencePort for RuntimeWaitProducerTerminalConvergence {
    async fn converge_wait_producer_terminal(
        &self,
        input: &AgentRunTerminalControlInput,
    ) -> Result<(), String> {
        let binding = self
            .bindings
            .load_by_thread_id(&input.runtime_thread_id)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| {
                format!(
                    "terminal wait-producer effect targets an unbound Runtime thread {}",
                    input.runtime_thread_id
                )
            })?;
        if binding.presentation_thread_id != input.presentation_thread_id
            || binding.binding_id != input.binding_id
            || binding.driver_generation != input.driver_generation
            || binding.source_thread_id.as_str() != input.source_thread_id
        {
            return Err(format!(
                "terminal wait-producer effect coordinates no longer match binding {} generation {}",
                input.binding_id, input.driver_generation.0
            ));
        }
        let frame_id = uuid::Uuid::parse_str(&binding.surface.source_frame_id).ok();
        let producer_last_message = self
            .producer_last_message(
                binding.target.run_id,
                binding.target.agent_id,
                input.runtime_thread_id.clone(),
            )
            .await?;
        self.convergence
            .observe_gate_producer_terminal(GateProducerTerminalEvent {
                producer: WaitProducerRef::AgentRunDelivery {
                    run_id: binding.target.run_id,
                    agent_id: binding.target.agent_id,
                    frame_id,
                },
                terminal_state: terminal_state(input.terminal).into(),
                terminal_message: input.message.clone(),
                terminal_diagnostic: input.diagnostic.as_ref().map(gate_terminal_diagnostic),
                producer_last_message,
                source_turn_id: input.source_turn_id.clone(),
                trace_ref: Some(input.presentation_thread_id.to_string()),
            })
            .await
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}

impl RuntimeWaitProducerTerminalConvergence {
    async fn producer_last_message(
        &self,
        run_id: uuid::Uuid,
        agent_id: uuid::Uuid,
        runtime_thread_id: agentdash_agent_runtime_contract::RuntimeThreadId,
    ) -> Result<Option<agentdash_application_workflow::gate::ProducerLastMessageEvidence>, String>
    {
        let page = self
            .journal
            .load_visible_journal_page_for_thread(
                AgentRunJournalQuery { run_id, agent_id },
                runtime_thread_id,
                0,
                u32::MAX,
            )
            .await
            .map_err(|error| error.to_string())?;
        Ok(last_agent_message_evidence(run_id, agent_id, &page.events))
    }
}

fn last_agent_message_evidence(
    run_id: uuid::Uuid,
    agent_id: uuid::Uuid,
    events: &[agentdash_application_agentrun::agent_run::AgentRunJournalEvent],
) -> Option<agentdash_application_workflow::gate::ProducerLastMessageEvidence> {
    let persisted = events
        .iter()
        .filter_map(journal_event_as_persisted)
        .collect::<Vec<_>>();
    let projections = session_item_projections(&persisted);
    let messages = filter_session_items(&projections, SessionItemView::Messages);
    let selected = messages
        .iter()
        .filter(|projection| {
            matches!(
                &projection.content,
                SessionItemContent::Message { role: "agent", .. }
            )
        })
        .filter_map(|projection| {
            let event = events
                .iter()
                .filter(|event| {
                    projection
                        .raw_events
                        .iter()
                        .any(|raw| raw.event_seq == event.journal_seq)
                })
                .max_by_key(|event| event.journal_seq)?;
            Some((projection, event.journal_seq, event.source_event_seq?.0))
        })
        .max_by_key(|(_, journal_seq, _)| *journal_seq);
    let (projection, _, source_event_seq) = selected?;
    producer_last_message_from_projection(run_id, agent_id, projection, source_event_seq)
}

fn bounded_message_preview(text: &str, limit: usize) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= limit {
        return trimmed.to_string();
    }
    let mut preview = trimmed.chars().take(limit).collect::<String>();
    preview.push_str("...");
    preview
}

fn journal_event_as_persisted(
    event: &agentdash_application_agentrun::agent_run::AgentRunJournalEvent,
) -> Option<agentdash_spi::PersistedSessionEvent> {
    let presentation = event.record.as_presentation()?;
    let carrier = event.record.carrier();
    let session_id = carrier
        .coordinate
        .source_thread_id
        .clone()
        .unwrap_or_else(|| event.source_runtime_thread_id.to_string());
    Some(agentdash_spi::PersistedSessionEvent {
        session_id: session_id.clone(),
        event_seq: event.journal_seq,
        occurred_at_ms: i64::try_from(carrier.recorded_at_ms).unwrap_or(i64::MAX),
        committed_at_ms: i64::try_from(carrier.recorded_at_ms).unwrap_or(i64::MAX),
        session_update_type: "notification".into(),
        turn_id: carrier.coordinate.source_turn_id.clone(),
        entry_index: carrier.coordinate.source_entry_index,
        tool_call_id: carrier.coordinate.source_item_id.clone(),
        ephemeral: false,
        notification: BackboneEnvelope::new(
            presentation.event.clone(),
            session_id,
            SourceInfo {
                connector_id: "canonical_runtime".into(),
                connector_type: "runtime_journal".into(),
                executor_id: None,
            },
        ),
    })
}

fn producer_last_message_from_projection(
    run_id: uuid::Uuid,
    agent_id: uuid::Uuid,
    projection: &SessionItemProjection,
    source_event_seq: u64,
) -> Option<agentdash_application_workflow::gate::ProducerLastMessageEvidence> {
    let SessionItemContent::Message {
        role: "agent",
        text,
        ..
    } = &projection.content
    else {
        return None;
    };
    let summary = bounded_message_preview(text, 2_000);
    if summary.is_empty() {
        return None;
    }
    let message = item_summary_for_view(projection, SessionItemView::Messages);
    Some(
        agentdash_application_workflow::gate::ProducerLastMessageEvidence {
            summary,
            message_path: format!(
                "lifecycle://agent-runs/{agent_id}/sessions/messages/{}",
                message
                    .path
                    .strip_prefix("session/messages/")
                    .unwrap_or(&message.path)
            ),
            journal_session_id: agent_run_journal_session_id(run_id, agent_id),
            source_event_seq,
        },
    )
}

fn terminal_state(terminal: agentdash_agent_runtime_contract::RuntimeTurnTerminal) -> &'static str {
    use agentdash_agent_runtime_contract::RuntimeTurnTerminal;
    match terminal {
        RuntimeTurnTerminal::Completed => "completed",
        RuntimeTurnTerminal::Interrupted => "interrupted",
        RuntimeTurnTerminal::Refused
        | RuntimeTurnTerminal::LimitReached
        | RuntimeTurnTerminal::Failed
        | RuntimeTurnTerminal::Lost => "failed",
    }
}

fn gate_terminal_diagnostic(
    diagnostic: &agentdash_agent_protocol::RuntimeTerminalDiagnostic,
) -> GateRuntimeTerminalDiagnostic {
    GateRuntimeTerminalDiagnostic {
        kind: diagnostic.kind.clone(),
        code: diagnostic.code.clone(),
        http_status: diagnostic.http_status,
        provider: diagnostic.provider.clone(),
        model: diagnostic.model.clone(),
        message: diagnostic.message.clone(),
        retryable: diagnostic.retryable,
    }
}

impl RuntimeTerminalApplicationEffectWorker {
    pub(crate) fn new(
        outbox: Arc<dyn RuntimeTerminalApplicationEffectOutbox>,
        effects: Arc<dyn AgentRunControlEffectPort>,
        owner: RuntimeWorkerId,
        lease_duration_ms: u64,
        batch_limit: u32,
    ) -> Result<Self, RuntimeStoreError> {
        if owner.as_str().trim().is_empty() {
            return Err(RuntimeStoreError::InvalidWorkClaim(
                "terminal application effect worker owner must not be empty".into(),
            ));
        }
        if lease_duration_ms == 0 || batch_limit == 0 {
            return Err(RuntimeStoreError::InvalidWorkClaim(
                "terminal application effect worker requires a positive lease and batch limit"
                    .into(),
            ));
        }
        Ok(Self {
            outbox,
            effects,
            owner,
            lease_duration_ms,
            batch_limit,
        })
    }

    pub(crate) async fn drain_once(&self) -> Result<usize, RuntimeStoreError> {
        let claims = self
            .outbox
            .claim_terminal_application_effects(RuntimeTerminalApplicationEffectClaimRequest {
                owner: self.owner.clone(),
                lease_duration_ms: self.lease_duration_ms,
                limit: self.batch_limit,
            })
            .await?;
        let count = claims.len();
        for claim in claims {
            self.execute_claim(claim).await?;
        }
        Ok(count)
    }

    async fn execute_claim(
        &self,
        claim: RuntimeTerminalApplicationEffectClaim,
    ) -> Result<(), RuntimeStoreError> {
        let input = terminal_control_input(&claim);
        match self.effects.observe_runtime_terminal(input).await {
            Ok(()) => self.outbox.ack_terminal_application_effect(&claim).await,
            Err(error) => {
                self.outbox
                    .release_terminal_application_effect(&claim, error)
                    .await
            }
        }
    }
}

fn terminal_control_input(
    claim: &RuntimeTerminalApplicationEffectClaim,
) -> AgentRunTerminalControlInput {
    let entry = &claim.entry;
    AgentRunTerminalControlInput {
        effect_id: entry.effect_id.as_str().to_string(),
        runtime_thread_id: entry.runtime_thread_id.clone(),
        presentation_thread_id: entry.presentation_thread_id.clone(),
        runtime_turn_id: entry.runtime_turn_id.clone(),
        presentation_turn_id: entry.presentation_turn_id.clone(),
        terminal_event_sequence: entry.terminal_event_sequence,
        terminal: entry.terminal,
        message: entry.message.clone(),
        diagnostic: entry.diagnostic.clone(),
        started_at_ms: entry.started_at_ms,
        completed_at_ms: entry.completed_at_ms,
        binding_id: entry.binding_id.clone(),
        driver_generation: entry.driver_generation,
        surface_revision: entry.surface_revision,
        surface_digest: entry.surface_digest.clone(),
        source_thread_id: entry.source_thread_id.clone(),
        source_turn_id: entry.source_turn_id.clone(),
        terminal_hook_effect_binding: entry.terminal_hook_effect_binding.clone(),
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::VecDeque, sync::Mutex};

    use agentdash_agent_protocol::{
        AgentDashThreadItem, BackboneEvent, backbone::item::ItemCompletedNotification,
        codex_app_server_protocol as codex,
    };
    use agentdash_agent_runtime::{
        RuntimeTerminalApplicationEffectId, RuntimeTerminalApplicationEffectOutboxEntry,
        RuntimeWorkClaimToken,
    };
    use agentdash_agent_runtime_contract::*;
    use agentdash_application_agentrun::agent_run::{
        AgentFrameHookRuntime, AgentRunJournalEvent, AgentRunJournalSegmentRole,
    };
    use agentdash_application_ports::agent_run_control_effect::{
        AgentRunPostTurnHandler, DynAgentRunPostTurnHandler,
    };
    use agentdash_infrastructure::agent_runtime_composition::AppliedNativeAgentRunSurface;
    use agentdash_spi::{
        AgentFrameHookEvaluationQuery, AgentFrameHookRefreshQuery, AgentFrameHookSnapshot,
        AgentFrameHookSnapshotQuery, ExecutionHookProvider, HookEffect, HookError, HookResolution,
    };
    use async_trait::async_trait;

    use super::*;

    #[derive(Default)]
    struct TestOutbox {
        claims: Mutex<VecDeque<RuntimeTerminalApplicationEffectClaim>>,
        acked: Mutex<Vec<String>>,
        released: Mutex<Vec<(String, String)>>,
    }

    #[async_trait]
    impl RuntimeTerminalApplicationEffectOutbox for TestOutbox {
        async fn claim_terminal_application_effects(
            &self,
            _: RuntimeTerminalApplicationEffectClaimRequest,
        ) -> Result<Vec<RuntimeTerminalApplicationEffectClaim>, RuntimeStoreError> {
            Ok(self.claims.lock().unwrap().drain(..).collect())
        }

        async fn ack_terminal_application_effect(
            &self,
            claim: &RuntimeTerminalApplicationEffectClaim,
        ) -> Result<(), RuntimeStoreError> {
            self.acked
                .lock()
                .unwrap()
                .push(claim.entry.effect_id.as_str().to_string());
            Ok(())
        }

        async fn release_terminal_application_effect(
            &self,
            claim: &RuntimeTerminalApplicationEffectClaim,
            error: String,
        ) -> Result<(), RuntimeStoreError> {
            self.released
                .lock()
                .unwrap()
                .push((claim.entry.effect_id.as_str().to_string(), error));
            Ok(())
        }
    }

    struct TestEffects {
        fail: bool,
        seen: Mutex<Vec<AgentRunTerminalControlInput>>,
    }

    struct TerminalEffectProvider;

    #[async_trait]
    impl ExecutionHookProvider for TerminalEffectProvider {
        async fn load_frame_snapshot(
            &self,
            _: AgentFrameHookSnapshotQuery,
        ) -> Result<AgentFrameHookSnapshot, HookError> {
            Ok(AgentFrameHookSnapshot::default())
        }

        async fn refresh_frame_snapshot(
            &self,
            _: AgentFrameHookRefreshQuery,
        ) -> Result<AgentFrameHookSnapshot, HookError> {
            Ok(AgentFrameHookSnapshot::default())
        }

        async fn evaluate_frame_hook(
            &self,
            _: AgentFrameHookEvaluationQuery,
        ) -> Result<HookResolution, HookError> {
            Ok(HookResolution {
                effects: vec![HookEffect {
                    kind: "agent_run_control_effect".into(),
                    payload: serde_json::json!({"value": 1}),
                    presentation: None,
                }],
                ..HookResolution::default()
            })
        }
    }

    struct RecordingPostTurnHandler {
        identity: RuntimeTerminalHookEffectHandlerRef,
        calls: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl AgentRunPostTurnHandler for RecordingPostTurnHandler {
        async fn on_event(&self, _: &str, _: &BackboneEnvelope) {}

        async fn execute_effects(
            &self,
            effect_id: &str,
            _: &str,
            _: &str,
            _: &[HookEffect],
        ) -> Result<(), String> {
            self.calls.lock().unwrap().push(effect_id.to_string());
            Ok(())
        }

        fn supported_effect_kinds(&self) -> &[&str] {
            &["agent_run_control_effect"]
        }

        fn durable_effect_handler(&self) -> Option<RuntimeTerminalHookEffectHandlerRef> {
            Some(self.identity.clone())
        }
    }

    struct RecordingHandlerRegistry {
        identity: RuntimeTerminalHookEffectHandlerRef,
        handler: DynAgentRunPostTurnHandler,
    }

    #[async_trait]
    impl AgentRunHookEffectHandlerRegistry for RecordingHandlerRegistry {
        async fn handler_for(
            &self,
            _: &PresentationThreadId,
            handler: &RuntimeTerminalHookEffectHandlerRef,
        ) -> Result<Option<DynAgentRunPostTurnHandler>, String> {
            Ok((handler == &self.identity).then(|| self.handler.clone()))
        }
    }

    #[async_trait]
    impl AgentRunControlEffectPort for TestEffects {
        async fn observe_runtime_terminal(
            &self,
            input: AgentRunTerminalControlInput,
        ) -> Result<(), String> {
            self.seen.lock().unwrap().push(input);
            if self.fail {
                Err("terminal side effect failed".into())
            } else {
                Ok(())
            }
        }
    }

    fn claim(effect_id: &str) -> RuntimeTerminalApplicationEffectClaim {
        RuntimeTerminalApplicationEffectClaim {
            entry: RuntimeTerminalApplicationEffectOutboxEntry {
                effect_id: RuntimeTerminalApplicationEffectId::new(effect_id).unwrap(),
                runtime_thread_id: RuntimeThreadId::new("runtime-thread").unwrap(),
                presentation_thread_id: PresentationThreadId::new("presentation-thread").unwrap(),
                runtime_turn_id: RuntimeTurnId::new("runtime-turn").unwrap(),
                presentation_turn_id: PresentationTurnId::new("presentation-turn").unwrap(),
                terminal_event_sequence: EventSequence(7),
                terminal: RuntimeTurnTerminal::Completed,
                message: Some("done".into()),
                diagnostic: None,
                started_at_ms: Some(10),
                completed_at_ms: 20,
                binding_id: RuntimeBindingId::new("binding").unwrap(),
                driver_generation: RuntimeDriverGeneration(3),
                surface_revision: SurfaceRevision(4),
                surface_digest: SurfaceDigest::new("surface-digest").unwrap(),
                source_thread_id: "source-thread".into(),
                source_turn_id: Some("source-turn".into()),
                terminal_hook_effect_binding: None,
            },
            token: RuntimeWorkClaimToken("claim-token".into()),
            owner: RuntimeWorkerId("worker".into()),
            lease_expires_at_ms: 100,
            attempt: 1,
        }
    }

    fn terminal_hook_binding() -> RuntimeTerminalHookEffectBinding {
        RuntimeTerminalHookEffectBinding {
            handler: RuntimeTerminalHookEffectHandlerRef {
                handler_type: RuntimeTerminalHookEffectHandlerType::new("agent_run_post_turn")
                    .unwrap(),
                handler_id: RuntimeTerminalHookEffectHandlerId::new("handler-fixture").unwrap(),
                revision: RuntimeTerminalHookEffectHandlerRevision(7),
            },
            supported_effect_kinds: std::collections::BTreeSet::from([RuntimeHookEffectKind::new(
                "agent_run_control_effect",
            )
            .unwrap()]),
        }
    }

    fn journal_event(sequence: u64, event: BackboneEvent) -> AgentRunJournalEvent {
        let thread_id = RuntimeThreadId::new("evidence-thread").unwrap();
        let record = RuntimeJournalRecord::new(
            RuntimeCarrierMetadata {
                thread_id: thread_id.clone(),
                recorded_at_ms: sequence,
                sequence: Some(EventSequence(sequence)),
                transient: None,
                revision: RuntimeRevision(sequence),
                operation_id: None,
                append_idempotency_key: None,
                binding_id: None,
                coordinate: RuntimePresentationCoordinate {
                    runtime_turn_id: None,
                    runtime_item_id: None,
                    interaction_id: None,
                    source_thread_id: Some("presentation-thread".into()),
                    source_turn_id: Some("source-turn".into()),
                    source_item_id: None,
                    source_request_id: None,
                    source_entry_index: Some(0),
                },
            },
            RuntimeJournalFact::Presentation(ImmutablePresentationEvent::new(
                PresentationDurability::Durable,
                event,
            )),
        )
        .unwrap();
        AgentRunJournalEvent {
            journal_seq: sequence,
            segment_role: AgentRunJournalSegmentRole::CurrentDelivery,
            source_runtime_thread_id: thread_id,
            source_event_seq: Some(EventSequence(sequence)),
            record,
        }
    }

    fn completed_item(sequence: u64, item: AgentDashThreadItem) -> AgentRunJournalEvent {
        journal_event(
            sequence,
            BackboneEvent::ItemCompleted(ItemCompletedNotification::new(
                item,
                "presentation-thread",
                "source-turn",
            )),
        )
    }

    fn worker(
        outbox: Arc<TestOutbox>,
        effects: Arc<TestEffects>,
    ) -> RuntimeTerminalApplicationEffectWorker {
        RuntimeTerminalApplicationEffectWorker::new(
            outbox,
            effects,
            RuntimeWorkerId("worker".into()),
            30_000,
            10,
        )
        .unwrap()
    }

    #[tokio::test]
    async fn acknowledges_only_after_application_effects_succeed() {
        let outbox = Arc::new(TestOutbox::default());
        outbox.claims.lock().unwrap().push_back(claim("effect-1"));
        let effects = Arc::new(TestEffects {
            fail: false,
            seen: Mutex::new(Vec::new()),
        });

        assert_eq!(
            worker(outbox.clone(), effects.clone())
                .drain_once()
                .await
                .unwrap(),
            1
        );
        assert_eq!(*outbox.acked.lock().unwrap(), ["effect-1"]);
        assert!(outbox.released.lock().unwrap().is_empty());
        let seen = effects.seen.lock().unwrap();
        assert_eq!(seen[0].effect_id, "effect-1");
        assert_eq!(seen[0].terminal_event_sequence, EventSequence(7));
        assert_eq!(seen[0].binding_id.as_str(), "binding");
        assert_eq!(seen[0].source_turn_id.as_deref(), Some("source-turn"));
    }

    #[tokio::test]
    async fn releases_failed_application_effect_for_durable_retry() {
        let outbox = Arc::new(TestOutbox::default());
        outbox.claims.lock().unwrap().push_back(claim("effect-2"));
        let effects = Arc::new(TestEffects {
            fail: true,
            seen: Mutex::new(Vec::new()),
        });

        assert_eq!(
            worker(outbox.clone(), effects).drain_once().await.unwrap(),
            1
        );
        assert!(outbox.acked.lock().unwrap().is_empty());
        assert_eq!(
            *outbox.released.lock().unwrap(),
            [("effect-2".into(), "terminal side effect failed".into())]
        );
    }

    #[test]
    fn last_message_evidence_reuses_main_session_item_projection_and_path() {
        let run_id = uuid::Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
        let agent_id = uuid::Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap();
        let reasoning: AgentDashThreadItem = codex::ThreadItem::Reasoning {
            id: "turn-1:0:reason".into(),
            summary: vec![],
            content: vec!["reason".into()],
        }
        .into();
        let message: AgentDashThreadItem = codex::ThreadItem::AgentMessage {
            id: "turn-1:0:msg".into(),
            text: "FULL FINAL".into(),
            phase: None,
            memory_citation: None,
        }
        .into();
        let events = vec![completed_item(6, reasoning), completed_item(7, message)];

        let evidence = last_agent_message_evidence(run_id, agent_id, &events).unwrap();
        assert_eq!(evidence.summary, "FULL FINAL");
        assert_eq!(
            evidence.message_path,
            "lifecycle://agent-runs/22222222-2222-2222-2222-222222222222/sessions/messages/0002__turn-1_0_msg__agent__FULL_FINAL.md"
        );
        assert_eq!(
            evidence.journal_session_id,
            "agentrun:11111111-1111-1111-1111-111111111111:22222222-2222-2222-2222-222222222222"
        );
        assert_eq!(evidence.source_event_seq, 7);
        assert_eq!(
            last_agent_message_evidence(run_id, agent_id, &events),
            Some(evidence),
            "refresh/retry must reproduce identical evidence"
        );
    }

    #[test]
    fn last_message_evidence_is_typed_none_without_agent_message() {
        let reasoning: AgentDashThreadItem = codex::ThreadItem::Reasoning {
            id: "turn-1:0:reason".into(),
            summary: vec![],
            content: vec!["reason".into()],
        }
        .into();
        assert!(
            last_agent_message_evidence(
                uuid::Uuid::new_v4(),
                uuid::Uuid::new_v4(),
                &[completed_item(1, reasoning)],
            )
            .is_none()
        );
    }

    #[tokio::test]
    async fn typed_terminal_hook_binding_executes_exact_surface_with_stable_effect_identity() {
        let binding = terminal_hook_binding();
        let registry = Arc::new(CompiledAgentRunToolRegistry::default());
        registry
            .put(
                crate::bootstrap::agent_runtime_surface::CompiledAgentRunToolBinding {
                    applied: AppliedNativeAgentRunSurface {
                        runtime_thread_id: RuntimeThreadId::new("runtime-thread").unwrap(),
                        binding_id: RuntimeBindingId::new("binding").unwrap(),
                        generation: RuntimeDriverGeneration(3),
                        source_thread_id: DriverThreadId::new("source-thread").unwrap(),
                        surface_revision: SurfaceRevision(4),
                        surface_digest: SurfaceDigest::new("surface-digest").unwrap(),
                        tool_set_revision: ToolSetRevision(1),
                        hook_plan_revision: HookPlanRevision(1),
                        hook_plan_digest: HookPlanDigest::new("hook-plan").unwrap(),
                        terminal_hook_effect_binding: Some(binding.clone()),
                    },
                    runtime_session_id: "presentation-thread".into(),
                    run_id: uuid::Uuid::nil(),
                    agent_id: uuid::Uuid::nil(),
                    frame_id: uuid::Uuid::nil(),
                    hook_runtime: Arc::new(AgentFrameHookRuntime::new(
                        uuid::Uuid::nil(),
                        uuid::Uuid::nil(),
                        uuid::Uuid::nil(),
                        1,
                        "presentation-thread".into(),
                        Arc::new(TerminalEffectProvider),
                        AgentFrameHookSnapshot::default(),
                    )),
                    catalog: agentdash_agent_runtime::ToolCatalogRevision {
                        revision: ToolSetRevision(1),
                        digest: "catalog".into(),
                        tools: vec![],
                        mcp_servers: vec![],
                    },
                    tools: std::collections::BTreeMap::new(),
                    terminal_hook_effect_binding: Some(binding.clone()),
                },
            )
            .await
            .unwrap();
        let handler = Arc::new(RecordingPostTurnHandler {
            identity: binding.handler.clone(),
            calls: Mutex::new(Vec::new()),
        });
        let effects = RuntimeTerminalHookEffects::new(
            registry,
            Arc::new(RecordingHandlerRegistry {
                identity: binding.handler.clone(),
                handler: handler.clone(),
            }),
        );
        let mut input = terminal_control_input(&claim("effect-hook"));
        input.terminal_hook_effect_binding = Some(binding);

        effects.execute_terminal_hooks(&input).await.unwrap();

        assert_eq!(
            *handler.calls.lock().unwrap(),
            ["effect-hook:terminal_hook_effects"]
        );
    }
}
