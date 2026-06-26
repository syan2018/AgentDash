use std::collections::BTreeSet;

use agentdash_domain::workflow::{
    ActivationRule, ActivityCompletionPolicy, ActivityJoinPolicy, DispatchState, ExecutorRunRef,
    LifecycleRun, NodePortValue, OrchestrationInstance, OrchestrationPlanSnapshot,
    OrchestrationSourceRef, OrchestrationStatus, PlanNode, PlanNodeKind, RuntimeNodeError,
    RuntimeNodeState, RuntimeNodeStatus, RuntimeTraceRef, StateExchangeRule, StateExchangeSnapshot,
    TransitionCondition,
};
use chrono::{DateTime, Utc};
use serde_json::{Map, Value};
use uuid::Uuid;

pub const ROOT_ORCHESTRATION_ROLE: &str = "root";

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct RootInputBinding {
    pub node_id: String,
    pub port: String,
}

impl RootInputBinding {
    pub fn new(node_id: impl Into<String>, port: impl Into<String>) -> Self {
        Self {
            node_id: node_id.into(),
            port: port.into(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct OrchestrationActivationInput {
    pub root_args: Option<Value>,
    pub root_input_bindings: Vec<RootInputBinding>,
}

pub fn activate_root_orchestration(
    source_ref: OrchestrationSourceRef,
    plan_snapshot: OrchestrationPlanSnapshot,
) -> OrchestrationInstance {
    activate_orchestration(ROOT_ORCHESTRATION_ROLE, source_ref, plan_snapshot)
}

pub fn activate_orchestration(
    role: impl Into<String>,
    source_ref: OrchestrationSourceRef,
    plan_snapshot: OrchestrationPlanSnapshot,
) -> OrchestrationInstance {
    activate_orchestration_with_input(
        role,
        source_ref,
        plan_snapshot,
        OrchestrationActivationInput::default(),
    )
}

pub fn activate_orchestration_with_input(
    role: impl Into<String>,
    source_ref: OrchestrationSourceRef,
    plan_snapshot: OrchestrationPlanSnapshot,
    activation_input: OrchestrationActivationInput,
) -> OrchestrationInstance {
    let entry_node_ids = plan_snapshot.entry_node_ids.clone();
    let mut instance = OrchestrationInstance::new(role, source_ref, plan_snapshot);
    materialize_plan_activation_with_input(&mut instance, entry_node_ids, &activation_input);
    instance
}

pub fn materialize_plan_activation(
    instance: &mut OrchestrationInstance,
    ready_node_ids: Vec<String>,
) {
    let activation_input = OrchestrationActivationInput::default();
    materialize_plan_activation_with_input(instance, ready_node_ids, &activation_input);
}

fn materialize_plan_activation_with_input(
    instance: &mut OrchestrationInstance,
    ready_node_ids: Vec<String>,
    activation_input: &OrchestrationActivationInput,
) {
    let ready_set = ready_node_ids.iter().cloned().collect::<BTreeSet<_>>();

    instance.activation.ready_node_ids = ready_node_ids.clone();
    instance.dispatch = DispatchState {
        ready_node_ids,
        ..DispatchState::default()
    };
    instance.node_tree = instance
        .plan_snapshot
        .nodes
        .iter()
        .map(|node| RuntimeNodeState {
            node_id: node.node_id.clone(),
            node_path: node.node_path.clone(),
            kind: node.kind,
            status: if ready_set.contains(node.node_id.as_str()) {
                RuntimeNodeStatus::Ready
            } else if plan_node_is_metadata_only(node) {
                RuntimeNodeStatus::Skipped
            } else {
                RuntimeNodeStatus::Pending
            },
            attempt: 1,
            inputs: Vec::new(),
            outputs: Vec::new(),
            executor_run_ref: None,
            children: Vec::new(),
            phase_path: plan_node_phase_path(node),
            started_at: None,
            completed_at: None,
            error: None,
            trace_refs: Vec::new(),
            cache: None,
        })
        .collect();
    materialize_root_input_bindings(&mut instance.node_tree, activation_input);
    instance.state_snapshot = StateExchangeSnapshot::default();
    instance.status = if instance.activation.ready_node_ids.is_empty() {
        OrchestrationStatus::Pending
    } else {
        OrchestrationStatus::Running
    };
    instance.updated_at = chrono::Utc::now();
}

fn materialize_root_input_bindings(
    node_tree: &mut [RuntimeNodeState],
    activation_input: &OrchestrationActivationInput,
) {
    let Some(args) = activation_input
        .root_args
        .as_ref()
        .and_then(Value::as_object)
    else {
        return;
    };

    for binding in &activation_input.root_input_bindings {
        let Some(value) = args.get(&binding.port).cloned() else {
            continue;
        };
        if let Some(node) = node_tree
            .iter_mut()
            .find(|node| node.node_id == binding.node_id)
        {
            upsert_node_port_value(&mut node.inputs, binding.port.clone(), value);
        }
    }
}

fn plan_node_is_metadata_only(node: &PlanNode) -> bool {
    if matches!(node.kind, PlanNodeKind::Phase) {
        return true;
    }
    node.metadata
        .as_ref()
        .and_then(|metadata| metadata.get("metadata_only"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn plan_node_phase_path(node: &PlanNode) -> Vec<String> {
    node.metadata
        .as_ref()
        .and_then(|metadata| metadata.get("phase_path"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

#[derive(Debug, Clone, PartialEq)]
pub enum OrchestrationRuntimeEvent {
    NodeStarted {
        node_path: String,
        attempt: u32,
        executor_run_ref: Option<ExecutorRunRef>,
        timestamp: DateTime<Utc>,
    },
    NodeCompleted {
        node_path: String,
        attempt: u32,
        outputs: Vec<NodePortValue>,
        timestamp: DateTime<Utc>,
    },
    NodeFailed {
        node_path: String,
        attempt: u32,
        error: RuntimeNodeError,
        timestamp: DateTime<Utc>,
    },
    NodeCancelled {
        node_path: String,
        attempt: u32,
        reason: Option<String>,
        timestamp: DateTime<Utc>,
    },
    NodeBlocked {
        node_path: String,
        attempt: u32,
        error: RuntimeNodeError,
        timestamp: DateTime<Utc>,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OrchestrationRuntimeApplyOutcome {
    pub activated_node_ids: Vec<String>,
    pub diagnostics: Vec<OrchestrationRuntimeDiagnostic>,
    pub terminal_idempotent: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrchestrationRuntimeDiagnostic {
    pub code: String,
    pub message: String,
    pub node_id: Option<String>,
    pub rule_id: Option<String>,
}

impl OrchestrationRuntimeDiagnostic {
    fn blocking(
        code: impl Into<String>,
        message: impl Into<String>,
        node_id: Option<String>,
        rule_id: Option<String>,
    ) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            node_id,
            rule_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OrchestrationRuntimeError {
    #[error("orchestration 不存在: {orchestration_id}")]
    OrchestrationNotFound { orchestration_id: Uuid },
    #[error("orchestration node 不存在: node_path={node_path}, attempt={attempt}")]
    NodeNotFound { node_path: String, attempt: u32 },
    #[error("plan node 不存在: {node_id}")]
    PlanNodeNotFound { node_id: String },
    #[error("runtime node 不存在: {node_id}")]
    RuntimeNodeNotFound { node_id: String },
    #[error("node `{node_id}` 缺少 required output ports: {missing_output_ports:?}")]
    CompletionPolicyRejected {
        node_id: String,
        missing_output_ports: Vec<String>,
    },
    #[error("state exchange `{rule_id}` 缺少 source output: {from_node_id}.{from_port}")]
    StateExchangeMissingOutput {
        rule_id: String,
        from_node_id: String,
        from_port: String,
    },
}

pub fn apply_orchestration_event_to_run(
    mut run: LifecycleRun,
    orchestration_id: Uuid,
    event: OrchestrationRuntimeEvent,
) -> Result<(LifecycleRun, OrchestrationRuntimeApplyOutcome), OrchestrationRuntimeError> {
    let outcome = {
        let orchestration = run
            .orchestrations
            .iter_mut()
            .find(|orchestration| orchestration.orchestration_id == orchestration_id)
            .ok_or(OrchestrationRuntimeError::OrchestrationNotFound { orchestration_id })?;
        apply_orchestration_event(orchestration, event)?
    };
    run.refresh_status_from_orchestrations();
    let now = Utc::now();
    run.updated_at = now;
    run.last_activity_at = now;
    Ok((run, outcome))
}

pub fn apply_orchestration_event(
    instance: &mut OrchestrationInstance,
    event: OrchestrationRuntimeEvent,
) -> Result<OrchestrationRuntimeApplyOutcome, OrchestrationRuntimeError> {
    let mut next = instance.clone();
    let outcome = apply_orchestration_event_inner(&mut next, event)?;
    *instance = next;
    Ok(outcome)
}

fn apply_orchestration_event_inner(
    instance: &mut OrchestrationInstance,
    event: OrchestrationRuntimeEvent,
) -> Result<OrchestrationRuntimeApplyOutcome, OrchestrationRuntimeError> {
    let mut outcome = OrchestrationRuntimeApplyOutcome::default();

    match event {
        OrchestrationRuntimeEvent::NodeStarted {
            node_path,
            attempt,
            executor_run_ref,
            timestamp,
        } => {
            let Some(node) = find_runtime_node_mut(&mut instance.node_tree, &node_path, attempt)
            else {
                return Err(OrchestrationRuntimeError::NodeNotFound { node_path, attempt });
            };
            if is_terminal_status(node.status) {
                return Ok(outcome);
            }
            let node_id = node.node_id.clone();
            node.status = RuntimeNodeStatus::Running;
            if node.started_at.is_none() {
                node.started_at = Some(timestamp);
            }
            if let Some(executor_run_ref) = executor_run_ref {
                let trace_ref = runtime_trace_ref_from_executor_run_ref(&executor_run_ref);
                node.executor_run_ref = Some(executor_run_ref);
                push_runtime_trace_ref(node, trace_ref);
            }
            node.completed_at = None;
            node.error = None;
            remove_ready_node(instance, &node_id);
        }
        OrchestrationRuntimeEvent::NodeCompleted {
            node_path,
            attempt,
            outputs,
            timestamp,
        } => {
            let node_snapshot = find_runtime_node(&instance.node_tree, &node_path, attempt)
                .cloned()
                .ok_or_else(|| OrchestrationRuntimeError::NodeNotFound {
                    node_path: node_path.clone(),
                    attempt,
                })?;
            if is_terminal_status(node_snapshot.status) {
                outcome.terminal_idempotent = true;
                return Ok(outcome);
            }
            validate_completion_policy(&instance.plan_snapshot, &node_snapshot.node_id, &outputs)?;

            let Some(node) = find_runtime_node_mut(&mut instance.node_tree, &node_path, attempt)
            else {
                return Err(OrchestrationRuntimeError::NodeNotFound { node_path, attempt });
            };
            node.status = RuntimeNodeStatus::Completed;
            node.outputs = outputs.clone();
            node.completed_at = Some(timestamp);
            node.error = None;
            remove_ready_node(instance, &node_snapshot.node_id);
            upsert_state_node_outputs(
                &mut instance.state_snapshot,
                &node_snapshot.node_id,
                &outputs,
            );
            materialize_state_exchange(instance, &node_snapshot.node_id, &mut outcome)?;
            activate_transition_successors(instance, &node_snapshot.node_id, &mut outcome)?;
        }
        OrchestrationRuntimeEvent::NodeFailed {
            node_path,
            attempt,
            error,
            timestamp,
        } => {
            let Some(node) = find_runtime_node_mut(&mut instance.node_tree, &node_path, attempt)
            else {
                return Err(OrchestrationRuntimeError::NodeNotFound { node_path, attempt });
            };
            if is_terminal_status(node.status) {
                outcome.terminal_idempotent = true;
                return Ok(outcome);
            }
            let node_id = node.node_id.clone();
            node.status = RuntimeNodeStatus::Failed;
            node.completed_at = Some(timestamp);
            node.error = Some(error);
            remove_ready_node(instance, &node_id);
        }
        OrchestrationRuntimeEvent::NodeCancelled {
            node_path,
            attempt,
            reason,
            timestamp,
        } => {
            let Some(node) = find_runtime_node_mut(&mut instance.node_tree, &node_path, attempt)
            else {
                return Err(OrchestrationRuntimeError::NodeNotFound { node_path, attempt });
            };
            if is_terminal_status(node.status) {
                outcome.terminal_idempotent = true;
                return Ok(outcome);
            }
            let node_id = node.node_id.clone();
            node.status = RuntimeNodeStatus::Cancelled;
            node.completed_at = Some(timestamp);
            node.error = reason.map(|message| RuntimeNodeError {
                code: "runtime_node_cancelled".to_string(),
                message,
                retryable: false,
                detail: None,
            });
            remove_ready_node(instance, &node_id);
        }
        OrchestrationRuntimeEvent::NodeBlocked {
            node_path,
            attempt,
            error,
            timestamp,
        } => {
            let Some(node) = find_runtime_node_mut(&mut instance.node_tree, &node_path, attempt)
            else {
                return Err(OrchestrationRuntimeError::NodeNotFound { node_path, attempt });
            };
            if is_terminal_status(node.status) {
                outcome.terminal_idempotent = true;
                return Ok(outcome);
            }
            let node_id = node.node_id.clone();
            node.status = RuntimeNodeStatus::Blocked;
            node.completed_at = Some(timestamp);
            node.error = Some(error);
            remove_ready_node(instance, &node_id);
        }
    }

    derive_orchestration_status(instance);
    instance.updated_at = Utc::now();
    Ok(outcome)
}

fn validate_completion_policy(
    plan: &OrchestrationPlanSnapshot,
    node_id: &str,
    outputs: &[NodePortValue],
) -> Result<(), OrchestrationRuntimeError> {
    let plan_node = plan
        .nodes
        .iter()
        .find(|node| node.node_id == node_id)
        .ok_or_else(|| OrchestrationRuntimeError::PlanNodeNotFound {
            node_id: node_id.to_string(),
        })?;
    let missing_output_ports = match &plan_node.completion_policy {
        Some(ActivityCompletionPolicy::OutputPorts { required_ports }) => required_ports
            .iter()
            .filter(|port| !outputs.iter().any(|output| output.port_key == **port))
            .cloned()
            .collect::<Vec<_>>(),
        Some(ActivityCompletionPolicy::HumanDecision { decision_port }) => {
            if outputs
                .iter()
                .any(|output| output.port_key == *decision_port)
            {
                Vec::new()
            } else {
                vec![decision_port.clone()]
            }
        }
        Some(
            ActivityCompletionPolicy::HookGate { .. }
            | ActivityCompletionPolicy::ExecutorTerminal
            | ActivityCompletionPolicy::OpenEnded,
        )
        | None => Vec::new(),
    };
    if missing_output_ports.is_empty() {
        Ok(())
    } else {
        Err(OrchestrationRuntimeError::CompletionPolicyRejected {
            node_id: node_id.to_string(),
            missing_output_ports,
        })
    }
}

fn upsert_state_node_outputs(
    snapshot: &mut StateExchangeSnapshot,
    node_id: &str,
    outputs: &[NodePortValue],
) {
    let mut object = Map::new();
    for output in outputs {
        object.insert(output.port_key.clone(), output.value.clone());
    }
    snapshot
        .node_outputs
        .insert(node_id.to_string(), Value::Object(object));
}

fn materialize_state_exchange(
    instance: &mut OrchestrationInstance,
    completed_node_id: &str,
    outcome: &mut OrchestrationRuntimeApplyOutcome,
) -> Result<(), OrchestrationRuntimeError> {
    let rules = instance
        .plan_snapshot
        .state_exchange_rules
        .iter()
        .filter(|rule| rule.from_node_id == completed_node_id)
        .cloned()
        .collect::<Vec<_>>();

    for rule in rules {
        if !state_exchange_rule_is_active(instance, &rule, outcome) {
            continue;
        }
        let value = node_output_value(
            &instance.state_snapshot,
            &rule.from_node_id,
            &rule.from_port,
        )
        .cloned()
        .ok_or_else(|| OrchestrationRuntimeError::StateExchangeMissingOutput {
            rule_id: rule.rule_id.clone(),
            from_node_id: rule.from_node_id.clone(),
            from_port: rule.from_port.clone(),
        })?;
        let Some(target) = find_runtime_node_by_id_mut(&mut instance.node_tree, &rule.to_node_id)
        else {
            return Err(OrchestrationRuntimeError::RuntimeNodeNotFound {
                node_id: rule.to_node_id,
            });
        };
        upsert_node_port_value(&mut target.inputs, rule.to_port, value);
    }
    Ok(())
}

fn state_exchange_rule_is_active(
    instance: &OrchestrationInstance,
    rule: &StateExchangeRule,
    outcome: &mut OrchestrationRuntimeApplyOutcome,
) -> bool {
    let Some(transition_rule_id) = rule.source_transition_id.as_deref() else {
        return true;
    };
    let Some(transition_rule) = find_transition_rule(&instance.plan_snapshot, transition_rule_id)
    else {
        outcome
            .diagnostics
            .push(OrchestrationRuntimeDiagnostic::blocking(
                "state_exchange_transition_missing",
                format!(
                    "state exchange `{}` references missing transition rule `{transition_rule_id}`",
                    rule.rule_id
                ),
                Some(rule.to_node_id.clone()),
                Some(rule.rule_id.clone()),
            ));
        return false;
    };
    transition_rule_satisfied(instance, transition_rule, outcome)
}

fn activate_transition_successors(
    instance: &mut OrchestrationInstance,
    completed_node_id: &str,
    outcome: &mut OrchestrationRuntimeApplyOutcome,
) -> Result<(), OrchestrationRuntimeError> {
    let targets = instance
        .plan_snapshot
        .activation_rules
        .iter()
        .filter_map(|rule| match rule {
            ActivationRule::Transition {
                from_node_id,
                to_node_id,
                ..
            } if from_node_id == completed_node_id => Some(to_node_id.clone()),
            _ => None,
        })
        .collect::<BTreeSet<_>>();

    for target_node_id in targets {
        let Some(target) = find_runtime_node_by_id(&instance.node_tree, &target_node_id) else {
            return Err(OrchestrationRuntimeError::RuntimeNodeNotFound {
                node_id: target_node_id,
            });
        };
        if target.status != RuntimeNodeStatus::Pending {
            continue;
        }
        let diagnostic_count_before = outcome.diagnostics.len();
        if target_transition_activation_satisfied(instance, &target_node_id, outcome) {
            let activated_node_id;
            {
                let Some(target) =
                    find_runtime_node_by_id_mut(&mut instance.node_tree, &target_node_id)
                else {
                    return Err(OrchestrationRuntimeError::RuntimeNodeNotFound {
                        node_id: target_node_id,
                    });
                };
                target.status = RuntimeNodeStatus::Ready;
                activated_node_id = target.node_id.clone();
            }
            push_ready_node(instance, &activated_node_id);
            outcome.activated_node_ids.push(activated_node_id);
        } else if outcome.diagnostics.len() > diagnostic_count_before {
            let diagnostic = outcome
                .diagnostics
                .last()
                .expect("diagnostic count increased");
            mark_runtime_node_blocked(&mut instance.node_tree, &target_node_id, diagnostic)?;
        } else if target_incoming_sources_are_terminal(instance, &target_node_id) {
            mark_runtime_node_skipped(&mut instance.node_tree, &target_node_id)?;
        }
    }
    Ok(())
}

fn target_transition_activation_satisfied(
    instance: &OrchestrationInstance,
    target_node_id: &str,
    outcome: &mut OrchestrationRuntimeApplyOutcome,
) -> bool {
    let incoming = instance
        .plan_snapshot
        .activation_rules
        .iter()
        .filter(|rule| {
            matches!(
                rule,
                ActivationRule::Transition { to_node_id, .. } if to_node_id == target_node_id
            )
        })
        .collect::<Vec<_>>();
    if incoming.is_empty() {
        return false;
    }
    let join_policy = incoming
        .iter()
        .find_map(|rule| match rule {
            ActivationRule::Transition { join_policy, .. } => Some(*join_policy),
            _ => None,
        })
        .unwrap_or(ActivityJoinPolicy::All);

    let satisfied_count = incoming
        .iter()
        .filter(|rule| transition_rule_satisfied(instance, rule, outcome))
        .count();

    match join_policy {
        ActivityJoinPolicy::All => satisfied_count == incoming.len(),
        ActivityJoinPolicy::Any | ActivityJoinPolicy::First => satisfied_count > 0,
        ActivityJoinPolicy::NOfM { n } => satisfied_count >= n as usize,
    }
}

fn target_incoming_sources_are_terminal(
    instance: &OrchestrationInstance,
    target_node_id: &str,
) -> bool {
    let incoming = instance
        .plan_snapshot
        .activation_rules
        .iter()
        .filter_map(|rule| match rule {
            ActivationRule::Transition {
                from_node_id,
                to_node_id,
                ..
            } if to_node_id == target_node_id => Some(from_node_id),
            _ => None,
        })
        .collect::<Vec<_>>();
    !incoming.is_empty()
        && incoming.iter().all(|source_node_id| {
            find_runtime_node_by_id(&instance.node_tree, source_node_id)
                .is_some_and(|node| is_terminal_status(node.status))
        })
}

fn mark_runtime_node_blocked(
    nodes: &mut [RuntimeNodeState],
    node_id: &str,
    diagnostic: &OrchestrationRuntimeDiagnostic,
) -> Result<(), OrchestrationRuntimeError> {
    let Some(node) = find_runtime_node_by_id_mut(nodes, node_id) else {
        return Err(OrchestrationRuntimeError::RuntimeNodeNotFound {
            node_id: node_id.to_string(),
        });
    };
    node.status = RuntimeNodeStatus::Blocked;
    node.error = Some(RuntimeNodeError {
        code: diagnostic.code.clone(),
        message: diagnostic.message.clone(),
        retryable: false,
        detail: Some(serde_json::json!({
            "rule_id": diagnostic.rule_id,
            "node_id": diagnostic.node_id,
        })),
    });
    Ok(())
}

fn mark_runtime_node_skipped(
    nodes: &mut [RuntimeNodeState],
    node_id: &str,
) -> Result<(), OrchestrationRuntimeError> {
    let Some(node) = find_runtime_node_by_id_mut(nodes, node_id) else {
        return Err(OrchestrationRuntimeError::RuntimeNodeNotFound {
            node_id: node_id.to_string(),
        });
    };
    node.status = RuntimeNodeStatus::Skipped;
    node.completed_at = Some(Utc::now());
    Ok(())
}

fn transition_rule_satisfied(
    instance: &OrchestrationInstance,
    rule: &ActivationRule,
    outcome: &mut OrchestrationRuntimeApplyOutcome,
) -> bool {
    let ActivationRule::Transition {
        rule_id,
        from_node_id,
        condition,
        max_traversals,
        ..
    } = rule
    else {
        return false;
    };
    if max_traversals.is_some() {
        outcome.diagnostics.push(OrchestrationRuntimeDiagnostic::blocking(
            "max_traversals_not_supported",
            format!(
                "transition `{rule_id}` declares max_traversals; traversal counting is not implemented in this runtime slice"
            ),
            Some(from_node_id.clone()),
            Some(rule_id.clone()),
        ));
        return false;
    }
    let Some(source) = find_runtime_node_by_id(&instance.node_tree, from_node_id) else {
        return false;
    };
    if source.status != RuntimeNodeStatus::Completed {
        return false;
    }
    transition_condition_matches(&instance.state_snapshot, condition)
}

fn transition_condition_matches(
    snapshot: &StateExchangeSnapshot,
    condition: &TransitionCondition,
) -> bool {
    match condition {
        TransitionCondition::Always => true,
        TransitionCondition::ArtifactFieldEquals {
            activity,
            port,
            path,
            value,
        } => {
            node_output_value(snapshot, activity, port)
                .and_then(|output| select_json_path(output, path))
                == Some(value)
        }
        TransitionCondition::HumanDecisionEquals {
            activity,
            decision_port,
            value,
        } => {
            node_output_value(snapshot, activity, decision_port).and_then(Value::as_str)
                == Some(value.as_str())
        }
        TransitionCondition::AgentSignalEquals {
            activity,
            signal_key,
            value,
        } => node_output_value(snapshot, activity, signal_key) == Some(value),
    }
}

fn select_json_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    if path.starts_with('/') {
        return value.pointer(path);
    }
    let mut current = value;
    for segment in path.split('.').filter(|segment| !segment.is_empty()) {
        current = current.get(segment)?;
    }
    Some(current)
}

fn node_output_value<'a>(
    snapshot: &'a StateExchangeSnapshot,
    node_id: &str,
    port_key: &str,
) -> Option<&'a Value> {
    snapshot.node_outputs.get(node_id)?.get(port_key)
}

fn find_transition_rule<'a>(
    plan: &'a OrchestrationPlanSnapshot,
    rule_id: &str,
) -> Option<&'a ActivationRule> {
    plan.activation_rules.iter().find(|rule| {
        matches!(
            rule,
            ActivationRule::Transition {
                rule_id: candidate,
                ..
            } if candidate == rule_id
        )
    })
}

fn upsert_node_port_value(values: &mut Vec<NodePortValue>, port_key: String, value: Value) {
    if let Some(existing) = values.iter_mut().find(|item| item.port_key == port_key) {
        existing.value = value;
    } else {
        values.push(NodePortValue { port_key, value });
    }
}

fn runtime_trace_ref_from_executor_run_ref(executor_run_ref: &ExecutorRunRef) -> RuntimeTraceRef {
    match executor_run_ref {
        ExecutorRunRef::RuntimeSession { session_id } => RuntimeTraceRef::RuntimeSession {
            session_id: session_id.clone(),
        },
        ExecutorRunRef::FunctionRun { run_id } => RuntimeTraceRef::FunctionRun {
            run_id: run_id.clone(),
        },
        ExecutorRunRef::HumanDecision { decision_id } => RuntimeTraceRef::HumanDecision {
            decision_id: decision_id.clone(),
        },
    }
}

fn push_runtime_trace_ref(node: &mut RuntimeNodeState, trace_ref: RuntimeTraceRef) {
    if !node
        .trace_refs
        .iter()
        .any(|existing| existing == &trace_ref)
    {
        node.trace_refs.push(trace_ref);
    }
}

fn remove_ready_node(instance: &mut OrchestrationInstance, node_id_or_path: &str) {
    instance
        .activation
        .ready_node_ids
        .retain(|node_id| node_id != node_id_or_path);
    instance
        .dispatch
        .ready_node_ids
        .retain(|node_id| node_id != node_id_or_path);
}

fn push_ready_node(instance: &mut OrchestrationInstance, node_id: &str) {
    if !instance
        .activation
        .ready_node_ids
        .iter()
        .any(|existing| existing == node_id)
    {
        instance.activation.ready_node_ids.push(node_id.to_string());
    }
    if !instance
        .dispatch
        .ready_node_ids
        .iter()
        .any(|existing| existing == node_id)
    {
        instance.dispatch.ready_node_ids.push(node_id.to_string());
    }
}

fn find_runtime_node<'a>(
    nodes: &'a [RuntimeNodeState],
    node_path: &str,
    attempt: u32,
) -> Option<&'a RuntimeNodeState> {
    for node in nodes {
        if node.node_path == node_path && node.attempt == attempt {
            return Some(node);
        }
        if let Some(found) = find_runtime_node(&node.children, node_path, attempt) {
            return Some(found);
        }
    }
    None
}

fn find_runtime_node_mut<'a>(
    nodes: &'a mut [RuntimeNodeState],
    node_path: &str,
    attempt: u32,
) -> Option<&'a mut RuntimeNodeState> {
    for node in nodes {
        if node.node_path == node_path && node.attempt == attempt {
            return Some(node);
        }
        if let Some(found) = find_runtime_node_mut(&mut node.children, node_path, attempt) {
            return Some(found);
        }
    }
    None
}

fn find_runtime_node_by_id<'a>(
    nodes: &'a [RuntimeNodeState],
    node_id: &str,
) -> Option<&'a RuntimeNodeState> {
    for node in nodes {
        if node.node_id == node_id {
            return Some(node);
        }
        if let Some(found) = find_runtime_node_by_id(&node.children, node_id) {
            return Some(found);
        }
    }
    None
}

fn find_runtime_node_by_id_mut<'a>(
    nodes: &'a mut [RuntimeNodeState],
    node_id: &str,
) -> Option<&'a mut RuntimeNodeState> {
    for node in nodes {
        if node.node_id == node_id {
            return Some(node);
        }
        if let Some(found) = find_runtime_node_by_id_mut(&mut node.children, node_id) {
            return Some(found);
        }
    }
    None
}

fn is_terminal_status(status: RuntimeNodeStatus) -> bool {
    matches!(
        status,
        RuntimeNodeStatus::Completed
            | RuntimeNodeStatus::Failed
            | RuntimeNodeStatus::Cancelled
            | RuntimeNodeStatus::Skipped
    )
}

fn derive_orchestration_status(instance: &mut OrchestrationInstance) {
    let statuses = collect_node_statuses(&instance.node_tree);
    instance.status = if statuses.contains(&RuntimeNodeStatus::Failed) {
        OrchestrationStatus::Failed
    } else if statuses.iter().any(|status| {
        matches!(
            status,
            RuntimeNodeStatus::Ready | RuntimeNodeStatus::Claiming | RuntimeNodeStatus::Running
        )
    }) {
        OrchestrationStatus::Running
    } else if statuses.contains(&RuntimeNodeStatus::Blocked) {
        OrchestrationStatus::Paused
    } else if !statuses.is_empty()
        && statuses.iter().all(|status| {
            matches!(
                status,
                RuntimeNodeStatus::Completed | RuntimeNodeStatus::Skipped
            )
        })
    {
        OrchestrationStatus::Completed
    } else if !statuses.is_empty()
        && statuses
            .iter()
            .all(|status| *status == RuntimeNodeStatus::Cancelled)
    {
        OrchestrationStatus::Cancelled
    } else {
        OrchestrationStatus::Pending
    };
}

fn collect_node_statuses(nodes: &[RuntimeNodeState]) -> Vec<RuntimeNodeStatus> {
    let mut statuses = Vec::new();
    for node in nodes {
        statuses.push(node.status);
        statuses.extend(collect_node_statuses(&node.children));
    }
    statuses
}

#[cfg(test)]
mod tests {
    use agentdash_domain::workflow::{
        ActivationRule, ActivityJoinPolicy, ArtifactAliasPolicy, ExecutorRunRef, ExecutorSpec,
        LifecycleRunStatus, NodePortValue, OrchestrationLimits, OrchestrationSourceRef, PlanNode,
        PlanNodeKind, RuntimeNodeStatus, StateExchangeRule, TransitionCondition,
    };
    use chrono::Utc;
    use serde_json::json;
    use uuid::Uuid;

