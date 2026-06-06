//! Read Model 投影构建器 — 组装 LifecycleRunView / SubjectExecutionView。
//!
//! 单一所有者：API 路由层不再内联投影逻辑，统一通过本模块构建 read model，
//! 确保 `runtime_trace_refs`、`subject_associations` 等字段始终被正确填充。

use serde_json::json;
use uuid::Uuid;

use agentdash_contracts::workflow::{
    ActiveActivityRefDto, ActivityAttemptView, LifecycleAgentRefDto, LifecycleAgentView,
    LifecycleExecutionEntry as ContractLifecycleExecutionEntry,
    LifecycleExecutionEventKind as ContractLifecycleExecutionEventKind, LifecycleRunRefDto,
    LifecycleRunStatus as ContractLifecycleRunStatus,
    LifecycleRunTopology as ContractLifecycleRunTopology, LifecycleRunView,
    LifecycleSubjectAssociationDto, ProjectActiveAgentsView, RuntimeSessionRefDto,
    SubjectExecutionView, SubjectRefDto, WorkflowGraphInstanceView,
};
use agentdash_domain::DomainError;
use agentdash_domain::workflow::{
    LifecycleAgent, LifecycleExecutionEventKind as DomainLifecycleExecutionEventKind, LifecycleRun,
    LifecycleRunStatus as DomainLifecycleRunStatus,
    LifecycleRunTopology as DomainLifecycleRunTopology, LifecycleSubjectAssociation, SubjectRef,
};

