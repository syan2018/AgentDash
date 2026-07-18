use std::sync::Arc;

use agentdash_domain::workflow::{
    GateWaitPolicyEnvelope, GateWaitPolicyTemplate, LifecycleGate, LifecycleGateRepository,
};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::WorkflowApplicationError;

use super::child_evidence::child_evidence_result_refs;
use super::commands::{
    CompleteChildResultGateCommand, LifecycleGateCommand, OpenCompanionGateCommand,
    OpenParentRequestGateCommand, OpenWorkflowHumanGateCommand, ResolveGatePayloadCommand,
    ResolveParentRequestGateCommand, ResolveWorkflowHumanGateCommand, RespondHumanGateCommand,
};
use super::outcome::{
    CompanionChildResultDeliveryIntent, CompanionHumanResponseDeliveryIntent,
    CompanionParentRequestDeliveryIntent, CompanionParentResponseDeliveryIntent,
    GateDeliveryIntent, GateTransitionKind, GateTransitionOutcome,
};

const WORKFLOW_HUMAN_GATE_KIND: &str = "orchestration_human_gate";
const COMPANION_PARENT_REQUEST_GATE_KIND: &str = "companion_parent_request";

#[derive(Clone)]
pub struct LifecycleGateResolver {
    gate_repo: Arc<dyn LifecycleGateRepository>,
}

impl LifecycleGateResolver {
    pub fn new(gate_repo: Arc<dyn LifecycleGateRepository>) -> Self {
        Self { gate_repo }
    }

    pub async fn execute(
        &self,
        command: LifecycleGateCommand,
    ) -> Result<GateTransitionOutcome, WorkflowApplicationError> {
        match command {
            LifecycleGateCommand::OpenCompanionGate(command) => {
                self.open_companion_gate(command).await
            }
            LifecycleGateCommand::OpenWorkflowHumanGate(command) => {
                self.open_workflow_human_gate(command).await
            }
            LifecycleGateCommand::ResolveWorkflowHumanGate(command) => {
                self.resolve_workflow_human_gate(command).await
            }
            LifecycleGateCommand::RespondHuman(command) => self.respond_human(command).await,
            LifecycleGateCommand::OpenParentRequest(command) => {
                self.open_parent_request(command).await
            }
            LifecycleGateCommand::ResolveParentRequest(command) => {
                self.resolve_parent_request(command).await
            }
            LifecycleGateCommand::CompleteChildResult(command) => {
                self.complete_child_result(command).await
            }
            LifecycleGateCommand::ResolveGatePayload(command) => {
                self.resolve_gate_payload(command).await
            }
        }
    }

    pub async fn open_companion_gate(
        &self,
        command: OpenCompanionGateCommand,
    ) -> Result<GateTransitionOutcome, WorkflowApplicationError> {
        Self::open_companion_gate_with_repo(self.gate_repo.as_ref(), command).await
    }

    pub async fn open_companion_gate_with_repo(
        gate_repo: &dyn LifecycleGateRepository,
        command: OpenCompanionGateCommand,
    ) -> Result<GateTransitionOutcome, WorkflowApplicationError> {
        if !command.gate_kind.starts_with("companion_") {
            return Err(WorkflowApplicationError::BadRequest(format!(
                "companion gate kind 必须以 companion_ 开头: {}",
                command.gate_kind
            )));
        }
        let gate = LifecycleGate::open(
            command.run_id,
            Some(command.agent_id),
            command.frame_id,
            command.gate_kind,
            command.correlation_id,
            command.payload,
        );
        let gate = attach_gate_wait_policy(gate, command.wait_policy)?;
        gate_repo.create(&gate).await?;

        Ok(GateTransitionOutcome {
            gate,
            transition: GateTransitionKind::Opened,
            delivery_intents: Vec::new(),
        })
    }