    use super::*;

    fn plan_node(node_id: &str, kind: PlanNodeKind) -> PlanNode {
        PlanNode {
            node_id: node_id.to_string(),
            node_path: node_id.to_string(),
            parent_node_id: None,
            kind,
            label: Some(node_id.to_string()),
            executor: None::<ExecutorSpec>,
            input_ports: Vec::new(),
            output_ports: Vec::new(),
            completion_policy: None,
            iteration_policy: None,
            join_policy: None,
            result_contract: None,
            metadata: None,
        }
    }

    fn plan_snapshot(source_ref: OrchestrationSourceRef) -> OrchestrationPlanSnapshot {
        OrchestrationPlanSnapshot {
            plan_digest: "sha256:activation-fixture".to_string(),
            plan_version: 1,
            source_ref,
            nodes: vec![
                plan_node("entry", PlanNodeKind::AgentCall),
                plan_node("follow_up", PlanNodeKind::Function),
            ],
            entry_node_ids: vec!["entry".to_string()],
            activation_rules: vec![ActivationRule::Entry {
                node_id: "entry".to_string(),
            }],
            state_exchange_rules: Vec::new(),
            limits: OrchestrationLimits::default(),
            metadata: None,
            created_at: Utc::now(),
        }
    }

    fn single_node_plan_snapshot(source_ref: OrchestrationSourceRef) -> OrchestrationPlanSnapshot {
        OrchestrationPlanSnapshot {
            plan_digest: "sha256:single-node-fixture".to_string(),
            plan_version: 1,
            source_ref,
            nodes: vec![plan_node("entry", PlanNodeKind::AgentCall)],
            entry_node_ids: vec!["entry".to_string()],
            activation_rules: vec![ActivationRule::Entry {
                node_id: "entry".to_string(),
            }],
            state_exchange_rules: Vec::new(),
            limits: OrchestrationLimits::default(),
            metadata: None,
            created_at: Utc::now(),
        }
    }

