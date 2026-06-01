use std::{collections::BTreeMap, sync::Arc};

use axum::{
    Json,
    extract::{Path, State},
};
use serde::Serialize;
use serde_json::{Value, json};
use uuid::Uuid;

use agentdash_contracts::workflow::{
    ActivityAttemptView, ActivityStateView, AgentAssignmentRefDto, AgentFrameRefDto,
    AgentFrameRuntimeView, LifecycleAgentRefDto, LifecycleAgentView, LifecycleExecutionEntry,
    LifecycleExecutionEventKind, LifecycleRunRefDto,
    LifecycleRunStatus as ContractLifecycleRunStatus, LifecycleRunView,
    LifecycleSubjectAssociationDto, RuntimeSessionRefDto, RuntimeSessionTraceView,
    SubjectExecutionView, SubjectRefDto, WorkflowGraphInstanceView,
};
use agentdash_domain::workflow::{
    ActivityLifecycleRunState, AgentAssignment, AgentFrame, LifecycleAgent,
    LifecycleExecutionEventKind as DomainLifecycleExecutionEventKind, LifecycleRun,
    LifecycleRunStatus as DomainLifecycleRunStatus, LifecycleSubjectAssociation, SubjectRef,
    WorkflowGraphInstance,
};

use crate::{
    app_state::AppState,
    auth::{
        CurrentUser, ProjectPermission, load_project_with_permission,
        load_story_and_project_with_permission, load_task_story_project_with_permission,
    },
    rpc::ApiError,
};

pub fn router() -> axum::Router<Arc<AppState>> {
    axum::Router::new()
        .route(
            "/lifecycle-runs/{id}/view",
            axum::routing::get(get_lifecycle_run_view),
        )
        .route(
            "/subjects/{kind}/{id}/execution",
            axum::routing::get(get_subject_execution),
        )
        .route(
            "/agent-frames/{id}/runtime",
            axum::routing::get(get_agent_frame_runtime),
        )
        .route(
            "/runtime-sessions/{id}/trace",
            axum::routing::get(get_runtime_session_trace),
        )
}

pub async fn get_lifecycle_run_view(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(run_id): Path<String>,
) -> Result<Json<LifecycleRunView>, ApiError> {
    let run_id = parse_uuid(&run_id, "run_id")?;
    let run = load_lifecycle_run(state.as_ref(), run_id).await?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        run.project_id,
        ProjectPermission::View,
    )
    .await?;

    Ok(Json(build_lifecycle_run_view(&state, &run).await?))
}

pub async fn get_subject_execution(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((kind, id)): Path<(String, String)>,
) -> Result<Json<SubjectExecutionView>, ApiError> {
    let subject = SubjectRef::new(kind, parse_uuid(&id, "subject_id")?);
    let view = build_subject_execution_view(&state, subject.clone()).await?;
    authorize_subject_execution_view(&state, &current_user, &subject, &view).await?;
    Ok(Json(view))
}

pub async fn get_agent_frame_runtime(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(frame_id): Path<String>,
) -> Result<Json<AgentFrameRuntimeView>, ApiError> {
    let frame_id = parse_uuid(&frame_id, "frame_id")?;
    let frame = state
        .repos
        .agent_frame_repo
        .get(frame_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("agent_frame 不存在: {frame_id}")))?;
    let agent = state
        .repos
        .lifecycle_agent_repo
        .get(frame.agent_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("lifecycle_agent 不存在: {}", frame.agent_id)))?;
    let run = load_lifecycle_run(state.as_ref(), agent.run_id).await?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        run.project_id,
        ProjectPermission::View,
    )
    .await?;

    Ok(Json(agent_frame_runtime_to_view(&frame)))
}

