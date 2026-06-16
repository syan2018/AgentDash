use std::sync::Arc;

use agentdash_application::task::plan::{
    RunTaskPlanFilter, archive_run_task, create_run_task, list_run_tasks,
    transition_run_task_status, update_run_task,
};
use agentdash_contracts::context as contract_context;
use agentdash_contracts::task::{
    CreateRunTaskRequest, RunTaskCommandResponse, RunTaskPlanResponse, TaskPlanStatus,
    TaskPriority, TaskResponse, UpdateRunTaskRequest, UpdateRunTaskStatusRequest,
};
use agentdash_contracts::workflow::SubjectRefDto;
use agentdash_domain::context_source::{
    ContextDelivery, ContextSlot, ContextSourceKind, ContextSourceRef,
};
use agentdash_domain::workflow::{
    LifecycleRun, LifecycleTaskPlanItemDraft, LifecycleTaskPlanItemPatch, SubjectRef,
};
use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    app_state::AppState,
    auth::{CurrentUser, ProjectPermission, load_project_with_permission},
    rpc::ApiError,
};

pub fn router() -> axum::Router<Arc<AppState>> {
    axum::Router::new()
        .route(
            "/lifecycle-runs/{run_id}/tasks",
            axum::routing::get(get_run_tasks).post(create_run_task_route),
        )
        .route(
            "/lifecycle-runs/{run_id}/tasks/{task_id}",
            axum::routing::patch(update_run_task_route),
        )
        .route(
            "/lifecycle-runs/{run_id}/tasks/{task_id}/status",
            axum::routing::patch(update_run_task_status_route),
        )
        .route(
            "/lifecycle-runs/{run_id}/tasks/{task_id}/archive",
            axum::routing::post(archive_run_task_route),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/tasks",
            axum::routing::get(get_agent_run_tasks).post(create_agent_run_task_route),
        )
}

#[derive(Debug, Deserialize)]
pub struct RunTaskPlanQuery {
    pub created_by_agent_id: Option<String>,
    pub owner_agent_id: Option<String>,
    pub assigned_agent_id: Option<String>,
    #[serde(default)]
    pub include_archived: bool,
}

pub async fn get_run_tasks(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(run_id): Path<String>,
    Query(query): Query<RunTaskPlanQuery>,
) -> Result<Json<RunTaskPlanResponse>, ApiError> {
    let run = load_authorized_run(&state, &current_user, &run_id, ProjectPermission::View).await?;
    let view = list_run_tasks(
        state.repos.lifecycle_run_repo.as_ref(),
        run.id,
        filter_from_query(query)?,
    )
    .await?;
    Ok(Json(run_plan_response(view)))
}

pub async fn create_run_task_route(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(run_id): Path<String>,
    Json(req): Json<CreateRunTaskRequest>,
) -> Result<Json<RunTaskCommandResponse>, ApiError> {
    let run = load_authorized_run(&state, &current_user, &run_id, ProjectPermission::Edit).await?;
    let result = create_run_task(
        state.repos.lifecycle_run_repo.as_ref(),
        run.id,
        draft_from_request(req, None)?,
    )
    .await?;
    Ok(Json(command_response(result)))
}

pub async fn update_run_task_route(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, task_id)): Path<(String, String)>,
    Json(req): Json<UpdateRunTaskRequest>,
) -> Result<Json<RunTaskCommandResponse>, ApiError> {
    let run = load_authorized_run(&state, &current_user, &run_id, ProjectPermission::Edit).await?;
    let task_id = parse_uuid(&task_id, "task_id")?;
    let result = update_run_task(
        state.repos.lifecycle_run_repo.as_ref(),
        run.id,
        task_id,
        patch_from_request(req)?,
    )
    .await?;
    Ok(Json(command_response(result)))
}

pub async fn update_run_task_status_route(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, task_id)): Path<(String, String)>,
    Json(req): Json<UpdateRunTaskStatusRequest>,
) -> Result<Json<RunTaskCommandResponse>, ApiError> {
    let run = load_authorized_run(&state, &current_user, &run_id, ProjectPermission::Edit).await?;
    let task_id = parse_uuid(&task_id, "task_id")?;
    let result = transition_run_task_status(
        state.repos.lifecycle_run_repo.as_ref(),
        run.id,
        task_id,
        domain_status(req.status),
    )
    .await?;
    Ok(Json(command_response(result)))
}

pub async fn archive_run_task_route(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, task_id)): Path<(String, String)>,
) -> Result<Json<RunTaskCommandResponse>, ApiError> {
    let run = load_authorized_run(&state, &current_user, &run_id, ProjectPermission::Edit).await?;
    let task_id = parse_uuid(&task_id, "task_id")?;
    let result = archive_run_task(state.repos.lifecycle_run_repo.as_ref(), run.id, task_id).await?;
    Ok(Json(command_response(result)))
}