    fn transition_plan_snapshot(
        source_ref: OrchestrationSourceRef,
        state_exchange_rules: Vec<StateExchangeRule>,
        condition: TransitionCondition,
        max_traversals: Option<u32>,
    ) -> OrchestrationPlanSnapshot {
        OrchestrationPlanSnapshot {
            plan_digest: "sha256:transition-fixture".to_string(),
            plan_version: 1,
            source_ref,
            nodes: vec![
                plan_node("entry", PlanNodeKind::AgentCall),
                plan_node("follow_up", PlanNodeKind::Function),
            ],
            entry_node_ids: vec!["entry".to_string()],
            activation_rules: vec![
                ActivationRule::Entry {
                    node_id: "entry".to_string(),
                },
                ActivationRule::Transition {
                    rule_id: "transition:0:entry->follow_up".to_string(),
                    from_node_id: "entry".to_string(),
                    to_node_id: "follow_up".to_string(),
                    condition,
                    join_policy: ActivityJoinPolicy::All,
                    max_traversals,
                    source_path: None,
                },
            ],
            state_exchange_rules,
            limits: OrchestrationLimits::default(),
            metadata: None,
            created_at: Utc::now(),
        }
    }

    fn workflow_source() -> OrchestrationSourceRef {
        OrchestrationSourceRef::WorkflowGraph {
            graph_id: Uuid::new_v4(),
            graph_version: Some(1),
        }
    }

