//! Read Model 投影构建器 — 组装 application-owned LifecycleRunView / SubjectExecutionView。
//!
//! 单一所有者：API 路由层不再内联投影逻辑，统一通过本模块构建 read model，
//! 确保 `runtime_trace_refs`、`subject_associations` 等字段始终被正确填充。

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serde_json::{Value, json};
use uuid::Uuid;

use agentdash_application_ports::agent_run_runtime::{
    AgentRunRuntimeBinding, AgentRunRuntimeBindingRepository,
};

pub use agentdash_application_ports::lifecycle_read_model::{
    ActiveRuntimeNodeRefView, AgentRunRefView, AgentRunView, ExecutorRunRefView,
    LifecycleExecutionEntryView, LifecycleExecutionEventKindView, LifecycleReadModelQueryPort,
    LifecycleRunRefView, LifecycleRunStatusView, LifecycleRunTopologyView, LifecycleRunView,
    LifecycleSubjectAssociationView, OrchestrationInstanceView, ProjectActiveAgentsView,
    RuntimeNodeView, RuntimeSessionRefView, SubjectExecutionView, SubjectRefView,
    SubjectRuntimeAttemptView,
};
use agentdash_domain::DomainError;
use agentdash_domain::workflow::{
    AgentFrameRepository, AgentLineage, AgentLineageRepository,
    ExecutorRunRef as DomainExecutorRunRef, LifecycleAgent, LifecycleAgentRepository,
    LifecycleExecutionEventKind as DomainLifecycleExecutionEventKind, LifecycleRun,
    LifecycleRunRepository, LifecycleRunStatus as DomainLifecycleRunStatus,
    LifecycleRunTopology as DomainLifecycleRunTopology, LifecycleSubjectAssociation,
    LifecycleSubjectAssociationRepository, OrchestrationInstance, RuntimeNodeState,
    RuntimeNodeStatus, SubjectRef,
};

#[derive(Clone)]
pub struct LifecycleReadModelQueryAdapter {
    repos: LifecycleReadModelRepos,
}

#[derive(Clone)]
pub struct LifecycleReadModelRepos {
    pub lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
    pub lifecycle_agent_repo: Arc<dyn LifecycleAgentRepository>,
    pub agent_frame_repo: Arc<dyn AgentFrameRepository>,
    pub lifecycle_subject_association_repo: Arc<dyn LifecycleSubjectAssociationRepository>,
    pub agent_lineage_repo: Arc<dyn AgentLineageRepository>,
    pub agent_run_runtime_binding_repo: Arc<dyn AgentRunRuntimeBindingRepository>,
}

impl LifecycleReadModelQueryAdapter {
    pub fn new(repos: LifecycleReadModelRepos) -> Self {
        Self { repos }
    }
}

#[async_trait::async_trait]
impl LifecycleReadModelQueryPort for LifecycleReadModelQueryAdapter {
    async fn lifecycle_run_view(&self, run_id: Uuid) -> Result<LifecycleRunView, DomainError> {
        let run = self
            .repos
            .lifecycle_run_repo
            .get_by_id(run_id)
            .await?
            .ok_or(DomainError::NotFound {
                entity: "lifecycle_run",
                id: run_id.to_string(),
            })?;
        build_lifecycle_run_view(&self.repos, &run).await
    }
}

// ── Public async builders ──────────────────────────────────────

/// 从 LifecycleRun 构建完整的 LifecycleRunView（含 trace refs、subject associations）。
pub async fn build_lifecycle_run_view(
    repos: &LifecycleReadModelRepos,
    run: &LifecycleRun,
) -> Result<LifecycleRunView, DomainError> {
    let agents = repos.lifecycle_agent_repo.list_by_run(run.id).await?;
    build_lifecycle_run_view_with_preloaded(repos, run, agents).await
}

