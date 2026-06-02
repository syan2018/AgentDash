//! Read Model 投影构建器 — 组装 LifecycleRunView / SubjectExecutionView。
//!
//! 单一所有者：API 路由层不再内联投影逻辑，统一通过本模块构建 read model，
//! 确保 `runtime_trace_refs`、`subject_associations` 等字段始终被正确填充。

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::{Value, json};
use uuid::Uuid;

use agentdash_contracts::workflow::{
    ActivityAttemptView, ActivityStateView, AgentAssignmentRefDto,
    ExecutorRunRef as ContractExecutorRunRef, LifecycleAgentRefDto, LifecycleAgentView,
    LifecycleExecutionEntry as ContractLifecycleExecutionEntry,
    LifecycleExecutionEventKind as ContractLifecycleExecutionEventKind, LifecycleRunRefDto,
    LifecycleRunStatus as ContractLifecycleRunStatus, LifecycleRunView,
    LifecycleSubjectAssociationDto, ProjectActiveAgentsView, RuntimeSessionRefDto,
    SubjectExecutionView, SubjectRefDto, WorkflowGraphInstanceView,
};
use agentdash_domain::workflow::{
    ActivityLifecycleRunState, AgentAssignment, ExecutorRunRef as DomainExecutorRunRef,
    LifecycleAgent, LifecycleExecutionEventKind as DomainLifecycleExecutionEventKind, LifecycleRun,
    LifecycleRunStatus as DomainLifecycleRunStatus, LifecycleSubjectAssociation, SubjectRef,
    WorkflowGraphInstance,
};
use agentdash_domain::DomainError;

use crate::repository_set::RepositorySet;

// ── Public async builders ──────────────────────────────────────

/// 从 LifecycleRun 构建完整的 LifecycleRunView（含 trace refs、subject associations）。
pub async fn build_lifecycle_run_view(
    repos: &RepositorySet,
    run: &LifecycleRun,
) -> Result<LifecycleRunView, DomainError> {
    let agents = repos.lifecycle_agent_repo.list_by_run(run.id).await?;
    let assignments = repos.agent_assignment_repo.list_by_run(run.id).await?;
    build_lifecycle_run_view_with_preloaded(repos, run, agents, assignments).await
}