pub async fn get_agent_run_tasks(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id)): Path<(String, String)>,
    Query(mut query): Query<RunTaskPlanQuery>,
) -> Result<Json<RunTaskPlanResponse>, ApiError> {
    let (run, agent_id) = load_authorized_agent_run(
        &state,
        &current_user,
        &run_id,
        &agent_id,
        ProjectPermission::View,
    )
    .await?;
    query
        .owner_agent_id
        .get_or_insert_with(|| agent_id.to_string());
    let view = list_run_tasks(
        state.repos.lifecycle_run_repo.as_ref(),
        run.id,
        filter_from_query(query)?,
    )
    .await?;
    Ok(Json(run_plan_response(view)))
}

pub async fn create_agent_run_task_route(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id)): Path<(String, String)>,
    Json(req): Json<CreateRunTaskRequest>,
) -> Result<Json<RunTaskCommandResponse>, ApiError> {
    let (run, agent_id) = load_authorized_agent_run(
        &state,
        &current_user,
        &run_id,
        &agent_id,
        ProjectPermission::Edit,
    )
    .await?;
    let result = create_run_task(
        state.repos.lifecycle_run_repo.as_ref(),
        run.id,
        draft_from_request(req, Some(agent_id))?,
    )
    .await?;
    Ok(Json(command_response(result)))
}

async fn load_authorized_run(
    state: &AppState,
    current_user: &agentdash_integration_api::AuthIdentity,
    run_id: &str,
    permission: ProjectPermission,
) -> Result<LifecycleRun, ApiError> {
    let run_id = parse_uuid(run_id, "run_id")?;
    let run = state
        .repos
        .lifecycle_run_repo
        .get_by_id(run_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("LifecycleRun {run_id} 不存在")))?;
    load_project_with_permission(state, current_user, run.project_id, permission).await?;
    Ok(run)
}

async fn load_authorized_agent_run(
    state: &AppState,
    current_user: &agentdash_integration_api::AuthIdentity,
    run_id: &str,
    agent_id: &str,
    permission: ProjectPermission,
) -> Result<(LifecycleRun, Uuid), ApiError> {
    let run = load_authorized_run(state, current_user, run_id, permission).await?;
    let agent_id = parse_uuid(agent_id, "agent_id")?;
    let agent = state
        .repos
        .lifecycle_agent_repo
        .get(agent_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("LifecycleAgent {agent_id} 不存在")))?;
    if agent.run_id != run.id || agent.project_id != run.project_id {
        return Err(ApiError::Conflict(
            "agent_id 与 run_id 不属于同一 AgentRun".to_string(),
        ));
    }
    Ok((run, agent_id))
}

fn filter_from_query(query: RunTaskPlanQuery) -> Result<RunTaskPlanFilter, ApiError> {
    Ok(RunTaskPlanFilter {
        created_by_agent_id: parse_optional_uuid(query.created_by_agent_id, "created_by_agent_id")?,
        owner_agent_id: parse_optional_uuid(query.owner_agent_id, "owner_agent_id")?,
        assigned_agent_id: parse_optional_uuid(query.assigned_agent_id, "assigned_agent_id")?,
        include_archived: query.include_archived,
    })
}

fn draft_from_request(
    req: CreateRunTaskRequest,
    agent_scope_id: Option<Uuid>,
) -> Result<LifecycleTaskPlanItemDraft, ApiError> {
    let title = req.title.trim();
    if title.is_empty() {
        return Err(ApiError::BadRequest("Task 标题不能为空".into()));
    }
    Ok(LifecycleTaskPlanItemDraft {
        id: None,
        title: title.to_string(),
        body: req.body,
        status: req.status.map(domain_status).unwrap_or_default(),
        priority: req.priority.map(domain_priority),
        created_by_agent_id: parse_optional_uuid(req.created_by_agent_id, "created_by_agent_id")?
            .or(agent_scope_id),
        owner_agent_id: parse_optional_uuid(req.owner_agent_id, "owner_agent_id")?
            .or(agent_scope_id),
        assigned_agent_id: parse_optional_uuid(req.assigned_agent_id, "assigned_agent_id")?,
        source_task_id: parse_optional_uuid(req.source_task_id, "source_task_id")?,
        context_refs: req
            .context_refs
            .into_iter()
            .map(domain_context_source_ref)
            .collect(),
        story_ref: req.story_ref.map(domain_subject_ref).transpose()?,
    })
}

fn patch_from_request(req: UpdateRunTaskRequest) -> Result<LifecycleTaskPlanItemPatch, ApiError> {
    let title = req
        .title
        .map(|title| {
            let title = title.trim();
            if title.is_empty() {
                Err(ApiError::BadRequest("Task 标题不能为空".into()))
            } else {
                Ok(title.to_string())
            }
        })
        .transpose()?;
    Ok(LifecycleTaskPlanItemPatch {
        title,
        body: req.body,
        priority: req.priority.map(|value| value.map(domain_priority)),
        owner_agent_id: req
            .owner_agent_id
            .map(|value| {
                value
                    .map(|id| parse_uuid(&id, "owner_agent_id"))
                    .transpose()
            })
            .transpose()?,
        assigned_agent_id: req
            .assigned_agent_id
            .map(|value| {
                value
                    .map(|id| parse_uuid(&id, "assigned_agent_id"))
                    .transpose()
            })
            .transpose()?,
        source_task_id: req
            .source_task_id
            .map(|value| {
                value
                    .map(|id| parse_uuid(&id, "source_task_id"))
                    .transpose()
            })
            .transpose()?,
        context_refs: req.context_refs.map(|refs| {
            refs.into_iter()
                .map(domain_context_source_ref)
                .collect::<Vec<_>>()
        }),
        story_ref: req
            .story_ref
            .map(|value| value.map(domain_subject_ref).transpose())
            .transpose()?,
    })
}