    #[test]
    fn orchestration_runtime_activation_materializes_entry_ready_nodes() {
        let graph_id = Uuid::new_v4();
        let source_ref = OrchestrationSourceRef::WorkflowGraph {
            graph_id,
            graph_version: Some(7),
        };

        let orchestration =
            activate_root_orchestration(source_ref.clone(), plan_snapshot(source_ref.clone()));

        assert_eq!(orchestration.role, ROOT_ORCHESTRATION_ROLE);
        assert_eq!(orchestration.source_ref, source_ref);
        assert_eq!(
            orchestration.plan_snapshot.plan_digest,
            "sha256:activation-fixture"
        );
        assert_eq!(orchestration.activation.ready_node_ids, vec!["entry"]);
        assert_eq!(orchestration.dispatch.ready_node_ids, vec!["entry"]);
        assert_eq!(orchestration.status, OrchestrationStatus::Running);
        assert!(orchestration.state_snapshot.variables.is_empty());
        assert!(orchestration.state_snapshot.node_outputs.is_empty());

        let nodes = orchestration
            .node_tree
            .iter()
            .map(|node| (node.node_id.as_str(), node.status, node.attempt))
            .collect::<Vec<_>>();
        assert_eq!(
            nodes,
            vec![
                ("entry", RuntimeNodeStatus::Ready, 1),
                ("follow_up", RuntimeNodeStatus::Pending, 1),
            ]
        );
    }