/// 使用已加载的 agents 构建 LifecycleRunView，避免重复查询。
pub async fn build_lifecycle_run_view_with_preloaded(
    repos: &LifecycleReadModelRepos,
    run: &LifecycleRun,
    agents: Vec<LifecycleAgent>,
) -> Result<LifecycleRunView, DomainError> {
    let mut subject_associations = repos
        .lifecycle_subject_association_repo
        .list_by_anchor(run.id, None)
        .await?;
    for agent in &agents {
        subject_associations.extend(
            repos
                .lifecycle_subject_association_repo
                .list_by_anchor(run.id, Some(agent.id))
                .await?,
        );
    }

    let orchestrations = orchestration_views(run);
    let active_runtime_node_refs = active_runtime_node_refs(run);
    let runtime_trace_refs = collect_runtime_trace_refs(repos, run.id).await?;
    let runtime_bindings = repos
        .agent_run_runtime_binding_repo
        .list_by_run(run.id)
        .await
        .map_err(runtime_binding_error)?;

    Ok(assemble_lifecycle_run_view(
        run,
        lifecycle_agent_views(&agents, &runtime_bindings),
        subject_associations
            .iter()
            .map(association_to_view)
            .collect(),
        orchestrations,
        active_runtime_node_refs,
        runtime_trace_refs,
    ))
}

/// 为给定 subject 构建 SubjectExecutionView。
pub async fn build_subject_execution_view(
    repos: &LifecycleReadModelRepos,
    subject: SubjectRef,
) -> Result<SubjectExecutionView, DomainError> {
    let associations = repos
        .lifecycle_subject_association_repo
        .list_by_subject(&subject)
        .await?;
    let run_ids = unique_run_ids(&associations);
    let runs = repos.lifecycle_run_repo.list_by_ids(&run_ids).await?;

    let mut run_views = Vec::with_capacity(runs.len());
    let mut current_agent: Option<AgentRunView> = None;
    let runtime_attempts = subject_runtime_attempt_history(repos, &associations, &runs).await?;
    let latest_runtime_node = runtime_attempts
        .first()
        .map(|attempt| attempt.runtime_node.clone());
    let artifacts = runtime_attempts
        .first()
        .map(|attempt| attempt.artifacts.clone())
        .unwrap_or_else(|| json!({}));

    for run in &runs {
        let agents = repos.lifecycle_agent_repo.list_by_run(run.id).await?;
        let runtime_bindings = repos
            .agent_run_runtime_binding_repo
            .list_by_run(run.id)
            .await
            .map_err(runtime_binding_error)?;
        let runtime_bindings_by_agent = runtime_bindings_by_agent(&runtime_bindings);

        if current_agent.is_none() {
            current_agent = select_current_agent(&associations, &agents).map(|agent| {
                lifecycle_agent_to_view(agent, runtime_bindings_by_agent.get(&agent.id).copied())
            });
        }

        run_views.push(build_lifecycle_run_view_with_preloaded(repos, run, agents).await?);
    }

    run_views.sort_by(|a, b| b.last_activity_at.cmp(&a.last_activity_at));

    Ok(SubjectExecutionView {
        subject_ref: subject_ref_to_view(&subject),
        associations: associations.iter().map(association_to_view).collect(),
        runs: run_views,
        current_agent,
        runtime_attempts,
        latest_runtime_node,
        artifacts,
    })
}

/// 构建项目维度的活跃 agent 聚合视图（仅含非终态 run）。
pub async fn build_project_active_agents_view(
    repos: &LifecycleReadModelRepos,
    project_id: uuid::Uuid,
) -> Result<ProjectActiveAgentsView, DomainError> {
    let all_runs = repos.lifecycle_run_repo.list_by_project(project_id).await?;
    let active_runs: Vec<_> = all_runs
        .into_iter()
        .filter(|r| {
            !matches!(
                r.status,
                DomainLifecycleRunStatus::Completed
                    | DomainLifecycleRunStatus::Failed
                    | DomainLifecycleRunStatus::Cancelled
            )
        })
        .collect();

    let mut run_views = Vec::with_capacity(active_runs.len());
    let mut all_agents = Vec::new();

    for run in &active_runs {
        let view = build_lifecycle_run_view(repos, run).await?;
        all_agents.extend(view.agents.clone());
        run_views.push(view);
    }

    run_views.sort_by(|a, b| b.last_activity_at.cmp(&a.last_activity_at));

    Ok(ProjectActiveAgentsView {
        project_id: project_id.to_string(),
        runs: run_views,
        agents: all_agents,
    })
}

// ── Internal async helpers ─────────────────────────────────────

