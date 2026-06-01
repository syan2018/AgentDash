use std::{collections::BTreeMap, sync::Arc};

use axum::{
    Json,
    extract::{Path, State},
};
use serde::Serialize;
use serde_json::json;
use uuid::Uuid;

use agentdash_contracts::workflow::{
    ActivityAttemptView, ActivityStateView, AgentAssignmentRefDto, LifecycleAgentRefDto,
    LifecycleAgentView, LifecycleExecutionEntry, LifecycleExecutionEventKind, LifecycleRunRefDto,
    LifecycleRunStatus as ContractLifecycleRunStatus, LifecycleRunView,
    LifecycleSubjectAssociationDto, SubjectExecutionView, SubjectRefDto, WorkflowGraphInstanceView,
};
use agentdash_domain::workflow::{
    AgentAssignment, LifecycleAgent,
    LifecycleExecutionEventKind as DomainLifecycleExecutionEventKind, LifecycleRun,
    LifecycleRunStatus as DomainLifecycleRunStatus, LifecycleSubjectAssociation, SubjectRef,
};

use crate::{
    app_state::AppState,
    auth::{CurrentUser, ProjectPermission, load_story_and_project_with_permission},
    rpc::ApiError,
};

pub fn router() -> axum::Router<Arc<AppState>> {
    axum::Router::new()
        .route("/stories/{id}/runs", axum::routing::get(list_story_runs))
        .route(
            "/stories/{id}/runs/active",
            axum::routing::get(get_active_story_run),
        )
}

/// GET /stories/{story_id}/runs
///
/// 返回 Story 对应的 SubjectExecutionView。旧 StoryRunOverview/run-link shape
/// 已从 public contract 删除；这里保留 route 入口，响应体切换为目标投影。
pub async fn list_story_runs(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(story_id): Path<String>,
) -> Result<Json<SubjectExecutionView>, ApiError> {
    let story_uuid = parse_story_id(&story_id)?;
    load_story_and_project_with_permission(
        state.as_ref(),
        &current_user,
        story_uuid,
        ProjectPermission::View,
    )
    .await?;

    let subject = SubjectRef::new("story", story_uuid);
    let view = build_subject_execution_view(&state, subject).await?;
    Ok(Json(view))
}

/// GET /stories/{story_id}/runs/active
///
/// 返回 Story 当前活跃执行投影；无 active run 时返回 null。
pub async fn get_active_story_run(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(story_id): Path<String>,
) -> Result<Json<Option<SubjectExecutionView>>, ApiError> {
    let story_uuid = parse_story_id(&story_id)?;
    load_story_and_project_with_permission(
        state.as_ref(),
        &current_user,
        story_uuid,
        ProjectPermission::View,
    )
    .await?;

    let subject = SubjectRef::new("story", story_uuid);
    let view = build_subject_execution_view(&state, subject).await?;
    let has_active_run = view.runs.iter().any(|run| {
        matches!(
            run.status,
            ContractLifecycleRunStatus::Ready | ContractLifecycleRunStatus::Running
        )
    });

    if has_active_run {
        Ok(Json(Some(view)))
    } else {
        Ok(Json(None))
    }
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
        let run_associations = associations
            .iter()
            .filter(|association| association.anchor_run_id == run.id)
            .cloned()
            .collect::<Vec<_>>();
        let agent_views = agents
            .iter()
            .map(lifecycle_agent_to_view)
            .collect::<Vec<_>>();

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

        run_views.push(lifecycle_run_to_view(
            run,
            agent_views,
            run_associations.iter().map(association_to_dto).collect(),
            workflow_graph_instances_for_run(run, &assignments),
        ));
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

fn parse_story_id(story_id: &str) -> Result<Uuid, ApiError> {
    story_id
        .parse()
        .map_err(|_| ApiError::BadRequest(format!("无效的 story_id: {story_id}")))
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
        runtime_trace_refs: Vec::new(),
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
    assignments: &[AgentAssignment],
) -> Vec<WorkflowGraphInstanceView> {
    let Some(activity_state) = &run.activity_state else {
        return Vec::new();
    };

    let mut attempts_by_activity: BTreeMap<String, Vec<ActivityAttemptView>> = BTreeMap::new();
    for attempt in &activity_state.attempts {
        attempts_by_activity
            .entry(attempt.activity_key.clone())
            .or_default()
            .push(activity_attempt_to_view(attempt, assignments));
    }

    let activities = attempts_by_activity
        .into_iter()
        .map(|(activity_key, attempts)| ActivityStateView {
            activity_key,
            status: serialized_string(&activity_state.status),
            attempts,
        })
        .collect();

    vec![WorkflowGraphInstanceView {
        id: run.lifecycle_id.to_string(),
        run_id: run.id.to_string(),
        graph_id: run.lifecycle_id.to_string(),
        role: "root".to_string(),
        status: serialized_string(&activity_state.status),
        activities,
    }]
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

fn serialized_string(value: &impl Serialize) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| "unknown".to_string())
}