    #[test]
    fn orchestration_runtime_activation_materializes_typed_root_input() {
        let source_ref = workflow_source();
        let orchestration = activate_orchestration_with_input(
            ROOT_ORCHESTRATION_ROLE,
            source_ref.clone(),
            plan_snapshot(source_ref),
            OrchestrationActivationInput {
                root_args: Some(json!({"topic": "runtime"})),
                root_input_bindings: vec![RootInputBinding::new("entry", "topic")],
            },
        );

        let entry = orchestration
            .node_tree
            .iter()
            .find(|node| node.node_id == "entry")
            .expect("entry node");
        assert_eq!(
            entry.inputs,
            vec![NodePortValue {
                port_key: "topic".to_string(),
                value: json!("runtime"),
            }]
        );
    }

    #[test]
    fn orchestration_runtime_default_activation_ignores_script_metadata_root_args() {
        let source_ref = workflow_source();
        let mut snapshot = plan_snapshot(source_ref.clone());
        snapshot.metadata = Some(json!({
            "script": {
                "args": {
                    "topic": "metadata-runtime"
                },
                "root_input_bindings": [
                    {
                        "node_id": "entry",
                        "port": "topic",
                        "source": "args"
                    }
                ]
            }
        }));

        let orchestration = activate_root_orchestration(source_ref, snapshot);

        let entry = orchestration
            .node_tree
            .iter()
            .find(|node| node.node_id == "entry")
            .expect("entry node");
        assert!(entry.inputs.is_empty());
    }

