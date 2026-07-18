use std::sync::Arc;

use agentdash_domain::workflow::{
    ExecutorSpec, FunctionActivityExecutorSpec, LifecycleRun, NodePortValue, PlanNode,
    RuntimeNodeError,
};
use agentdash_platform_spi::{ApiRequestOutcome, BashExecOutcome, FunctionRunner};
use serde_json::{Value, json};

use super::ready_node::{RunningNodeView, RuntimeNodeCoordinate};

#[derive(Clone, Default)]
pub(super) struct FunctionNodeRunner {
    runner: Option<Arc<dyn FunctionRunner>>,
}

impl FunctionNodeRunner {
    pub(super) fn new() -> Self {
        Self { runner: None }
    }

    pub(super) fn with_runner(mut self, runner: Arc<dyn FunctionRunner>) -> Self {
        self.runner = Some(runner);
        self
    }

    pub(super) async fn execute(
        &self,
        run: &LifecycleRun,
        coordinate: &RuntimeNodeCoordinate,
    ) -> Result<Vec<NodePortValue>, RuntimeNodeError> {
        let runner = self.runner.as_ref().ok_or_else(|| RuntimeNodeError {
            code: "function_runner_unavailable".to_string(),
            message: "orchestration executor 缺少 FunctionRunner".to_string(),
            retryable: true,
            detail: None,
        })?;
        let (context, plan_node, executor) = {
            let view = RunningNodeView::for_coordinate(run, coordinate).map_err(|error| {
                RuntimeNodeError {
                    code: "running_node_view_unavailable".to_string(),
                    message: error.to_string(),
                    retryable: false,
                    detail: Some(coordinate.detail()),
                }
            })?;
            (
                function_context(run, &view),
                view.plan_node.clone(),
                view.plan_node.executor.clone(),
            )
        };
        let Some(executor) = executor else {
            return Err(RuntimeNodeError {
                code: "executor_spec_missing".to_string(),
                message: "Function/LocalEffect node 缺少 executor spec".to_string(),
                retryable: false,
                detail: Some(coordinate.detail()),
            });
        };
        let spec = match executor {
            ExecutorSpec::Function { spec } => spec,
            ExecutorSpec::LocalEffect {
                capability_key,
                input,
            } => {
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
            _ => {
                return Err(RuntimeNodeError {
                    code: "executor_spec_mismatch".to_string(),
                    message: "Function/LocalEffect node 的 executor spec 类型不匹配".to_string(),
                    retryable: false,
                    detail: Some(coordinate.detail()),
                });
            }
        };
        match spec {
            FunctionActivityExecutorSpec::ApiRequest(spec) => {
                let outcome = runner
                    .run_api_request(&spec, &context)
                    .await
                    .map_err(|error| RuntimeNodeError {
                        code: "api_request_failed".to_string(),
                        message: error,
                        retryable: true,
                        detail: None,
                    })?;
                if !(200..300).contains(&outcome.status) {
                    return Err(RuntimeNodeError {
                        code: "api_request_status_failed".to_string(),
                        message: format!("API request 返回非成功状态: {}", outcome.status),
                        retryable: false,
                        detail: Some(json!({
                            "status": outcome.status,
                            "body_text": outcome.body_text,
                            "body_json": outcome.body_json,
                        })),
                    });
                }
                Ok(api_request_outputs(&plan_node, outcome))
            }
            FunctionActivityExecutorSpec::BashExec(spec) => {
                let outcome =
                    runner
                        .run_bash(&spec, &context)
                        .await
                        .map_err(|error| RuntimeNodeError {
                            code: "bash_exec_failed".to_string(),
                            message: error,
                            retryable: true,
                            detail: None,
                        })?;
                if outcome.success {
                    Ok(bash_exec_outputs(&plan_node, outcome))
                } else {
                    Err(RuntimeNodeError {
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
                    })
                }
            }
        }
    }
}

fn function_context(run: &LifecycleRun, view: &RunningNodeView<'_>) -> Value {
    let inputs = view
        .runtime_node
        .inputs
        .iter()
        .map(|input| (input.port_key.clone(), input.value.clone()))
        .collect::<serde_json::Map<_, _>>();
    let state = serde_json::to_value(view.state_snapshot).unwrap_or(Value::Null);
    json!({
        "run_id": run.id,
        "project_id": run.project_id,
        "orchestration_id": view.coordinate.orchestration_id,
        "node": {
            "id": view.runtime_node.node_id.clone(),
            "path": view.coordinate.node_path.clone(),
            "attempt": view.coordinate.attempt,
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
