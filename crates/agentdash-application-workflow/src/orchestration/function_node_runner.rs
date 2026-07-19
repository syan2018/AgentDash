use std::sync::Arc;

use agentdash_domain::workflow::{
    ExecutorSpec, FunctionActivityExecutorSpec, LifecycleRun, NodePortValue, PlanNode,
    RuntimeNodeError, WorkflowFunctionEffectRequest, WorkflowFunctionTerminalResult,
};
use agentdash_platform_spi::{
    ApiRequestOutcome, BashExecOutcome, FunctionEffectObservation, FunctionEffectRawOutcome,
    FunctionEffectRequest, FunctionEffectSpec, FunctionRunner,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::ready_node::{ReadyNodeView, RunningNodeView, RuntimeNodeCoordinate};

#[derive(Clone, Default)]
pub(super) struct FunctionNodeRunner {
    runner: Option<Arc<dyn FunctionRunner>>,
}

pub(super) struct PreparedFunctionEffect {
    pub(super) request: WorkflowFunctionEffectRequest,
    plan_node: PlanNode,
}

impl FunctionNodeRunner {
    pub(super) fn new() -> Self {
        Self { runner: None }
    }

    pub(super) fn with_runner(mut self, runner: Arc<dyn FunctionRunner>) -> Self {
        self.runner = Some(runner);
        self
    }

    pub(super) fn is_composed(&self) -> bool {
        self.runner.is_some()
    }

    pub(super) fn prepare_ready(
        &self,
        run: &LifecycleRun,
        coordinate: &RuntimeNodeCoordinate,
        identity: agentdash_domain::workflow::WorkflowExecutorEffectIdentity,
    ) -> Result<PreparedFunctionEffect, RuntimeNodeError> {
        let view =
            ReadyNodeView::for_coordinate(run, coordinate).map_err(|error| RuntimeNodeError {
                code: "running_node_view_unavailable".to_string(),
                message: error.to_string(),
                retryable: false,
                detail: Some(coordinate.detail()),
            })?;
        let context = function_context(run, coordinate, view.runtime_node, view.state_snapshot);
        let plan_node = view.plan_node.clone();
        let spec = match view.plan_node.executor.clone() {
            Some(ExecutorSpec::Function { spec }) => spec,
            Some(ExecutorSpec::LocalEffect {
                capability_key,
                input,
            }) => {
                return Err(RuntimeNodeError {
                    code: "local_effect_capability_not_supported".to_string(),
                    message: format!(
                        "LocalEffect capability `{capability_key}` 尚未接入具体 effect executor"
                    ),
                    retryable: false,
                    detail: Some(coordinate.detail_with([
                        ("capability_key", json!(capability_key)),
                        ("input", json!(input)),
                    ])),
                });
            }
            None => {
                return Err(RuntimeNodeError {
                    code: "executor_spec_missing".to_string(),
                    message: "Function/LocalEffect node 缺少 executor spec".to_string(),
                    retryable: false,
                    detail: Some(coordinate.detail()),
                });
            }
            Some(_) => {
                return Err(RuntimeNodeError {
                    code: "executor_spec_mismatch".to_string(),
                    message: "Function/LocalEffect node 的 executor spec 类型不匹配".to_string(),
                    retryable: false,
                    detail: Some(coordinate.detail()),
                });
            }
        };
        let payload = serde_json::to_vec(&(&identity, &spec, &context))
            .expect("Workflow Function effect payload serializes");
        let payload_digest = format!("sha256:{:x}", Sha256::digest(payload));
        Ok(PreparedFunctionEffect {
            request: WorkflowFunctionEffectRequest {
                identity,
                payload_digest,
                spec,
                context,
            },
            plan_node,
        })
    }

    pub(super) fn prepare_recovery(
        &self,
        run: &LifecycleRun,
        coordinate: &RuntimeNodeCoordinate,
        request: WorkflowFunctionEffectRequest,
    ) -> Result<PreparedFunctionEffect, RuntimeNodeError> {
        let view =
            RunningNodeView::for_coordinate(run, coordinate).map_err(|error| RuntimeNodeError {
                code: "running_node_view_unavailable".to_string(),
                message: error.to_string(),
                retryable: false,
                detail: Some(coordinate.detail()),
            })?;
        if !matches!(
            view.plan_node.executor.as_ref(),
            Some(ExecutorSpec::Function { spec }) if spec == &request.spec
        ) {
            return Err(RuntimeNodeError {
                code: "function_effect_plan_drift".to_owned(),
                message: "durable Function effect request 与 plan snapshot 不一致".to_owned(),
                retryable: false,
                detail: Some(coordinate.detail()),
            });
        }
        Ok(PreparedFunctionEffect {
            request,
            plan_node: view.plan_node.clone(),
        })
    }

    pub(super) async fn dispatch(
        &self,
        prepared: &PreparedFunctionEffect,
    ) -> Result<Option<WorkflowFunctionTerminalResult>, RuntimeNodeError> {
        let runner = self.runner.as_ref().ok_or_else(|| RuntimeNodeError {
            code: "function_effect_protocol_not_composed".to_string(),
            message: "orchestration executor 缺少 durable Function effect protocol".to_string(),
            retryable: true,
            detail: None,
        })?;
        let inspected = runner
            .inspect_effect(&prepared.request.identity.effect_id)
            .await
            .map_err(effect_protocol_error)?;
        let observation = match inspected {
            FunctionEffectObservation::Unknown => {
                let spec = match prepared.request.spec.clone() {
                    FunctionActivityExecutorSpec::ApiRequest(spec) => {
                        FunctionEffectSpec::ApiRequest(spec)
                    }
                    FunctionActivityExecutorSpec::BashExec(spec) => {
                        FunctionEffectSpec::BashExec(spec)
                    }
                };
                let request = FunctionEffectRequest {
                    effect_id: prepared.request.identity.effect_id.clone(),
                    payload_digest: prepared.request.payload_digest.clone(),
                    spec,
                    context: prepared.request.context.clone(),
                };
                match runner.execute_effect(request).await {
                    Ok(observation) => observation,
                    Err(_) => runner
                        .inspect_effect(&prepared.request.identity.effect_id)
                        .await
                        .map_err(effect_protocol_error)?,
                }
            }
            observation => observation,
        };
        Ok(match observation {
            FunctionEffectObservation::Unknown | FunctionEffectObservation::Accepted => None,
            FunctionEffectObservation::Succeeded(raw) => {
                Some(raw_terminal(&prepared.plan_node, raw))
            }
            FunctionEffectObservation::Failed { message, retryable } => {
                Some(WorkflowFunctionTerminalResult::Failed {
                    error: RuntimeNodeError {
                        code: "function_effect_failed".to_owned(),
                        message,
                        retryable,
                        detail: None,
                    },
                })
            }
        })
    }
}

fn effect_protocol_error(message: String) -> RuntimeNodeError {
    RuntimeNodeError {
        code: "function_effect_protocol_unavailable".to_owned(),
        message,
        retryable: true,
        detail: None,
    }
}

fn raw_terminal(
    plan_node: &PlanNode,
    raw: FunctionEffectRawOutcome,
) -> WorkflowFunctionTerminalResult {
    match raw {
        FunctionEffectRawOutcome::ApiRequest(outcome) if (200..300).contains(&outcome.status) => {
            WorkflowFunctionTerminalResult::Completed {
                outputs: api_request_outputs(plan_node, outcome),
            }
        }
        FunctionEffectRawOutcome::ApiRequest(outcome) => WorkflowFunctionTerminalResult::Failed {
            error: RuntimeNodeError {
                code: "api_request_status_failed".to_string(),
                message: format!("API request 返回非成功状态: {}", outcome.status),
                retryable: false,
                detail: Some(json!({
                    "status": outcome.status,
                    "body_text": outcome.body_text,
                    "body_json": outcome.body_json,
                })),
            },
        },
        FunctionEffectRawOutcome::BashExec(outcome) if outcome.success => {
            WorkflowFunctionTerminalResult::Completed {
                outputs: bash_exec_outputs(plan_node, outcome),
            }
        }
        FunctionEffectRawOutcome::BashExec(outcome) => WorkflowFunctionTerminalResult::Failed {
            error: RuntimeNodeError {
                code: "bash_exec_nonzero".to_string(),
                message: format!(
                    "bash exec failed: exit_code={:?}, stderr={}",
                    outcome.exit_code, outcome.stderr
                ),
                retryable: false,
                detail: Some(json!({
                    "exit_code": outcome.exit_code,
                    "stdout": outcome.stdout,
                    "stderr": outcome.stderr,
                })),
            },
        },
    }
}

fn function_context(
    run: &LifecycleRun,
    coordinate: &RuntimeNodeCoordinate,
    runtime_node: &agentdash_domain::workflow::RuntimeNodeState,
    state_snapshot: &agentdash_domain::workflow::StateExchangeSnapshot,
) -> Value {
    let inputs = runtime_node
        .inputs
        .iter()
        .map(|input| (input.port_key.clone(), input.value.clone()))
        .collect::<serde_json::Map<_, _>>();
    let state = serde_json::to_value(state_snapshot).unwrap_or(Value::Null);
    json!({
        "run_id": run.id,
        "project_id": run.project_id,
        "orchestration_id": coordinate.orchestration_id,
        "node": {
            "id": runtime_node.node_id.clone(),
            "path": coordinate.node_path.clone(),
            "attempt": coordinate.attempt,
            "inputs": inputs,
        },
        "state": state,
    })
}

fn api_request_outputs(plan_node: &PlanNode, outcome: ApiRequestOutcome) -> Vec<NodePortValue> {
    let raw = json!({
        "status": outcome.status,
        "body_text": outcome.body_text,
        "body_json": outcome.body_json,
    });
    map_declared_outputs(plan_node, raw)
}

fn bash_exec_outputs(plan_node: &PlanNode, outcome: BashExecOutcome) -> Vec<NodePortValue> {
    let raw = json!({
        "success": outcome.success,
        "exit_code": outcome.exit_code,
        "stdout": outcome.stdout,
        "stderr": outcome.stderr,
    });
    map_declared_outputs(plan_node, raw)
}

fn map_declared_outputs(plan_node: &PlanNode, raw: Value) -> Vec<NodePortValue> {
    if plan_node.output_ports.is_empty() {
        return Vec::new();
    }
    if plan_node.output_ports.len() == 1 {
        return vec![NodePortValue {
            port_key: plan_node.output_ports[0].key.clone(),
            value: raw,
        }];
    }
    plan_node
        .output_ports
        .iter()
        .map(|port| NodePortValue {
            port_key: port.key.clone(),
            value: raw.get(&port.key).cloned().unwrap_or(Value::Null),
        })
        .collect()
}
