use std::sync::Arc;

use agentdash_domain::workflow::{
    AgentRunDeliveryBindingRepository, LifecycleGate, LifecycleGateRepository,
    WaitObligationDeclaration, WaitProducerRef,
};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::WorkflowApplicationError;

use super::{
    CompleteChildResultGateCommand, GateDeliveryIntent, GateNotificationIntent,
    LifecycleGateResolver, outcome::CompanionChildResultDeliveryIntent,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WaitProducerTerminalEvent {
    pub producer: WaitProducerRef,
    pub terminal_state: String,
    pub terminal_message: Option<String>,
    pub source_turn_id: Option<String>,
    pub trace_ref: Option<String>,
}

#[derive(Debug, Clone)]
pub struct WaitObligationConvergenceResult {
    pub outcomes: Vec<WaitObligationConvergenceOutcome>,
}

impl WaitObligationConvergenceResult {
    pub fn no_matching_obligation(&self) -> bool {
        self.outcomes.is_empty()
    }
}

#[derive(Debug, Clone)]
pub struct WaitObligationConvergenceOutcome {
    pub gate_id: Uuid,
    pub kind: WaitObligationConvergenceOutcomeKind,
    pub result_status: Option<String>,
    pub delivery_intents: Vec<GateDeliveryIntent>,
    pub notification_intents: Vec<GateNotificationIntent>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitObligationConvergenceOutcomeKind {
    Resolved,
    AlreadyResolvedEnsuredDelivery,
}

#[derive(Clone)]
pub struct WaitObligationConvergenceService {
    gate_repo: Arc<dyn LifecycleGateRepository>,
    delivery_binding_repo: Arc<dyn AgentRunDeliveryBindingRepository>,
}

impl WaitObligationConvergenceService {
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
        event: WaitProducerTerminalEvent,
    ) -> Result<WaitObligationConvergenceResult, WorkflowApplicationError> {
        let gates = self
            .gate_repo
            .list_by_wait_producer(&event.producer)
            .await?;
        let mut outcomes = Vec::new();
        for gate in gates {
            let Some(declaration) = gate
                .payload_json
                .as_ref()
                .and_then(WaitObligationDeclaration::from_payload)
            else {
                continue;
            };
            if declaration.expected_result.kind != "companion_result" {
                continue;
            }
            let outcome = if gate.is_open() {
                self.resolve_open_gate_from_terminal(gate, declaration, &event)
                    .await?
            } else {
                self.ensure_resolved_gate_delivery(gate, declaration, &event)
                    .await?
            };
            outcomes.push(outcome);
        }
        Ok(WaitObligationConvergenceResult { outcomes })
    }

    async fn resolve_open_gate_from_terminal(
        &self,
        gate: LifecycleGate,
        declaration: WaitObligationDeclaration,
        event: &WaitProducerTerminalEvent,
    ) -> Result<WaitObligationConvergenceOutcome, WorkflowApplicationError> {
        let result_payload = producer_terminal_result_payload(&declaration, event);
        let result_status = result_payload
            .get("status")
            .and_then(Value::as_str)
            .map(str::to_string);
        let intent = self
            .companion_child_result_intent(&gate, &declaration, event, result_payload.clone())
            .await?;
        let outcome = match LifecycleGateResolver::new(self.gate_repo.clone())
            .complete_child_result(CompleteChildResultGateCommand {
                gate_id: gate.id,
                request_id: intent.request_id.clone(),
                run_id: intent.run_id,
                parent_agent_id: intent.parent_agent_id,
                parent_delivery_runtime_session_id: intent
                    .parent_delivery_runtime_session_id
                    .clone(),
                child_agent_id: intent.child_agent_id,
                child_delivery_runtime_session_id: intent.child_delivery_runtime_session_id.clone(),
                resolved_turn_id: intent.resolved_turn_id.clone(),
                companion_label: companion_label(&gate),
                payload: result_payload,
                resolved_by: producer_resolved_by(&event.producer),
            })
            .await
        {
            Ok(outcome) => outcome,
            Err(WorkflowApplicationError::Conflict(message)) => {
                let Some(latest_gate) = self.gate_repo.get(gate.id).await? else {
                    return Err(WorkflowApplicationError::NotFound(format!(
                        "wait obligation gate {} disappeared after resolve conflict",
                        gate.id
                    )));
                };
                if latest_gate.is_open() {
                    return Err(WorkflowApplicationError::Conflict(message));
                }
                let latest_declaration = latest_gate
                    .payload_json
                    .as_ref()
                    .and_then(WaitObligationDeclaration::from_payload)
                    .unwrap_or(declaration);
                return self
                    .ensure_resolved_gate_delivery(latest_gate, latest_declaration, event)
                    .await;
            }
            Err(error) => return Err(error),
        };

        Ok(WaitObligationConvergenceOutcome {
            gate_id: gate.id,
            kind: WaitObligationConvergenceOutcomeKind::Resolved,
            result_status,
            delivery_intents: outcome.delivery_intents,
            notification_intents: outcome.notification_intents,
        })
    }

    async fn ensure_resolved_gate_delivery(
        &self,
        gate: LifecycleGate,
        declaration: WaitObligationDeclaration,
        event: &WaitProducerTerminalEvent,
    ) -> Result<WaitObligationConvergenceOutcome, WorkflowApplicationError> {
        let payload = gate.payload_json.clone().unwrap_or_else(|| json!({}));
        let result_status = payload
            .get("status")
            .and_then(Value::as_str)
            .map(str::to_string);
        let intent = self
            .companion_child_result_intent(&gate, &declaration, event, payload)
            .await?;
        Ok(WaitObligationConvergenceOutcome {
            gate_id: gate.id,
            kind: WaitObligationConvergenceOutcomeKind::AlreadyResolvedEnsuredDelivery,
            result_status,
            delivery_intents: vec![GateDeliveryIntent::CompanionChildResultToParent(intent)],
            notification_intents: Vec::new(),
        })
    }

    async fn companion_child_result_intent(
        &self,
        gate: &LifecycleGate,
        declaration: &WaitObligationDeclaration,
        event: &WaitProducerTerminalEvent,
        payload: Value,
    ) -> Result<CompanionChildResultDeliveryIntent, WorkflowApplicationError> {
        let WaitProducerRef::AgentRunDelivery { agent_id, .. } = &event.producer;
        let parent_binding = self
            .delivery_binding_repo
            .get_current(
                declaration.wake.target_run_id,
                declaration.wake.target_agent_id,
            )
            .await?
            .ok_or_else(|| {
                WorkflowApplicationError::Conflict(format!(
                    "wait obligation gate {} 缺少 parent delivery binding",
                    gate.id
                ))
            })?;
        Ok(CompanionChildResultDeliveryIntent {
            gate_id: gate.id,
            request_id: payload_request_id(gate, declaration, &payload),
            run_id: declaration.wake.target_run_id,
            parent_agent_id: declaration.wake.target_agent_id,
            parent_delivery_runtime_session_id: parent_binding.runtime_session_id,
            child_agent_id: *agent_id,
            child_delivery_runtime_session_id: event.trace_ref.clone(),
            resolved_turn_id: payload
                .get("resolved_turn_id")
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| event.source_turn_id.clone())
                .unwrap_or_else(|| "producer-terminal".to_string()),
            payload,
        })
    }
}

