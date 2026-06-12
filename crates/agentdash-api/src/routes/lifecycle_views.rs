use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, State},
};
use serde_json::Value;
use uuid::Uuid;

use agentdash_application::workflow::lifecycle_run_view_builder::{
    self, SubjectExecutionView as SubjectExecutionReadModel,
};
use agentdash_application::workflow::{AgentFrameSurfaceExt, ConversationModelConfigResolver};
use agentdash_contracts::workflow::{
    AgentFrameRefDto, AgentFrameRuntimeView, ConversationModelConfigSource, LifecycleRunView,
    ProjectActiveAgentsView, RuntimeSessionRefDto, RuntimeSessionTraceView, SubjectExecutionView,
};
use agentdash_domain::workflow::{
    AgentFrame, LifecycleRun, RuntimeSessionExecutionAnchor, SubjectRef,
};

use crate::{
    app_state::AppState,
    auth::{
        CurrentUser, ProjectPermission, load_project_with_permission,
        load_story_and_project_with_permission, load_task_story_project_with_permission,
    },
    rpc::ApiError,
};

use super::lifecycle_contracts::{
    lifecycle_run_view_to_contract, project_active_agents_view_to_contract,
    subject_execution_view_to_contract,
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
            "/sessions/{id}/trace",
            axum::routing::get(get_session_trace),
        )
        .route(
            "/projects/{id}/active-agents",
            axum::routing::get(get_project_active_agents),
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

    let view = lifecycle_run_view_builder::build_lifecycle_run_view(&state.repos, &run).await?;
    Ok(Json(lifecycle_run_view_to_contract(view)))
}

pub async fn get_subject_execution(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((kind, id)): Path<(String, String)>,
) -> Result<Json<SubjectExecutionView>, ApiError> {
    let subject = SubjectRef::new(kind, parse_uuid(&id, "subject_id")?);
    let view =
        lifecycle_run_view_builder::build_subject_execution_view(&state.repos, subject.clone())
            .await?;
    authorize_subject_execution_view(&state, &current_user, &subject, &view).await?;
    Ok(Json(subject_execution_view_to_contract(view)))
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

    let runtime_refs = runtime_refs_for_agent(state.as_ref(), frame.agent_id).await?;
    Ok(Json(agent_frame_runtime_to_view(&frame, runtime_refs)))
}

pub async fn get_session_trace(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(runtime_session_id): Path<String>,
) -> Result<Json<RuntimeSessionTraceView>, ApiError> {
    let session_project_id =
        authorize_runtime_session_shell(state.as_ref(), &current_user, &runtime_session_id).await?;
    let frame = match state
        .repos
        .execution_anchor_repo
        .find_by_session(&runtime_session_id)
        .await?
    {
        Some(anchor) => Some(
            resolve_frame_from_anchor(
                state.as_ref(),
                &anchor,
                session_project_id,
                &runtime_session_id,
            )
            .await?,
        ),
        None => None,
    };

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
        frame_ref: frame.as_ref().map(agent_frame_ref),
        events,
        turns: Vec::new(),
    }))
}

pub async fn get_project_active_agents(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<ProjectActiveAgentsView>, ApiError> {
    let project_id = parse_uuid(&id, "project_id")?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::View,
    )
    .await?;

    let view =
        lifecycle_run_view_builder::build_project_active_agents_view(&state.repos, project_id)
            .await?;
    Ok(Json(project_active_agents_view_to_contract(view)))
}

