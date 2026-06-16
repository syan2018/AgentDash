use agentdash_application::workflow::lifecycle_run_view_builder as app;
use agentdash_contracts::workflow as contract;

pub(crate) fn lifecycle_run_view_to_contract(
    view: app::LifecycleRunView,
) -> contract::LifecycleRunView {
    contract::LifecycleRunView {
        run_ref: contract::LifecycleRunRefDto {
            run_id: view.run_ref.run_id,
        },
        project_id: view.project_id,
        topology: lifecycle_topology_to_contract(view.topology),
        status: lifecycle_status_to_contract(view.status),
        orchestrations: view
            .orchestrations
            .into_iter()
            .map(orchestration_to_contract)
            .collect(),
        active_runtime_node_refs: view
            .active_runtime_node_refs
            .into_iter()
            .map(active_runtime_node_ref_to_contract)
            .collect(),
        agents: view.agents.into_iter().map(agent_run_to_contract).collect(),
        subject_associations: view
            .subject_associations
            .into_iter()
            .map(subject_association_to_contract)
            .collect(),
        runtime_trace_refs: view
            .runtime_trace_refs
            .into_iter()
            .map(runtime_session_ref_to_contract)
            .collect(),
        execution_log: view
            .execution_log
            .into_iter()
            .map(execution_entry_to_contract)
            .collect(),
        created_at: view.created_at,
        updated_at: view.updated_at,
        last_activity_at: view.last_activity_at,
    }
}

pub(crate) fn subject_execution_view_to_contract(
    view: app::SubjectExecutionView,
) -> contract::SubjectExecutionView {
    contract::SubjectExecutionView {
        subject_ref: subject_ref_to_contract(view.subject_ref),
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
        latest_runtime_node: view.latest_runtime_node.map(runtime_node_to_contract),
        artifacts: view.artifacts,
    }
}

pub(crate) fn project_active_agents_view_to_contract(
    view: app::ProjectActiveAgentsView,
) -> contract::ProjectActiveAgentsView {
    contract::ProjectActiveAgentsView {
        project_id: view.project_id,
        runs: view
            .runs
            .into_iter()
            .map(lifecycle_run_view_to_contract)
            .collect(),
        agents: view.agents.into_iter().map(agent_run_to_contract).collect(),
    }
}

pub(crate) fn lifecycle_status_to_contract(
    status: app::LifecycleRunStatusView,
) -> contract::LifecycleRunStatus {
    match status {
        app::LifecycleRunStatusView::Draft => contract::LifecycleRunStatus::Draft,
        app::LifecycleRunStatusView::Ready => contract::LifecycleRunStatus::Ready,
        app::LifecycleRunStatusView::Running => contract::LifecycleRunStatus::Running,
        app::LifecycleRunStatusView::Blocked => contract::LifecycleRunStatus::Blocked,
        app::LifecycleRunStatusView::Completed => contract::LifecycleRunStatus::Completed,
        app::LifecycleRunStatusView::Failed => contract::LifecycleRunStatus::Failed,
        app::LifecycleRunStatusView::Cancelled => contract::LifecycleRunStatus::Cancelled,
    }
}

fn lifecycle_topology_to_contract(
    topology: app::LifecycleRunTopologyView,
) -> contract::LifecycleRunTopology {
    match topology {
        app::LifecycleRunTopologyView::Graphless => contract::LifecycleRunTopology::Graphless,
        app::LifecycleRunTopologyView::WorkflowGraph => {
            contract::LifecycleRunTopology::WorkflowGraph
        }
    }
}

fn subject_ref_to_contract(subject: app::SubjectRefView) -> contract::SubjectRefDto {
    contract::SubjectRefDto {
        kind: subject.kind,
        id: subject.id,
    }
}

fn runtime_session_ref_to_contract(
    runtime_ref: app::RuntimeSessionRefView,
) -> contract::RuntimeSessionRefDto {
    contract::RuntimeSessionRefDto {
        runtime_session_id: runtime_ref.runtime_session_id,
    }
}

pub(crate) fn subject_association_to_contract(
    association: app::LifecycleSubjectAssociationView,
) -> contract::LifecycleSubjectAssociationDto {
    contract::LifecycleSubjectAssociationDto {
        id: association.id,
        anchor_run_id: association.anchor_run_id,
        anchor_agent_id: association.anchor_agent_id,
        subject_ref: subject_ref_to_contract(association.subject_ref),
        role: association.role,
        metadata: association.metadata,
        created_at: association.created_at,
    }
}