    #[test]
    fn orchestration_runtime_node_started_updates_executor_ref_and_ready_queue() {
        let source_ref = workflow_source();
        let mut orchestration =
            activate_root_orchestration(source_ref.clone(), plan_snapshot(source_ref));

        let outcome = apply_orchestration_event(
            &mut orchestration,
            OrchestrationRuntimeEvent::NodeStarted {
                node_path: "entry".to_string(),
                attempt: 1,
                executor_run_ref: Some(ExecutorRunRef::RuntimeSession {
                    session_id: "session-1".to_string(),
                }),
                timestamp: Utc::now(),
            },
        )
        .expect("node started");

        assert!(outcome.activated_node_ids.is_empty());
        assert!(orchestration.dispatch.ready_node_ids.is_empty());
        let entry = orchestration
            .node_tree
            .iter()
            .find(|node| node.node_id == "entry")
            .expect("entry node");
        assert_eq!(entry.status, RuntimeNodeStatus::Running);
        assert_eq!(
            entry.executor_run_ref,
            Some(ExecutorRunRef::RuntimeSession {
                session_id: "session-1".to_string()
            })
        );
        assert_eq!(
            entry.trace_refs,
            vec![RuntimeTraceRef::RuntimeSession {
                session_id: "session-1".to_string()
            }]
        );
        assert!(entry.started_at.is_some());
    }