/// 使用已加载的 agents/assignments 构建 LifecycleRunView，避免重复查询。
pub async fn build_lifecycle_run_view_with_preloaded(
    repos: &RepositorySet,
    run: &LifecycleRun,
    agents: Vec<LifecycleAgent>,
    assignments: Vec<AgentAssignment>,
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

    let graph_instances = repos
        .workflow_graph_instance_repo
        .list_by_run(run.id)
        .await?;
    let workflow_graph_instances = workflow_graph_instances_for_run(&graph_instances, &assignments);
    let runtime_trace_refs = collect_runtime_trace_refs(repos, &agents).await?;

    Ok(assemble_lifecycle_run_view(
        run,
        agents.iter().map(lifecycle_agent_to_view).collect(),
        subject_associations
            .iter()
            .map(association_to_dto)
            .collect(),
        workflow_graph_instances,
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
    let mut latest_attempt: Option<ActivityAttemptView> = None;
    let mut artifacts = json!({});

    for run in &runs {
        let agents = repos.lifecycle_agent_repo.list_by_run(run.id).await?;
        let assignments = repos.agent_assignment_repo.list_by_run(run.id).await?;
        let graph_instances = repos
            .workflow_graph_instance_repo
            .list_by_run(run.id)
            .await?;

        if current_agent.is_none() {
            current_agent =
                select_current_agent(&associations, &agents).map(lifecycle_agent_to_view);
        }
        if latest_attempt.is_none() {
            latest_attempt = latest_attempt_view(&graph_instances, &assignments);
        }
        if artifacts == json!({}) {
            artifacts = graph_instances_outputs_json(&graph_instances);
        }

        run_views.push(
            build_lifecycle_run_view_with_preloaded(repos, run, agents, assignments).await?,
        );
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
    agents: &[LifecycleAgent],
) -> Result<Vec<RuntimeSessionRefDto>, DomainError> {
    let mut refs = Vec::new();
    for agent in agents {
        for frame in repos.agent_frame_repo.list_by_agent(agent.id).await? {
            for session_id in frame.runtime_session_ids() {
                if !refs
                    .iter()
                    .any(|item: &RuntimeSessionRefDto| item.runtime_session_id == session_id)
                {
                    refs.push(RuntimeSessionRefDto {
                        runtime_session_id: session_id,
                    });
                }
            }
        }
    }
    Ok(refs)
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
        created_at: agent.created_at.to_rfc3339(),
        updated_at: agent.updated_at.to_rfc3339(),
    }
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

fn executor_run_ref_to_dto(er: &DomainExecutorRunRef) -> ContractExecutorRunRef {
    match er {
        DomainExecutorRunRef::RuntimeSession { session_id } => {
            ContractExecutorRunRef::RuntimeSession {
                session_id: session_id.clone(),
            }
        }
        DomainExecutorRunRef::FunctionRun { run_id } => ContractExecutorRunRef::FunctionRun {
            run_id: run_id.clone(),
        },
        DomainExecutorRunRef::HumanDecision { decision_id } => {
            ContractExecutorRunRef::HumanDecision {
                decision_id: decision_id.clone(),
            }
        }
    }
}

fn assemble_lifecycle_run_view(
    run: &LifecycleRun,
    agents: Vec<LifecycleAgentView>,
    subject_associations: Vec<LifecycleSubjectAssociationDto>,
    workflow_graph_instances: Vec<WorkflowGraphInstanceView>,
    runtime_trace_refs: Vec<RuntimeSessionRefDto>,
) -> LifecycleRunView {
    LifecycleRunView {
        run_ref: LifecycleRunRefDto {
            run_id: run.id.to_string(),
        },
        project_id: run.project_id.to_string(),
        lifecycle_id: run.lifecycle_id.to_string(),
        status: status_to_dto(run.status),
        workflow_graph_instances,
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

fn workflow_graph_instances_for_run(
    graph_instances: &[WorkflowGraphInstance],
    assignments: &[AgentAssignment],
) -> Vec<WorkflowGraphInstanceView> {
    graph_instances
        .iter()
        .map(|instance| WorkflowGraphInstanceView {
            id: instance.id.to_string(),
            run_id: instance.run_id.to_string(),
            graph_id: instance.graph_id.to_string(),
            role: instance.role.clone(),
            status: instance.status.clone(),
            activities: instance
                .activity_state
                .as_ref()
                .map(|state| activity_state_views(instance.id, state, assignments))
                .unwrap_or_default(),
        })
        .collect()
}

fn activity_state_views(
    graph_instance_id: Uuid,
    activity_state: &ActivityLifecycleRunState,
    assignments: &[AgentAssignment],
) -> Vec<ActivityStateView> {
    let mut attempts_by_activity: BTreeMap<String, Vec<ActivityAttemptView>> = BTreeMap::new();
    for attempt in &activity_state.attempts {
        attempts_by_activity
            .entry(attempt.activity_key.clone())
            .or_default()
            .push(activity_attempt_to_view(
                graph_instance_id,
                attempt,
                assignments,
            ));
    }

    attempts_by_activity
        .into_iter()
        .map(|(activity_key, attempts)| ActivityStateView {
            activity_key,
            status: serialized_string(&activity_state.status),
            attempts,
        })
        .collect()
}

fn activity_attempt_to_view(
    graph_instance_id: Uuid,
    attempt: &agentdash_domain::workflow::ActivityAttemptState,
    assignments: &[AgentAssignment],
) -> ActivityAttemptView {
    let assignment = assignments.iter().find(|a| {
        a.graph_instance_id == graph_instance_id
            && a.activity_key == attempt.activity_key
            && a.attempt == attempt.attempt as i32
    });

    ActivityAttemptView {
        graph_instance_id: Some(graph_instance_id.to_string()),
        activity_key: attempt.activity_key.clone(),
        attempt: attempt.attempt,
        status: serialized_string(&attempt.status),
        assignment_ref: assignment.map(|a| AgentAssignmentRefDto {
            assignment_id: a.id.to_string(),
            run_id: Some(a.run_id.to_string()),
            agent_id: Some(a.agent_id.to_string()),
            frame_id: Some(a.frame_id.to_string()),
        }),
        executor_run_ref: attempt.executor_run.as_ref().map(executor_run_ref_to_dto),
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

fn latest_attempt_view(
    graph_instances: &[WorkflowGraphInstance],
    assignments: &[AgentAssignment],
) -> Option<ActivityAttemptView> {
    graph_instances
        .iter()
        .filter_map(|instance| {
            let state = instance.activity_state.as_ref()?;
            state
                .attempts
                .iter()
                .max_by_key(|a| (a.completed_at, a.started_at, a.attempt))
                .map(|a| (instance.id, a))
        })
        .max_by_key(|(_, a)| (a.completed_at, a.started_at, a.attempt))
        .map(|(graph_instance_id, attempt)| {
            activity_attempt_to_view(graph_instance_id, attempt, assignments)
        })
}

fn graph_instances_outputs_json(graph_instances: &[WorkflowGraphInstance]) -> Value {
    let outputs = graph_instances
        .iter()
        .filter_map(|instance| instance.activity_state.as_ref())
        .flat_map(|state| state.outputs.iter())
        .collect::<Vec<_>>();
    if outputs.is_empty() {
        return json!({});
    }
    serde_json::to_value(outputs).unwrap_or_else(|_| json!({}))
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

fn serialized_string(value: &impl Serialize) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| "unknown".to_string())
}