fn run_plan_response(
    view: agentdash_application::task::plan::RunTaskPlanView,
) -> RunTaskPlanResponse {
    RunTaskPlanResponse {
        project_id: view.project_id.to_string(),
        run_id: view.run_id.to_string(),
        tasks: view
            .tasks
            .into_iter()
            .map(|task| {
                TaskResponse::from_plan_item(
                    view.project_id.to_string(),
                    view.run_id.to_string(),
                    task,
                )
            })
            .collect(),
    }
}

fn command_response(
    result: agentdash_application::task::plan::RunTaskCommandResult,
) -> RunTaskCommandResponse {
    RunTaskCommandResponse {
        project_id: result.project_id.to_string(),
        run_id: result.run_id.to_string(),
        task: TaskResponse::from_plan_item(
            result.project_id.to_string(),
            result.run_id.to_string(),
            result.task,
        ),
    }
}

fn parse_uuid(raw: &str, field: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(raw).map_err(|_| ApiError::BadRequest(format!("无效的 {field}: {raw}")))
}

fn parse_optional_uuid(raw: Option<String>, field: &str) -> Result<Option<Uuid>, ApiError> {
    raw.map(|value| parse_uuid(value.trim(), field)).transpose()
}

fn domain_subject_ref(value: SubjectRefDto) -> Result<SubjectRef, ApiError> {
    Ok(SubjectRef::new(
        value.kind,
        parse_uuid(&value.id, "subject_ref.id")?,
    ))
}

fn domain_status(value: TaskPlanStatus) -> agentdash_domain::workflow::TaskPlanStatus {
    match value {
        TaskPlanStatus::Open => agentdash_domain::workflow::TaskPlanStatus::Open,
        TaskPlanStatus::Active => agentdash_domain::workflow::TaskPlanStatus::Active,
        TaskPlanStatus::Review => agentdash_domain::workflow::TaskPlanStatus::Review,
        TaskPlanStatus::Blocked => agentdash_domain::workflow::TaskPlanStatus::Blocked,
        TaskPlanStatus::Done => agentdash_domain::workflow::TaskPlanStatus::Done,
        TaskPlanStatus::Dropped => agentdash_domain::workflow::TaskPlanStatus::Dropped,
    }
}

fn domain_priority(value: TaskPriority) -> agentdash_domain::workflow::TaskPriority {
    match value {
        TaskPriority::P0 => agentdash_domain::workflow::TaskPriority::P0,
        TaskPriority::P1 => agentdash_domain::workflow::TaskPriority::P1,
        TaskPriority::P2 => agentdash_domain::workflow::TaskPriority::P2,
        TaskPriority::P3 => agentdash_domain::workflow::TaskPriority::P3,
    }
}

fn domain_context_source_ref(value: contract_context::ContextSourceRef) -> ContextSourceRef {
    ContextSourceRef {
        kind: match value.kind {
            contract_context::ContextSourceKind::ManualText => ContextSourceKind::ManualText,
            contract_context::ContextSourceKind::File => ContextSourceKind::File,
            contract_context::ContextSourceKind::ProjectSnapshot => {
                ContextSourceKind::ProjectSnapshot
            }
            contract_context::ContextSourceKind::HttpFetch => ContextSourceKind::HttpFetch,
            contract_context::ContextSourceKind::McpResource => ContextSourceKind::McpResource,
            contract_context::ContextSourceKind::EntityRef => ContextSourceKind::EntityRef,
        },
        locator: value.locator,
        label: value.label,
        slot: match value.slot {
            contract_context::ContextSlot::Requirements => ContextSlot::Requirements,
            contract_context::ContextSlot::Constraints => ContextSlot::Constraints,
            contract_context::ContextSlot::Codebase => ContextSlot::Codebase,
            contract_context::ContextSlot::References => ContextSlot::References,
            contract_context::ContextSlot::InstructionAppend => ContextSlot::InstructionAppend,
        },
        priority: value.priority,
        required: value.required,
        max_chars: value.max_chars,
        delivery: match value.delivery {
            contract_context::ContextDelivery::Inline => ContextDelivery::Inline,
            contract_context::ContextDelivery::Resource => ContextDelivery::Resource,
            contract_context::ContextDelivery::Lazy => ContextDelivery::Lazy,
        },
    }
}
