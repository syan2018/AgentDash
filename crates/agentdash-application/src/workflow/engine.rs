use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use agentdash_domain::workflow::{
    ActivityAttemptState, ActivityAttemptStatus, ActivityCompletionPolicy, ActivityInputArtifact,
    ActivityLifecycleRunState, ActivityOutputArtifact, ActivityPortValue, ActivityRunStatus,
    ActivityTransition, ExecutorRunRef, TransitionCondition, WorkflowGraph,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ActivityEvent {
    SchedulerClaimAccepted {
        activity_key: String,
        attempt: u32,
    },
    SchedulerStartFailed {
        activity_key: String,
        attempt: u32,
        error: String,
        retryable: bool,
    },
    ExecutorStarted {
        activity_key: String,
        attempt: u32,
        executor_run: ExecutorRunRef,
    },
    ActivityCompleted {
        activity_key: String,
        attempt: u32,
        outputs: Vec<ActivityPortValue>,
        summary: Option<String>,
    },
    ActivityFailed {
        activity_key: String,
        attempt: u32,
        error: String,
    },
    ActivityCancelled {
        activity_key: String,
        attempt: u32,
        reason: Option<String>,
    },
    HumanDecisionSubmitted {
        activity_key: String,
        attempt: u32,
        decision_port: String,
        decision: Value,
        summary: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LifecycleEngineError {
    ActivityNotFound(String),
    AttemptNotFound {
        activity_key: String,
        attempt: u32,
    },
    InvalidAttemptStatus {
        activity_key: String,
        attempt: u32,
        expected: &'static str,
        actual: ActivityAttemptStatus,
    },
    CompletionPolicyRejected(String),
    AttemptLimitReached {
        activity_key: String,
        max_attempts: u32,
    },
    ArtifactMissing(String),
}

impl std::fmt::Display for LifecycleEngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ActivityNotFound(activity_key) => {
                write!(f, "activity 不存在: {activity_key}")
            }
            Self::AttemptNotFound {
                activity_key,
                attempt,
            } => {
                write!(f, "activity attempt 不存在: {activity_key}#{attempt}")
            }
            Self::InvalidAttemptStatus {
                activity_key,
                attempt,
                expected,
                actual,
            } => write!(
                f,
                "activity attempt 状态不允许: {activity_key}#{attempt} expected={expected}, actual={actual:?}"
            ),
            Self::CompletionPolicyRejected(message) => f.write_str(message),
            Self::AttemptLimitReached {
                activity_key,
                max_attempts,
            } => {
                write!(
                    f,
                    "activity `{activity_key}` 已达到 max_attempts={max_attempts}"
                )
            }
            Self::ArtifactMissing(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for LifecycleEngineError {}

pub struct LifecycleEngine;

impl LifecycleEngine {
    pub fn initialize(
        definition: &WorkflowGraph,
        graph_instance_id: uuid::Uuid,
    ) -> Result<ActivityLifecycleRunState, LifecycleEngineError> {
        if !definition
            .activities
            .iter()
            .any(|activity| activity.key == definition.entry_activity_key)
        {
            return Err(LifecycleEngineError::ActivityNotFound(
                definition.entry_activity_key.clone(),
            ));
        }
        let mut attempts = vec![ActivityAttemptState {
            activity_key: definition.entry_activity_key.clone(),
            attempt: 1,
            status: ActivityAttemptStatus::Ready,
            executor_run: None,
            started_at: None,
            completed_at: None,
            summary: None,
        }];
        attempts.extend(
            definition
                .activities
                .iter()
                .filter(|activity| activity.key != definition.entry_activity_key)
                .map(|activity| ActivityAttemptState {
                    activity_key: activity.key.clone(),
                    attempt: 1,
                    status: ActivityAttemptStatus::Pending,
                    executor_run: None,
                    started_at: None,
                    completed_at: None,
                    summary: None,
                }),
        );
        let mut state = ActivityLifecycleRunState {
            graph_instance_id,
            status: ActivityRunStatus::Ready,
            attempts,
            outputs: vec![],
            inputs: vec![],
        };
        derive_run_status(definition, &mut state);
        Ok(state)
    }

    pub fn apply_event(
        definition: &WorkflowGraph,
        state: &mut ActivityLifecycleRunState,
        event: ActivityEvent,
    ) -> Result<(), LifecycleEngineError> {
        match event {
            ActivityEvent::SchedulerClaimAccepted {
                activity_key,
                attempt,
            } => {
                let attempt_state = find_attempt_mut(state, &activity_key, attempt)?;
                expect_status(attempt_state, ActivityAttemptStatus::Ready, "ready attempt")?;
                attempt_state.status = ActivityAttemptStatus::Claiming;
            }
            ActivityEvent::SchedulerStartFailed {
                activity_key,
                attempt,
                error,
                retryable,
            } => {
                let attempt_state = find_attempt_mut(state, &activity_key, attempt)?;
                expect_status(
                    attempt_state,
                    ActivityAttemptStatus::Claiming,
                    "claiming attempt",
                )?;
                if retryable {
                    attempt_state.status = ActivityAttemptStatus::Ready;
                } else {
                    attempt_state.status = ActivityAttemptStatus::Failed;
                    attempt_state.completed_at = Some(Utc::now());
                }
                attempt_state.summary = Some(error);
            }
            ActivityEvent::ExecutorStarted {
                activity_key,
                attempt,
                executor_run,
            } => {
                let attempt_state = find_attempt_mut(state, &activity_key, attempt)?;
                expect_status(
                    attempt_state,
                    ActivityAttemptStatus::Claiming,
                    "claiming attempt",
                )?;
                let now = Utc::now();
                attempt_state.status = ActivityAttemptStatus::Running;
                attempt_state.executor_run = Some(executor_run);
                attempt_state.started_at.get_or_insert(now);
            }
            ActivityEvent::ActivityCompleted {
                activity_key,
                attempt,
                outputs,
                summary,
            } => {
                complete_attempt(definition, state, &activity_key, attempt, outputs, summary)?;
            }
            ActivityEvent::ActivityFailed {
                activity_key,
                attempt,
                error,
            } => {
                let attempt_state = find_attempt_mut(state, &activity_key, attempt)?;
                expect_status(
                    attempt_state,
                    ActivityAttemptStatus::Running,
                    "running attempt",
                )?;
                let now = Utc::now();
                attempt_state.status = ActivityAttemptStatus::Failed;
                attempt_state.completed_at = Some(now);
                attempt_state.summary = Some(error);
            }
            ActivityEvent::ActivityCancelled {
                activity_key,
                attempt,
                reason,
            } => {
                let attempt_state = find_attempt_mut(state, &activity_key, attempt)?;
                expect_cancellable_status(attempt_state)?;
                let now = Utc::now();
                attempt_state.status = ActivityAttemptStatus::Cancelled;
                attempt_state.completed_at = Some(now);
                attempt_state.summary = reason;
            }
            ActivityEvent::HumanDecisionSubmitted {
                activity_key,
                attempt,
                decision_port,
                decision,
                summary,
            } => {
                complete_attempt(
                    definition,
                    state,
                    &activity_key,
                    attempt,
                    vec![ActivityPortValue {
                        port_key: decision_port,
                        value: decision,
                    }],
                    summary,
                )?;
            }
        }
        derive_run_status(definition, state);
        Ok(())
    }
}

fn complete_attempt(
    definition: &WorkflowGraph,
    state: &mut ActivityLifecycleRunState,
    activity_key: &str,
    attempt: u32,
    outputs: Vec<ActivityPortValue>,
    summary: Option<String>,
) -> Result<(), LifecycleEngineError> {
    let activity = definition
        .activities
        .iter()
        .find(|activity| activity.key == activity_key)
        .ok_or_else(|| LifecycleEngineError::ActivityNotFound(activity_key.to_string()))?;
    validate_completion_policy(activity, &outputs)?;

    let now = Utc::now();
    {
        let attempt_state = find_attempt_mut(state, activity_key, attempt)?;
        expect_status(
            attempt_state,
            ActivityAttemptStatus::Running,
            "running attempt",
        )?;
        attempt_state.status = ActivityAttemptStatus::Completed;
        attempt_state.completed_at = Some(now);
        attempt_state.summary = summary;
    }

    for output in outputs {
        state.outputs.push(ActivityOutputArtifact {
            activity_key: activity_key.to_string(),
            attempt,
            port_key: output.port_key,
            value: output.value,
            created_at: now,
        });
    }

    advance_successors(definition, state, activity_key)
}

fn validate_completion_policy(
    activity: &agentdash_domain::workflow::ActivityDefinition,
    outputs: &[ActivityPortValue],
) -> Result<(), LifecycleEngineError> {
    match &activity.completion_policy {
        ActivityCompletionPolicy::OutputPorts { required_ports } => {
            let missing = required_ports
                .iter()
                .filter(|port| !outputs.iter().any(|output| output.port_key == **port))
                .cloned()
                .collect::<Vec<_>>();
            if missing.is_empty() {
                Ok(())
            } else {
                Err(LifecycleEngineError::CompletionPolicyRejected(format!(
                    "activity `{}` 缺少 required output ports: {}",
                    activity.key,
                    missing.join(", ")
                )))
            }
        }
        ActivityCompletionPolicy::HumanDecision { decision_port } => {
            if outputs
                .iter()
                .any(|output| output.port_key == *decision_port)
            {
                Ok(())
            } else {
                Err(LifecycleEngineError::CompletionPolicyRejected(format!(
                    "activity `{}` 缺少 human decision port: {}",
                    activity.key, decision_port
                )))
            }
        }
        ActivityCompletionPolicy::HookGate { .. }
        | ActivityCompletionPolicy::ExecutorTerminal
        | ActivityCompletionPolicy::OpenEnded => Ok(()),
    }
}

fn advance_successors(
    definition: &WorkflowGraph,
    state: &mut ActivityLifecycleRunState,
    completed_activity_key: &str,
) -> Result<(), LifecycleEngineError> {
    let targets = definition
        .transitions
        .iter()
        .filter(|transition| transition.from == completed_activity_key)
        .filter(|transition| transition_condition_matches(definition, state, transition))
        .map(|transition| transition.to.clone())
        .collect::<std::collections::BTreeSet<_>>();

    for target_key in targets {
        if state.attempts.iter().any(|attempt| {
            attempt.activity_key == target_key
                && matches!(
                    attempt.status,
                    ActivityAttemptStatus::Ready
                        | ActivityAttemptStatus::Claiming
                        | ActivityAttemptStatus::Running
                )
        }) {
            continue;
        }
        let incoming = definition
            .transitions
            .iter()
            .filter(|transition| transition.to == target_key)
            .collect::<Vec<_>>();
        if incoming
            .iter()
            .all(|transition| transition_condition_matches(definition, state, transition))
        {
            create_ready_attempt(definition, state, &target_key, &incoming)?;
        }
    }
    Ok(())
}

fn create_ready_attempt(
    definition: &WorkflowGraph,
    state: &mut ActivityLifecycleRunState,
    activity_key: &str,
    incoming: &[&ActivityTransition],
) -> Result<(), LifecycleEngineError> {
    let activity = definition
        .activities
        .iter()
        .find(|activity| activity.key == activity_key)
        .ok_or_else(|| LifecycleEngineError::ActivityNotFound(activity_key.to_string()))?;
    let existing_pending_index = state.attempts.iter().position(|attempt| {
        attempt.activity_key == activity_key && attempt.status == ActivityAttemptStatus::Pending
    });
    let next_attempt = existing_pending_index
        .map(|index| state.attempts[index].attempt)
        .unwrap_or_else(|| {
            state
                .attempts
                .iter()
                .filter(|attempt| attempt.activity_key == activity_key)
                .map(|attempt| attempt.attempt)
                .max()
                .unwrap_or(0)
                + 1
        });
    if existing_pending_index.is_none() {
        if let Some(max_attempts) = activity.iteration_policy.max_attempts
            && next_attempt > max_attempts
        {
            return Err(LifecycleEngineError::AttemptLimitReached {
                activity_key: activity_key.to_string(),
                max_attempts,
            });
        }
    }

    let now = Utc::now();
    for transition in incoming {
        bind_transition_artifacts(state, activity_key, next_attempt, transition, now)?;
    }

    if let Some(index) = existing_pending_index {
        state.attempts[index].status = ActivityAttemptStatus::Ready;
    } else {
        state.attempts.push(ActivityAttemptState {
            activity_key: activity_key.to_string(),
            attempt: next_attempt,
            status: ActivityAttemptStatus::Ready,
            executor_run: None,
            started_at: None,
            completed_at: None,
            summary: None,
        });
    }
    Ok(())
}

fn bind_transition_artifacts(
    state: &mut ActivityLifecycleRunState,
    target_activity_key: &str,
    target_attempt: u32,
    transition: &ActivityTransition,
    now: DateTime<Utc>,
) -> Result<(), LifecycleEngineError> {
    for binding in &transition.artifact_bindings {
        let source_activity_key = binding
            .from_activity
            .as_deref()
            .unwrap_or(transition.from.as_str());
        let source =
            latest_output(state, source_activity_key, &binding.from_port).ok_or_else(|| {
                LifecycleEngineError::ArtifactMissing(format!(
                    "artifact 不存在: {}.{}",
                    source_activity_key, binding.from_port
                ))
            })?;
        state.inputs.push(ActivityInputArtifact {
            activity_key: target_activity_key.to_string(),
            attempt: target_attempt,
            port_key: binding.to_port.clone(),
            source_activity_key: source.activity_key.clone(),
            source_attempt: source.attempt,
            source_port_key: source.port_key.clone(),
            value: source.value.clone(),
            created_at: now,
        });
    }
    Ok(())
}

fn transition_condition_matches(
    definition: &WorkflowGraph,
    state: &ActivityLifecycleRunState,
    transition: &ActivityTransition,
) -> bool {
    if latest_completed_attempt(state, &transition.from).is_none() {
        return false;
    }
    let condition_matches = match &transition.condition {
        TransitionCondition::Always => true,
        TransitionCondition::HumanDecisionEquals {
            activity,
            decision_port,
            value,
        } => {
            latest_output(state, activity, decision_port)
                .and_then(|artifact| artifact.value.as_str().map(str::to_string))
                .as_deref()
                == Some(value.as_str())
        }
        TransitionCondition::ArtifactFieldEquals {
            activity,
            port,
            path,
            value,
        } => {
            latest_output(state, activity, port)
                .and_then(|artifact| select_json_path(&artifact.value, path))
                == Some(value)
        }
        TransitionCondition::AgentSignalEquals {
            activity,
            signal_key,
            value,
        } => {
            latest_output(state, activity, signal_key).map(|artifact| &artifact.value)
                == Some(value)
        }
    };
    condition_matches
        && definition
            .activities
            .iter()
            .any(|activity| activity.key == transition.to)
}

fn latest_completed_attempt<'a>(
    state: &'a ActivityLifecycleRunState,
    activity_key: &str,
) -> Option<&'a ActivityAttemptState> {
    state
        .attempts
        .iter()
        .filter(|attempt| {
            attempt.activity_key == activity_key
                && attempt.status == ActivityAttemptStatus::Completed
        })
        .max_by_key(|attempt| attempt.attempt)
}

fn latest_output<'a>(
    state: &'a ActivityLifecycleRunState,
    activity_key: &str,
    port_key: &str,
) -> Option<&'a ActivityOutputArtifact> {
    state
        .outputs
        .iter()
        .filter(|artifact| artifact.activity_key == activity_key && artifact.port_key == port_key)
        .max_by_key(|artifact| artifact.attempt)
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

fn find_attempt_mut<'a>(
    state: &'a mut ActivityLifecycleRunState,
    activity_key: &str,
    attempt: u32,
) -> Result<&'a mut ActivityAttemptState, LifecycleEngineError> {
    state
        .attempts
        .iter_mut()
        .find(|item| item.activity_key == activity_key && item.attempt == attempt)
        .ok_or_else(|| LifecycleEngineError::AttemptNotFound {
            activity_key: activity_key.to_string(),
            attempt,
        })
}