async fn collect_runtime_trace_refs(
    repos: &LifecycleReadModelRepos,
    run_id: Uuid,
) -> Result<Vec<RuntimeSessionRefView>, DomainError> {
    Ok(repos
        .agent_run_runtime_binding_repo
        .list_by_run(run_id)
        .await
        .map_err(runtime_binding_error)?
        .into_iter()
        .map(|binding| RuntimeSessionRefView {
            runtime_session_id: binding.thread_id.to_string(),
        })
        .collect())
}

async fn subject_runtime_attempt_history(
    repos: &LifecycleReadModelRepos,
    associations: &[LifecycleSubjectAssociation],
    runs: &[LifecycleRun],
) -> Result<Vec<SubjectRuntimeAttemptView>, DomainError> {
    let mut attempts = Vec::new();
    let mut seen_runtime_sessions = HashSet::new();

    for association in associations {
        let agents = resolve_association_history_agents(repos, association).await?;
        let Some(run) = runs.iter().find(|run| run.id == association.anchor_run_id) else {
            continue;
        };
        for agent in agents {
            let current_frame_id = repos
                .agent_frame_repo
                .get_latest(agent.id)
                .await?
                .map(|frame| frame.id);
            let bindings = repos
                .agent_run_runtime_binding_repo
                .list_by_agent(agent.id)
                .await
                .map_err(runtime_binding_error)?;
            for binding in bindings {
                if binding.target.run_id != run.id
                    || !seen_runtime_sessions.insert(binding.thread_id.to_string())
                {
                    continue;
                }
                let Some(attempt) = runtime_attempt_from_binding(run, &binding, current_frame_id)
                else {
                    continue;
                };
                attempts.push(attempt);
            }
        }
    }

    sort_subject_runtime_attempts(&mut attempts);

    Ok(attempts)
}

async fn resolve_association_history_agents(
    repos: &LifecycleReadModelRepos,
    association: &LifecycleSubjectAssociation,
) -> Result<Vec<LifecycleAgent>, DomainError> {
    if !association_role_can_own_runtime_attempts(&association.role) {
        return Ok(Vec::new());
    }

    if let Some(agent_id) = association.anchor_agent_id {
        let agent = repos.lifecycle_agent_repo.get(agent_id).await?;
        return Ok(agent
            .filter(|agent| agent.run_id == association.anchor_run_id)
            .into_iter()
            .collect());
    }
    let agents = repos
        .lifecycle_agent_repo
        .list_by_run(association.anchor_run_id)
        .await?;
    let lineages = repos
        .agent_lineage_repo
        .list_by_run(association.anchor_run_id)
        .await?;
    Ok(filter_whole_run_history_agents(agents, &lineages))
}

fn association_role_can_own_runtime_attempts(role: &str) -> bool {
    matches!(role, "subject" | "task_execution")
}

fn filter_whole_run_history_agents(
    agents: Vec<LifecycleAgent>,
    lineages: &[AgentLineage],
) -> Vec<LifecycleAgent> {
    if lineages.is_empty() {
        return if agents.len() == 1 {
            agents
        } else {
            Vec::new()
        };
    }

    let child_agent_ids = lineages
        .iter()
        .map(|lineage| lineage.child_agent_id)
        .collect::<HashSet<_>>();
    agents
        .into_iter()
        .filter(|agent| !child_agent_ids.contains(&agent.id))
        .collect()
}

fn sort_subject_runtime_attempts(attempts: &mut [SubjectRuntimeAttemptView]) {
    attempts.sort_by(|a, b| {
        b.observed_at.cmp(&a.observed_at).then_with(|| {
            b.runtime_session_ref
                .runtime_session_id
                .cmp(&a.runtime_session_ref.runtime_session_id)
        })
    });
}