use crate::repository_set::RepositorySet;

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

    let workflow_graph_instances = Vec::new();
    let active_activity_refs = Vec::new();
    let runtime_trace_refs = collect_runtime_trace_refs(repos, run.id).await?;

    Ok(assemble_lifecycle_run_view(
        run,
        lifecycle_agent_views(repos, &agents).await?,
        subject_associations
            .iter()
            .map(association_to_dto)
            .collect(),
        workflow_graph_instances,
        active_activity_refs,
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
    let mut current_agent: Option<LifecycleAgentView> = None;
    let latest_attempt: Option<ActivityAttemptView> = None;
    let artifacts = json!({});

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
        subject_ref: subject_ref_to_dto(&subject),
        associations: associations.iter().map(association_to_dto).collect(),
        runs: run_views,
        current_agent,
        latest_attempt,
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
) -> Result<Vec<RuntimeSessionRefDto>, DomainError> {
    Ok(repos
        .execution_anchor_repo
        .list_by_run(run_id)
        .await?
        .into_iter()
        .map(|anchor| RuntimeSessionRefDto {
            runtime_session_id: anchor.runtime_session_id,
        })
        .collect())
}

// ── Pure conversion functions (pub for reuse) ──────────────────

pub fn subject_ref_to_dto(subject: &SubjectRef) -> SubjectRefDto {
    SubjectRefDto {
        kind: subject.kind.clone(),
        id: subject.id.to_string(),
    }
}

pub fn association_to_dto(
    association: &LifecycleSubjectAssociation,
) -> LifecycleSubjectAssociationDto {
    LifecycleSubjectAssociationDto {
        id: association.id.to_string(),
        anchor_run_id: association.anchor_run_id.to_string(),
        anchor_agent_id: association.anchor_agent_id.map(|id| id.to_string()),
        subject_ref: SubjectRefDto {
            kind: association.subject_kind.clone(),
            id: association.subject_id.to_string(),
        },
        role: association.role.clone(),
        metadata: association.metadata_json.clone(),
        created_at: association.created_at.to_rfc3339(),
    }
}

pub fn lifecycle_agent_to_view(agent: &LifecycleAgent) -> LifecycleAgentView {
    lifecycle_agent_to_view_with_delivery(agent, None)
}

fn lifecycle_agent_to_view_with_delivery(
    agent: &LifecycleAgent,
    delivery_runtime_session_id: Option<String>,
) -> LifecycleAgentView {
    LifecycleAgentView {
        agent_ref: LifecycleAgentRefDto {
            run_id: agent.run_id.to_string(),
            agent_id: agent.id.to_string(),
        },
        project_id: agent.project_id.to_string(),
        agent_kind: agent.agent_kind.clone(),
        agent_role: agent.agent_role.clone(),
        project_agent_id: agent.project_agent_id.map(|id| id.to_string()),
        status: agent.status.clone(),
        current_frame_id: agent.current_frame_id.map(|id| id.to_string()),
        delivery_runtime_ref: delivery_runtime_session_id
            .map(|runtime_session_id| RuntimeSessionRefDto { runtime_session_id }),
        last_delivery_status: None,
        created_at: agent.created_at.to_rfc3339(),
        updated_at: agent.updated_at.to_rfc3339(),
    }
}

async fn lifecycle_agent_views(
    repos: &RepositorySet,
    agents: &[LifecycleAgent],
) -> Result<Vec<LifecycleAgentView>, DomainError> {
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

fn status_to_dto(status: DomainLifecycleRunStatus) -> ContractLifecycleRunStatus {
    match status {
        DomainLifecycleRunStatus::Draft => ContractLifecycleRunStatus::Draft,
        DomainLifecycleRunStatus::Ready => ContractLifecycleRunStatus::Ready,
        DomainLifecycleRunStatus::Running => ContractLifecycleRunStatus::Running,
        DomainLifecycleRunStatus::Blocked => ContractLifecycleRunStatus::Blocked,
        DomainLifecycleRunStatus::Completed => ContractLifecycleRunStatus::Completed,
        DomainLifecycleRunStatus::Failed => ContractLifecycleRunStatus::Failed,
        DomainLifecycleRunStatus::Cancelled => ContractLifecycleRunStatus::Cancelled,
    }
}

fn topology_to_dto(topology: DomainLifecycleRunTopology) -> ContractLifecycleRunTopology {
    match topology {
        DomainLifecycleRunTopology::Graphless => ContractLifecycleRunTopology::Graphless,
        DomainLifecycleRunTopology::WorkflowGraph => ContractLifecycleRunTopology::WorkflowGraph,
    }
}

fn assemble_lifecycle_run_view(
    run: &LifecycleRun,
    agents: Vec<LifecycleAgentView>,
    subject_associations: Vec<LifecycleSubjectAssociationDto>,
    workflow_graph_instances: Vec<WorkflowGraphInstanceView>,
    active_activity_refs: Vec<ActiveActivityRefDto>,
    runtime_trace_refs: Vec<RuntimeSessionRefDto>,
) -> LifecycleRunView {
    LifecycleRunView {
        run_ref: LifecycleRunRefDto {
            run_id: run.id.to_string(),
        },
        project_id: run.project_id.to_string(),
        topology: topology_to_dto(run.topology),
        root_graph_id: run.root_graph_id.map(|id| id.to_string()),
        status: status_to_dto(run.status),
        workflow_graph_instances,
        active_activity_refs,
        agents,
        subject_associations,
        runtime_trace_refs,
        execution_log: run
            .execution_log
            .iter()
            .map(execution_entry_to_dto)
            .collect(),
        created_at: run.created_at.to_rfc3339(),
        updated_at: run.updated_at.to_rfc3339(),
        last_activity_at: run.last_activity_at.to_rfc3339(),
    }
}

fn execution_entry_to_dto(
    entry: &agentdash_domain::workflow::LifecycleExecutionEntry,
) -> ContractLifecycleExecutionEntry {
    ContractLifecycleExecutionEntry {
        timestamp: entry.timestamp,
        activity_key: entry.activity_key.clone(),
        event_kind: execution_event_kind_to_dto(entry.event_kind),
        summary: entry.summary.clone(),
        detail: entry.detail.clone(),
    }
}

fn execution_event_kind_to_dto(
    kind: DomainLifecycleExecutionEventKind,
) -> ContractLifecycleExecutionEventKind {
    match kind {
        DomainLifecycleExecutionEventKind::ActivityActivated => {
            ContractLifecycleExecutionEventKind::ActivityActivated
        }
        DomainLifecycleExecutionEventKind::ActivityCompleted => {
            ContractLifecycleExecutionEventKind::ActivityCompleted
        }
        DomainLifecycleExecutionEventKind::ConstraintBlocked => {
            ContractLifecycleExecutionEventKind::ConstraintBlocked
        }
        DomainLifecycleExecutionEventKind::CompletionEvaluated => {
            ContractLifecycleExecutionEventKind::CompletionEvaluated
        }
        DomainLifecycleExecutionEventKind::ArtifactAppended => {
            ContractLifecycleExecutionEventKind::ArtifactAppended
        }
        DomainLifecycleExecutionEventKind::ContextInjected => {
            ContractLifecycleExecutionEventKind::ContextInjected
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
