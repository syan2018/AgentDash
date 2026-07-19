use std::sync::Arc;

use agentdash_domain::workflow::{
    ActivityCompletionPolicy, ExecutorRunRef, ExecutorSpec, LifecycleGate, LifecycleRun,
    NodePortValue, PlanNode, PlanNodeKind, RuntimeNodeState, WorkflowExecutorEffectIdentity,
    WorkflowExecutorEffectRepository, WorkflowExecutorEffectRepositoryError,
    WorkflowHumanGateOpenEffect, WorkflowHumanGateResolutionEffect,
    WorkflowHumanGateResolutionReceipt,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::WorkflowApplicationError;

use super::executor_launcher::{OpenedHumanGate, SubmitHumanGateDecisionInput};
use super::ready_node::{ReadyNodeView, RunningNodeView, RuntimeNodeCoordinate};
use super::runtime::OrchestrationRuntimeEvent;

const WORKFLOW_HUMAN_GATE_KIND: &str = "orchestration_human_gate";

#[derive(Clone)]
pub(super) struct HumanGateLauncher {
    effect_repo: Arc<dyn WorkflowExecutorEffectRepository>,
}

impl HumanGateLauncher {
    pub(super) fn new(effect_repo: Arc<dyn WorkflowExecutorEffectRepository>) -> Self {
        Self { effect_repo }
    }

    pub(super) async fn open(
        &self,
        run: &LifecycleRun,
        coordinate: &RuntimeNodeCoordinate,
    ) -> Result<HumanGateOpenOutcome, WorkflowApplicationError> {
        let (plan_node_id, label, executor) = {
            let view = ReadyNodeView::for_coordinate(run, coordinate)?;
            (
                view.plan_node.node_id.clone(),
                view.plan_node.label.clone(),
                view.plan_node.executor.clone(),
            )
        };
        match executor.clone() {
            Some(ExecutorSpec::Human { .. }) => {}
            None => {
                return Ok(HumanGateOpenOutcome::blocked(
                    "human_gate_executor_missing",
                    "HumanGate node 缺少 Human executor spec",
                    false,
                ));
            }
            Some(_) => {
                return Ok(HumanGateOpenOutcome::blocked(
                    "human_gate_executor_mismatch",
                    "HumanGate node 的 executor spec 类型不匹配",
                    false,
                ));
            }
        }

        let identity = human_gate_effect_identity(coordinate, "open");
        let gate_id = stable_uuid(&identity.effect_id);
        let correlation_id = format!(
            "workflow-human-gate:{}:{}#{}",
            coordinate.orchestration_id, coordinate.node_path, coordinate.attempt
        );
        let payload = json!({
            "contract": "orchestration_human_gate.v1",
            "run_id": run.id,
            "orchestration_id": coordinate.orchestration_id,
            "node_path": coordinate.node_path,
            "attempt": coordinate.attempt,
            "plan_node_id": plan_node_id,
            "label": label,
            "executor": executor,
        });
        let payload_digest = digest(&(
            &identity,
            gate_id,
            WORKFLOW_HUMAN_GATE_KIND,
            &correlation_id,
            &payload,
        ));
        let mut gate = LifecycleGate::open(
            run.id,
            None,
            None,
            WORKFLOW_HUMAN_GATE_KIND,
            correlation_id,
            Some(payload),
        );
        gate.id = gate_id;
        let receipt = self
            .effect_repo
            .open_human_gate(WorkflowHumanGateOpenEffect {
                identity,
                payload_digest,
                gate,
            })
            .await
            .map_err(effect_repository_error)?;
        let gate_id = receipt.effect.gate.id;

        Ok(HumanGateOpenOutcome::Opened {
            opened: OpenedHumanGate {
                run_id: coordinate.run_id,
                orchestration_id: coordinate.orchestration_id,
                node_path: coordinate.node_path.clone(),
                attempt: coordinate.attempt,
                gate_id,
            },
            event: Box::new(OrchestrationRuntimeEvent::NodeStarted {
                node_path: coordinate.node_path.clone(),
                attempt: coordinate.attempt,
                executor_run_ref: Some(ExecutorRunRef::HumanDecision {
                    decision_id: gate_id.to_string(),
                }),
                timestamp: chrono::Utc::now(),
            }),
        })
    }

    pub(super) async fn resolve_decision(
        &self,
        run: &LifecycleRun,
        input: &SubmitHumanGateDecisionInput,
        coordinate: &RuntimeNodeCoordinate,
    ) -> Result<HumanGateDecision, WorkflowApplicationError> {
        let (gate_id, outputs) = {
            let view = RunningNodeView::for_coordinate(run, coordinate)?;
            if view.plan_node.kind != PlanNodeKind::HumanGate {
                return Err(WorkflowApplicationError::Conflict(format!(
                    "node {} 不是 HumanGate",
                    input.node_path
                )));
            }
            (
                human_gate_id_from_node(view.runtime_node)?,
                human_decision_outputs(view.plan_node, input.decision.clone()),
            )
        };
        let identity = human_gate_effect_identity(coordinate, "resolve");
        let payload_digest = digest(&(
            &identity,
            gate_id,
            &input.decision,
            &input.resolved_by,
            &outputs,
        ));
        let receipt = self
            .effect_repo
            .resolve_human_gate(WorkflowHumanGateResolutionEffect {
                identity,
                payload_digest,
                gate_id,
                decision: input.decision.clone(),
                resolved_by: input.resolved_by.clone(),
                outputs,
            })
            .await
            .map_err(effect_repository_error)?;
        Ok(HumanGateDecision::from_receipt(receipt))
    }

    pub(super) async fn inspect_resolution(
        &self,
        gate_id: Uuid,
    ) -> Result<Option<HumanGateDecision>, WorkflowApplicationError> {
        self.effect_repo
            .get_human_gate_resolution(gate_id)
            .await
            .map(|receipt| receipt.map(HumanGateDecision::from_receipt))
            .map_err(effect_repository_error)
    }
}

pub(super) enum HumanGateOpenOutcome {
    Opened {
        opened: OpenedHumanGate,
        event: Box<OrchestrationRuntimeEvent>,
    },
    Blocked {
        code: String,
        message: String,
        retryable: bool,
    },
}

impl HumanGateOpenOutcome {
    fn blocked(code: &str, message: impl Into<String>, retryable: bool) -> Self {
        Self::Blocked {
            code: code.to_string(),
            message: message.into(),
            retryable,
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct HumanGateDecision {
    pub(super) gate_id: Uuid,
    pub(super) outputs: Vec<NodePortValue>,
    pub(super) decision: Value,
    pub(super) resolved_by: String,
}

impl HumanGateDecision {
    fn from_receipt(receipt: WorkflowHumanGateResolutionReceipt) -> Self {
        Self {
            gate_id: receipt.effect.gate_id,
            outputs: receipt.effect.outputs,
            decision: receipt.effect.decision,
            resolved_by: receipt.effect.resolved_by,
        }
    }
}

pub(super) fn human_gate_id_from_node(
    node: &RuntimeNodeState,
) -> Result<Uuid, WorkflowApplicationError> {
    match &node.executor_run_ref {
        Some(ExecutorRunRef::HumanDecision { decision_id }) => Uuid::parse_str(decision_id)
            .map_err(|error| {
                WorkflowApplicationError::Internal(format!("decision_id 非 UUID: {error}"))
            }),
        _ => Err(WorkflowApplicationError::Conflict(format!(
            "runtime node {} 没有关联 human decision gate",
            node.node_path
        ))),
    }
}

fn human_decision_outputs(plan_node: &PlanNode, decision: Value) -> Vec<NodePortValue> {
    let decision_port = match &plan_node.completion_policy {
        Some(ActivityCompletionPolicy::HumanDecision { decision_port }) => decision_port.clone(),
        _ => plan_node
            .output_ports
            .iter()
            .find(|port| port.key == "decision")
            .or_else(|| plan_node.output_ports.first())
            .map(|port| port.key.clone())
            .unwrap_or_else(|| "decision".to_string()),
    };
    vec![NodePortValue {
        port_key: decision_port,
        value: decision,
    }]
}

fn human_gate_effect_identity(
    coordinate: &RuntimeNodeCoordinate,
    operation: &str,
) -> WorkflowExecutorEffectIdentity {
    WorkflowExecutorEffectIdentity {
        effect_id: format!(
            "workflow-human-gate-{operation}:{}:{}#{}",
            coordinate.orchestration_id, coordinate.node_path, coordinate.attempt
        ),
        lifecycle_run_id: coordinate.run_id,
        orchestration_id: coordinate.orchestration_id,
        node_path: coordinate.node_path.clone(),
        attempt: coordinate.attempt,
    }
}

fn stable_uuid(seed: &str) -> Uuid {
    let hash = Sha256::digest(seed.as_bytes());
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&hash[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

fn digest<T: serde::Serialize>(value: &T) -> String {
    let bytes = serde_json::to_vec(value).expect("Workflow HumanGate effect serializes");
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn effect_repository_error(
    error: WorkflowExecutorEffectRepositoryError,
) -> WorkflowApplicationError {
    match error {
        WorkflowExecutorEffectRepositoryError::PayloadConflict { .. } => {
            WorkflowApplicationError::Conflict(error.to_string())
        }
        WorkflowExecutorEffectRepositoryError::Persistence(_) => {
            WorkflowApplicationError::Internal(error.to_string())
        }
    }
}