    pub async fn open_workflow_human_gate(
        &self,
        command: OpenWorkflowHumanGateCommand,
    ) -> Result<GateTransitionOutcome, WorkflowApplicationError> {
        let gate = LifecycleGate::open(
            command.run_id,
            None,
            None,
            WORKFLOW_HUMAN_GATE_KIND,
            workflow_human_gate_correlation_id(
                command.orchestration_id,
                &command.node_path,
                command.attempt,
            ),
            Some(json!({
                "contract": "orchestration_human_gate.v1",
                "run_id": command.run_id,
                "orchestration_id": command.orchestration_id,
                "node_path": command.node_path,
                "attempt": command.attempt,
                "plan_node_id": command.plan_node_id,
                "label": command.label,
                "executor": command.executor,
            })),
        );
        self.gate_repo.create(&gate).await?;

        Ok(GateTransitionOutcome {
            gate,
            transition: GateTransitionKind::Opened,
            delivery_intents: Vec::new(),
        })
    }

    pub async fn resolve_workflow_human_gate(
        &self,
        command: ResolveWorkflowHumanGateCommand,
    ) -> Result<GateTransitionOutcome, WorkflowApplicationError> {
        let mut gate = self.load_open_gate(command.gate_id).await?;
        if gate.gate_kind != WORKFLOW_HUMAN_GATE_KIND {
            return Err(WorkflowApplicationError::Conflict(format!(
                "gate {} 不是 workflow HumanGate",
                gate.id
            )));
        }
        gate.payload_json = Some(command.decision);
        gate.resolve(command.resolved_by);
        self.gate_repo.update(&gate).await?;

        Ok(GateTransitionOutcome {
            gate,
            transition: GateTransitionKind::Resolved,
            delivery_intents: Vec::new(),
        })
    }

    pub async fn respond_human(
        &self,
        command: RespondHumanGateCommand,
    ) -> Result<GateTransitionOutcome, WorkflowApplicationError> {
        let mut gate = self.load_open_gate(command.gate_id).await?;
        let metadata = gate.payload_json.clone();
        let request_type = metadata
            .as_ref()
            .and_then(|payload| payload.get("request_type"))
            .and_then(Value::as_str)
            .map(str::to_string);
        let turn_id = metadata
            .as_ref()
            .and_then(|payload| payload.get("turn_id"))
            .and_then(Value::as_str)
            .map(str::to_string);
        let agent_id = gate.agent_id.ok_or_else(|| {
            WorkflowApplicationError::Conflict(format!(
                "human response gate {} 缺少 requesting agent owner",
                gate.id
            ))
        })?;
        let request_id = gate.id.to_string();
        let intent =
            GateDeliveryIntent::CompanionHumanResponse(CompanionHumanResponseDeliveryIntent {
                gate_id: gate.id,
                request_id: request_id.clone(),
                run_id: gate.run_id,
                agent_id,
                turn_id,
                request_type,
                payload: command.payload.clone(),
            });

        gate.payload_json = Some(command.payload);
        gate.resolve(command.resolved_by);
        self.gate_repo.update(&gate).await?;

        Ok(GateTransitionOutcome {
            gate,
            transition: GateTransitionKind::Resolved,
            delivery_intents: vec![intent],
        })
    }

    pub async fn open_parent_request(
        &self,
        command: OpenParentRequestGateCommand,
    ) -> Result<GateTransitionOutcome, WorkflowApplicationError> {
        let mut gate = LifecycleGate::open(
            command.run_id,
            Some(command.parent_agent_id),
            Some(command.parent_frame_id),
            COMPANION_PARENT_REQUEST_GATE_KIND,
            "pending-parent-request",
            None,
        );
        gate.correlation_id = gate.id.to_string();
        let request_id = gate.id.to_string();
        let payload = json!({
            "gate_id": request_id,
            "request_id": request_id,
            "run_id": command.run_id.to_string(),
            "child_agent_id": command.child_agent_id.to_string(),
            "child_frame_id": command.child_frame_id.to_string(),
            "parent_agent_id": command.parent_agent_id.to_string(),
            "parent_frame_id": command.parent_frame_id.to_string(),
            "companion_label": command.companion_label,
            "companion_session_id": command.child_runtime_thread_id,
            "child_runtime_thread_id": command.child_runtime_thread_id,
            "parent_session_id": command.parent_runtime_thread_id,
            "parent_runtime_thread_id": command.parent_runtime_thread_id,
            "request_type": "review",
            "adoption_mode": agentdash_platform_spi::action_type::FOLLOW_UP_REQUIRED,
            "status": "pending",
            "summary": command.message,
            "turn_id": command.turn_id,
            "wait": command.wait,
            "payload": command.payload,
        });
        gate.payload_json = Some(payload.clone());
        self.gate_repo.create(&gate).await?;

        let delivery_intent =
            GateDeliveryIntent::CompanionParentRequest(CompanionParentRequestDeliveryIntent {
                gate_id: gate.id,
                request_id: gate.id.to_string(),
                run_id: command.run_id,
                parent_agent_id: command.parent_agent_id,
                parent_runtime_thread_id: command.parent_runtime_thread_id.clone(),
                child_agent_id: command.child_agent_id,
                child_runtime_thread_id: command.child_runtime_thread_id,
                turn_id: command.turn_id.clone(),
                wait: command.wait,
                payload: payload.clone(),
            });
        Ok(GateTransitionOutcome {
            gate,
            transition: GateTransitionKind::Opened,
            delivery_intents: vec![delivery_intent],
        })
    }

