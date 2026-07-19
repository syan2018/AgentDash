use agentdash_application_lifecycle::run_view_builder as app;
use agentdash_contracts::workflow as contract;
use agentdash_domain::workflow::{
    ExecutorRunRef as DomainExecutorRunRef,
    LifecycleExecutionEventKind as DomainLifecycleExecutionEventKind,
    LifecycleRunStatus as DomainLifecycleRunStatus,
    LifecycleRunTopology as DomainLifecycleRunTopology, OrchestrationInstance,
    PlanNodeKind as DomainPlanNodeKind, RuntimeNodeState, RuntimeNodeStatus,
    RuntimeTraceRef as DomainRuntimeTraceRef,
};

use crate::rpc::ApiError;

pub(crate) fn lifecycle_run_view_query_error(error: app::LifecycleRunViewQueryError) -> ApiError {
    match error {
        app::LifecycleRunViewQueryError::Domain(error) => ApiError::from(error),
        app::LifecycleRunViewQueryError::RunNotFound { run_id } => {
            ApiError::NotFound(format!("lifecycle_run 不存在: {run_id}"))
        }
        app::LifecycleRunViewQueryError::ProductBinding { message, .. } => {
            ApiError::Internal(format!("读取 AgentRun Product binding 失败: {message}"))
        }
        app::LifecycleRunViewQueryError::RuntimeProjection { message, .. } => {
            ApiError::ServiceUnavailable(format!(
                "读取 AgentRun Product projection 失败: {message}"
            ))
        }
    }
}

pub(crate) fn subject_execution_view_to_contract(
    view: app::SubjectExecutionView,
) -> contract::SubjectExecutionView {
    contract::SubjectExecutionView {
        subject_ref: contract::SubjectRefDto {
            kind: view.subject_ref.kind,
            id: view.subject_ref.id.to_string(),
        },
        associations: view
            .associations
            .into_iter()
            .map(subject_association_to_contract)
            .collect(),
        runs: view
            .runs
            .into_iter()
            .map(lifecycle_run_view_to_contract)
            .collect(),
        current_agent: view.current_agent.map(agent_run_to_contract),
        attempts: view
            .attempts
            .into_iter()
            .map(subject_execution_attempt_to_contract)
            .collect(),
        current_attempt: view
            .current_attempt
            .map(subject_execution_attempt_to_contract),
        artifacts: view.artifacts,
    }
}

pub(crate) fn project_active_agents_view_to_contract(
    view: app::ProjectActiveAgentsView,
) -> contract::ProjectActiveAgentsView {
    contract::ProjectActiveAgentsView {
        project_id: view.project_id.to_string(),
        runs: view
            .runs
            .into_iter()
            .map(lifecycle_run_view_to_contract)
            .collect(),
        agents: view.agents.into_iter().map(agent_run_to_contract).collect(),
    }
}

pub(crate) fn lifecycle_run_view_to_contract(
    view: app::LifecycleRunView,
) -> contract::LifecycleRunView {
    let agents = view.agents.into_iter().map(agent_run_to_contract).collect();
    let run = view.run;

    contract::LifecycleRunView {
        run_ref: contract::LifecycleRunRefDto {
            run_id: run.id.to_string(),
        },
        project_id: run.project_id.to_string(),
        topology: lifecycle_topology_to_contract(run.topology),
        status: lifecycle_status_to_contract(run.status),
        orchestrations: run
            .orchestrations
            .iter()
            .map(orchestration_to_contract)
            .collect(),
        active_runtime_node_refs: active_runtime_node_refs_to_contract(&run),
        agents,
        subject_associations: view
            .subject_associations
            .into_iter()
            .map(subject_association_to_contract)
            .collect(),
        execution_log: run
            .execution_log
            .iter()
            .map(execution_entry_to_contract)
            .collect(),
        created_at: run.created_at.to_rfc3339(),
        updated_at: run.updated_at.to_rfc3339(),
        last_activity_at: run.last_activity_at.to_rfc3339(),
    }
}