fn runtime_attempt_from_binding(
    run: &LifecycleRun,
    binding: &AgentRunRuntimeBinding,
    current_frame_id: Option<Uuid>,
) -> Option<SubjectRuntimeAttemptView> {
    let (orchestration, node) = run.orchestrations.iter().find_map(|orchestration| {
        find_runtime_node_by_thread(&orchestration.node_tree, binding.thread_id.as_str())
            .map(|node| (orchestration, node))
    })?;
    let orchestration_id = orchestration.orchestration_id;
    let node_path = node.node_path.as_str();
    let attempt = node.attempt.max(1);
    let observed_at = node
        .completed_at
        .or(node.started_at)
        .unwrap_or(run.updated_at);
    let runtime_node = runtime_node_to_view(node);
    let artifacts = runtime_node_artifacts(orchestration, node);
    Some(SubjectRuntimeAttemptView {
        run_ref: LifecycleRunRefView {
            run_id: run.id.to_string(),
        },
        agent_ref: AgentRunRefView {
            run_id: run.id.to_string(),
            agent_id: binding.target.agent_id.to_string(),
        },
        runtime_session_ref: RuntimeSessionRefView {
            runtime_session_id: binding.thread_id.to_string(),
        },
        launch_frame_id: current_frame_id
            .map(|id| id.to_string())
            .unwrap_or_default(),
        current_frame_id: current_frame_id.map(|id| id.to_string()),
        orchestration_id: orchestration_id.to_string(),
        node_path: node_path.to_string(),
        attempt,
        status: status_string(&node.status),
        observed_at: observed_at.to_rfc3339(),
        runtime_node,
        artifacts,
    })
}

// ── Pure conversion functions (pub for reuse) ──────────────────

pub fn subject_ref_to_view(subject: &SubjectRef) -> SubjectRefView {
    SubjectRefView {
        kind: subject.kind.clone(),
        id: subject.id.to_string(),
    }
}

pub fn association_to_view(
    association: &LifecycleSubjectAssociation,
) -> LifecycleSubjectAssociationView {
    LifecycleSubjectAssociationView {
        id: association.id.to_string(),
        anchor_run_id: association.anchor_run_id.to_string(),
        anchor_agent_id: association.anchor_agent_id.map(|id| id.to_string()),
        subject_ref: SubjectRefView {
            kind: association.subject_kind.clone(),
            id: association.subject_id.to_string(),
        },
        role: association.role.clone(),
        metadata: association.metadata_json.clone(),
        created_at: association.created_at.to_rfc3339(),
    }
}

pub fn lifecycle_agent_to_view(
    agent: &LifecycleAgent,
    _runtime_binding: Option<&AgentRunRuntimeBinding>,
) -> AgentRunView {
    AgentRunView {
        agent_ref: AgentRunRefView {
            run_id: agent.run_id.to_string(),
            agent_id: agent.id.to_string(),
        },
        project_id: agent.project_id.to_string(),
        source: agent.source.as_str().to_string(),
        project_agent_id: agent.project_agent_id.map(|id| id.to_string()),
        status: agent.status.clone(),
        last_delivery_status: None,
        created_at: agent.created_at.to_rfc3339(),
        updated_at: agent.updated_at.to_rfc3339(),
    }
}

fn lifecycle_agent_views(
    agents: &[LifecycleAgent],
    runtime_bindings: &[AgentRunRuntimeBinding],
) -> Vec<AgentRunView> {
    let runtime_bindings_by_agent = runtime_bindings_by_agent(runtime_bindings);
    agents
        .iter()
        .map(|agent| {
            lifecycle_agent_to_view(agent, runtime_bindings_by_agent.get(&agent.id).copied())
        })
        .collect()
}

fn runtime_bindings_by_agent(
    runtime_bindings: &[AgentRunRuntimeBinding],
) -> HashMap<Uuid, &AgentRunRuntimeBinding> {
    runtime_bindings
        .iter()
        .map(|binding| (binding.target.agent_id, binding))
        .collect()
}

// ── Internal pure helpers ──────────────────────────────────────

fn status_to_view(status: DomainLifecycleRunStatus) -> LifecycleRunStatusView {
    match status {
        DomainLifecycleRunStatus::Draft => LifecycleRunStatusView::Draft,
        DomainLifecycleRunStatus::Ready => LifecycleRunStatusView::Ready,
        DomainLifecycleRunStatus::Running => LifecycleRunStatusView::Running,
        DomainLifecycleRunStatus::Blocked => LifecycleRunStatusView::Blocked,
        DomainLifecycleRunStatus::Completed => LifecycleRunStatusView::Completed,
        DomainLifecycleRunStatus::Failed => LifecycleRunStatusView::Failed,
        DomainLifecycleRunStatus::Cancelled => LifecycleRunStatusView::Cancelled,
    }
}