    pub async fn resolve_parent_request(
        &self,
        command: ResolveParentRequestGateCommand,
    ) -> Result<GateTransitionOutcome, WorkflowApplicationError> {
        let mut gate = self.load_open_gate(command.gate_id).await?;
        if gate.gate_kind != COMPANION_PARENT_REQUEST_GATE_KIND {
            return Err(WorkflowApplicationError::Conflict(format!(
                "gate {} 不是 companion parent request",
                gate.id
            )));
        }
        if gate.agent_id != Some(command.parent_agent_id)
            || gate.frame_id != Some(command.parent_frame_id)
        {
            return Err(WorkflowApplicationError::Conflict(format!(
                "parent request gate {} 不属于当前 parent frame {}",
                gate.id, command.parent_frame_id
            )));
        }

        let mut payload = command.payload.clone();
        let object = payload.as_object_mut().ok_or_else(|| {
            WorkflowApplicationError::BadRequest("payload 必须是 JSON object".to_string())
        })?;
        object.insert("gate_id".to_string(), json!(gate.id.to_string()));
        object.insert("request_id".to_string(), json!(gate.id.to_string()));
        object.insert(
            "resolved_turn_id".to_string(),
            json!(command.resolved_turn_id.clone()),
        );
        object.insert("run_id".to_string(), json!(command.run_id.to_string()));
        object.insert(
            "parent_agent_id".to_string(),
            json!(command.parent_agent_id.to_string()),
        );
        object.insert(
            "parent_frame_id".to_string(),
            json!(command.parent_frame_id.to_string()),
        );
        object.insert(
            "parent_runtime_thread_id".to_string(),
            json!(command.parent_runtime_thread_id.clone()),
        );
        object.insert(
            "child_agent_id".to_string(),
            json!(command.child_agent_id.to_string()),
        );
        object.insert(
            "child_frame_id".to_string(),
            json!(command.child_frame_id.to_string()),
        );
        object.insert(
            "child_runtime_thread_id".to_string(),
            json!(command.child_runtime_thread_id.clone()),
        );

        gate.payload_json = Some(payload.clone());
        gate.resolve(command.resolved_by);
        self.gate_repo.update(&gate).await?;

        let delivery_intent = GateDeliveryIntent::CompanionParentResponseToChild(
            CompanionParentResponseDeliveryIntent {
                gate_id: gate.id,
                request_id: gate.id.to_string(),
                run_id: command.run_id,
                parent_agent_id: command.parent_agent_id,
                parent_runtime_thread_id: command.parent_runtime_thread_id.clone(),
                child_agent_id: command.child_agent_id,
                child_runtime_thread_id: command.child_runtime_thread_id,
                resolved_turn_id: command.resolved_turn_id.clone(),
                payload: payload.clone(),
            },
        );
        Ok(GateTransitionOutcome {
            gate,
            transition: GateTransitionKind::Resolved,
            delivery_intents: vec![delivery_intent],
        })
    }

    pub async fn resolve_gate_payload(
        &self,
        command: ResolveGatePayloadCommand,
    ) -> Result<GateTransitionOutcome, WorkflowApplicationError> {
        let mut gate = self.load_open_gate(command.gate_id).await?;
        let existing_payload = gate.payload_json.clone();
        let mut payload = command.payload;
        preserve_wait_policy_metadata(&mut payload, existing_payload.as_ref());
        gate.payload_json = Some(payload.clone());
        gate.resolve(command.resolved_by);
        self.gate_repo.update(&gate).await?;

        Ok(GateTransitionOutcome {
            gate,
            transition: GateTransitionKind::Resolved,
            delivery_intents: Vec::new(),
        })
    }