fn producer_terminal_result_payload(
    declaration: &WaitObligationDeclaration,
    event: &WaitProducerTerminalEvent,
) -> Value {
    let declared_status = declaration
        .on_producer_terminal_without_result
        .result_for_terminal_state(&event.terminal_state);
    let (status, failure_kind, summary) = match declared_status {
        "cancelled" => (
            "cancelled",
            "runtime_terminal_cancelled",
            "SubAgent runtime was interrupted before companion_respond.",
        ),
        "protocol_failed" => (
            "failed",
            "missing_companion_respond",
            "SubAgent runtime completed without companion_respond.",
        ),
        _ => (
            "failed",
            "runtime_terminal_failed",
            "SubAgent runtime failed before companion_respond.",
        ),
    };
    json!({
        "status": status,
        "declared_status": declared_status,
        "summary": summary,
        "terminal_state": event.terminal_state,
        "terminal_message": event.terminal_message,
        "delivery_trace_ref": event.trace_ref,
        "resolved_turn_id": event.source_turn_id,
        "failure_kind": failure_kind,
        "source": "producer_terminal",
        "findings": [],
        "follow_ups": [],
        "artifact_refs": [],
    })
}

fn payload_request_id(
    gate: &LifecycleGate,
    declaration: &WaitObligationDeclaration,
    payload: &Value,
) -> String {
    payload
        .get("request_id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| declaration.expected_result.correlation_ref.clone())
        .unwrap_or_else(|| gate.correlation_id.clone())
}

fn companion_label(gate: &LifecycleGate) -> String {
    gate.payload_json
        .as_ref()
        .and_then(|payload| payload.get("companion_label"))
        .and_then(Value::as_str)
        .unwrap_or("companion")
        .to_string()
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
        workflow::{AgentRunDeliveryBinding, DeliveryBindingStatus, RuntimeSessionExecutionAnchor},
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
                        .and_then(WaitObligationDeclaration::from_payload)
                        .is_some_and(|declaration| declaration.wait_source.producer == *producer)
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

    #[tokio::test]
    async fn producer_terminal_resolves_once_then_ensures_delivery() {
        let run_id = Uuid::new_v4();
        let parent_agent_id = Uuid::new_v4();
        let child_agent_id = Uuid::new_v4();
        let child_frame_id = Uuid::new_v4();
        let mut gate = LifecycleGate::open(
            run_id,
            Some(child_agent_id),
            Some(child_frame_id),
            "companion_wait_follow_up",
            "dispatch-1",
            Some(json!({ "companion_label": "reviewer" })),
        );
        let declaration = WaitObligationDeclaration::companion_agent_run_delivery(
            run_id,
            child_agent_id,
            Some(child_frame_id),
            "dispatch-1",
            run_id,
            parent_agent_id,
            gate.id,
        );
        gate.payload_json = Some(
            declaration
                .write_into_payload(gate.payload_json.take())
                .expect("declaration payload"),
        );

        let gate_repo = Arc::new(FixtureGateRepo::default());
        gate_repo.create(&gate).await.expect("seed gate");
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
        let service = WaitObligationConvergenceService::new(gate_repo.clone(), delivery_repo);
        let event = WaitProducerTerminalEvent {
            producer: WaitProducerRef::AgentRunDelivery {
                run_id,
                agent_id: child_agent_id,
                frame_id: Some(child_frame_id),
            },
            terminal_state: "completed".to_string(),
            terminal_message: None,
            source_turn_id: Some("child-turn".to_string()),
            trace_ref: Some("child-session".to_string()),
        };

        let first = service
            .observe_producer_terminal(event.clone())
            .await
            .expect("first convergence");
        assert_eq!(first.outcomes.len(), 1);
        assert_eq!(
            first.outcomes[0].kind,
            WaitObligationConvergenceOutcomeKind::Resolved
        );
        let stored = gate_repo
            .get(gate.id)
            .await
            .expect("load gate")
            .expect("gate");
        let payload = stored.payload_json.as_ref().expect("payload");
        assert_eq!(payload["status"], json!("failed"));
        assert_eq!(payload["declared_status"], json!("protocol_failed"));
        assert_eq!(payload["failure_kind"], json!("missing_companion_respond"));
        assert_eq!(payload["source"], json!("producer_terminal"));
        assert!(payload.get("wait_source").is_some());

        let replay = service
            .observe_producer_terminal(event)
            .await
            .expect("replay convergence");
        assert_eq!(replay.outcomes.len(), 1);
        assert_eq!(
            replay.outcomes[0].kind,
            WaitObligationConvergenceOutcomeKind::AlreadyResolvedEnsuredDelivery
        );
        let stored_after_replay = gate_repo
            .get(gate.id)
            .await
            .expect("load gate")
            .expect("gate");
        assert_eq!(stored_after_replay.payload_json, stored.payload_json);
    }

    #[tokio::test]
    async fn producer_terminal_does_not_overwrite_existing_companion_result() {
        let run_id = Uuid::new_v4();
        let parent_agent_id = Uuid::new_v4();
        let child_agent_id = Uuid::new_v4();
        let child_frame_id = Uuid::new_v4();
        let mut gate = LifecycleGate::open(
            run_id,
            Some(child_agent_id),
            Some(child_frame_id),
            "companion_wait_follow_up",
            "dispatch-1",
            Some(json!({ "status": "completed", "source": "companion_respond" })),
        );
        let declaration = WaitObligationDeclaration::companion_agent_run_delivery(
            run_id,
            child_agent_id,
            Some(child_frame_id),
            "dispatch-1",
            run_id,
            parent_agent_id,
            gate.id,
        );
        gate.payload_json = Some(
            declaration
                .write_into_payload(gate.payload_json.take())
                .expect("declaration payload"),
        );
        gate.resolve("child_agent:normal-result");
        let initial_payload = gate.payload_json.clone();

        let gate_repo = Arc::new(FixtureGateRepo::default());
        gate_repo.create(&gate).await.expect("seed gate");
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
        let service = WaitObligationConvergenceService::new(gate_repo.clone(), delivery_repo);

        let result = service
            .observe_producer_terminal(WaitProducerTerminalEvent {
                producer: WaitProducerRef::AgentRunDelivery {
                    run_id,
                    agent_id: child_agent_id,
                    frame_id: Some(child_frame_id),
                },
                terminal_state: "failed".to_string(),
                terminal_message: Some("late failure".to_string()),
                source_turn_id: Some("child-turn".to_string()),
                trace_ref: Some("child-session".to_string()),
            })
            .await
            .expect("convergence");

        assert_eq!(result.outcomes.len(), 1);
        assert_eq!(
            result.outcomes[0].kind,
            WaitObligationConvergenceOutcomeKind::AlreadyResolvedEnsuredDelivery
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
    async fn producer_terminal_race_with_companion_result_ensures_existing_delivery() {
        let run_id = Uuid::new_v4();
        let parent_agent_id = Uuid::new_v4();
        let child_agent_id = Uuid::new_v4();
        let child_frame_id = Uuid::new_v4();
        let mut gate = LifecycleGate::open(
            run_id,
            Some(child_agent_id),
            Some(child_frame_id),
            "companion_wait_follow_up",
            "dispatch-1",
            Some(json!({ "companion_label": "reviewer" })),
        );
        let declaration = WaitObligationDeclaration::companion_agent_run_delivery(
            run_id,
            child_agent_id,
            Some(child_frame_id),
            "dispatch-1",
            run_id,
            parent_agent_id,
            gate.id,
        );
        gate.payload_json = Some(
            declaration
                .write_into_payload(gate.payload_json.take())
                .expect("declaration payload"),
        );

        let gate_repo = Arc::new(FixtureGateRepo::default());
        gate_repo.create(&gate).await.expect("seed gate");
        *gate_repo.resolve_on_next_get.lock().unwrap() = Some(gate.id);
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
        let service = WaitObligationConvergenceService::new(gate_repo.clone(), delivery_repo);

        let result = service
            .observe_producer_terminal(WaitProducerTerminalEvent {
                producer: WaitProducerRef::AgentRunDelivery {
                    run_id,
                    agent_id: child_agent_id,
                    frame_id: Some(child_frame_id),
                },
                terminal_state: "failed".to_string(),
                terminal_message: Some("late failure".to_string()),
                source_turn_id: Some("child-turn".to_string()),
                trace_ref: Some("child-session".to_string()),
            })
            .await
            .expect("convergence");

        assert_eq!(result.outcomes.len(), 1);
        assert_eq!(
            result.outcomes[0].kind,
            WaitObligationConvergenceOutcomeKind::AlreadyResolvedEnsuredDelivery
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