    #[test]
    fn orchestration_runtime_node_started_refreshes_lifecycle_run_as_running() {
        let source_ref = workflow_source();
        let orchestration =
            activate_root_orchestration(source_ref.clone(), single_node_plan_snapshot(source_ref));
        let orchestration_id = orchestration.orchestration_id;
        let mut run = LifecycleRun::new_control(Uuid::new_v4());
        assert!(run.add_orchestration(orchestration));

        let (run, _) = apply_orchestration_event_to_run(
            run,
            orchestration_id,
            OrchestrationRuntimeEvent::NodeStarted {
                node_path: "entry".to_string(),
                attempt: 1,
                executor_run_ref: Some(ExecutorRunRef::RuntimeSession {
                    session_id: "session-1".to_string(),
                }),
                timestamp: Utc::now(),
            },
        )
        .expect("node started");

        assert_eq!(run.status, LifecycleRunStatus::Running);
        assert_eq!(
            run.orchestration_by_id(orchestration_id)
                .expect("orchestration")
                .status,
            OrchestrationStatus::Running
        );
    }

    #[test]
    fn orchestration_runtime_terminal_events_refresh_lifecycle_run_status_matrix() {
        for (event, expected_run_status, expected_orchestration_status) in [
            (
                OrchestrationRuntimeEvent::NodeCompleted {
                    node_path: "entry".to_string(),
                    attempt: 1,
                    outputs: Vec::new(),
                    timestamp: Utc::now(),
                },
                LifecycleRunStatus::Completed,
                OrchestrationStatus::Completed,
            ),
            (
                OrchestrationRuntimeEvent::NodeFailed {
                    node_path: "entry".to_string(),
                    attempt: 1,
                    error: RuntimeNodeError {
                        code: "executor_failed".to_string(),
                        message: "executor failed".to_string(),
                        retryable: false,
                        detail: None,
                    },
                    timestamp: Utc::now(),
                },
                LifecycleRunStatus::Failed,
                OrchestrationStatus::Failed,
            ),
            (
                OrchestrationRuntimeEvent::NodeBlocked {
                    node_path: "entry".to_string(),
                    attempt: 1,
                    error: RuntimeNodeError {
                        code: "human_gate_waiting".to_string(),
                        message: "waiting for decision".to_string(),
                        retryable: false,
                        detail: None,
                    },
                    timestamp: Utc::now(),
                },
                LifecycleRunStatus::Blocked,
                OrchestrationStatus::Paused,
            ),
            (
                OrchestrationRuntimeEvent::NodeCancelled {
                    node_path: "entry".to_string(),
                    attempt: 1,
                    reason: Some("user cancelled".to_string()),
                    timestamp: Utc::now(),
                },
                LifecycleRunStatus::Cancelled,
                OrchestrationStatus::Cancelled,
            ),
        ] {
            let source_ref = workflow_source();
            let orchestration = activate_root_orchestration(
                source_ref.clone(),
                single_node_plan_snapshot(source_ref),
            );
            let orchestration_id = orchestration.orchestration_id;
            let mut run = LifecycleRun::new_control(Uuid::new_v4());
            assert!(run.add_orchestration(orchestration));

            let (run, _) =
                apply_orchestration_event_to_run(run, orchestration_id, event).expect("event");

            assert_eq!(run.status, expected_run_status);
            assert_eq!(
                run.orchestration_by_id(orchestration_id)
                    .expect("orchestration")
                    .status,
                expected_orchestration_status
            );
        }
    }

    #[test]
    fn orchestration_runtime_append_orchestration_refreshes_run_status_from_all_instances() {
        let root_source = workflow_source();
        let root = activate_root_orchestration(
            root_source.clone(),
            single_node_plan_snapshot(root_source),
        );
        let root_id = root.orchestration_id;
        let mut run = LifecycleRun::new_control(Uuid::new_v4());
        assert!(run.add_orchestration(root));
        let (mut run, _) = apply_orchestration_event_to_run(
            run,
            root_id,
            OrchestrationRuntimeEvent::NodeCompleted {
                node_path: "entry".to_string(),
                attempt: 1,
                outputs: Vec::new(),
                timestamp: Utc::now(),
            },
        )
        .expect("complete root");
        assert_eq!(run.status, LifecycleRunStatus::Completed);

        let append_source = workflow_source();
        let append = activate_root_orchestration(
            append_source.clone(),
            single_node_plan_snapshot(append_source),
        );
        let append_id = append.orchestration_id;
        assert!(run.add_orchestration(append));

        let (run, _) = apply_orchestration_event_to_run(
            run,
            append_id,
            OrchestrationRuntimeEvent::NodeStarted {
                node_path: "entry".to_string(),
                attempt: 1,
                executor_run_ref: Some(ExecutorRunRef::FunctionRun {
                    run_id: "append-function-1".to_string(),
                }),
                timestamp: Utc::now(),
            },
        )
        .expect("start append");

        assert_eq!(run.status, LifecycleRunStatus::Running);
        assert_eq!(
            run.orchestration_by_id(root_id)
                .expect("root orchestration")
                .status,
            OrchestrationStatus::Completed
        );
        assert_eq!(
            run.orchestration_by_id(append_id)
                .expect("append orchestration")
                .status,
            OrchestrationStatus::Running
        );
    }