    pub async fn complete_child_result(
        &self,
        command: CompleteChildResultGateCommand,
    ) -> Result<GateTransitionOutcome, WorkflowApplicationError> {
        let mut gate = self.load_open_gate(command.gate_id).await?;
        let summary = command
            .payload
            .get("summary")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        let status = normalize_companion_result_status(
            command.payload.get("status").and_then(Value::as_str),
        )?;
        let existing_payload = gate.payload_json.clone();
        let result_refs = child_evidence_result_refs(
            gate.id,
            command.run_id,
            command.child_agent_id,
            gate.frame_id,
            command.child_runtime_thread_id.as_deref(),
        );
        let mut payload = json!({
            "gate_id": gate.id.to_string(),
            "request_id": command.request_id,
            "status": status,
            "summary": summary,
            "findings": command.payload.get("findings"),
            "follow_ups": command.payload.get("follow_ups"),
            "artifact_refs": command.payload.get("artifact_refs"),
            "terminal_state": command.payload.get("terminal_state"),
            "terminal_message": command.payload.get("terminal_message"),
            "delivery_trace_ref": command.payload.get("delivery_trace_ref"),
            "failure_kind": command.payload.get("failure_kind"),
            "declared_status": command.payload.get("declared_status"),
            "source": command.payload.get("source"),
            "child_agent_id": command.child_agent_id.to_string(),
            "parent_agent_id": command.parent_agent_id.to_string(),
            "resolved_turn_id": command.resolved_turn_id,
            "result_refs": result_refs,
        });
        preserve_wait_policy_metadata(&mut payload, existing_payload.as_ref());

        gate.payload_json = Some(payload.clone());
        gate.resolve(command.resolved_by);
        self.gate_repo.update(&gate).await?;

        let delivery_intent =
            GateDeliveryIntent::CompanionChildResultToParent(CompanionChildResultDeliveryIntent {
                gate_id: gate.id,
                request_id: command.request_id,
                run_id: command.run_id,
                parent_agent_id: command.parent_agent_id,
                parent_runtime_thread_id: command.parent_runtime_thread_id.clone(),
                child_agent_id: command.child_agent_id,
                child_runtime_thread_id: command.child_runtime_thread_id.clone(),
                resolved_turn_id: command.resolved_turn_id.clone(),
                payload: payload.clone(),
            });
        Ok(GateTransitionOutcome {
            gate,
            transition: GateTransitionKind::Resolved,
            delivery_intents: vec![delivery_intent],
        })
    }

    async fn load_open_gate(
        &self,
        gate_id: Uuid,
    ) -> Result<LifecycleGate, WorkflowApplicationError> {
        let gate =
            self.gate_repo.get(gate_id).await?.ok_or_else(|| {
                WorkflowApplicationError::NotFound(format!("gate 不存在: {gate_id}"))
            })?;
        if !gate.is_open() {
            return Err(WorkflowApplicationError::Conflict(format!(
                "gate {gate_id} 已经 resolved"
            )));
        }
        Ok(gate)
    }
}

fn workflow_human_gate_correlation_id(
    orchestration_id: Uuid,
    node_path: &str,
    attempt: u32,
) -> String {
    format!("orchestration:{orchestration_id}:node:{node_path}:attempt:{attempt}")
}

fn normalize_companion_result_status(
    status: Option<&str>,
) -> Result<&'static str, WorkflowApplicationError> {
    match status.unwrap_or("completed").trim() {
        "" => Ok("completed"),
        "completed" => Ok("completed"),
        "blocked" => Ok("blocked"),
        "needs_follow_up" => Ok("needs_follow_up"),
        "failed" => Ok("failed"),
        "cancelled" => Ok("cancelled"),
        other => Err(WorkflowApplicationError::BadRequest(format!(
            "payload.status 不支持 `{other}`，应为 completed/blocked/needs_follow_up/failed/cancelled"
        ))),
    }
}