pub async fn get_runtime_session_trace(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(runtime_session_id): Path<String>,
) -> Result<Json<RuntimeSessionTraceView>, ApiError> {
    let frame = state
        .repos
        .agent_frame_repo
        .find_by_runtime_session(&runtime_session_id)
        .await?
        .ok_or_else(|| {
            ApiError::NotFound(format!(
                "runtime_session 未附着到 AgentFrame: {runtime_session_id}"
            ))
        })?;
    let agent = state
        .repos
        .lifecycle_agent_repo
        .get(frame.agent_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("lifecycle_agent 不存在: {}", frame.agent_id)))?;
    let run = load_lifecycle_run(state.as_ref(), agent.run_id).await?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        run.project_id,
        ProjectPermission::View,
    )
    .await?;

    let events = state
        .services
        .session_eventing
        .list_event_page(&runtime_session_id, 0, 200)
        .await?
        .events
        .into_iter()
        .filter_map(|event| serde_json::to_value(event).ok())
        .collect::<Vec<_>>();

    Ok(Json(RuntimeSessionTraceView {
        runtime_session_ref: RuntimeSessionRefDto { runtime_session_id },
        frame_ref: Some(agent_frame_ref(&frame)),
        events,
        turns: Vec::new(),
    }))
}

async fn build_subject_execution_view(
    state: &Arc<AppState>,
    subject: SubjectRef,
) -> Result<SubjectExecutionView, ApiError> {
    let associations = state
        .repos
        .lifecycle_subject_association_repo
        .list_by_subject(&subject)
        .await?;
    let run_ids = unique_run_ids(&associations);
    let runs = state.repos.lifecycle_run_repo.list_by_ids(&run_ids).await?;

    let mut run_views = Vec::with_capacity(runs.len());
    let mut current_agent: Option<LifecycleAgentView> = None;
    let mut latest_attempt: Option<ActivityAttemptView> = None;
    let mut artifacts = json!({});

    for run in &runs {
        let agents = state.repos.lifecycle_agent_repo.list_by_run(run.id).await?;
        let assignments = state
            .repos
            .agent_assignment_repo
            .list_by_run(run.id)
            .await?;

        if current_agent.is_none() {
            current_agent =
                select_current_agent(&associations, &agents).map(lifecycle_agent_to_view);
        }
        if latest_attempt.is_none() {
            latest_attempt = latest_attempt_view(run, &assignments);
        }
        if artifacts == json!({}) {
            artifacts = run
                .activity_state
                .as_ref()
                .and_then(|state| serde_json::to_value(&state.outputs).ok())
                .unwrap_or_else(|| json!({}));
        }

        run_views.push(build_lifecycle_run_view_with_facts(state, run, agents, assignments).await?);
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

async fn build_lifecycle_run_view(
    state: &Arc<AppState>,
    run: &LifecycleRun,
) -> Result<LifecycleRunView, ApiError> {
    let agents = state.repos.lifecycle_agent_repo.list_by_run(run.id).await?;
    let assignments = state
        .repos
        .agent_assignment_repo
        .list_by_run(run.id)
        .await?;
    build_lifecycle_run_view_with_facts(state, run, agents, assignments).await
}

async fn build_lifecycle_run_view_with_facts(
    state: &Arc<AppState>,
    run: &LifecycleRun,
    agents: Vec<LifecycleAgent>,
    assignments: Vec<AgentAssignment>,
) -> Result<LifecycleRunView, ApiError> {
    let mut subject_associations = Vec::new();
    subject_associations.extend(
        state
            .repos
            .lifecycle_subject_association_repo
            .list_by_anchor(run.id, None)
            .await?,
    );
    for agent in &agents {
        subject_associations.extend(
            state
                .repos
                .lifecycle_subject_association_repo
                .list_by_anchor(run.id, Some(agent.id))
                .await?,
        );
    }

    let graph_instances = state
        .repos
        .workflow_graph_instance_repo
        .list_by_run(run.id)
        .await?;
    let workflow_graph_instances =
        workflow_graph_instances_for_run(run, &graph_instances, &assignments);
    let runtime_trace_refs = runtime_trace_refs_for_agents(state, &agents).await?;

    Ok(lifecycle_run_to_view(
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

async fn runtime_trace_refs_for_agents(
    state: &Arc<AppState>,
    agents: &[LifecycleAgent],
) -> Result<Vec<RuntimeSessionRefDto>, ApiError> {
    let mut refs = Vec::new();
    for agent in agents {
        for frame in state.repos.agent_frame_repo.list_by_agent(agent.id).await? {
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

async fn authorize_subject_execution_view(
    state: &Arc<AppState>,
    current_user: &agentdash_plugin_api::AuthIdentity,
    subject: &SubjectRef,
    view: &SubjectExecutionView,
) -> Result<(), ApiError> {
    if let Some(project_id) = view
        .runs
        .first()
        .and_then(|run| Uuid::parse_str(&run.project_id).ok())
    {
        load_project_with_permission(state, current_user, project_id, ProjectPermission::View)
            .await?;
        return Ok(());
    }

    match subject.kind.as_str() {
        "project" => {
            load_project_with_permission(state, current_user, subject.id, ProjectPermission::View)
                .await?;
            Ok(())
        }
        "story" => {
            load_story_and_project_with_permission(
                state,
                current_user,
                subject.id,
                ProjectPermission::View,
            )
            .await?;
            Ok(())
        }
        "task" => {
            load_task_story_project_with_permission(
                state,
                current_user,
                subject.id,
                ProjectPermission::View,
            )
            .await?;
            Ok(())
        }
        "lifecycle_run" => {
            let run = load_lifecycle_run(state, subject.id).await?;
            load_project_with_permission(
                state,
                current_user,
                run.project_id,
                ProjectPermission::View,
            )
            .await?;
            Ok(())
        }
        _ => Err(ApiError::NotFound(format!(
            "subject 没有关联 lifecycle execution: {}/{}",
            subject.kind, subject.id
        ))),
    }
}

async fn load_lifecycle_run(state: &AppState, run_id: Uuid) -> Result<LifecycleRun, ApiError> {
    state
        .repos
        .lifecycle_run_repo
        .get_by_id(run_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("lifecycle_run 不存在: {run_id}")))
}

fn parse_uuid(raw: &str, field: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(raw).map_err(|_| ApiError::BadRequest(format!("无效的 {field}: {raw}")))
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

fn subject_ref_to_dto(subject: &SubjectRef) -> SubjectRefDto {
    SubjectRefDto {
        kind: subject.kind.clone(),
        id: subject.id.to_string(),
    }
}

fn association_to_dto(association: &LifecycleSubjectAssociation) -> LifecycleSubjectAssociationDto {
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

fn lifecycle_agent_to_view(agent: &LifecycleAgent) -> LifecycleAgentView {
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

fn lifecycle_run_to_view(
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
    run: &LifecycleRun,
    graph_instances: &[WorkflowGraphInstance],
    assignments: &[AgentAssignment],
) -> Vec<WorkflowGraphInstanceView> {
    if !graph_instances.is_empty() {
        return graph_instances
            .iter()
            .map(|instance| {
                let state = instance
                    .activity_state_json
                    .as_ref()
                    .and_then(|value| {
                        serde_json::from_value::<ActivityLifecycleRunState>(value.clone()).ok()
                    })
                    .or_else(|| run.activity_state.clone());
                WorkflowGraphInstanceView {
                    id: instance.id.to_string(),
                    run_id: instance.run_id.to_string(),
                    graph_id: instance.graph_id.to_string(),
                    role: instance.role.clone(),
                    status: instance.status.clone(),
                    activities: state
                        .as_ref()
                        .map(|state| activity_state_views(state, assignments))
                        .unwrap_or_default(),
                }
            })
            .collect();
    }

    let Some(activity_state) = &run.activity_state else {
        return Vec::new();
    };

    vec![WorkflowGraphInstanceView {
        id: run.lifecycle_id.to_string(),
        run_id: run.id.to_string(),
        graph_id: run.lifecycle_id.to_string(),
        role: "root".to_string(),
        status: serialized_string(&activity_state.status),
        activities: activity_state_views(activity_state, assignments),
    }]
}

fn activity_state_views(
    activity_state: &ActivityLifecycleRunState,
    assignments: &[AgentAssignment],
) -> Vec<ActivityStateView> {
    let mut attempts_by_activity: BTreeMap<String, Vec<ActivityAttemptView>> = BTreeMap::new();
    for attempt in &activity_state.attempts {
        attempts_by_activity
            .entry(attempt.activity_key.clone())
            .or_default()
            .push(activity_attempt_to_view(attempt, assignments));
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

fn execution_entry_to_dto(
    entry: &agentdash_domain::workflow::LifecycleExecutionEntry,
) -> LifecycleExecutionEntry {
    LifecycleExecutionEntry {
        timestamp: entry.timestamp,
        activity_key: entry.activity_key.clone(),
        event_kind: execution_event_kind_to_dto(entry.event_kind),
        summary: entry.summary.clone(),
        detail: entry.detail.clone(),
    }
}

fn execution_event_kind_to_dto(
    kind: DomainLifecycleExecutionEventKind,
) -> LifecycleExecutionEventKind {
    match kind {
        DomainLifecycleExecutionEventKind::ActivityActivated => {
            LifecycleExecutionEventKind::ActivityActivated
        }
        DomainLifecycleExecutionEventKind::ActivityCompleted => {
            LifecycleExecutionEventKind::ActivityCompleted
        }
        DomainLifecycleExecutionEventKind::ConstraintBlocked => {
            LifecycleExecutionEventKind::ConstraintBlocked
        }
        DomainLifecycleExecutionEventKind::CompletionEvaluated => {
            LifecycleExecutionEventKind::CompletionEvaluated
        }
        DomainLifecycleExecutionEventKind::ArtifactAppended => {
            LifecycleExecutionEventKind::ArtifactAppended
        }
        DomainLifecycleExecutionEventKind::ContextInjected => {
            LifecycleExecutionEventKind::ContextInjected
        }
    }
}

fn latest_attempt_view(
    run: &LifecycleRun,
    assignments: &[AgentAssignment],
) -> Option<ActivityAttemptView> {
    let activity_state = run.activity_state.as_ref()?;
    activity_state
        .attempts
        .iter()
        .max_by_key(|attempt| (attempt.completed_at, attempt.started_at, attempt.attempt))
        .map(|attempt| activity_attempt_to_view(attempt, assignments))
}

fn activity_attempt_to_view(
    attempt: &agentdash_domain::workflow::ActivityAttemptState,
    assignments: &[AgentAssignment],
) -> ActivityAttemptView {
    let assignment = assignments.iter().find(|assignment| {
        assignment.activity_key == attempt.activity_key
            && assignment.attempt == attempt.attempt as i32
    });

    ActivityAttemptView {
        graph_instance_id: assignment.map(|assignment| assignment.graph_instance_id.to_string()),
        activity_key: attempt.activity_key.clone(),
        attempt: attempt.attempt,
        status: serialized_string(&attempt.status),
        assignment_ref: assignment.map(|assignment| AgentAssignmentRefDto {
            assignment_id: assignment.id.to_string(),
            run_id: Some(assignment.run_id.to_string()),
            agent_id: Some(assignment.agent_id.to_string()),
            frame_id: Some(assignment.frame_id.to_string()),
        }),
        executor_run_ref: attempt
            .executor_run
            .as_ref()
            .and_then(|executor_run| serde_json::to_value(executor_run).ok()),
    }
}

fn select_current_agent<'a>(
    associations: &[LifecycleSubjectAssociation],
    agents: &'a [LifecycleAgent],
) -> Option<&'a LifecycleAgent> {
    associations
        .iter()
        .find_map(|association| association.anchor_agent_id)
        .and_then(|agent_id| agents.iter().find(|agent| agent.id == agent_id))
        .or_else(|| agents.iter().find(|agent| agent.status == "active"))
        .or_else(|| agents.first())
}

fn agent_frame_runtime_to_view(frame: &AgentFrame) -> AgentFrameRuntimeView {
    AgentFrameRuntimeView {
        frame_ref: agent_frame_ref(frame),
        procedure_id: frame.procedure_id.map(|id| id.to_string()),
        graph_instance_id: frame.graph_instance_id.map(|id| id.to_string()),
        activity_key: frame.activity_key.clone(),
        capability_surface: frame
            .effective_capability_json
            .clone()
            .unwrap_or(Value::Null),
        context_slice: frame.context_slice_json.clone().unwrap_or(Value::Null),
        vfs_surface: frame.vfs_surface_json.clone().unwrap_or(Value::Null),
        mcp_surface: frame.mcp_surface_json.clone().unwrap_or(Value::Null),
        runtime_session_refs: frame
            .runtime_session_ids()
            .into_iter()
            .map(|runtime_session_id| RuntimeSessionRefDto { runtime_session_id })
            .collect(),
        execution_profile: frame.execution_profile_json.clone(),
    }
}

fn agent_frame_ref(frame: &AgentFrame) -> AgentFrameRefDto {
    AgentFrameRefDto {
        agent_id: frame.agent_id.to_string(),
        frame_id: frame.id.to_string(),
        revision: Some(frame.revision),
    }
}

fn serialized_string(value: &impl Serialize) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| "unknown".to_string())
}