pub(crate) fn agent_run_to_contract(agent: app::AgentRunView) -> contract::AgentRunView {
    contract::AgentRunView {
        agent_ref: contract::AgentRunRefDto {
            run_id: agent.agent_ref.run_id,
            agent_id: agent.agent_ref.agent_id,
        },
        project_id: agent.project_id,
        source: agent.source,
        project_agent_id: agent.project_agent_id,
        status: agent.status,
        current_frame_id: agent.current_frame_id,
        delivery_runtime_ref: agent
            .delivery_runtime_ref
            .map(runtime_session_ref_to_contract),
        last_delivery_status: agent.last_delivery_status,
        created_at: agent.created_at,
        updated_at: agent.updated_at,
    }
}

fn orchestration_to_contract(
    orchestration: app::OrchestrationInstanceView,
) -> contract::OrchestrationInstanceView {
    contract::OrchestrationInstanceView {
        orchestration_id: orchestration.orchestration_id,
        role: orchestration.role,
        status: orchestration.status,
        plan_digest: orchestration.plan_digest,
        source_ref: orchestration.source_ref,
        ready_node_ids: orchestration.ready_node_ids,
        nodes: orchestration
            .nodes
            .into_iter()
            .map(runtime_node_to_contract)
            .collect(),
        created_at: orchestration.created_at,
        updated_at: orchestration.updated_at,
    }
}

fn runtime_node_to_contract(node: app::RuntimeNodeView) -> contract::RuntimeNodeView {
    contract::RuntimeNodeView {
        node_id: node.node_id,
        node_path: node.node_path,
        kind: node.kind,
        status: node.status,
        attempt: node.attempt,
        executor_run_ref: node.executor_run_ref.map(executor_run_ref_to_contract),
        started_at: node.started_at,
        completed_at: node.completed_at,
        children: node
            .children
            .into_iter()
            .map(runtime_node_to_contract)
            .collect(),
    }
}

fn executor_run_ref_to_contract(refs: app::ExecutorRunRefView) -> contract::ExecutorRunRef {
    match refs {
        app::ExecutorRunRefView::RuntimeSession { session_id } => {
            contract::ExecutorRunRef::RuntimeSession { session_id }
        }
        app::ExecutorRunRefView::FunctionRun { run_id } => {
            contract::ExecutorRunRef::FunctionRun { run_id }
        }
        app::ExecutorRunRefView::HumanDecision { decision_id } => {
            contract::ExecutorRunRef::HumanDecision { decision_id }
        }
    }
}

fn active_runtime_node_ref_to_contract(
    node_ref: app::ActiveRuntimeNodeRefView,
) -> contract::ActiveRuntimeNodeRefDto {
    contract::ActiveRuntimeNodeRefDto {
        run_id: node_ref.run_id,
        orchestration_id: node_ref.orchestration_id,
        node_path: node_ref.node_path,
        attempt: node_ref.attempt,
        status: node_ref.status,
    }
}

fn execution_entry_to_contract(
    entry: app::LifecycleExecutionEntryView,
) -> contract::LifecycleExecutionEntry {
    contract::LifecycleExecutionEntry {
        timestamp: entry.timestamp,
        activity_key: entry.activity_key,
        event_kind: execution_event_kind_to_contract(entry.event_kind),
        summary: entry.summary,
        detail: entry.detail,
    }
}

fn execution_event_kind_to_contract(
    kind: app::LifecycleExecutionEventKindView,
) -> contract::LifecycleExecutionEventKind {
    match kind {
        app::LifecycleExecutionEventKindView::ActivityActivated => {
            contract::LifecycleExecutionEventKind::ActivityActivated
        }
        app::LifecycleExecutionEventKindView::ActivityCompleted => {
            contract::LifecycleExecutionEventKind::ActivityCompleted
        }
        app::LifecycleExecutionEventKindView::ConstraintBlocked => {
            contract::LifecycleExecutionEventKind::ConstraintBlocked
        }
        app::LifecycleExecutionEventKindView::CompletionEvaluated => {
            contract::LifecycleExecutionEventKind::CompletionEvaluated
        }
        app::LifecycleExecutionEventKindView::ArtifactAppended => {
            contract::LifecycleExecutionEventKind::ArtifactAppended
        }
        app::LifecycleExecutionEventKindView::ContextInjected => {
            contract::LifecycleExecutionEventKind::ContextInjected
        }
    }
}