fn preserve_wait_policy_metadata(payload: &mut Value, existing: Option<&Value>) {
    let Some(target) = payload.as_object_mut() else {
        return;
    };
    let Some(existing) = existing.and_then(Value::as_object) else {
        return;
    };
    for key in ["schema_version", "wait_policy", "display"] {
        if let Some(value) = existing.get(key) {
            target.insert(key.to_string(), value.clone());
        }
    }
}

fn attach_gate_wait_policy(
    mut gate: LifecycleGate,
    wait_policy: Option<GateWaitPolicyTemplate>,
) -> Result<LifecycleGate, WorkflowApplicationError> {
    let Some(wait_policy) = wait_policy else {
        return Ok(gate);
    };
    let agent_id = gate.agent_id.ok_or_else(|| {
        WorkflowApplicationError::Conflict(format!(
            "companion gate {} missing agent owner for gate wait policy",
            gate.id
        ))
    })?;
    let wait_policy =
        wait_policy.into_agent_run_delivery_policy(gate.run_id, agent_id, gate.frame_id, gate.id);
    let payload = GateWaitPolicyEnvelope::new(wait_policy)
        .write_into_payload(gate.payload_json.take())
        .map_err(|error| {
            WorkflowApplicationError::Internal(format!(
                "gate wait policy payload serialize failed: {error}"
            ))
        })?;
    gate.payload_json = Some(payload);
    Ok(gate)
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
    };

    use agentdash_domain::{
        DomainError,
        workflow::{GateWaitPolicyEnvelope, LifecycleGateRepository, WaitProducerRef},
    };

    use super::*;
    use crate::gate::{
        OpenCompanionGateCommand, OpenParentRequestGateCommand, RespondHumanGateCommand,
    };

    #[derive(Default)]
    struct FixtureGateRepo {
        gates: Mutex<HashMap<Uuid, LifecycleGate>>,
    }

    #[async_trait::async_trait]
    impl LifecycleGateRepository for FixtureGateRepo {
        async fn create(&self, gate: &LifecycleGate) -> Result<(), DomainError> {
            self.gates.lock().unwrap().insert(gate.id, gate.clone());
            Ok(())
        }

        async fn get(&self, id: Uuid) -> Result<Option<LifecycleGate>, DomainError> {
            Ok(self.gates.lock().unwrap().get(&id).cloned())
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
                        .is_some_and(|declaration| declaration.wait_policy.source == *producer)
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

    #[tokio::test]
    async fn respond_human_resolves_gate_without_mailbox_payload_blob() {
        let repo = Arc::new(FixtureGateRepo::default());
        let run_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let gate = LifecycleGate::open(
            run_id,
            Some(agent_id),
            None,
            "companion_human_request",
            "human-request",
            Some(json!({
                "request_type": "review",
                "turn_id": "turn-1",
            })),
        );
        let gate_id = gate.id;
        repo.create(&gate).await.expect("seed gate");

        let outcome = LifecycleGateResolver::new(repo.clone())
            .respond_human(RespondHumanGateCommand {
                gate_id,
                payload: json!({ "status": "approved" }),
                resolved_by: "companion_respond".to_string(),
            })
            .await
            .expect("resolve gate");

        assert_eq!(outcome.transition, GateTransitionKind::Resolved);
        assert_eq!(outcome.delivery_intents.len(), 1);
        let stored = repo.get(gate_id).await.expect("load gate").expect("gate");
        assert!(!stored.is_open());
        assert_eq!(
            stored
                .payload_json
                .as_ref()
                .and_then(|payload| payload.get("status")),
            Some(&json!("approved"))
        );
        assert!(
            stored
                .payload_json
                .as_ref()
                .and_then(|payload| payload.get("human_mailbox_delivery"))
                .is_none()
        );
    }

    #[tokio::test]
    async fn open_companion_gate_creates_request_fact_without_delivery_intents() {
        let repo = Arc::new(FixtureGateRepo::default());
        let run_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let frame_id = Uuid::new_v4();

        let outcome = LifecycleGateResolver::new(repo.clone())
            .open_companion_gate(OpenCompanionGateCommand {
                run_id,
                agent_id,
                frame_id: Some(frame_id),
                gate_kind: "companion_human_request".to_string(),
                correlation_id: "human-request".to_string(),
                payload: Some(json!({
                    "session_id": "requesting-session",
                    "turn_id": "turn-1",
                    "request_type": "review",
                })),
                wait_policy: None,
            })
            .await
            .expect("open companion gate");

        assert_eq!(outcome.transition, GateTransitionKind::Opened);
        assert!(outcome.delivery_intents.is_empty());
        assert_eq!(outcome.gate.run_id, run_id);
        assert_eq!(outcome.gate.agent_id, Some(agent_id));
        assert_eq!(outcome.gate.frame_id, Some(frame_id));
        assert_eq!(outcome.gate.gate_kind, "companion_human_request");
        let stored = repo
            .get(outcome.gate.id)
            .await
            .expect("load gate")
            .expect("gate");
        assert!(stored.is_open());
        assert_eq!(
            stored
                .payload_json
                .as_ref()
                .and_then(|payload| payload.get("human_mailbox_delivery")),
            None
        );
    }

    #[tokio::test]
    async fn open_parent_request_creates_request_fact_without_delivery_status() {
        let repo = Arc::new(FixtureGateRepo::default());
        let run_id = Uuid::new_v4();
        let parent_agent_id = Uuid::new_v4();
        let parent_frame_id = Uuid::new_v4();
        let child_agent_id = Uuid::new_v4();
        let child_frame_id = Uuid::new_v4();

        let outcome = LifecycleGateResolver::new(repo.clone())
            .open_parent_request(OpenParentRequestGateCommand {
                run_id,
                parent_agent_id,
                parent_frame_id,
                parent_runtime_thread_id: "parent-session".to_string(),
                child_agent_id,
                child_frame_id,
                child_runtime_thread_id: "child-session".to_string(),
                turn_id: "turn-1".to_string(),
                wait: true,
                companion_label: "child:test".to_string(),
                message: "please review".to_string(),
                payload: json!({ "message": "please review" }),
            })
            .await
            .expect("open gate");

        assert_eq!(outcome.transition, GateTransitionKind::Opened);
        assert_eq!(outcome.delivery_intents.len(), 1);
        let payload = outcome.gate.payload_json.as_ref().expect("payload");
        assert_eq!(payload["status"], json!("pending"));
        assert!(payload.get("parent_mailbox_delivery").is_none());
    }

    #[tokio::test]
    async fn complete_child_result_includes_child_evidence_locator() {
        let repo = Arc::new(FixtureGateRepo::default());
        let run_id = Uuid::new_v4();
        let parent_agent_id = Uuid::new_v4();
        let child_agent_id = Uuid::new_v4();
        let child_frame_id = Uuid::new_v4();
        let gate = LifecycleGate::open(
            run_id,
            Some(child_agent_id),
            Some(child_frame_id),
            "companion_wait_follow_up",
            "dispatch-1",
            Some(json!({ "summary": "pending" })),
        );
        let gate_id = gate.id;
        repo.create(&gate).await.expect("seed gate");

        let outcome = LifecycleGateResolver::new(repo.clone())
            .complete_child_result(CompleteChildResultGateCommand {
                gate_id,
                request_id: "dispatch-1".to_string(),
                run_id,
                parent_agent_id,
                parent_runtime_thread_id: "parent-session".to_string(),
                child_agent_id,
                child_runtime_thread_id: Some("child-session".to_string()),
                resolved_turn_id: "child-turn".to_string(),
                companion_label: "reviewer".to_string(),
                payload: json!({
                    "status": "completed",
                    "summary": "done",
                    "findings": [],
                    "follow_ups": [],
                    "artifact_refs": []
                }),
                resolved_by: format!("child_agent:{child_agent_id}"),
            })
            .await
            .expect("complete child result");

        let payload = outcome.gate.payload_json.as_ref().expect("payload");
        assert_eq!(payload["status"], json!("completed"));
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
                && entry
                    .get("uri")
                    .and_then(Value::as_str)
                    .is_some_and(|value| value.starts_with("lifecycle://agent-runs/"))
        }));
        assert!(
            !serde_json::to_string(&payload["result_refs"])
                .expect("serialize refs")
                .contains("\"path\"")
        );
    }
}