fn topology_to_view(topology: DomainLifecycleRunTopology) -> LifecycleRunTopologyView {
    match topology {
        DomainLifecycleRunTopology::Plain => LifecycleRunTopologyView::Plain,
        DomainLifecycleRunTopology::WorkflowGraph => LifecycleRunTopologyView::WorkflowGraph,
    }
}

fn assemble_lifecycle_run_view(
    run: &LifecycleRun,
    agents: Vec<AgentRunView>,
    subject_associations: Vec<LifecycleSubjectAssociationView>,
    orchestrations: Vec<OrchestrationInstanceView>,
    active_runtime_node_refs: Vec<ActiveRuntimeNodeRefView>,
    runtime_trace_refs: Vec<RuntimeSessionRefView>,
) -> LifecycleRunView {
    LifecycleRunView {
        run_ref: LifecycleRunRefView {
            run_id: run.id.to_string(),
        },
        project_id: run.project_id.to_string(),
        topology: topology_to_view(run.topology),
        status: status_to_view(run.status),
        orchestrations,
        active_runtime_node_refs,
        agents,
        subject_associations,
        runtime_trace_refs,
        execution_log: run
            .execution_log
            .iter()
            .map(execution_entry_to_view)
            .collect(),
        created_at: run.created_at.to_rfc3339(),
        updated_at: run.updated_at.to_rfc3339(),
        last_activity_at: run.last_activity_at.to_rfc3339(),
    }
}

fn orchestration_views(run: &LifecycleRun) -> Vec<OrchestrationInstanceView> {
    run.orchestrations
        .iter()
        .map(orchestration_to_view)
        .collect()
}

fn orchestration_to_view(instance: &OrchestrationInstance) -> OrchestrationInstanceView {
    OrchestrationInstanceView {
        orchestration_id: instance.orchestration_id.to_string(),
        role: instance.role.clone(),
        status: status_string(&instance.status),
        plan_digest: instance.plan_snapshot.plan_digest.clone(),
        source_ref: serde_json::to_value(&instance.source_ref).unwrap_or(serde_json::Value::Null),
        ready_node_ids: instance.dispatch.ready_node_ids.clone(),
        nodes: instance
            .node_tree
            .iter()
            .map(runtime_node_to_view)
            .collect(),
        created_at: instance.created_at.to_rfc3339(),
        updated_at: instance.updated_at.to_rfc3339(),
    }
}

fn runtime_node_to_view(node: &RuntimeNodeState) -> RuntimeNodeView {
    RuntimeNodeView {
        node_id: node.node_id.clone(),
        node_path: node.node_path.clone(),
        kind: status_string(&node.kind),
        status: status_string(&node.status),
        attempt: node.attempt,
        executor_run_ref: node.executor_run_ref.as_ref().map(executor_run_ref_to_view),
        started_at: node.started_at.map(|timestamp| timestamp.to_rfc3339()),
        completed_at: node.completed_at.map(|timestamp| timestamp.to_rfc3339()),
        children: node.children.iter().map(runtime_node_to_view).collect(),
    }
}

fn executor_run_ref_to_view(refs: &DomainExecutorRunRef) -> ExecutorRunRefView {
    match refs {
        DomainExecutorRunRef::AgentRun { run_id, agent_id } => ExecutorRunRefView::AgentRun {
            run_id: run_id.to_string(),
            agent_id: agent_id.to_string(),
        },
        DomainExecutorRunRef::FunctionRun { run_id } => ExecutorRunRefView::FunctionRun {
            run_id: run_id.clone(),
        },
        DomainExecutorRunRef::HumanDecision { decision_id } => ExecutorRunRefView::HumanDecision {
            decision_id: decision_id.clone(),
        },
    }
}

fn active_runtime_node_refs(run: &LifecycleRun) -> Vec<ActiveRuntimeNodeRefView> {
    run.orchestrations
        .iter()
        .flat_map(|instance| {
            flatten_runtime_nodes(&instance.node_tree)
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
                .map(move |node| ActiveRuntimeNodeRefView {
                    run_id: run.id.to_string(),
                    orchestration_id: instance.orchestration_id.to_string(),
                    node_path: node.node_path.clone(),
                    attempt: node.attempt,
                    status: status_string(&node.status),
                })
        })
        .collect()
}