fn expect_status(
    attempt_state: &ActivityAttemptState,
    expected: ActivityAttemptStatus,
    expected_label: &'static str,
) -> Result<(), LifecycleEngineError> {
    if attempt_state.status == expected {
        Ok(())
    } else {
        Err(LifecycleEngineError::InvalidAttemptStatus {
            activity_key: attempt_state.activity_key.clone(),
            attempt: attempt_state.attempt,
            expected: expected_label,
            actual: attempt_state.status,
        })
    }
}

fn expect_cancellable_status(
    attempt_state: &ActivityAttemptState,
) -> Result<(), LifecycleEngineError> {
    if matches!(
        attempt_state.status,
        ActivityAttemptStatus::Ready
            | ActivityAttemptStatus::Claiming
            | ActivityAttemptStatus::Running
    ) {
        Ok(())
    } else {
        Err(LifecycleEngineError::InvalidAttemptStatus {
            activity_key: attempt_state.activity_key.clone(),
            attempt: attempt_state.attempt,
            expected: "ready/claiming/running attempt",
            actual: attempt_state.status,
        })
    }
}

fn derive_run_status(definition: &WorkflowGraph, state: &mut ActivityLifecycleRunState) {
    if state.attempts.iter().any(|attempt| {
        matches!(
            attempt.status,
            ActivityAttemptStatus::Claiming | ActivityAttemptStatus::Running
        )
    }) {
        state.status = ActivityRunStatus::Running;
        return;
    }
    if state
        .attempts
        .iter()
        .any(|attempt| attempt.status == ActivityAttemptStatus::Ready)
    {
        state.status = ActivityRunStatus::Ready;
        return;
    }
    if state
        .attempts
        .iter()
        .any(|attempt| attempt.status == ActivityAttemptStatus::Failed)
    {
        state.status = ActivityRunStatus::Failed;
        return;
    }
    if state
        .attempts
        .iter()
        .any(|attempt| attempt.status == ActivityAttemptStatus::Cancelled)
    {
        state.status = ActivityRunStatus::Cancelled;
        return;
    }
    if state
        .attempts
        .iter()
        .any(|attempt| attempt.status == ActivityAttemptStatus::Pending)
    {
        state.status = ActivityRunStatus::Blocked;
        return;
    }
    let terminal_activities = definition
        .activities
        .iter()
        .filter(|activity| {
            !definition
                .transitions
                .iter()
                .any(|transition| transition.from == activity.key)
        })
        .map(|activity| activity.key.as_str())
        .collect::<Vec<_>>();
    if terminal_activities
        .iter()
        .any(|activity_key| latest_completed_attempt(state, activity_key).is_some())
    {
        state.status = ActivityRunStatus::Completed;
    } else {
        state.status = ActivityRunStatus::Blocked;
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use uuid::Uuid;

    use agentdash_domain::workflow::{
        ActivityCompletionPolicy, ActivityDefinition, ActivityExecutorSpec,
        ActivityIterationPolicy, ActivityTransitionKind, AgentActivityExecutorSpec,
        ArtifactAliasPolicy, ArtifactBinding, DefinitionSource, HumanActivityExecutorSpec,
        HumanApprovalExecutorSpec, OutputPortDefinition,
    };

    use super::*;

    fn test_graph_instance_id() -> uuid::Uuid {
        uuid::Uuid::new_v4()
    }

    fn output_port(key: &str) -> OutputPortDefinition {
        OutputPortDefinition {
            key: key.to_string(),
            description: format!("{key} output"),
            gate_strategy: Default::default(),
            gate_params: None,
        }
    }

    fn approval_definition() -> WorkflowGraph {
        WorkflowGraph::new(
            Uuid::new_v4(),
            "approval_flow",
            "Approval flow",
            "",
            DefinitionSource::UserAuthored,
            "plan",
            vec![
                ActivityDefinition {
                    key: "plan".to_string(),
                    description: "plan".to_string(),
                    executor: ActivityExecutorSpec::Agent(
                        AgentActivityExecutorSpec::create_activity_agent("wf_plan"),
                    ),
                    input_ports: vec![agentdash_domain::workflow::InputPortDefinition {
                        key: "feedback".to_string(),
                        description: "feedback".to_string(),
                        context_strategy: Default::default(),
                        context_template: None,
                        standalone_fulfillment: Default::default(),
                    }],
                    output_ports: vec![output_port("proposal")],
                    completion_policy: ActivityCompletionPolicy::OutputPorts {
                        required_ports: vec!["proposal".to_string()],
                    },
                    iteration_policy: ActivityIterationPolicy {
                        max_attempts: Some(3),
                        artifact_alias: ArtifactAliasPolicy::LatestAndHistory,
                    },
                    join_policy: Default::default(),
                },
                ActivityDefinition {
                    key: "approval".to_string(),
                    description: "approval".to_string(),
                    executor: ActivityExecutorSpec::Human(HumanActivityExecutorSpec::Approval(
                        HumanApprovalExecutorSpec {
                            form_schema_key: "approval.plan_review".to_string(),
                            title: None,
                        },
                    )),
                    input_ports: vec![agentdash_domain::workflow::InputPortDefinition {
                        key: "proposal".to_string(),
                        description: "proposal".to_string(),
                        context_strategy: Default::default(),
                        context_template: None,
                        standalone_fulfillment: Default::default(),
                    }],
                    output_ports: vec![output_port("decision")],
                    completion_policy: ActivityCompletionPolicy::HumanDecision {
                        decision_port: "decision".to_string(),
                    },
                    iteration_policy: ActivityIterationPolicy {
                        max_attempts: Some(3),
                        artifact_alias: ArtifactAliasPolicy::LatestAndHistory,
                    },
                    join_policy: Default::default(),
                },
                ActivityDefinition {
                    key: "implement".to_string(),
                    description: "implement".to_string(),
                    executor: ActivityExecutorSpec::Agent(
                        AgentActivityExecutorSpec::create_activity_agent("wf_implement"),
                    ),
                    input_ports: vec![agentdash_domain::workflow::InputPortDefinition {
                        key: "approved_plan".to_string(),
                        description: "approved plan".to_string(),
                        context_strategy: Default::default(),
                        context_template: None,
                        standalone_fulfillment: Default::default(),
                    }],
                    output_ports: vec![output_port("summary")],
                    completion_policy: ActivityCompletionPolicy::ExecutorTerminal,
                    iteration_policy: Default::default(),
                    join_policy: Default::default(),
                },
            ],
            vec![
                ActivityTransition {
                    from: "plan".to_string(),
                    to: "approval".to_string(),
                    kind: ActivityTransitionKind::Flow,
                    condition: TransitionCondition::Always,
                    artifact_bindings: vec![ArtifactBinding {
                        from_activity: None,
                        from_port: "proposal".to_string(),
                        to_port: "proposal".to_string(),
                        alias: ArtifactAliasPolicy::Latest,
                    }],
                    max_traversals: None,
                },
                ActivityTransition {
                    from: "approval".to_string(),
                    to: "implement".to_string(),
                    kind: ActivityTransitionKind::Flow,
                    condition: TransitionCondition::HumanDecisionEquals {
                        activity: "approval".to_string(),
                        decision_port: "decision".to_string(),
                        value: "approved".to_string(),
                    },
                    artifact_bindings: vec![ArtifactBinding {
                        from_activity: Some("plan".to_string()),
                        from_port: "proposal".to_string(),
                        to_port: "approved_plan".to_string(),
                        alias: ArtifactAliasPolicy::Latest,
                    }],
                    max_traversals: None,
                },
                ActivityTransition {
                    from: "approval".to_string(),
                    to: "plan".to_string(),
                    kind: ActivityTransitionKind::Flow,
                    condition: TransitionCondition::HumanDecisionEquals {
                        activity: "approval".to_string(),
                        decision_port: "decision".to_string(),
                        value: "rejected".to_string(),
                    },
                    artifact_bindings: vec![ArtifactBinding {
                        from_activity: None,
                        from_port: "decision".to_string(),
                        to_port: "feedback".to_string(),
                        alias: ArtifactAliasPolicy::Latest,
                    }],
                    max_traversals: None,
                },
            ],
        )
        .expect("definition")
    }

    fn artifact_condition_definition() -> WorkflowGraph {
        WorkflowGraph::new(
            Uuid::new_v4(),
            "artifact_condition",
            "Artifact condition",
            "",
            DefinitionSource::UserAuthored,
            "plan",
            vec![
                ActivityDefinition {
                    key: "plan".to_string(),
                    description: "plan".to_string(),
                    executor: ActivityExecutorSpec::Agent(
                        AgentActivityExecutorSpec::create_activity_agent("wf_plan"),
                    ),
                    input_ports: vec![],
                    output_ports: vec![output_port("proposal")],
                    completion_policy: ActivityCompletionPolicy::OutputPorts {
                        required_ports: vec!["proposal".to_string()],
                    },
                    iteration_policy: Default::default(),
                    join_policy: Default::default(),
                },
                ActivityDefinition {
                    key: "implement".to_string(),
                    description: "implement".to_string(),
                    executor: ActivityExecutorSpec::Agent(
                        AgentActivityExecutorSpec::create_activity_agent("wf_implement"),
                    ),
                    input_ports: vec![agentdash_domain::workflow::InputPortDefinition {
                        key: "approved_plan".to_string(),
                        description: "approved plan".to_string(),
                        context_strategy: Default::default(),
                        context_template: None,
                        standalone_fulfillment: Default::default(),
                    }],
                    output_ports: vec![output_port("summary")],
                    completion_policy: ActivityCompletionPolicy::ExecutorTerminal,
                    iteration_policy: Default::default(),
                    join_policy: Default::default(),
                },
            ],
            vec![ActivityTransition {
                from: "plan".to_string(),
                to: "implement".to_string(),
                kind: ActivityTransitionKind::Flow,
                condition: TransitionCondition::ArtifactFieldEquals {
                    activity: "plan".to_string(),
                    port: "proposal".to_string(),
                    path: "status".to_string(),
                    value: json!("approved"),
                },
                artifact_bindings: vec![ArtifactBinding {
                    from_activity: None,
                    from_port: "proposal".to_string(),
                    to_port: "approved_plan".to_string(),
                    alias: ArtifactAliasPolicy::Latest,
                }],
                max_traversals: None,
            }],
        )
        .expect("definition")
    }

    fn start_attempt(
        definition: &WorkflowGraph,
        state: &mut ActivityLifecycleRunState,
        activity_key: &str,
        attempt: u32,
    ) {
        LifecycleEngine::apply_event(
            definition,
            state,
            ActivityEvent::SchedulerClaimAccepted {
                activity_key: activity_key.to_string(),
                attempt,
            },
        )
        .expect("claim");
        LifecycleEngine::apply_event(
            definition,
            state,
            ActivityEvent::ExecutorStarted {
                activity_key: activity_key.to_string(),
                attempt,
                executor_run: ExecutorRunRef::RuntimeSession {
                    session_id: format!("{activity_key}-{attempt}"),
                },
            },
        )
        .expect("start");
    }

    #[test]
    fn cancelling_running_attempt_marks_graph_cancelled() {
        let definition = approval_definition();
        let mut state =
            LifecycleEngine::initialize(&definition, test_graph_instance_id()).expect("init");

        start_attempt(&definition, &mut state, "plan", 1);
        LifecycleEngine::apply_event(
            &definition,
            &mut state,
            ActivityEvent::ActivityCancelled {
                activity_key: "plan".to_string(),
                attempt: 1,
                reason: Some("user requested cancel".to_string()),
            },
        )
        .expect("cancel");

        let attempt = state
            .attempts
            .iter()
            .find(|attempt| attempt.activity_key == "plan" && attempt.attempt == 1)
            .expect("plan attempt");
        assert_eq!(attempt.status, ActivityAttemptStatus::Cancelled);
        assert_eq!(attempt.summary.as_deref(), Some("user requested cancel"));
        assert_eq!(state.status, ActivityRunStatus::Cancelled);
    }

    #[test]
    fn cancelling_terminal_attempt_is_rejected() {
        let definition = approval_definition();
        let mut state =
            LifecycleEngine::initialize(&definition, test_graph_instance_id()).expect("init");

        start_attempt(&definition, &mut state, "plan", 1);
        LifecycleEngine::apply_event(
            &definition,
            &mut state,
            ActivityEvent::ActivityFailed {
                activity_key: "plan".to_string(),
                attempt: 1,
                error: "failed".to_string(),
            },
        )
        .expect("fail");

        let error = LifecycleEngine::apply_event(
            &definition,
            &mut state,
            ActivityEvent::ActivityCancelled {
                activity_key: "plan".to_string(),
                attempt: 1,
                reason: None,
            },
        )
        .expect_err("terminal attempt cannot be cancelled");
        assert!(matches!(
            error,
            LifecycleEngineError::InvalidAttemptStatus {
                actual: ActivityAttemptStatus::Failed,
                ..
            }
        ));
    }

    #[test]
    fn approval_rejection_creates_next_plan_attempt_with_feedback() {
        let definition = approval_definition();
        let mut state =
            LifecycleEngine::initialize(&definition, test_graph_instance_id()).expect("init");

        start_attempt(&definition, &mut state, "plan", 1);
        LifecycleEngine::apply_event(
            &definition,
            &mut state,
            ActivityEvent::ActivityCompleted {
                activity_key: "plan".to_string(),
                attempt: 1,
                outputs: vec![ActivityPortValue {
                    port_key: "proposal".to_string(),
                    value: json!({"title": "v1"}),
                }],
                summary: Some("planned".to_string()),
            },
        )
        .expect("complete plan");
        assert!(state.attempts.iter().any(|attempt| {
            attempt.activity_key == "approval"
                && attempt.attempt == 1
                && attempt.status == ActivityAttemptStatus::Ready
        }));

        start_attempt(&definition, &mut state, "approval", 1);
        LifecycleEngine::apply_event(
            &definition,
            &mut state,
            ActivityEvent::HumanDecisionSubmitted {
                activity_key: "approval".to_string(),
                attempt: 1,
                decision_port: "decision".to_string(),
                decision: json!("rejected"),
                summary: Some("needs changes".to_string()),
            },
        )
        .expect("reject");

        assert!(state.attempts.iter().any(|attempt| {
            attempt.activity_key == "plan"
                && attempt.attempt == 2
                && attempt.status == ActivityAttemptStatus::Ready
        }));
        assert!(state.attempts.iter().any(|attempt| {
            attempt.activity_key == "plan"
                && attempt.attempt == 1
                && attempt.status == ActivityAttemptStatus::Completed
        }));
        assert!(state.inputs.iter().any(|artifact| {
            artifact.activity_key == "plan"
                && artifact.attempt == 2
                && artifact.port_key == "feedback"
                && artifact.value == json!("rejected")
        }));
    }

    #[test]
    fn approval_approved_activates_implement_with_latest_plan() {
        let definition = approval_definition();
        let mut state =
            LifecycleEngine::initialize(&definition, test_graph_instance_id()).expect("init");

        start_attempt(&definition, &mut state, "plan", 1);
        LifecycleEngine::apply_event(
            &definition,
            &mut state,
            ActivityEvent::ActivityCompleted {
                activity_key: "plan".to_string(),
                attempt: 1,
                outputs: vec![ActivityPortValue {
                    port_key: "proposal".to_string(),
                    value: json!({"title": "v1"}),
                }],
                summary: None,
            },
        )
        .expect("complete plan");
        start_attempt(&definition, &mut state, "approval", 1);
        LifecycleEngine::apply_event(
            &definition,
            &mut state,
            ActivityEvent::HumanDecisionSubmitted {
                activity_key: "approval".to_string(),
                attempt: 1,
                decision_port: "decision".to_string(),
                decision: json!("approved"),
                summary: None,
            },
        )
        .expect("approve");

        assert!(state.attempts.iter().any(|attempt| {
            attempt.activity_key == "implement"
                && attempt.attempt == 1
                && attempt.status == ActivityAttemptStatus::Ready
        }));
        assert!(state.inputs.iter().any(|artifact| {
            artifact.activity_key == "implement"
                && artifact.attempt == 1
                && artifact.port_key == "approved_plan"
                && artifact.value == json!({"title": "v1"})
        }));
    }

    #[test]
    fn completion_policy_rejects_missing_output_port() {
        let definition = approval_definition();
        let mut state =
            LifecycleEngine::initialize(&definition, test_graph_instance_id()).expect("init");
        start_attempt(&definition, &mut state, "plan", 1);

        let error = LifecycleEngine::apply_event(
            &definition,
            &mut state,
            ActivityEvent::ActivityCompleted {
                activity_key: "plan".to_string(),
                attempt: 1,
                outputs: vec![],
                summary: None,
            },
        )
        .expect_err("missing output should fail");

        assert!(matches!(
            error,
            LifecycleEngineError::CompletionPolicyRejected(_)
        ));
        assert!(state.attempts.iter().any(|attempt| {
            attempt.activity_key == "plan"
                && attempt.attempt == 1
                && attempt.status == ActivityAttemptStatus::Running
        }));
    }

    #[test]
    fn scheduler_start_failure_can_retry_claiming_attempt() {
        let definition = approval_definition();
        let mut state =
            LifecycleEngine::initialize(&definition, test_graph_instance_id()).expect("init");
        LifecycleEngine::apply_event(
            &definition,
            &mut state,
            ActivityEvent::SchedulerClaimAccepted {
                activity_key: "plan".to_string(),
                attempt: 1,
            },
        )
        .expect("claim");

        LifecycleEngine::apply_event(
            &definition,
            &mut state,
            ActivityEvent::SchedulerStartFailed {
                activity_key: "plan".to_string(),
                attempt: 1,
                error: "prompt rejected".to_string(),
                retryable: true,
            },
        )
        .expect("start failed");

        assert!(state.attempts.iter().any(|attempt| {
            attempt.activity_key == "plan"
                && attempt.attempt == 1
                && attempt.status == ActivityAttemptStatus::Ready
        }));
    }

    #[test]
    fn failed_attempt_does_not_activate_successor() {
        let definition = approval_definition();
        let mut state =
            LifecycleEngine::initialize(&definition, test_graph_instance_id()).expect("init");
        start_attempt(&definition, &mut state, "plan", 1);

        LifecycleEngine::apply_event(
            &definition,
            &mut state,
            ActivityEvent::ActivityFailed {
                activity_key: "plan".to_string(),
                attempt: 1,
                error: "executor failed".to_string(),
            },
        )
        .expect("fail plan");

        assert_eq!(state.status, ActivityRunStatus::Failed);
        assert!(!state.attempts.iter().any(|attempt| {
            attempt.activity_key == "approval" && attempt.status == ActivityAttemptStatus::Ready
        }));
    }

    #[test]
    fn all_join_waits_for_every_incoming_transition() {
        let mut definition = approval_definition();
        definition.activities.push(ActivityDefinition {
            key: "security_review".to_string(),
            description: "security review".to_string(),
            executor: ActivityExecutorSpec::Human(HumanActivityExecutorSpec::Approval(
                HumanApprovalExecutorSpec {
                    form_schema_key: "approval.security".to_string(),
                    title: None,
                },
            )),
            input_ports: vec![agentdash_domain::workflow::InputPortDefinition {
                key: "proposal".to_string(),
                description: "proposal".to_string(),
                context_strategy: Default::default(),
                context_template: None,
                standalone_fulfillment: Default::default(),
            }],
            output_ports: vec![output_port("decision")],
            completion_policy: ActivityCompletionPolicy::HumanDecision {
                decision_port: "decision".to_string(),
            },
            iteration_policy: ActivityIterationPolicy {
                max_attempts: Some(3),
                artifact_alias: ArtifactAliasPolicy::LatestAndHistory,
            },
            join_policy: Default::default(),
        });
        definition.transitions.push(ActivityTransition {
            from: "plan".to_string(),
            to: "security_review".to_string(),
            kind: ActivityTransitionKind::Flow,
            condition: TransitionCondition::Always,
            artifact_bindings: vec![ArtifactBinding {
                from_activity: None,
                from_port: "proposal".to_string(),
                to_port: "proposal".to_string(),
                alias: ArtifactAliasPolicy::Latest,
            }],
            max_traversals: None,
        });
        definition.transitions.push(ActivityTransition {
            from: "security_review".to_string(),
            to: "implement".to_string(),
            kind: ActivityTransitionKind::Flow,
            condition: TransitionCondition::HumanDecisionEquals {
                activity: "security_review".to_string(),
                decision_port: "decision".to_string(),
                value: "approved".to_string(),
            },
            artifact_bindings: vec![],
            max_traversals: None,
        });

        let mut state =
            LifecycleEngine::initialize(&definition, test_graph_instance_id()).expect("init");
        start_attempt(&definition, &mut state, "plan", 1);
        LifecycleEngine::apply_event(
            &definition,
            &mut state,
            ActivityEvent::ActivityCompleted {
                activity_key: "plan".to_string(),
                attempt: 1,
                outputs: vec![ActivityPortValue {
                    port_key: "proposal".to_string(),
                    value: json!({"title": "v1"}),
                }],
                summary: None,
            },
        )
        .expect("complete plan");
        start_attempt(&definition, &mut state, "approval", 1);
        LifecycleEngine::apply_event(
            &definition,
            &mut state,
            ActivityEvent::HumanDecisionSubmitted {
                activity_key: "approval".to_string(),
                attempt: 1,
                decision_port: "decision".to_string(),
                decision: json!("approved"),
                summary: None,
            },
        )
        .expect("approve product review");

        assert!(!state.attempts.iter().any(|attempt| {
            attempt.activity_key == "implement" && attempt.status == ActivityAttemptStatus::Ready
        }));

        start_attempt(&definition, &mut state, "security_review", 1);
        LifecycleEngine::apply_event(
            &definition,
            &mut state,
            ActivityEvent::HumanDecisionSubmitted {
                activity_key: "security_review".to_string(),
                attempt: 1,
                decision_port: "decision".to_string(),
                decision: json!("approved"),
                summary: None,
            },
        )
        .expect("approve security review");

        assert!(state.attempts.iter().any(|attempt| {
            attempt.activity_key == "implement"
                && attempt.attempt == 1
                && attempt.status == ActivityAttemptStatus::Ready
        }));
    }

    #[test]
    fn artifact_field_condition_activates_successor_on_matching_path() {
        let definition = artifact_condition_definition();
        let mut state =
            LifecycleEngine::initialize(&definition, test_graph_instance_id()).expect("init");
        start_attempt(&definition, &mut state, "plan", 1);

        LifecycleEngine::apply_event(
            &definition,
            &mut state,
            ActivityEvent::ActivityCompleted {
                activity_key: "plan".to_string(),
                attempt: 1,
                outputs: vec![ActivityPortValue {
                    port_key: "proposal".to_string(),
                    value: json!({"status": "approved", "title": "v1"}),
                }],
                summary: None,
            },
        )
        .expect("complete plan");

        assert!(state.attempts.iter().any(|attempt| {
            attempt.activity_key == "implement"
                && attempt.attempt == 1
                && attempt.status == ActivityAttemptStatus::Ready
        }));
        assert!(state.inputs.iter().any(|artifact| {
            artifact.activity_key == "implement"
                && artifact.attempt == 1
                && artifact.port_key == "approved_plan"
                && artifact.value == json!({"status": "approved", "title": "v1"})
        }));
    }
}