fn lifecycle_status_to_contract(status: DomainLifecycleRunStatus) -> contract::LifecycleRunStatus {
    match status {
        DomainLifecycleRunStatus::Draft => contract::LifecycleRunStatus::Draft,
        DomainLifecycleRunStatus::Ready => contract::LifecycleRunStatus::Ready,
        DomainLifecycleRunStatus::Running => contract::LifecycleRunStatus::Running,
        DomainLifecycleRunStatus::Blocked => contract::LifecycleRunStatus::Blocked,
        DomainLifecycleRunStatus::Completed => contract::LifecycleRunStatus::Completed,
        DomainLifecycleRunStatus::Failed => contract::LifecycleRunStatus::Failed,
        DomainLifecycleRunStatus::Cancelled => contract::LifecycleRunStatus::Cancelled,
    }
}

fn lifecycle_topology_to_contract(
    topology: DomainLifecycleRunTopology,
) -> contract::LifecycleRunTopology {
    match topology {
        DomainLifecycleRunTopology::Plain => contract::LifecycleRunTopology::Plain,
        DomainLifecycleRunTopology::WorkflowGraph => contract::LifecycleRunTopology::WorkflowGraph,
    }
}

fn subject_association_to_contract(
    association: agentdash_domain::workflow::LifecycleSubjectAssociation,
) -> contract::LifecycleSubjectAssociationDto {
    contract::LifecycleSubjectAssociationDto {
        id: association.id.to_string(),
        anchor_run_id: association.anchor_run_id.to_string(),
        anchor_agent_id: association.anchor_agent_id.map(|id| id.to_string()),
        subject_ref: contract::SubjectRefDto {
            kind: association.subject_kind,
            id: association.subject_id.to_string(),
        },
        role: association.role,
        metadata: association.metadata_json,
        created_at: association.created_at.to_rfc3339(),
    }
}

fn agent_run_to_contract(
    view: app::LifecycleAgentExecutionView,
) -> contract::LifecycleAgentExecutionView {
    let last_delivery_status = view
        .current_attempt
        .as_ref()
        .map(|attempt| status_string(&attempt.status));
    let agent = view.agent;

    contract::LifecycleAgentExecutionView {
        agent: contract::AgentRunView {
            agent_ref: contract::AgentRunRefDto {
                run_id: agent.run_id.to_string(),
                agent_id: agent.id.to_string(),
            },
            project_id: agent.project_id.to_string(),
            source: agent.source.as_str().to_string(),
            project_agent_id: agent.project_agent_id.map(|id| id.to_string()),
            status: agent.status,
            last_delivery_status,
            created_at: agent.created_at.to_rfc3339(),
            updated_at: agent.updated_at.to_rfc3339(),
        },
        runtime: runtime_execution_trace_to_contract(view.runtime),
        current_attempt: view.current_attempt.map(execution_attempt_to_contract),
        attempts: view
            .attempts
            .into_iter()
            .map(execution_attempt_to_contract)
            .collect(),
    }
}

fn runtime_execution_trace_to_contract(
    runtime: app::RuntimeExecutionTraceView,
) -> contract::LifecycleRuntimeExecutionTraceView {
    match runtime {
        app::RuntimeExecutionTraceView::Absent { target, reason } => {
            contract::LifecycleRuntimeExecutionTraceView::Absent {
                target: contract::AgentRunRefDto {
                    run_id: target.run_id.to_string(),
                    agent_id: target.agent_id.to_string(),
                },
                reason: match reason {
                    app::RuntimeTraceAbsenceReason::ProductBindingMissing => {
                        contract::LifecycleRuntimeTraceAbsenceReason::ProductBindingMissing
                    }
                },
            }
        }
        app::RuntimeExecutionTraceView::Current { binding, snapshot } => {
            contract::LifecycleRuntimeExecutionTraceView::Current {
                binding: runtime_binding_to_contract(binding),
                snapshot,
            }
        }
        app::RuntimeExecutionTraceView::Stale { reason, evidence } => {
            contract::LifecycleRuntimeExecutionTraceView::Stale {
                reason: match reason {
                    app::RuntimeTraceStaleReason::ProductBindingTargetMismatch => {
                        contract::LifecycleRuntimeTraceStaleReason::ProductBindingTargetMismatch
                    }
                    app::RuntimeTraceStaleReason::ProjectionBindingMissing => {
                        contract::LifecycleRuntimeTraceStaleReason::ProjectionBindingMissing
                    }
                    app::RuntimeTraceStaleReason::ProductBindingChanged => {
                        contract::LifecycleRuntimeTraceStaleReason::ProductBindingChanged
                    }
                    app::RuntimeTraceStaleReason::RuntimeThreadMismatch => {
                        contract::LifecycleRuntimeTraceStaleReason::RuntimeThreadMismatch
                    }
                    app::RuntimeTraceStaleReason::RuntimeSourceBindingMismatch => {
                        contract::LifecycleRuntimeTraceStaleReason::RuntimeSourceBindingMismatch
                    }
                },
                evidence: contract::LifecycleRuntimeTraceFenceEvidenceView {
                    expected_target: agent_run_target_to_contract(evidence.expected_target),
                    observed_target: evidence.observed_target.map(agent_run_target_to_contract),
                    expected_runtime_thread_id: evidence.expected_runtime_thread_id,
                    observed_runtime_thread_id: evidence.observed_runtime_thread_id,
                    expected_source_binding: evidence.expected_source_binding,
                    observed_source_binding: evidence.observed_source_binding,
                    observed_snapshot: evidence.observed_snapshot,
                },
            }
        }
    }
}