fn flatten_runtime_nodes(nodes: &[RuntimeNodeState]) -> Vec<&RuntimeNodeState> {
    fn collect<'a>(node: &'a RuntimeNodeState, acc: &mut Vec<&'a RuntimeNodeState>) {
        acc.push(node);
        for child in &node.children {
            collect(child, acc);
        }
    }

    let mut flattened = Vec::new();
    for node in nodes {
        collect(node, &mut flattened);
    }
    flattened
}

fn find_runtime_node_by_thread<'a>(
    nodes: &'a [RuntimeNodeState],
    thread_id: &str,
) -> Option<&'a RuntimeNodeState> {
    nodes.iter().find_map(|node| {
        if matches!(
            node.executor_run_ref.as_ref(),
            Some(DomainExecutorRunRef::RuntimeSession { session_id }) if session_id == thread_id
        ) {
            return Some(node);
        }
        find_runtime_node_by_thread(&node.children, thread_id)
    })
}

fn runtime_binding_error(
    error: agentdash_application_ports::agent_run_runtime::AgentRunRuntimeBindingError,
) -> DomainError {
    DomainError::Database {
        operation: "agent_run_runtime_binding",
        message: error.to_string(),
    }
}

fn runtime_node_artifacts(orchestration: &OrchestrationInstance, node: &RuntimeNodeState) -> Value {
    let node_outputs = orchestration
        .state_snapshot
        .node_outputs
        .get(&node.node_id)
        .cloned()
        .unwrap_or(Value::Null);
    let artifact_refs = orchestration
        .state_snapshot
        .artifacts
        .iter()
        .filter(|artifact| artifact.node_path.as_deref() == Some(node.node_path.as_str()))
        .collect::<Vec<_>>();

    json!({
        "node_outputs": node_outputs,
        "artifact_refs": serde_json::to_value(artifact_refs).unwrap_or(Value::Null),
    })
}

fn status_string<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| "unknown".to_string())
}

fn execution_entry_to_view(
    entry: &agentdash_domain::workflow::LifecycleExecutionEntry,
) -> LifecycleExecutionEntryView {
    LifecycleExecutionEntryView {
        timestamp: entry.timestamp,
        activity_key: entry.activity_key.clone(),
        event_kind: execution_event_kind_to_view(entry.event_kind),
        summary: entry.summary.clone(),
        detail: entry.detail.clone(),
    }
}

fn execution_event_kind_to_view(
    kind: DomainLifecycleExecutionEventKind,
) -> LifecycleExecutionEventKindView {
    match kind {
        DomainLifecycleExecutionEventKind::ActivityActivated => {
            LifecycleExecutionEventKindView::ActivityActivated
        }
        DomainLifecycleExecutionEventKind::ActivityCompleted => {
            LifecycleExecutionEventKindView::ActivityCompleted
        }
        DomainLifecycleExecutionEventKind::ConstraintBlocked => {
            LifecycleExecutionEventKindView::ConstraintBlocked
        }
        DomainLifecycleExecutionEventKind::CompletionEvaluated => {
            LifecycleExecutionEventKindView::CompletionEvaluated
        }
        DomainLifecycleExecutionEventKind::ArtifactAppended => {
            LifecycleExecutionEventKindView::ArtifactAppended
        }
        DomainLifecycleExecutionEventKind::ContextInjected => {
            LifecycleExecutionEventKindView::ContextInjected
        }
    }
}

fn select_current_agent<'a>(
    associations: &[LifecycleSubjectAssociation],
    agents: &'a [LifecycleAgent],
) -> Option<&'a LifecycleAgent> {
    associations
        .iter()
        .find_map(|a| a.anchor_agent_id)
        .and_then(|agent_id| agents.iter().find(|agent| agent.id == agent_id))
        .or_else(|| agents.iter().find(|agent| agent.status == "active"))
        .or_else(|| agents.first())
}

fn unique_run_ids(associations: &[LifecycleSubjectAssociation]) -> Vec<Uuid> {
    let mut run_ids = Vec::new();
    for association in associations {
        if !run_ids.contains(&association.anchor_run_id) {
            run_ids.push(association.anchor_run_id);
        }
    }
    run_ids
}
