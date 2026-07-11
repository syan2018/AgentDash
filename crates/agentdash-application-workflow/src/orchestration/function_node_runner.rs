use std::sync::Arc;

use agentdash_application_ports::operation_script::{
    OperationScriptError, OperationScriptLimits, OperationScriptResultValue,
};
use agentdash_domain::workflow::{
    ExecutorSpec, FunctionActivityExecutorSpec, LifecycleRun, NodePortValue,
    OperationScriptExecutorLimits, OperationScriptExecutorSpec, OperationScriptInputBinding,
    PlanNode, RuntimeNodeError,
};
use agentdash_spi::{ApiRequestOutcome, BashExecOutcome, FunctionRunner};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::{
    SharedWorkflowOperationScriptCaller, WorkflowOperationScriptCallContext,
    WorkflowOperationScriptCallerError, WorkflowOperationScriptProgram,
};

use super::ready_node::{RunningNodeView, RuntimeNodeCoordinate};

#[derive(Clone, Default)]
pub(super) struct FunctionNodeRunner {
    runner: Option<Arc<dyn FunctionRunner>>,
    operation_script_caller: SharedWorkflowOperationScriptCaller,
}

impl FunctionNodeRunner {
    pub(super) fn new() -> Self {
        Self {
            runner: None,
            operation_script_caller: SharedWorkflowOperationScriptCaller::default(),
        }
    }

    pub(super) fn with_runner(mut self, runner: Arc<dyn FunctionRunner>) -> Self {
        self.runner = Some(runner);
        self
    }

    pub(super) fn with_operation_script_caller(
        mut self,
        caller: SharedWorkflowOperationScriptCaller,
    ) -> Self {
        self.operation_script_caller = caller;
        self
    }

    pub(super) async fn execute(
        &self,
        run: &LifecycleRun,
        coordinate: &RuntimeNodeCoordinate,
        cancel: CancellationToken,
    ) -> Result<Vec<NodePortValue>, RuntimeNodeError> {
        let (context, node_input, plan_node, executor) = {
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
                node_input(&view),
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
                let runner = self.require_function_runner()?;
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
                let runner = self.require_function_runner()?;
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
            FunctionActivityExecutorSpec::OperationScript(spec) => {
                let caller =
                    self.operation_script_caller
                        .get()
                        .await
                        .ok_or_else(|| RuntimeNodeError {
                            code: "operation_script_caller_unavailable".to_string(),
                            message: "orchestration executor 缺少 Workflow OperationScript caller"
                                .to_string(),
                            retryable: true,
                            detail: Some(coordinate.detail()),
                        })?;
                let script_input = match spec.input_binding {
                    OperationScriptInputBinding::NodeInput => node_input,
                };
                let program = workflow_script_program(&spec, script_input)?;
                let call_context = WorkflowOperationScriptCallContext {
                    principal: agentdash_domain::operation::OperationPrincipalRef::WorkflowNode {
                        run_id: run.id,
                        node_key: coordinate.node_path.clone(),
                    },
                    scope: agentdash_domain::operation::OperationScopeRef::Project {
                        project_id: run.project_id,
                    },
                    origin: agentdash_domain::operation::OperationOriginRef::Workflow,
                    trace_id: workflow_trace_id(coordinate),
                    attachment_ref: None,
                };
                let preflight = caller
                    .preflight(program.clone(), call_context.clone(), cancel.clone())
                    .await
                    .map_err(operation_script_node_error)?;
                let outcome = caller
                    .run(program, call_context, preflight.token, cancel)
                    .await
                    .map_err(operation_script_node_error)?;
                let raw = operation_script_output_value(outcome.value);
                Ok(map_declared_outputs(&plan_node, raw))
            }
        }
    }

    #[allow(clippy::result_large_err)]
    fn require_function_runner(&self) -> Result<&Arc<dyn FunctionRunner>, RuntimeNodeError> {
        self.runner.as_ref().ok_or_else(|| RuntimeNodeError {
            code: "function_runner_unavailable".to_string(),
            message: "orchestration executor 缺少 FunctionRunner".to_string(),
            retryable: true,
            detail: None,
        })
    }
}