    #[test]
    fn orchestration_runtime_node_completed_activates_simple_transition() {
        let source_ref = workflow_source();
        let mut orchestration = activate_root_orchestration(
            source_ref.clone(),
            transition_plan_snapshot(source_ref, Vec::new(), TransitionCondition::Always, None),
        );

        let outcome = apply_orchestration_event(
            &mut orchestration,
            OrchestrationRuntimeEvent::NodeCompleted {
                node_path: "entry".to_string(),
                attempt: 1,
                outputs: Vec::new(),
                timestamp: Utc::now(),
            },
        )
        .expect("node completed");

        assert_eq!(outcome.activated_node_ids, vec!["follow_up"]);
        assert!(outcome.diagnostics.is_empty());
        assert_eq!(orchestration.dispatch.ready_node_ids, vec!["follow_up"]);
        let states = orchestration
            .node_tree
            .iter()
            .map(|node| (node.node_id.as_str(), node.status))
            .collect::<Vec<_>>();
        assert_eq!(
            states,
            vec![
                ("entry", RuntimeNodeStatus::Completed),
                ("follow_up", RuntimeNodeStatus::Ready),
            ]
        );
    }

    #[test]
    fn orchestration_runtime_node_completed_materializes_state_exchange() {
        let source_ref = workflow_source();
        let state_exchange_rule = StateExchangeRule {
            rule_id: "artifact:0:0:entry->follow_up".to_string(),
            from_node_id: "entry".to_string(),
            from_port: "proposal".to_string(),
            to_node_id: "follow_up".to_string(),
            to_port: "proposal_in".to_string(),
            alias: ArtifactAliasPolicy::Latest,
            source_transition_id: Some("transition:0:entry->follow_up".to_string()),
            source_path: None,
        };
        let mut orchestration = activate_root_orchestration(
            source_ref.clone(),
            transition_plan_snapshot(
                source_ref,
                vec![state_exchange_rule],
                TransitionCondition::Always,
                None,
            ),
        );

        let output = NodePortValue {
            port_key: "proposal".to_string(),
            value: json!({"title": "plan"}),
        };
        let outcome = apply_orchestration_event(
            &mut orchestration,
            OrchestrationRuntimeEvent::NodeCompleted {
                node_path: "entry".to_string(),
                attempt: 1,
                outputs: vec![output.clone()],
                timestamp: Utc::now(),
            },
        )
        .expect("node completed");

        assert_eq!(outcome.activated_node_ids, vec!["follow_up"]);
        assert_eq!(
            orchestration.state_snapshot.node_outputs["entry"]["proposal"],
            output.value
        );
        let follow_up = orchestration
            .node_tree
            .iter()
            .find(|node| node.node_id == "follow_up")
            .expect("follow up node");
        assert_eq!(
            follow_up.inputs,
            vec![NodePortValue {
                port_key: "proposal_in".to_string(),
                value: json!({"title": "plan"}),
            }]
        );
    }

    #[test]
    fn orchestration_runtime_duplicate_terminal_event_is_idempotent() {
        let source_ref = workflow_source();
        let mut orchestration = activate_root_orchestration(
            source_ref.clone(),
            transition_plan_snapshot(source_ref, Vec::new(), TransitionCondition::Always, None),
        );

        apply_orchestration_event(
            &mut orchestration,
            OrchestrationRuntimeEvent::NodeCompleted {
                node_path: "entry".to_string(),
                attempt: 1,
                outputs: vec![NodePortValue {
                    port_key: "result".to_string(),
                    value: json!("first"),
                }],
                timestamp: Utc::now(),
            },
        )
        .expect("first terminal");
        let duplicate = apply_orchestration_event(
            &mut orchestration,
            OrchestrationRuntimeEvent::NodeCompleted {
                node_path: "entry".to_string(),
                attempt: 1,
                outputs: vec![NodePortValue {
                    port_key: "result".to_string(),
                    value: json!("second"),
                }],
                timestamp: Utc::now(),
            },
        )
        .expect("duplicate terminal");

        assert!(duplicate.terminal_idempotent);
        assert!(duplicate.activated_node_ids.is_empty());
        assert_eq!(orchestration.dispatch.ready_node_ids, vec!["follow_up"]);
        assert_eq!(
            orchestration.state_snapshot.node_outputs["entry"]["result"],
            json!("first")
        );
    }

    #[test]
    fn orchestration_runtime_max_traversals_blocks_activation_with_diagnostic() {
        let source_ref = workflow_source();
        let mut orchestration = activate_root_orchestration(
            source_ref.clone(),
            transition_plan_snapshot(source_ref, Vec::new(), TransitionCondition::Always, Some(1)),
        );

        let outcome = apply_orchestration_event(
            &mut orchestration,
            OrchestrationRuntimeEvent::NodeCompleted {
                node_path: "entry".to_string(),
                attempt: 1,
                outputs: Vec::new(),
                timestamp: Utc::now(),
            },
        )
        .expect("node completed");

        assert!(outcome.activated_node_ids.is_empty());
        assert_eq!(outcome.diagnostics[0].code, "max_traversals_not_supported");
        let follow_up = orchestration
            .node_tree
            .iter()
            .find(|node| node.node_id == "follow_up")
            .expect("follow up node");
        assert_eq!(follow_up.status, RuntimeNodeStatus::Blocked);
        assert_eq!(
            follow_up.error.as_ref().map(|error| error.code.as_str()),
            Some("max_traversals_not_supported")
        );
    }

    #[test]
    fn orchestration_runtime_condition_false_skips_unreachable_successor() {
        let source_ref = workflow_source();
        let mut orchestration = activate_root_orchestration(
            source_ref.clone(),
            transition_plan_snapshot(
                source_ref,
                Vec::new(),
                TransitionCondition::AgentSignalEquals {
                    activity: "entry".to_string(),
                    signal_key: "decision".to_string(),
                    value: json!("go"),
                },
                None,
            ),
        );

        let outcome = apply_orchestration_event(
            &mut orchestration,
            OrchestrationRuntimeEvent::NodeCompleted {
                node_path: "entry".to_string(),
                attempt: 1,
                outputs: vec![NodePortValue {
                    port_key: "decision".to_string(),
                    value: json!("stop"),
                }],
                timestamp: Utc::now(),
            },
        )
        .expect("node completed");

        assert!(outcome.activated_node_ids.is_empty());
        let follow_up = orchestration
            .node_tree
            .iter()
            .find(|node| node.node_id == "follow_up")
            .expect("follow up node");
        assert_eq!(follow_up.status, RuntimeNodeStatus::Skipped);
        assert_eq!(orchestration.status, OrchestrationStatus::Completed);
    }
}