fn agent_run_target_to_contract(
    target: agentdash_domain::agent_run_target::AgentRunTarget,
) -> contract::AgentRunRefDto {
    contract::AgentRunRefDto {
        run_id: target.run_id.to_string(),
        agent_id: target.agent_id.to_string(),
    }
}

fn runtime_binding_to_contract(
    binding: agentdash_application_agentrun::agent_run::AgentRunProductRuntimeBinding,
) -> contract::LifecycleAgentRuntimeBindingView {
    contract::LifecycleAgentRuntimeBindingView {
        target: contract::AgentRunRefDto {
            run_id: binding.target.run_id.to_string(),
            agent_id: binding.target.agent_id.to_string(),
        },
        runtime_thread_id: binding.runtime_thread_id,
        source_binding: binding.source_binding,
    }
}

fn execution_attempt_to_contract(
    attempt: app::LifecycleExecutionAttemptView,
) -> contract::LifecycleExecutionAttemptView {
    contract::LifecycleExecutionAttemptView {
        orchestration_id: attempt.orchestration_id.to_string(),
        node_path: attempt.node_path,
        attempt: attempt.attempt,
        status: status_string(&attempt.status),
        observed_at: attempt.observed_at.to_rfc3339(),
        artifacts: attempt.artifacts,
        runtime_node: lifecycle_runtime_node_to_contract(attempt.runtime_node),
    }
}

fn subject_execution_attempt_to_contract(
    attempt: app::SubjectExecutionAttemptView,
) -> contract::SubjectExecutionAttemptView {
    contract::SubjectExecutionAttemptView {
        target: agent_run_target_to_contract(attempt.target),
        runtime: runtime_execution_trace_to_contract(attempt.runtime),
        attempt: execution_attempt_to_contract(attempt.attempt),
    }
}

fn lifecycle_runtime_node_to_contract(
    node: app::LifecycleRuntimeNodeView,
) -> contract::LifecycleRuntimeNodeView {
    contract::LifecycleRuntimeNodeView {
        node_id: node.node_id,
        node_path: node.node_path,
        kind: lifecycle_runtime_node_kind_to_contract(node.kind),
        status: lifecycle_runtime_node_status_to_contract(node.status),
        attempt: node.attempt,
        inputs: node
            .inputs
            .into_iter()
            .map(|port| contract::LifecycleNodePortValueView {
                port_key: port.port_key,
                value: port.value,
            })
            .collect(),
        outputs: node
            .outputs
            .into_iter()
            .map(|port| contract::LifecycleNodePortValueView {
                port_key: port.port_key,
                value: port.value,
            })
            .collect(),
        executor_run_ref: node
            .executor_run_ref
            .as_ref()
            .map(executor_run_ref_to_contract),
        agent_call_target: node.agent_call_target.map(agent_run_target_to_contract),
        started_at: node.started_at.map(|timestamp| timestamp.to_rfc3339()),
        completed_at: node.completed_at.map(|timestamp| timestamp.to_rfc3339()),
        error: node
            .error
            .map(|error| contract::LifecycleRuntimeNodeErrorView {
                code: error.code,
                message: error.message,
                retryable: error.retryable,
                detail: error.detail,
            }),
        trace_refs: node
            .trace_refs
            .into_iter()
            .map(lifecycle_runtime_trace_ref_to_contract)
            .collect(),
        artifacts: node.artifacts,
        children: node
            .children
            .into_iter()
            .map(lifecycle_runtime_node_to_contract)
            .collect(),
    }
}