fn node_input(view: &RunningNodeView<'_>) -> Value {
    Value::Object(
        view.runtime_node
            .inputs
            .iter()
            .map(|input| (input.port_key.clone(), input.value.clone()))
            .collect(),
    )
}

#[allow(clippy::result_large_err)]
fn workflow_script_program(
    spec: &OperationScriptExecutorSpec,
    input: Value,
) -> Result<WorkflowOperationScriptProgram, RuntimeNodeError> {
    Ok(WorkflowOperationScriptProgram {
        language: spec.language.clone(),
        host_api_version: spec.host_api_version,
        source: spec.source.clone(),
        input,
        requested_operations: spec.requested_operations.clone(),
        limits: operation_script_limits(spec.limits)?,
    })
}

#[allow(clippy::result_large_err)]
fn operation_script_limits(
    limits: OperationScriptExecutorLimits,
) -> Result<OperationScriptLimits, RuntimeNodeError> {
    Ok(OperationScriptLimits {
        timeout_ms: limits.timeout_ms,
        max_source_bytes: checked_limit("max_source_bytes", limits.max_source_bytes)?,
        max_input_bytes: checked_limit("max_input_bytes", limits.max_input_bytes)?,
        max_output_bytes: checked_limit("max_output_bytes", limits.max_output_bytes)?,
        max_rhai_operations: limits.max_rhai_operations,
        max_call_levels: checked_limit("max_call_levels", limits.max_call_levels)?,
        max_string_size: checked_limit("max_string_size", limits.max_string_size)?,
        max_array_size: checked_limit("max_array_size", limits.max_array_size)?,
        max_map_size: checked_limit("max_map_size", limits.max_map_size)?,
        max_operation_calls: checked_limit("max_operation_calls", limits.max_operation_calls)?,
        max_parallel_operations: checked_limit(
            "max_parallel_operations",
            limits.max_parallel_operations,
        )?,
    })
}

#[allow(clippy::result_large_err)]
fn checked_limit(field: &'static str, value: u64) -> Result<usize, RuntimeNodeError> {
    usize::try_from(value).map_err(|_| RuntimeNodeError {
        code: "operation_script_limit_out_of_range".to_string(),
        message: format!("OperationScript limit `{field}` 超出当前 executor 可表示范围"),
        retryable: false,
        detail: Some(json!({ "field": field, "value": value })),
    })
}

fn operation_script_output_value(value: OperationScriptResultValue) -> Value {
    match value {
        OperationScriptResultValue::Inline { value } => value,
        OperationScriptResultValue::Ref { result_ref } => json!({
            "kind": "ref",
            "result_ref": result_ref,
        }),
    }
}

fn workflow_trace_id(coordinate: &RuntimeNodeCoordinate) -> String {
    format!(
        "workflow:{}:{}:{}:{}",
        coordinate.run_id, coordinate.orchestration_id, coordinate.node_path, coordinate.attempt
    )
}

fn operation_script_node_error(error: WorkflowOperationScriptCallerError) -> RuntimeNodeError {
    let (code, retryable, detail) = match &error {
        WorkflowOperationScriptCallerError::Surface(surface) => (
            surface.code().to_string(),
            matches!(
                surface.kind(),
                agentdash_application_operation_gateway::OperationExecutionErrorKind::ProviderFailed
            ),
            None,
        ),
        WorkflowOperationScriptCallerError::OperationUnavailable { operation_ref } => (
            "operation_script_operation_unavailable".to_string(),
            false,
            Some(json!({ "operation_ref": operation_ref })),
        ),
        WorkflowOperationScriptCallerError::DescriptorSerialization(_) => (
            "operation_script_descriptor_serialize_failed".to_string(),
            false,
            None,
        ),
        WorkflowOperationScriptCallerError::Script(script) => script_error_detail(script),
    };
    RuntimeNodeError {
        code,
        message: error.to_string(),
        retryable,
        detail,
    }
}

