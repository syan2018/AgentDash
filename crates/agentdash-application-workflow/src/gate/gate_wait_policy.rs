use std::sync::Arc;

use agentdash_domain::workflow::{
    AgentRunDeliveryBindingRepository, GateWaitPolicyEnvelope, LifecycleGate,
    LifecycleGateRepository, WaitProducerRef,
};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::WorkflowApplicationError;

use super::{
    GateDeliveryIntent, GateMailboxWakeIntent, LifecycleGateResolver, ResolveGatePayloadCommand,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateProducerTerminalEvent {
    pub producer: WaitProducerRef,
    pub terminal_state: String,
    pub terminal_message: Option<String>,
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
    delivery_binding_repo: Arc<dyn AgentRunDeliveryBindingRepository>,
}

impl GateProducerTerminalConvergenceService {
    pub fn new(
        gate_repo: Arc<dyn LifecycleGateRepository>,
        delivery_binding_repo: Arc<dyn AgentRunDeliveryBindingRepository>,
    ) -> Self {
        Self {
            gate_repo,
            delivery_binding_repo,
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
            .delivery_binding_repo
            .get_current(wake_target.target_run_id, wake_target.target_agent_id)
            .await?
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
            target_delivery_runtime_session_id: target_binding.runtime_session_id,
            producer_agent_id: *agent_id,
            producer_delivery_runtime_session_id: event.trace_ref.clone(),
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
    json!({
        "gate_id": gate.id.to_string(),
        "request_id": payload_request_id(gate, envelope, &json!({})),
        "status": terminal_outcome.status,
        "summary": "Producer reached terminal before the expected result was written.",
        "terminal_state": event.terminal_state,
        "terminal_message": event.terminal_message,
        "delivery_trace_ref": event.trace_ref,
        "resolved_turn_id": event.source_turn_id,
        "failure_kind": terminal_outcome.failure_kind,
        "source": "producer_terminal",
        "findings": [],
        "follow_ups": [],
        "artifact_refs": [],
    })
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
        collections::HashMap,
        sync::{Arc, Mutex},
    };

    use agentdash_domain::{
        DomainError,
        workflow::{
            AgentRunDeliveryBinding, DeliveryBindingStatus, GateWaitPolicy,
            RuntimeSessionExecutionAnchor, WaitExpectedResult, WaitTerminalOutcome,
            WaitTerminalPolicy, WaitWakeTarget,
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

    struct FixtureDeliveryBindingRepo {
        binding: AgentRunDeliveryBinding,
    }

    #[async_trait::async_trait]
    impl AgentRunDeliveryBindingRepository for FixtureDeliveryBindingRepo {
        async fn upsert(&self, _binding: &AgentRunDeliveryBinding) -> Result<(), DomainError> {
            Ok(())
        }

        async fn get_current(
            &self,
            run_id: Uuid,
            agent_id: Uuid,
        ) -> Result<Option<AgentRunDeliveryBinding>, DomainError> {
            if self.binding.run_id == run_id && self.binding.agent_id == agent_id {
                Ok(Some(self.binding.clone()))
            } else {
                Ok(None)
            }
        }

        async fn list_by_run(
            &self,
            run_id: Uuid,
        ) -> Result<Vec<AgentRunDeliveryBinding>, DomainError> {
            if self.binding.run_id == run_id {
                Ok(vec![self.binding.clone()])
            } else {
                Ok(Vec::new())
            }
        }

        async fn delete_by_session(&self, _runtime_session_id: &str) -> Result<(), DomainError> {
            Ok(())
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
        let parent_anchor = RuntimeSessionExecutionAnchor::new_dispatch(
            "parent-session".to_string(),
            run_id,
            Uuid::new_v4(),
            parent_agent_id,
        );
        let delivery_repo = Arc::new(FixtureDeliveryBindingRepo {
            binding: AgentRunDeliveryBinding::from_anchor(
                &parent_anchor,
                DeliveryBindingStatus::Running,
                parent_anchor.updated_at,
            ),
        });
        let service = GateProducerTerminalConvergenceService::new(gate_repo.clone(), delivery_repo);
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