async fn authorize_subject_execution_view(
    state: &Arc<AppState>,
    current_user: &agentdash_integration_api::AuthIdentity,
    subject: &SubjectRef,
    view: &SubjectExecutionReadModel,
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

async fn authorize_runtime_session_shell(
    state: &AppState,
    current_user: &agentdash_integration_api::AuthIdentity,
    runtime_session_id: &str,
) -> Result<Uuid, ApiError> {
    let _meta = state
        .services
        .session_core
        .get_session_meta(runtime_session_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("会话 {runtime_session_id} 不存在")))?;
    let anchor = state
        .repos
        .execution_anchor_repo
        .find_by_session(runtime_session_id)
        .await?
        .ok_or_else(|| {
            ApiError::BadRequest(format!(
                "runtime session 缺少 RuntimeSessionExecutionAnchor: {runtime_session_id}"
            ))
        })?;
    let run = load_lifecycle_run(state, anchor.run_id).await?;
    let project_id = run.project_id;
    load_project_with_permission(state, current_user, project_id, ProjectPermission::View).await?;
    Ok(project_id)
}

async fn resolve_frame_from_anchor(
    state: &AppState,
    anchor: &RuntimeSessionExecutionAnchor,
    session_project_id: Uuid,
    runtime_session_id: &str,
) -> Result<AgentFrame, ApiError> {
    let run = load_lifecycle_run(state, anchor.run_id).await?;
    if run.project_id != session_project_id {
        return Err(ApiError::BadRequest(format!(
            "runtime session project 与 anchor run project 不一致: {runtime_session_id}"
        )));
    }
    let agent = state
        .repos
        .lifecycle_agent_repo
        .get(anchor.agent_id)
        .await?
        .ok_or_else(|| {
            ApiError::NotFound(format!("lifecycle_agent 不存在: {}", anchor.agent_id))
        })?;
    if agent.run_id != run.id || agent.project_id != run.project_id {
        return Err(ApiError::BadRequest(format!(
            "runtime session anchor agent 与 run 不一致: {runtime_session_id}"
        )));
    }
    state
        .repos
        .agent_frame_repo
        .get_current(agent.id)
        .await?
        .or(state
            .repos
            .agent_frame_repo
            .get(anchor.launch_frame_id)
            .await?)
        .ok_or_else(|| {
            ApiError::NotFound(format!(
                "lifecycle_agent {} 没有 current AgentFrame",
                agent.id
            ))
        })
}

fn parse_uuid(raw: &str, field: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(raw).map_err(|_| ApiError::BadRequest(format!("无效的 {field}: {raw}")))
}

pub(crate) async fn runtime_refs_for_agent(
    state: &AppState,
    agent_id: Uuid,
) -> Result<Vec<RuntimeSessionRefDto>, ApiError> {
    Ok(state
        .repos
        .execution_anchor_repo
        .list_by_agent(agent_id)
        .await?
        .into_iter()
        .map(|anchor| RuntimeSessionRefDto {
            runtime_session_id: anchor.runtime_session_id,
        })
        .collect())
}

pub(crate) fn agent_frame_runtime_to_view(
    frame: &AgentFrame,
    runtime_session_refs: Vec<RuntimeSessionRefDto>,
) -> AgentFrameRuntimeView {
    AgentFrameRuntimeView {
        frame_ref: agent_frame_ref(frame),
        capability_surface: frame
            .effective_capability_json
            .clone()
            .unwrap_or(Value::Null),
        context_slice: frame.context_slice_json.clone().unwrap_or(Value::Null),
        vfs_surface: frame.vfs_surface_json.clone().unwrap_or(Value::Null),
        mcp_surface: frame.mcp_surface_json.clone().unwrap_or(Value::Null),
        runtime_session_refs,
        execution_profile: frame.execution_profile_json.clone(),
        effective_executor_config: frame.typed_execution_profile().map(|config| {
            ConversationModelConfigResolver::view_for_config(
                &config,
                ConversationModelConfigSource::FrameExecutionProfile,
            )
        }),
    }
}

pub(crate) fn agent_frame_ref(frame: &AgentFrame) -> AgentFrameRefDto {
    AgentFrameRefDto {
        agent_id: frame.agent_id.to_string(),
        frame_id: frame.id.to_string(),
        revision: Some(frame.revision),
    }
}