fn lifecycle_runtime_node_kind_to_contract(
    kind: DomainPlanNodeKind,
) -> contract::LifecycleRuntimeNodeKind {
    match kind {
        DomainPlanNodeKind::Activity => contract::LifecycleRuntimeNodeKind::Activity,
        DomainPlanNodeKind::AgentCall => contract::LifecycleRuntimeNodeKind::AgentCall,
        DomainPlanNodeKind::Function => contract::LifecycleRuntimeNodeKind::Function,
        DomainPlanNodeKind::LocalEffect => contract::LifecycleRuntimeNodeKind::LocalEffect,
        DomainPlanNodeKind::ExtensionAction => contract::LifecycleRuntimeNodeKind::ExtensionAction,
        DomainPlanNodeKind::HumanGate => contract::LifecycleRuntimeNodeKind::HumanGate,
        DomainPlanNodeKind::Phase => contract::LifecycleRuntimeNodeKind::Phase,
        DomainPlanNodeKind::ParallelGroup => contract::LifecycleRuntimeNodeKind::ParallelGroup,
        DomainPlanNodeKind::Pipeline => contract::LifecycleRuntimeNodeKind::Pipeline,
        DomainPlanNodeKind::Barrier => contract::LifecycleRuntimeNodeKind::Barrier,
        DomainPlanNodeKind::Subworkflow => contract::LifecycleRuntimeNodeKind::Subworkflow,
    }
}

fn lifecycle_runtime_node_status_to_contract(
    status: RuntimeNodeStatus,
) -> contract::LifecycleRuntimeNodeStatus {
    match status {
        RuntimeNodeStatus::Pending => contract::LifecycleRuntimeNodeStatus::Pending,
        RuntimeNodeStatus::Ready => contract::LifecycleRuntimeNodeStatus::Ready,
        RuntimeNodeStatus::Claiming => contract::LifecycleRuntimeNodeStatus::Claiming,
        RuntimeNodeStatus::Running => contract::LifecycleRuntimeNodeStatus::Running,
        RuntimeNodeStatus::Blocked => contract::LifecycleRuntimeNodeStatus::Blocked,
        RuntimeNodeStatus::Completed => contract::LifecycleRuntimeNodeStatus::Completed,
        RuntimeNodeStatus::Failed => contract::LifecycleRuntimeNodeStatus::Failed,
        RuntimeNodeStatus::Cancelled => contract::LifecycleRuntimeNodeStatus::Cancelled,
        RuntimeNodeStatus::Skipped => contract::LifecycleRuntimeNodeStatus::Skipped,
    }
}

fn lifecycle_runtime_trace_ref_to_contract(
    refs: DomainRuntimeTraceRef,
) -> contract::LifecycleRuntimeTraceRefView {
    match refs {
        DomainRuntimeTraceRef::RuntimeThread { thread_id } => {
            contract::LifecycleRuntimeTraceRefView::RuntimeThread { thread_id }
        }
        DomainRuntimeTraceRef::AgentRun { run_id, agent_id } => {
            contract::LifecycleRuntimeTraceRefView::AgentRun {
                run_id: run_id.to_string(),
                agent_id: agent_id.to_string(),
            }
        }
        DomainRuntimeTraceRef::FunctionRun { run_id } => {
            contract::LifecycleRuntimeTraceRefView::FunctionRun { run_id }
        }
        DomainRuntimeTraceRef::HumanDecision { decision_id } => {
            contract::LifecycleRuntimeTraceRefView::HumanDecision { decision_id }
        }
        DomainRuntimeTraceRef::EffectInvocation {
            effect_id,
            effect_kind,
        } => contract::LifecycleRuntimeTraceRefView::EffectInvocation {
            effect_id,
            effect_kind,
        },
    }
}

fn orchestration_to_contract(
    orchestration: &OrchestrationInstance,
) -> contract::OrchestrationInstanceView {
    contract::OrchestrationInstanceView {
        orchestration_id: orchestration.orchestration_id.to_string(),
        role: orchestration.role.clone(),
        status: status_string(&orchestration.status),
        plan_digest: orchestration.plan_snapshot.plan_digest.clone(),
        source_ref: serde_json::to_value(&orchestration.source_ref)
            .unwrap_or(serde_json::Value::Null),
        ready_node_ids: orchestration.dispatch.ready_node_ids.clone(),
        nodes: orchestration
            .node_tree
            .iter()
            .map(runtime_node_to_contract)
            .collect(),
        created_at: orchestration.created_at.to_rfc3339(),
        updated_at: orchestration.updated_at.to_rfc3339(),
    }
}

