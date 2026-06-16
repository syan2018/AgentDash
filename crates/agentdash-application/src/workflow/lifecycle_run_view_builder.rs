//! Read Model 投影构建器 — 组装 application-owned LifecycleRunView / SubjectExecutionView。
//!
//! 单一所有者：API 路由层不再内联投影逻辑，统一通过本模块构建 read model，
//! 确保 `runtime_trace_refs`、`subject_associations` 等字段始终被正确填充。

use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use uuid::Uuid;

use agentdash_domain::DomainError;
use agentdash_domain::workflow::{
    ExecutorRunRef as DomainExecutorRunRef, LifecycleAgent,
    LifecycleExecutionEventKind as DomainLifecycleExecutionEventKind, LifecycleRun,
    LifecycleRunStatus as DomainLifecycleRunStatus,
    LifecycleRunTopology as DomainLifecycleRunTopology, LifecycleSubjectAssociation,
    OrchestrationInstance, RuntimeNodeState, RuntimeNodeStatus, RuntimeSessionExecutionAnchor,
    SubjectRef,
};

use crate::repository_set::RepositorySet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubjectRefView {
    pub kind: String,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleRunRefView {
    pub run_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunRefView {
    pub run_id: String,
    pub agent_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeSessionRefView {
    pub runtime_session_id: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LifecycleSubjectAssociationView {
    pub id: String,
    pub anchor_run_id: String,
    pub anchor_agent_id: Option<String>,
    pub subject_ref: SubjectRefView,
    pub role: String,
    pub metadata: Option<Value>,
    pub created_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleRunStatusView {
    Draft,
    Ready,
    Running,
    Blocked,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleRunTopologyView {
    Graphless,
    WorkflowGraph,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleExecutionEventKindView {
    ActivityActivated,
    ActivityCompleted,
    ConstraintBlocked,
    CompletionEvaluated,
    ArtifactAppended,
    ContextInjected,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExecutorRunRefView {
    RuntimeSession { session_id: String },
    FunctionRun { run_id: String },
    HumanDecision { decision_id: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct LifecycleExecutionEntryView {
    pub timestamp: DateTime<Utc>,
    pub activity_key: String,
    pub event_kind: LifecycleExecutionEventKindView,
    pub summary: String,
    pub detail: Option<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeNodeView {
    pub node_id: String,
    pub node_path: String,
    pub kind: String,
    pub status: String,
    pub attempt: u32,
    pub executor_run_ref: Option<ExecutorRunRefView>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub children: Vec<RuntimeNodeView>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveRuntimeNodeRefView {
    pub run_id: String,
    pub orchestration_id: String,
    pub node_path: String,
    pub attempt: u32,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OrchestrationInstanceView {
    pub orchestration_id: String,
    pub role: String,
    pub status: String,
    pub plan_digest: String,
    pub source_ref: Value,
    pub ready_node_ids: Vec<String>,
    pub nodes: Vec<RuntimeNodeView>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentRunView {
    pub agent_ref: AgentRunRefView,
    pub project_id: String,
    pub source: String,
    pub project_agent_id: Option<String>,
    pub status: String,
    pub current_frame_id: Option<String>,
    pub delivery_runtime_ref: Option<RuntimeSessionRefView>,
    pub last_delivery_status: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LifecycleRunView {
    pub run_ref: LifecycleRunRefView,
    pub project_id: String,
    pub topology: LifecycleRunTopologyView,
    pub status: LifecycleRunStatusView,
    pub orchestrations: Vec<OrchestrationInstanceView>,
    pub active_runtime_node_refs: Vec<ActiveRuntimeNodeRefView>,
    pub agents: Vec<AgentRunView>,
    pub subject_associations: Vec<LifecycleSubjectAssociationView>,
    pub runtime_trace_refs: Vec<RuntimeSessionRefView>,
    pub execution_log: Vec<LifecycleExecutionEntryView>,
    pub created_at: String,
    pub updated_at: String,
    pub last_activity_at: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SubjectExecutionView {
    pub subject_ref: SubjectRefView,
    pub associations: Vec<LifecycleSubjectAssociationView>,
    pub runs: Vec<LifecycleRunView>,
    pub current_agent: Option<AgentRunView>,
    pub latest_runtime_node: Option<RuntimeNodeView>,
    pub artifacts: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProjectActiveAgentsView {
    pub project_id: String,
    pub runs: Vec<LifecycleRunView>,
    pub agents: Vec<AgentRunView>,
}

// ── Public async builders ──────────────────────────────────────

/// 从 LifecycleRun 构建完整的 LifecycleRunView（含 trace refs、subject associations）。
pub async fn build_lifecycle_run_view(
    repos: &RepositorySet,
    run: &LifecycleRun,
) -> Result<LifecycleRunView, DomainError> {
    let agents = repos.lifecycle_agent_repo.list_by_run(run.id).await?;
    build_lifecycle_run_view_with_preloaded(repos, run, agents).await
}

/// 使用已加载的 agents 构建 LifecycleRunView，避免重复查询。
pub async fn build_lifecycle_run_view_with_preloaded(
    repos: &RepositorySet,
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

    Ok(assemble_lifecycle_run_view(
        run,
        lifecycle_agent_views(repos, &agents).await?,
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
    repos: &RepositorySet,
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
    let runtime_projection = latest_subject_runtime_projection(repos, &associations, &runs).await?;
    let latest_runtime_node = runtime_projection
        .as_ref()
        .map(|projection| projection.node.clone());
    let artifacts = runtime_projection
        .map(|projection| projection.artifacts)
        .unwrap_or_else(|| json!({}));

    for run in &runs {
        let agents = repos.lifecycle_agent_repo.list_by_run(run.id).await?;

        if current_agent.is_none() {
            current_agent =
                select_current_agent(&associations, &agents).map(lifecycle_agent_to_view);
        }

        run_views.push(build_lifecycle_run_view_with_preloaded(repos, run, agents).await?);
    }

    run_views.sort_by(|a, b| b.last_activity_at.cmp(&a.last_activity_at));

    Ok(SubjectExecutionView {
        subject_ref: subject_ref_to_view(&subject),
        associations: associations.iter().map(association_to_view).collect(),
        runs: run_views,
        current_agent,
        latest_runtime_node,
        artifacts,
    })
}

/// 构建项目维度的活跃 agent 聚合视图（仅含非终态 run）。
pub async fn build_project_active_agents_view(
    repos: &RepositorySet,
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
    repos: &RepositorySet,
    run_id: Uuid,
) -> Result<Vec<RuntimeSessionRefView>, DomainError> {
    Ok(repos
        .execution_anchor_repo
        .list_by_run(run_id)
        .await?
        .into_iter()
        .map(|anchor| RuntimeSessionRefView {
            runtime_session_id: anchor.runtime_session_id,
        })
        .collect())
}

struct SubjectRuntimeProjection {
    node: RuntimeNodeView,
    artifacts: Value,
    observed_at: DateTime<Utc>,
}

async fn latest_subject_runtime_projection(
    repos: &RepositorySet,
    associations: &[LifecycleSubjectAssociation],
    runs: &[LifecycleRun],
) -> Result<Option<SubjectRuntimeProjection>, DomainError> {
    let mut latest: Option<SubjectRuntimeProjection> = None;

    for association in associations {
        let Some(agent) = resolve_association_agent(repos, association).await? else {
            continue;
        };
        let Some(current_frame_id) = agent.current_frame_id else {
            continue;
        };
        let Some(run) = runs.iter().find(|run| run.id == association.anchor_run_id) else {
            continue;
        };
        let anchors = repos.execution_anchor_repo.list_by_agent(agent.id).await?;
        for anchor in anchors {
            if anchor.run_id != run.id || anchor.launch_frame_id != current_frame_id {
                continue;
            }
            let Some(projection) = runtime_projection_from_anchor(run, &anchor) else {
                continue;
            };
            if latest
                .as_ref()
                .map(|current| projection.observed_at > current.observed_at)
                .unwrap_or(true)
            {
                latest = Some(projection);
            }
        }
    }

    Ok(latest)
}

async fn resolve_association_agent(
    repos: &RepositorySet,
    association: &LifecycleSubjectAssociation,
) -> Result<Option<LifecycleAgent>, DomainError> {
    if let Some(agent_id) = association.anchor_agent_id {
        let agent = repos.lifecycle_agent_repo.get(agent_id).await?;
        return Ok(agent.filter(|agent| agent.run_id == association.anchor_run_id));
    }
    Ok(repos
        .lifecycle_agent_repo
        .list_by_run(association.anchor_run_id)
        .await?
        .into_iter()
        .filter(|agent| agent.status == "active")
        .max_by_key(|agent| agent.updated_at))
}

fn runtime_projection_from_anchor(
    run: &LifecycleRun,
    anchor: &RuntimeSessionExecutionAnchor,
) -> Option<SubjectRuntimeProjection> {
    let orchestration_id = anchor.orchestration_id?;
    let node_path = anchor.node_path.as_deref()?;
    let attempt = anchor.node_attempt.unwrap_or(1);
    let orchestration = run
        .orchestrations
        .iter()
        .find(|item| item.orchestration_id == orchestration_id)?;
    let node = find_runtime_node_by_path(&orchestration.node_tree, node_path, attempt)?;
    let observed_at = node
        .completed_at
        .or(node.started_at)
        .unwrap_or(anchor.updated_at);
    Some(SubjectRuntimeProjection {
        node: runtime_node_to_view(node),
        artifacts: runtime_node_artifacts(orchestration, node),
        observed_at,
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

pub fn lifecycle_agent_to_view(agent: &LifecycleAgent) -> AgentRunView {
    lifecycle_agent_to_view_with_delivery(agent, None)
}

fn lifecycle_agent_to_view_with_delivery(
    agent: &LifecycleAgent,
    delivery_runtime_session_id: Option<String>,
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
        current_frame_id: agent.current_frame_id.map(|id| id.to_string()),
        delivery_runtime_ref: delivery_runtime_session_id
            .map(|runtime_session_id| RuntimeSessionRefView { runtime_session_id }),
        last_delivery_status: None,
        created_at: agent.created_at.to_rfc3339(),
        updated_at: agent.updated_at.to_rfc3339(),
    }
}

async fn lifecycle_agent_views(
    repos: &RepositorySet,
    agents: &[LifecycleAgent],
) -> Result<Vec<AgentRunView>, DomainError> {
    let mut views = Vec::with_capacity(agents.len());
    for agent in agents {
        let delivery_runtime_session_id =
            resolve_agent_delivery_runtime_session_id(repos, agent).await?;
        views.push(lifecycle_agent_to_view_with_delivery(
            agent,
            delivery_runtime_session_id,
        ));
    }
    Ok(views)
}

async fn resolve_agent_delivery_runtime_session_id(
    repos: &RepositorySet,
    agent: &LifecycleAgent,
) -> Result<Option<String>, DomainError> {
    Ok(repos
        .execution_anchor_repo
        .latest_for_agent(agent.id)
        .await?
        .map(|anchor| anchor.runtime_session_id))
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
        DomainLifecycleRunTopology::Graphless => LifecycleRunTopologyView::Graphless,
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
        DomainExecutorRunRef::RuntimeSession { session_id } => ExecutorRunRefView::RuntimeSession {
            session_id: session_id.clone(),
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

fn find_runtime_node_by_path<'a>(
    nodes: &'a [RuntimeNodeState],
    node_path: &str,
    attempt: u32,
) -> Option<&'a RuntimeNodeState> {
    for node in nodes {
        if node.node_path == node_path && node.attempt == attempt {
            return Some(node);
        }
        if let Some(found) = find_runtime_node_by_path(&node.children, node_path, attempt) {
            return Some(found);
        }
    }
    None
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
