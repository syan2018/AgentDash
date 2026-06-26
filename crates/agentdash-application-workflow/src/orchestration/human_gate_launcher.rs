use std::sync::Arc;

use agentdash_domain::workflow::{
    ActivityCompletionPolicy, ExecutorRunRef, ExecutorSpec, LifecycleGate, LifecycleGateRepository,
    LifecycleRun, NodePortValue, PlanNode, PlanNodeKind, RuntimeNodeState,
};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::WorkflowApplicationError;

use super::executor_launcher::{OpenedHumanGate, SubmitHumanGateDecisionInput};
use super::ready_node::{ReadyNodeView, RunningNodeView, RuntimeNodeCoordinate};
use super::runtime::OrchestrationRuntimeEvent;

#[derive(Clone)]
pub(super) struct HumanGateLauncher {
    lifecycle_gate_repo: Arc<dyn LifecycleGateRepository>,
}

impl HumanGateLauncher {
    pub(super) fn new(lifecycle_gate_repo: Arc<dyn LifecycleGateRepository>) -> Self {
        Self {
            lifecycle_gate_repo,
        }
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
        let gate = LifecycleGate::open(
            run.id,
            None,
            None,
            "orchestration_human_gate",
            human_gate_correlation_id(
                coordinate.orchestration_id,
                &coordinate.node_path,
                coordinate.attempt,
            ),
            Some(json!({
                "contract": "orchestration_human_gate.v1",
                "run_id": coordinate.run_id,
                "orchestration_id": coordinate.orchestration_id,
                "node_path": coordinate.node_path.clone(),
                "attempt": coordinate.attempt,
                "plan_node_id": plan_node_id,
                "label": label,
                "executor": executor,
            })),
        );
        let gate_id = gate.id;
        self.lifecycle_gate_repo.create(&gate).await?;

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
        let mut gate = self
            .lifecycle_gate_repo
            .get(gate_id)
            .await?
            .ok_or_else(|| WorkflowApplicationError::NotFound(format!("gate 不存在: {gate_id}")))?;
        if !gate.is_open() {
            return Err(WorkflowApplicationError::Conflict(format!(
                "gate {gate_id} 已经 resolved"
            )));
        }
        gate.payload_json = Some(input.decision.clone());
        gate.resolve(input.resolved_by.clone());
        self.lifecycle_gate_repo.update(&gate).await?;

        Ok(HumanGateDecision { gate_id, outputs })
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

pub(super) struct HumanGateDecision {
    pub(super) gate_id: Uuid,
    pub(super) outputs: Vec<NodePortValue>,
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

fn human_gate_id_from_node(node: &RuntimeNodeState) -> Result<Uuid, WorkflowApplicationError> {
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

fn human_gate_correlation_id(orchestration_id: Uuid, node_path: &str, attempt: u32) -> String {
    format!("orchestration:{orchestration_id}:node:{node_path}:attempt:{attempt}")
}
