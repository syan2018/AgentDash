use std::sync::Arc;

use agentdash_application_ports::agent_run_runtime::{
    AgentRunRuntimeBindingRepository, AgentRunRuntimeTarget,
};
use agentdash_domain::workflow::{
    GateWaitPolicyEnvelope, LifecycleGate, LifecycleGateRepository, WaitProducerRef,
};
use serde_json::{Value, json};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RuntimeTerminalDiagnostic {
    pub kind: String,
    pub code: Option<String>,
    pub http_status: Option<u16>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub message: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ProducerLastMessageEvidence {
    pub summary: String,
    pub message_path: String,
    pub journal_session_id: String,
    pub source_event_seq: u64,
}

use crate::WorkflowApplicationError;

use super::{
    GateDeliveryIntent, GateMailboxWakeIntent, LifecycleGateResolver, ResolveGatePayloadCommand,
    child_evidence::child_evidence_result_refs,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateProducerTerminalEvent {
    pub producer: WaitProducerRef,
    pub terminal_state: String,
    pub terminal_message: Option<String>,
    pub terminal_diagnostic: Option<RuntimeTerminalDiagnostic>,
    pub producer_last_message: Option<ProducerLastMessageEvidence>,
    pub source_turn_id: Option<String>,
    pub trace_ref: Option<String>,
}

#[derive(Debug, Clone)]
pub struct GateProducerTerminalConvergenceResult {
    pub outcomes: Vec<GateProducerTerminalConvergenceOutcome>,
}

impl GateProducerTerminalConvergenceResult {
    pub fn no_matching_gate_wait_policy(&self) -> bool {
        self.outcomes.is_empty()
    }
}

#[derive(Debug, Clone)]
pub struct GateProducerTerminalConvergenceOutcome {
    pub gate_id: Uuid,
    pub kind: GateProducerTerminalConvergenceOutcomeKind,
    pub result_status: Option<String>,
    pub delivery_intents: Vec<GateDeliveryIntent>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateProducerTerminalConvergenceOutcomeKind {
    Resolved,
    AlreadyResolvedEnsuredDelivery,
}

#[derive(Clone)]
pub struct GateProducerTerminalConvergenceService {
    gate_repo: Arc<dyn LifecycleGateRepository>,
    runtime_binding_repo: Arc<dyn AgentRunRuntimeBindingRepository>,
}

impl GateProducerTerminalConvergenceService {
    pub fn new(
        gate_repo: Arc<dyn LifecycleGateRepository>,
        runtime_binding_repo: Arc<dyn AgentRunRuntimeBindingRepository>,
    ) -> Self {
        Self {
            gate_repo,
            runtime_binding_repo,
        }
    }

    pub async fn observe_producer_terminal(
        &self,
        event: GateProducerTerminalEvent,
    ) -> Result<GateProducerTerminalConvergenceResult, WorkflowApplicationError> {
        let gates = self
            .gate_repo
            .list_by_wait_producer(&event.producer)
            .await?;
        let mut outcomes = Vec::new();
        for gate in gates {
            let Some(envelope) = gate
                .payload_json
                .as_ref()
                .and_then(GateWaitPolicyEnvelope::from_payload_opt)
            else {
                continue;
            };
            let outcome = if gate.is_open() {
                self.resolve_open_gate_from_terminal(gate, envelope, &event)
                    .await?
            } else {
                self.ensure_resolved_gate_delivery(gate, envelope, &event)
                    .await?
            };
            outcomes.push(outcome);
        }
        Ok(GateProducerTerminalConvergenceResult { outcomes })
    }

    async fn resolve_open_gate_from_terminal(
        &self,
        gate: LifecycleGate,
        envelope: GateWaitPolicyEnvelope,
        event: &GateProducerTerminalEvent,
    ) -> Result<GateProducerTerminalConvergenceOutcome, WorkflowApplicationError> {
        let result_payload = producer_terminal_result_payload(&gate, &envelope, event);
        let result_status = result_payload
            .get("status")
            .and_then(Value::as_str)
            .map(str::to_string);
        let intent = self
            .mailbox_wake_intent(&gate, &envelope, event, result_payload.clone())
            .await?;
        match LifecycleGateResolver::new(self.gate_repo.clone())
            .resolve_gate_payload(ResolveGatePayloadCommand {
                gate_id: gate.id,
                payload: result_payload,
                resolved_by: producer_resolved_by(&event.producer),
            })
            .await
        {
            Ok(outcome) => outcome,
            Err(WorkflowApplicationError::Conflict(message)) => {
                let Some(latest_gate) = self.gate_repo.get(gate.id).await? else {
                    return Err(WorkflowApplicationError::NotFound(format!(
                        "gate producer terminal convergence gate {} disappeared after resolve conflict",
                        gate.id
                    )));
                };
                if latest_gate.is_open() {
                    return Err(WorkflowApplicationError::Conflict(message));
                }
                let latest_envelope = latest_gate
                    .payload_json
                    .as_ref()
                    .and_then(GateWaitPolicyEnvelope::from_payload_opt)
                    .unwrap_or(envelope);
                return self
                    .ensure_resolved_gate_delivery(latest_gate, latest_envelope, event)
                    .await;
            }
            Err(error) => return Err(error),
        };

        Ok(GateProducerTerminalConvergenceOutcome {
            gate_id: gate.id,
            kind: GateProducerTerminalConvergenceOutcomeKind::Resolved,
            result_status,
            delivery_intents: vec![GateDeliveryIntent::MailboxWake(intent)],
        })
    }

    async fn ensure_resolved_gate_delivery(
        &self,
        gate: LifecycleGate,
        envelope: GateWaitPolicyEnvelope,
        event: &GateProducerTerminalEvent,
    ) -> Result<GateProducerTerminalConvergenceOutcome, WorkflowApplicationError> {
        let payload = gate.payload_json.clone().unwrap_or_else(|| json!({}));
        let result_status = payload
            .get("status")
            .and_then(Value::as_str)
            .map(str::to_string);
        let intent = self
            .mailbox_wake_intent(&gate, &envelope, event, payload)
            .await?;
        Ok(GateProducerTerminalConvergenceOutcome {
            gate_id: gate.id,
            kind: GateProducerTerminalConvergenceOutcomeKind::AlreadyResolvedEnsuredDelivery,
            result_status,
            delivery_intents: vec![GateDeliveryIntent::MailboxWake(intent)],
        })
    }

    async fn mailbox_wake_intent(
        &self,
        gate: &LifecycleGate,
        envelope: &GateWaitPolicyEnvelope,
        event: &GateProducerTerminalEvent,
        payload: Value,
    ) -> Result<GateMailboxWakeIntent, WorkflowApplicationError> {
        let WaitProducerRef::AgentRunDelivery { agent_id, .. } = &event.producer;
        let wake_target = &envelope.wait_policy.wake_target;
        let target_binding = self
            .runtime_binding_repo
            .load(&AgentRunRuntimeTarget {
                run_id: wake_target.target_run_id,
                agent_id: wake_target.target_agent_id,
            })
            .await
            .map_err(|error| WorkflowApplicationError::Internal(error.to_string()))?
            .ok_or_else(|| {
                WorkflowApplicationError::Conflict(format!(
                    "gate producer terminal convergence gate {} missing target delivery binding",
                    gate.id
                ))
            })?;
        Ok(GateMailboxWakeIntent {
            gate_id: gate.id,
            namespace: wake_target.namespace.clone(),
            request_id: payload_request_id(gate, envelope, &payload),
            target_run_id: wake_target.target_run_id,
            target_agent_id: wake_target.target_agent_id,
            target_runtime_thread_id: target_binding.thread_id.to_string(),
            producer_agent_id: *agent_id,
            producer_runtime_thread_id: event.trace_ref.clone(),
            resolved_turn_id: payload
                .get("resolved_turn_id")
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| event.source_turn_id.clone())
                .unwrap_or_else(|| "producer-terminal".to_string()),
            client_command_id: wake_target.client_command_id.clone(),
            payload,
        })
    }
}

fn producer_terminal_result_payload(
    gate: &LifecycleGate,
    envelope: &GateWaitPolicyEnvelope,
    event: &GateProducerTerminalEvent,
) -> Value {
    let terminal_outcome = envelope
        .wait_policy
        .terminal_policy
        .outcome_for_terminal_state(&event.terminal_state);
    let summary = event
        .terminal_diagnostic
        .as_ref()
        .map(diagnostic_summary)
        .or_else(|| {
            event
                .producer_last_message
                .as_ref()
                .map(|message| message.summary.clone())
        })
        .unwrap_or_else(|| {
            "Producer reached terminal before the expected result was written.".to_string()
        });
    let WaitProducerRef::AgentRunDelivery {
        run_id,
        agent_id,
        frame_id,
    } = &event.producer;
    let result_refs = child_evidence_result_refs(
        gate.id,
        *run_id,
        *agent_id,
        *frame_id,
        event.trace_ref.as_deref(),
    );
    json!({
        "gate_id": gate.id.to_string(),
        "request_id": payload_request_id(gate, envelope, &json!({})),
        "status": terminal_outcome.status,
        "summary": summary,
        "terminal_state": event.terminal_state,
        "terminal_message": event.terminal_message,
        "diagnostic": event.terminal_diagnostic,
        "fallback_message": event.producer_last_message,
        "delivery_trace_ref": event.trace_ref,
        "resolved_turn_id": event.source_turn_id,
        "failure_kind": terminal_outcome.failure_kind,
        "source": "producer_terminal",
        "findings": [],
        "follow_ups": [],
        "artifact_refs": [],
        "result_refs": result_refs,
    })
}

fn diagnostic_summary(diagnostic: &RuntimeTerminalDiagnostic) -> String {
    match (
        diagnostic.provider.as_deref(),
        diagnostic.http_status,
        diagnostic.code.as_deref(),
    ) {
        (Some(provider), Some(status), Some(code)) => {
            format!(
                "{provider} returned {status} {code}: {}",
                diagnostic.message
            )
        }
        (Some(provider), Some(status), None) => {
            format!("{provider} returned {status}: {}", diagnostic.message)
        }
        (Some(provider), None, Some(code)) => {
            format!("{provider} {code}: {}", diagnostic.message)
        }
        _ => diagnostic.message.clone(),
    }
}

fn payload_request_id(
    gate: &LifecycleGate,
    envelope: &GateWaitPolicyEnvelope,
    payload: &Value,
) -> String {
    payload
        .get("request_id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| envelope.wait_policy.expected_result.correlation_ref.clone())
        .unwrap_or_else(|| gate.correlation_id.clone())
}

fn producer_resolved_by(producer: &WaitProducerRef) -> String {
    match producer {
        WaitProducerRef::AgentRunDelivery { agent_id, .. } => {
            format!("producer_terminal:agent_run_delivery:{agent_id}")
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{BTreeSet, HashMap},
        str::FromStr,
        sync::{Arc, Mutex},
    };

    use agentdash_agent_runtime_contract::*;
    use agentdash_application_ports::agent_run_runtime::{
        AgentRunRuntimeBinding, AgentRunRuntimeBindingError,
    };
    use agentdash_domain::{
        DomainError,
        workflow::{
            GateWaitPolicy, WaitExpectedResult, WaitTerminalOutcome, WaitTerminalPolicy,
            WaitWakeTarget,
        },
    };

    use super::*;

    #[derive(Default)]
    struct FixtureGateRepo {
        gates: Mutex<HashMap<Uuid, LifecycleGate>>,
        resolve_on_next_get: Mutex<Option<Uuid>>,
    }

    #[async_trait::async_trait]
    impl LifecycleGateRepository for FixtureGateRepo {
        async fn create(&self, gate: &LifecycleGate) -> Result<(), DomainError> {
            self.gates.lock().unwrap().insert(gate.id, gate.clone());
            Ok(())
        }

        async fn get(&self, id: Uuid) -> Result<Option<LifecycleGate>, DomainError> {
            let resolve_on_get = self.resolve_on_next_get.lock().unwrap().take();
            let mut gates = self.gates.lock().unwrap();
            if resolve_on_get == Some(id)
                && let Some(gate) = gates.get_mut(&id)
            {
                if let Some(payload) = gate.payload_json.as_mut() {
                    payload["status"] = json!("completed");
                    payload["summary"] = json!("normal companion result");
                    payload["source"] = json!("companion_respond");
                }
                gate.resolve("child_agent:normal-result");
            }
            Ok(gates.get(&id).cloned())
        }

        async fn list_open_for_agent(
            &self,
            agent_id: Uuid,
        ) -> Result<Vec<LifecycleGate>, DomainError> {
            Ok(self
                .gates
                .lock()
                .unwrap()
                .values()
                .filter(|gate| gate.agent_id == Some(agent_id) && gate.is_open())
                .cloned()
                .collect())
        }

        async fn list_open_gate_wait_policies(
            &self,
            limit: usize,
        ) -> Result<Vec<LifecycleGate>, DomainError> {
            Ok(self
                .gates
                .lock()
                .unwrap()
                .values()
                .filter(|gate| {
                    gate.is_open()
                        && gate
                            .payload_json
                            .as_ref()
                            .and_then(GateWaitPolicyEnvelope::from_payload_opt)
                            .is_some()
                })
                .take(limit)
                .cloned()
                .collect())
        }

        async fn list_by_wait_producer(
            &self,
            producer: &WaitProducerRef,
        ) -> Result<Vec<LifecycleGate>, DomainError> {
            Ok(self
                .gates
                .lock()
                .unwrap()
                .values()
                .filter(|gate| {
                    gate.payload_json
                        .as_ref()
                        .and_then(GateWaitPolicyEnvelope::from_payload_opt)
                        .is_some_and(|envelope| envelope.wait_policy.source == *producer)
                })
                .cloned()
                .collect())
        }

        async fn find_by_agent_and_correlation(
            &self,
            agent_id: Uuid,
            correlation_id: &str,
        ) -> Result<Option<LifecycleGate>, DomainError> {
            Ok(self
                .gates
                .lock()
                .unwrap()
                .values()
                .find(|gate| {
                    gate.agent_id == Some(agent_id) && gate.correlation_id == correlation_id
                })
                .cloned())
        }

        async fn update(&self, gate: &LifecycleGate) -> Result<(), DomainError> {
            self.gates.lock().unwrap().insert(gate.id, gate.clone());
            Ok(())
        }
    }

    struct FixtureRuntimeBindingRepo {
        binding: AgentRunRuntimeBinding,
    }

    #[async_trait::async_trait]
    impl AgentRunRuntimeBindingRepository for FixtureRuntimeBindingRepo {
        async fn load(
            &self,
            target: &AgentRunRuntimeTarget,
        ) -> Result<Option<AgentRunRuntimeBinding>, AgentRunRuntimeBindingError> {
            Ok((&self.binding.target == target).then(|| self.binding.clone()))
        }

        async fn load_by_thread_id(
            &self,
            thread_id: &RuntimeThreadId,
        ) -> Result<Option<AgentRunRuntimeBinding>, AgentRunRuntimeBindingError> {
            Ok((&self.binding.thread_id == thread_id).then(|| self.binding.clone()))
        }

        async fn list_by_run(
            &self,
            run_id: Uuid,
        ) -> Result<Vec<AgentRunRuntimeBinding>, AgentRunRuntimeBindingError> {
            Ok((self.binding.target.run_id == run_id)
                .then(|| self.binding.clone())
                .into_iter()
                .collect())
        }

        async fn list_by_agent(
            &self,
            agent_id: Uuid,
        ) -> Result<Vec<AgentRunRuntimeBinding>, AgentRunRuntimeBindingError> {
            Ok((self.binding.target.agent_id == agent_id)
                .then(|| self.binding.clone())
                .into_iter()
                .collect())
        }

        async fn insert(
            &self,
            binding: AgentRunRuntimeBinding,
        ) -> Result<AgentRunRuntimeBinding, AgentRunRuntimeBindingError> {
            Ok(binding)
        }
    }

    fn runtime_id<T: FromStr>(value: &str) -> T
    where
        T::Err: std::fmt::Debug,
    {
        value.parse().expect("valid runtime id")
    }

    fn runtime_binding(run_id: Uuid, agent_id: Uuid) -> AgentRunRuntimeBinding {
        AgentRunRuntimeBinding {
            target: AgentRunRuntimeTarget { run_id, agent_id },
            thread_id: runtime_id("parent-session"),
            binding_id: runtime_id("parent-binding"),
            driver_generation: RuntimeDriverGeneration(1),
            source_thread_id: runtime_id("parent-source"),
            profile_digest: runtime_id("parent-profile"),
            profile_provenance: ProfileProvenance {
                service_digest: runtime_id("parent-service"),
                transport_digest: runtime_id("parent-transport"),
                host_policy_digest: runtime_id("parent-policy"),
            },
            bound_profile: RuntimeProfile {
                reference_class: ReferenceRuntimeClass::ManagedThread,
                input: InputProfile {
                    modalities: BTreeSet::new(),
                },
                instruction: InstructionProfile {
                    channels: BTreeSet::new(),
                    configuration_boundary: ConfigurationBoundary::Binding,
                },
                tools: ToolProfile {
                    channels: BTreeSet::new(),
                    configuration_boundary: ConfigurationBoundary::Binding,
                    cancellation: true,
                },
                workspace: WorkspaceProfile {
                    capabilities: BTreeSet::new(),
                    mechanism: DeliveryMechanism::Native,
                },
                interactions: InteractionProfile {
                    kinds: BTreeSet::new(),
                    durable_correlation: true,
                },
                lifecycle: BTreeSet::new(),
                hooks: HookProfile {
                    points: Vec::new(),
                    configuration_boundary: ConfigurationBoundary::Binding,
                },
                context: ContextProfile {
                    capabilities: BTreeSet::new(),
                    fidelity: ContextFidelity::Opaque,
                    activation_idempotent: false,
                },
                telemetry_config: BTreeSet::new(),
            },
            surface_digest: runtime_id("parent-surface"),
            settings_revision: ThreadSettingsRevision(0),
            tool_set_revision: ToolSetRevision(0),
            hook_plan: BoundRuntimeHookPlan {
                revision: HookPlanRevision(1),
                digest: runtime_id("parent-hook"),
                entries: Vec::new(),
            },
        }
    }

    fn companion_wait_policy(
        run_id: Uuid,
        child_agent_id: Uuid,
        child_frame_id: Uuid,
        parent_agent_id: Uuid,
        gate_id: Uuid,
    ) -> GateWaitPolicyEnvelope {
        GateWaitPolicyEnvelope::new(GateWaitPolicy {
            source: WaitProducerRef::AgentRunDelivery {
                run_id,
                agent_id: child_agent_id,
                frame_id: Some(child_frame_id),
            },
            expected_result: WaitExpectedResult {
                kind: "companion_result".to_string(),
                correlation_ref: Some("dispatch-1".to_string()),
            },
            terminal_policy: WaitTerminalPolicy {
                failed: WaitTerminalOutcome {
                    status: "failed".to_string(),
                    failure_kind: "runtime_terminal_failed".to_string(),
                },
                interrupted: WaitTerminalOutcome {
                    status: "cancelled".to_string(),
                    failure_kind: "runtime_terminal_cancelled".to_string(),
                },
                completed: WaitTerminalOutcome {
                    status: "failed".to_string(),
                    failure_kind: "missing_companion_respond".to_string(),
                },
            },
            wake_target: WaitWakeTarget {
                namespace: "companion".to_string(),
                target_run_id: run_id,
                target_agent_id: parent_agent_id,
                client_command_id: format!("companion-result:{gate_id}"),
            },
        })
        .with_display_value("companion_label", json!("reviewer"))
    }

    fn open_companion_gate(
        run_id: Uuid,
        parent_agent_id: Uuid,
        child_agent_id: Uuid,
        child_frame_id: Uuid,
        payload: Value,
    ) -> LifecycleGate {
        let mut gate = LifecycleGate::open(
            run_id,
            Some(child_agent_id),
            Some(child_frame_id),
            "companion_wait_follow_up",
            "dispatch-1",
            Some(payload),
        );
        let envelope = companion_wait_policy(
            run_id,
            child_agent_id,
            child_frame_id,
            parent_agent_id,
            gate.id,
        );
        gate.payload_json = Some(
            envelope
                .write_into_payload(gate.payload_json.take())
                .expect("declaration payload"),
        );
        gate
    }

    async fn service_fixture(
        gate: &LifecycleGate,
        run_id: Uuid,
        parent_agent_id: Uuid,
    ) -> (Arc<FixtureGateRepo>, GateProducerTerminalConvergenceService) {
        let gate_repo = Arc::new(FixtureGateRepo::default());
        gate_repo.create(gate).await.expect("seed gate");
        let runtime_binding_repo = Arc::new(FixtureRuntimeBindingRepo {
            binding: runtime_binding(run_id, parent_agent_id),
        });
        let service =
            GateProducerTerminalConvergenceService::new(gate_repo.clone(), runtime_binding_repo);
        (gate_repo, service)
    }

    fn terminal_event(
        run_id: Uuid,
        child_agent_id: Uuid,
        child_frame_id: Uuid,
        terminal_state: &str,
    ) -> GateProducerTerminalEvent {
        GateProducerTerminalEvent {
            producer: WaitProducerRef::AgentRunDelivery {
                run_id,
                agent_id: child_agent_id,
                frame_id: Some(child_frame_id),
            },
            terminal_state: terminal_state.to_string(),
            terminal_message: None,
            terminal_diagnostic: None,
            producer_last_message: None,
            source_turn_id: Some("child-turn".to_string()),
            trace_ref: Some("child-session".to_string()),
        }
    }

    #[tokio::test]
    async fn producer_terminal_resolves_once_then_ensures_delivery() {
        let run_id = Uuid::new_v4();
        let parent_agent_id = Uuid::new_v4();
        let child_agent_id = Uuid::new_v4();
        let child_frame_id = Uuid::new_v4();
        let gate = open_companion_gate(
            run_id,
            parent_agent_id,
            child_agent_id,
            child_frame_id,
            json!({ "preview": "review requested" }),
        );
        let (gate_repo, service) = service_fixture(&gate, run_id, parent_agent_id).await;
        let event = terminal_event(run_id, child_agent_id, child_frame_id, "completed");

        let first = service
            .observe_producer_terminal(event.clone())
            .await
            .expect("first convergence");
        assert_eq!(first.outcomes.len(), 1);
        assert_eq!(
            first.outcomes[0].kind,
            GateProducerTerminalConvergenceOutcomeKind::Resolved
        );
        let stored = gate_repo
            .get(gate.id)
            .await
            .expect("load gate")
            .expect("gate");
        let payload = stored.payload_json.as_ref().expect("payload");
        assert_eq!(payload["status"], json!("failed"));
        assert_eq!(payload["failure_kind"], json!("missing_companion_respond"));
        assert_eq!(payload["source"], json!("producer_terminal"));
        assert_eq!(
            payload["result_refs"]["child"]["run_id"],
            json!(run_id.to_string())
        );
        assert_eq!(
            payload["result_refs"]["child"]["agent_id"],
            json!(child_agent_id.to_string())
        );
        assert_eq!(
            payload["result_refs"]["child"]["frame_id"],
            json!(child_frame_id.to_string())
        );
        assert_eq!(
            payload["result_refs"]["child"]["runtime_thread_id"],
            json!("child-session")
        );
        let evidence = payload["result_refs"]["evidence"]
            .as_array()
            .expect("evidence refs");
        assert!(evidence.iter().any(|entry| {
            entry.get("kind") == Some(&json!("lifecycle_file"))
                && entry.get("mount_id") == Some(&json!("lifecycle"))
                && entry.get("uri")
                    == Some(&json!(format!(
                        "lifecycle://agent-runs/{child_agent_id}/sessions/messages"
                    )))
                && entry.get("runtime_thread_id") == Some(&json!("child-session"))
        }));
        assert!(
            !serde_json::to_string(&payload["result_refs"])
                .expect("serialize refs")
                .contains("session/events.json")
        );
        assert!(payload.get("wait_policy").is_some());
        assert!(matches!(
            &first.outcomes[0].delivery_intents[0],
            GateDeliveryIntent::MailboxWake(intent) if intent.namespace == "companion"
        ));

        let replay = service
            .observe_producer_terminal(event)
            .await
            .expect("replay convergence");
        assert_eq!(replay.outcomes.len(), 1);
        assert_eq!(
            replay.outcomes[0].kind,
            GateProducerTerminalConvergenceOutcomeKind::AlreadyResolvedEnsuredDelivery
        );
        let stored_after_replay = gate_repo
            .get(gate.id)
            .await
            .expect("load gate")
            .expect("gate");
        assert_eq!(stored_after_replay.payload_json, stored.payload_json);
    }

    #[tokio::test]
    async fn producer_terminal_payload_preserves_runtime_provider_diagnostic() {
        let run_id = Uuid::new_v4();
        let parent_agent_id = Uuid::new_v4();
        let child_agent_id = Uuid::new_v4();
        let child_frame_id = Uuid::new_v4();
        let gate = open_companion_gate(
            run_id,
            parent_agent_id,
            child_agent_id,
            child_frame_id,
            json!({ "preview": "review requested" }),
        );
        let (gate_repo, service) = service_fixture(&gate, run_id, parent_agent_id).await;
        let mut event = terminal_event(run_id, child_agent_id, child_frame_id, "failed");
        event.terminal_diagnostic = Some(RuntimeTerminalDiagnostic {
            kind: "provider".to_string(),
            code: Some("invalid_request".to_string()),
            http_status: Some(400),
            provider: Some("Example LLM".to_string()),
            model: Some("example-chat-large".to_string()),
            message: "request rejected by provider".to_string(),
            retryable: false,
        });

        let result = service
            .observe_producer_terminal(event)
            .await
            .expect("terminal convergence");
        assert_eq!(result.outcomes.len(), 1);

        let stored = gate_repo
            .get(gate.id)
            .await
            .expect("load gate")
            .expect("gate");
        let payload = stored.payload_json.as_ref().expect("payload");
        assert_eq!(
            payload["summary"],
            json!("Example LLM returned 400 invalid_request: request rejected by provider")
        );
        assert_eq!(payload["diagnostic"]["kind"], json!("provider"));
        assert_eq!(payload["diagnostic"]["code"], json!("invalid_request"));
        assert_eq!(payload["diagnostic"]["http_status"], json!(400));
        assert_eq!(payload["diagnostic"]["provider"], json!("Example LLM"));
        assert_eq!(payload["diagnostic"]["model"], json!("example-chat-large"));
        assert_eq!(payload["diagnostic"]["retryable"], json!(false));
    }

    #[tokio::test]
    async fn producer_terminal_payload_uses_last_agent_message_when_no_diagnostic() {
        let run_id = Uuid::new_v4();
        let parent_agent_id = Uuid::new_v4();
        let child_agent_id = Uuid::new_v4();
        let child_frame_id = Uuid::new_v4();
        let gate = open_companion_gate(
            run_id,
            parent_agent_id,
            child_agent_id,
            child_frame_id,
            json!({ "preview": "review requested" }),
        );
        let (gate_repo, service) = service_fixture(&gate, run_id, parent_agent_id).await;
        let mut event = terminal_event(run_id, child_agent_id, child_frame_id, "completed");
        event.producer_last_message = Some(ProducerLastMessageEvidence {
            summary: "我已经完成 review，但忘记调用 companion_respond。".to_string(),
            message_path: format!(
                "lifecycle://agent-runs/{child_agent_id}/sessions/messages/0002__msg-agent__agent__review.md"
            ),
            journal_session_id: format!("agentrun:{run_id}:{child_agent_id}"),
            source_event_seq: 42,
        });

        let result = service
            .observe_producer_terminal(event)
            .await
            .expect("terminal convergence");
        assert_eq!(result.outcomes.len(), 1);

        let stored = gate_repo
            .get(gate.id)
            .await
            .expect("load gate")
            .expect("gate");
        let payload = stored.payload_json.as_ref().expect("payload");
        assert_eq!(
            payload["summary"],
            json!("我已经完成 review，但忘记调用 companion_respond。")
        );
        assert_eq!(payload["failure_kind"], json!("missing_companion_respond"));
        assert_eq!(payload["source"], json!("producer_terminal"));
        assert_eq!(
            payload["fallback_message"]["message_path"],
            json!(format!(
                "lifecycle://agent-runs/{child_agent_id}/sessions/messages/0002__msg-agent__agent__review.md"
            ))
        );
        assert_eq!(payload["fallback_message"]["source_event_seq"], json!(42));
    }

    #[tokio::test]
    async fn producer_terminal_diagnostic_summary_wins_over_last_agent_message() {
        let run_id = Uuid::new_v4();
        let parent_agent_id = Uuid::new_v4();
        let child_agent_id = Uuid::new_v4();
        let child_frame_id = Uuid::new_v4();
        let gate = open_companion_gate(
            run_id,
            parent_agent_id,
            child_agent_id,
            child_frame_id,
            json!({ "preview": "review requested" }),
        );
        let (gate_repo, service) = service_fixture(&gate, run_id, parent_agent_id).await;
        let mut event = terminal_event(run_id, child_agent_id, child_frame_id, "failed");
        event.terminal_diagnostic = Some(RuntimeTerminalDiagnostic {
            kind: "provider".to_string(),
            code: Some("rate_limit".to_string()),
            http_status: Some(429),
            provider: Some("Example LLM".to_string()),
            model: None,
            message: "too many requests".to_string(),
            retryable: true,
        });
        event.producer_last_message = Some(ProducerLastMessageEvidence {
            summary: "这条不应该成为顶层 summary。".to_string(),
            message_path: format!(
                "lifecycle://agent-runs/{child_agent_id}/sessions/messages/0002__msg-agent__agent__ignored.md"
            ),
            journal_session_id: format!("agentrun:{run_id}:{child_agent_id}"),
            source_event_seq: 8,
        });

        service
            .observe_producer_terminal(event)
            .await
            .expect("terminal convergence");

        let stored = gate_repo
            .get(gate.id)
            .await
            .expect("load gate")
            .expect("gate");
        let payload = stored.payload_json.as_ref().expect("payload");
        assert_eq!(
            payload["summary"],
            json!("Example LLM returned 429 rate_limit: too many requests")
        );
        assert_eq!(payload["fallback_message"]["source_event_seq"], json!(8));
    }

    #[tokio::test]
    async fn producer_terminal_does_not_overwrite_existing_result() {
        let run_id = Uuid::new_v4();
        let parent_agent_id = Uuid::new_v4();
        let child_agent_id = Uuid::new_v4();
        let child_frame_id = Uuid::new_v4();
        let mut gate = open_companion_gate(
            run_id,
            parent_agent_id,
            child_agent_id,
            child_frame_id,
            json!({ "preview": "normal companion result" }),
        );
        if let Some(payload) = gate.payload_json.as_mut() {
            payload["status"] = json!("completed");
            payload["source"] = json!("companion_respond");
        }
        gate.resolve("child_agent:normal-result");
        let initial_payload = gate.payload_json.clone();
        let (gate_repo, service) = service_fixture(&gate, run_id, parent_agent_id).await;

        let result = service
            .observe_producer_terminal(terminal_event(
                run_id,
                child_agent_id,
                child_frame_id,
                "failed",
            ))
            .await
            .expect("convergence");

        assert_eq!(result.outcomes.len(), 1);
        assert_eq!(
            result.outcomes[0].kind,
            GateProducerTerminalConvergenceOutcomeKind::AlreadyResolvedEnsuredDelivery
        );
        assert_eq!(
            result.outcomes[0].result_status.as_deref(),
            Some("completed")
        );
        assert_eq!(result.outcomes[0].delivery_intents.len(), 1);
        let stored = gate_repo
            .get(gate.id)
            .await
            .expect("load gate")
            .expect("gate");
        assert_eq!(stored.payload_json, initial_payload);
    }

    #[tokio::test]
    async fn producer_terminal_race_with_result_ensures_existing_delivery() {
        let run_id = Uuid::new_v4();
        let parent_agent_id = Uuid::new_v4();
        let child_agent_id = Uuid::new_v4();
        let child_frame_id = Uuid::new_v4();
        let gate = open_companion_gate(
            run_id,
            parent_agent_id,
            child_agent_id,
            child_frame_id,
            json!({ "preview": "review requested" }),
        );
        let (gate_repo, service) = service_fixture(&gate, run_id, parent_agent_id).await;
        *gate_repo.resolve_on_next_get.lock().unwrap() = Some(gate.id);

        let result = service
            .observe_producer_terminal(terminal_event(
                run_id,
                child_agent_id,
                child_frame_id,
                "failed",
            ))
            .await
            .expect("convergence");

        assert_eq!(result.outcomes.len(), 1);
        assert_eq!(
            result.outcomes[0].kind,
            GateProducerTerminalConvergenceOutcomeKind::AlreadyResolvedEnsuredDelivery
        );
        assert_eq!(
            result.outcomes[0].result_status.as_deref(),
            Some("completed")
        );
        assert_eq!(result.outcomes[0].delivery_intents.len(), 1);
        let stored = gate_repo
            .get(gate.id)
            .await
            .expect("load gate")
            .expect("gate");
        let payload = stored.payload_json.expect("payload");
        assert_eq!(payload["status"], json!("completed"));
        assert_eq!(payload["source"], json!("companion_respond"));
        assert_eq!(payload["summary"], json!("normal companion result"));
    }
}