fn script_error_detail(error: &OperationScriptError) -> (String, bool, Option<Value>) {
    match error {
        OperationScriptError::CapacityExceeded => {
            ("operation_script_capacity_exceeded".to_string(), true, None)
        }
        OperationScriptError::Cancelled => ("operation_script_cancelled".to_string(), false, None),
        OperationScriptError::DeadlineExceeded => (
            "operation_script_deadline_exceeded".to_string(),
            false,
            None,
        ),
        OperationScriptError::ExecutionFailed {
            calls,
            partial,
            outcome_unknown,
            ..
        } => (
            "operation_script_execution_failed".to_string(),
            false,
            Some(json!({
                "calls": calls,
                "partial": partial,
                "outcome_unknown": outcome_unknown,
            })),
        ),
        OperationScriptError::NestedOperation {
            code,
            outcome_unknown,
        } => (
            code.clone(),
            false,
            Some(json!({ "outcome_unknown": outcome_unknown })),
        ),
        _ => ("operation_script_failed".to_string(), false, None),
    }
}

fn function_context(run: &LifecycleRun, view: &RunningNodeView<'_>) -> Value {
    let inputs = node_input(view);
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

#[cfg(test)]
mod operation_script_node_tests {
    use agentdash_domain::operation::OperationRef;

    use super::*;

    fn spec() -> OperationScriptExecutorSpec {
        OperationScriptExecutorSpec {
            language: "rhai_v1".to_string(),
            host_api_version: 1,
            source: "input".to_string(),
            input_binding: OperationScriptInputBinding::NodeInput,
            requested_operations: vec![
                OperationRef::new("workflow", "fixture", "echo", 1).expect("operation ref"),
            ],
            limits: OperationScriptExecutorLimits::default(),
        }
    }

    #[test]
    fn workflow_node_passes_whole_node_input_without_expression_dsl() {
        let input = json!({"topic":"runtime", "minimum":3});
        let program = workflow_script_program(&spec(), input.clone()).expect("program");
        assert_eq!(program.input, input);
        assert_eq!(program.requested_operations, spec().requested_operations);
    }

    #[test]
    fn cancelled_and_outcome_unknown_failures_are_not_retryable() {
        let (_, cancelled_retryable, _) = script_error_detail(&OperationScriptError::Cancelled);
        assert!(!cancelled_retryable);
        let (_, failed_retryable, detail) =
            script_error_detail(&OperationScriptError::ExecutionFailed {
                diagnostic: "cancelled nested call".to_string(),
                calls: Vec::new(),
                partial: true,
                outcome_unknown: true,
            });
        assert!(!failed_retryable);
        assert_eq!(detail.expect("detail")["outcome_unknown"], true);
    }

    #[test]
    fn operation_script_success_maps_inline_value_or_typed_result_ref() {
        let plan_node = PlanNode {
            node_id: "script".to_string(),
            node_path: "script".to_string(),
            parent_node_id: None,
            kind: agentdash_domain::workflow::PlanNodeKind::Function,
            label: Some("OperationScript".to_string()),
            executor: None,
            input_ports: Vec::new(),
            output_ports: vec![agentdash_domain::workflow::OutputPortDefinition {
                key: "result".to_string(),
                description: "script result".to_string(),
                gate_strategy: agentdash_domain::workflow::GateStrategy::Existence,
                gate_params: None,
            }],
            completion_policy: None,
            iteration_policy: None,
            join_policy: None,
            result_contract: None,
            metadata: None,
        };
        let inline_value = operation_script_output_value(OperationScriptResultValue::Inline {
            value: json!({"clean":true}),
        });
        let inline = map_declared_outputs(&plan_node, inline_value);
        assert_eq!(inline[0].value, json!({"clean":true}));

        let result_ref = agentdash_application_ports::operation_script::OperationScriptResultRef {
            result_id: uuid::Uuid::new_v4(),
        };
        let typed_ref =
            operation_script_output_value(OperationScriptResultValue::Ref { result_ref });
        let mapped_ref = map_declared_outputs(&plan_node, typed_ref.clone());
        assert_eq!(mapped_ref[0].value, typed_ref);
    }
}