fn runtime_node_to_contract(node: &RuntimeNodeState) -> contract::RuntimeNodeView {
    contract::RuntimeNodeView {
        node_id: node.node_id.clone(),
        node_path: node.node_path.clone(),
        kind: status_string(&node.kind),
        status: status_string(&node.status),
        attempt: node.attempt,
        executor_run_ref: node
            .executor_run_ref
            .as_ref()
            .map(executor_run_ref_to_contract),
        started_at: node.started_at.map(|timestamp| timestamp.to_rfc3339()),
        completed_at: node.completed_at.map(|timestamp| timestamp.to_rfc3339()),
        children: node.children.iter().map(runtime_node_to_contract).collect(),
    }
}

fn executor_run_ref_to_contract(refs: &DomainExecutorRunRef) -> contract::ExecutorRunRef {
    match refs {
        DomainExecutorRunRef::AgentRun { run_id, agent_id } => contract::ExecutorRunRef::AgentRun {
            run_id: run_id.to_string(),
            agent_id: agent_id.to_string(),
        },
        DomainExecutorRunRef::FunctionRun { run_id } => contract::ExecutorRunRef::FunctionRun {
            run_id: run_id.clone(),
        },
        DomainExecutorRunRef::HumanDecision { decision_id } => {
            contract::ExecutorRunRef::HumanDecision {
                decision_id: decision_id.clone(),
            }
        }
    }
}

fn active_runtime_node_refs_to_contract(
    run: &agentdash_domain::workflow::LifecycleRun,
) -> Vec<contract::ActiveRuntimeNodeRefDto> {
    run.orchestrations
        .iter()
        .flat_map(|orchestration| {
            flatten_runtime_nodes(&orchestration.node_tree)
                .into_iter()
                .filter(|node| {
                    matches!(
                        node.status,
                        RuntimeNodeStatus::Ready
                            | RuntimeNodeStatus::Claiming
                            | RuntimeNodeStatus::Running
                            | RuntimeNodeStatus::Blocked
                    )
                })
                .map(move |node| contract::ActiveRuntimeNodeRefDto {
                    run_id: run.id.to_string(),
                    orchestration_id: orchestration.orchestration_id.to_string(),
                    node_path: node.node_path.clone(),
                    attempt: node.attempt,
                    status: status_string(&node.status),
                })
        })
        .collect()
}

fn flatten_runtime_nodes(nodes: &[RuntimeNodeState]) -> Vec<&RuntimeNodeState> {
    fn collect<'a>(node: &'a RuntimeNodeState, flattened: &mut Vec<&'a RuntimeNodeState>) {
        flattened.push(node);
        for child in &node.children {
            collect(child, flattened);
        }
    }

    let mut flattened = Vec::new();
    for node in nodes {
        collect(node, &mut flattened);
    }
    flattened
}

fn execution_entry_to_contract(
    entry: &agentdash_domain::workflow::LifecycleExecutionEntry,
) -> contract::LifecycleExecutionEntry {
    contract::LifecycleExecutionEntry {
        timestamp: entry.timestamp,
        activity_key: entry.activity_key.clone(),
        event_kind: match entry.event_kind {
            DomainLifecycleExecutionEventKind::ActivityActivated => {
                contract::LifecycleExecutionEventKind::ActivityActivated
            }
            DomainLifecycleExecutionEventKind::ActivityCompleted => {
                contract::LifecycleExecutionEventKind::ActivityCompleted
            }
            DomainLifecycleExecutionEventKind::ConstraintBlocked => {
                contract::LifecycleExecutionEventKind::ConstraintBlocked
            }
            DomainLifecycleExecutionEventKind::CompletionEvaluated => {
                contract::LifecycleExecutionEventKind::CompletionEvaluated
            }
            DomainLifecycleExecutionEventKind::ArtifactAppended => {
                contract::LifecycleExecutionEventKind::ArtifactAppended
            }
            DomainLifecycleExecutionEventKind::ContextInjected => {
                contract::LifecycleExecutionEventKind::ContextInjected
            }
        },
        summary: entry.summary.clone(),
        detail: entry.detail.clone(),
    }
}

fn status_string<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| "unknown".to_string())
}
