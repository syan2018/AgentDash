use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, State},
};
use uuid::Uuid;

use agentdash_application::agent_run::{
    AgentFrameRefReadModel, AgentFrameRuntimeReadModel, AgentRunPresentationReadModelError,
    AgentRunRuntimeSurfaceQueryError, ConversationEffectiveExecutorConfigModel,
    ConversationModelConfigSourceModel, RuntimeSessionRefReadModel, RuntimeSessionTraceReadModel,
    SessionRuntimeControlPlaneStatusModel,
};
use agentdash_application::lifecycle::run_view_builder::{
    self, SubjectExecutionView as SubjectExecutionReadModel,
};
use agentdash_contracts::workflow::{
    AgentFrameRefDto, AgentFrameRuntimeView, ConversationEffectiveExecutorConfigView,
    ConversationModelConfigSource, LifecycleRunView, ProjectActiveAgentsView, RuntimeSessionRefDto,
    RuntimeSessionTraceView, SessionRuntimeControlPlaneStatus, SubjectExecutionView,
};
use agentdash_domain::workflow::{LifecycleRun, SubjectRef};

use crate::{
    app_state::AppState,
    auth::{
        CurrentUser, ProjectPermission, load_project_with_permission,
        load_story_and_project_with_permission,
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

    let view = run_view_builder::build_lifecycle_run_view(&state.repos, &run).await?;
    Ok(Json(lifecycle_run_view_to_contract(view)))
}

pub async fn get_subject_execution(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((kind, id)): Path<(String, String)>,
) -> Result<Json<SubjectExecutionView>, ApiError> {
    let subject = SubjectRef::new(kind, parse_uuid(&id, "subject_id")?);
    let view =
        run_view_builder::build_subject_execution_view(&state.repos, subject.clone()).await?;
    authorize_subject_execution_view(&state, &current_user, &subject, &view).await?;
    Ok(Json(subject_execution_view_to_contract(view)))
}

pub async fn get_agent_frame_runtime(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(frame_id): Path<String>,
) -> Result<Json<AgentFrameRuntimeView>, ApiError> {
    let frame_id = parse_uuid(&frame_id, "frame_id")?;
    let view = state
        .services
        .presentation_read_model_query
        .agent_frame_runtime(frame_id)
        .await
        .map_err(presentation_read_model_error_to_api)?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        view.project_id,
        ProjectPermission::View,
    )
    .await?;

    Ok(Json(agent_frame_runtime_to_view(view)))
}

pub async fn get_session_trace(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(runtime_session_id): Path<String>,
) -> Result<Json<RuntimeSessionTraceView>, ApiError> {
    authorize_runtime_session_shell(state.as_ref(), &current_user, &runtime_session_id).await?;
    let view = state
        .services
        .presentation_read_model_query
        .runtime_session_trace(&runtime_session_id)
        .await
        .map_err(presentation_read_model_error_to_api)?;

    Ok(Json(runtime_session_trace_to_contract(view)))
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

    let view = run_view_builder::build_project_active_agents_view(&state.repos, project_id).await?;
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

fn parse_uuid(raw: &str, field: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(raw).map_err(|_| ApiError::BadRequest(format!("无效的 {field}: {raw}")))
}

pub(crate) fn agent_frame_runtime_to_view(
    frame: AgentFrameRuntimeReadModel,
) -> AgentFrameRuntimeView {
    AgentFrameRuntimeView {
        frame_ref: agent_frame_ref_to_contract(frame.frame_ref),
        capability_surface: frame.capability_surface,
        context_slice: frame.context_slice,
        vfs_surface: frame.vfs_surface,
        mcp_surface: frame.mcp_surface,
        runtime_session_refs: frame
            .runtime_session_refs
            .into_iter()
            .map(runtime_session_ref_to_contract)
            .collect(),
        execution_profile: frame.execution_profile,
        effective_executor_config: frame
            .effective_executor_config
            .map(conversation_effective_executor_config_to_contract),
    }
}

fn conversation_effective_executor_config_to_contract(
    config: ConversationEffectiveExecutorConfigModel,
) -> ConversationEffectiveExecutorConfigView {
    ConversationEffectiveExecutorConfigView {
        executor: config.executor,
        provider_id: config.provider_id,
        model_id: config.model_id,
        agent_id: config.agent_id,
        thinking_level: config.thinking_level,
        permission_policy: config.permission_policy,
        source: match config.source {
            ConversationModelConfigSourceModel::ProjectAgentPreset => {
                ConversationModelConfigSource::ProjectAgentPreset
            }
            ConversationModelConfigSourceModel::FrameExecutionProfile => {
                ConversationModelConfigSource::FrameExecutionProfile
            }
            ConversationModelConfigSourceModel::UserOverride => {
                ConversationModelConfigSource::UserOverride
            }
            ConversationModelConfigSourceModel::ExecutorDiscoveryDefault => {
                ConversationModelConfigSource::ExecutorDiscoveryDefault
            }
            ConversationModelConfigSourceModel::Unspecified => {
                ConversationModelConfigSource::Unspecified
            }
        },
    }
}

fn agent_frame_ref_to_contract(frame: AgentFrameRefReadModel) -> AgentFrameRefDto {
    AgentFrameRefDto {
        agent_id: frame.agent_id,
        frame_id: frame.frame_id,
        revision: frame.revision,
    }
}

fn runtime_session_ref_to_contract(
    runtime_ref: RuntimeSessionRefReadModel,
) -> RuntimeSessionRefDto {
    RuntimeSessionRefDto {
        runtime_session_id: runtime_ref.runtime_session_id,
    }
}

fn runtime_session_trace_to_contract(
    trace: RuntimeSessionTraceReadModel,
) -> RuntimeSessionTraceView {
    RuntimeSessionTraceView {
        runtime_session_ref: RuntimeSessionRefDto {
            runtime_session_id: trace.runtime_session_id,
        },
        frame_ref: trace.frame_ref.map(agent_frame_ref_to_contract),
        events: trace.events,
        turns: trace.turns,
    }
}

pub(crate) fn session_runtime_control_status_to_contract(
    status: SessionRuntimeControlPlaneStatusModel,
) -> SessionRuntimeControlPlaneStatus {
    match status {
        SessionRuntimeControlPlaneStatusModel::UnboundTrace => {
            SessionRuntimeControlPlaneStatus::UnboundTrace
        }
        SessionRuntimeControlPlaneStatusModel::AnchoredIdle => {
            SessionRuntimeControlPlaneStatus::AnchoredIdle
        }
        SessionRuntimeControlPlaneStatusModel::AnchoredRunning => {
            SessionRuntimeControlPlaneStatus::AnchoredRunning
        }
        SessionRuntimeControlPlaneStatusModel::AnchoredCancelling => {
            SessionRuntimeControlPlaneStatus::AnchoredCancelling
        }
        SessionRuntimeControlPlaneStatusModel::Terminal => {
            SessionRuntimeControlPlaneStatus::Terminal
        }
        SessionRuntimeControlPlaneStatusModel::FrameMissing => {
            SessionRuntimeControlPlaneStatus::FrameMissing
        }
    }
}

pub(crate) fn presentation_read_model_error_to_api(
    error: AgentRunPresentationReadModelError,
) -> ApiError {
    match error {
        AgentRunPresentationReadModelError::MissingSession { runtime_session_id } => {
            ApiError::NotFound(format!("会话 {runtime_session_id} 不存在"))
        }
        AgentRunPresentationReadModelError::MissingLifecycleRun { run_id } => {
            ApiError::NotFound(format!("lifecycle_run 不存在: {run_id}"))
        }
        AgentRunPresentationReadModelError::MissingLifecycleAgent { agent_id } => {
            ApiError::NotFound(format!("lifecycle_agent 不存在: {agent_id}"))
        }
        AgentRunPresentationReadModelError::MissingAgentFrame { frame_id } => {
            ApiError::NotFound(format!("agent_frame 不存在: {frame_id}"))
        }
        AgentRunPresentationReadModelError::ControlPlaneMismatch { message } => {
            ApiError::BadRequest(message)
        }
        AgentRunPresentationReadModelError::RuntimeSurface(error) => match error {
            AgentRunRuntimeSurfaceQueryError::MissingAnchor {
                runtime_session_id, ..
            } => ApiError::NotFound(format!(
                "runtime_session 缺少 RuntimeSessionExecutionAnchor: {runtime_session_id}"
            )),
            AgentRunRuntimeSurfaceQueryError::MissingLifecycleRun { run_id, .. } => {
                ApiError::NotFound(format!("lifecycle_run 不存在: {run_id}"))
            }
            AgentRunRuntimeSurfaceQueryError::MissingLifecycleAgent { agent_id, .. } => {
                ApiError::NotFound(format!("lifecycle_agent 不存在: {agent_id}"))
            }
            AgentRunRuntimeSurfaceQueryError::MissingCurrentFrame { agent_id, .. } => {
                ApiError::NotFound(format!(
                    "lifecycle_agent {agent_id} 没有 current AgentFrame"
                ))
            }
            AgentRunRuntimeSurfaceQueryError::AnchorControlPlaneMismatch { .. }
            | AgentRunRuntimeSurfaceQueryError::MissingSurfaceClosure { .. }
            | AgentRunRuntimeSurfaceQueryError::MissingRuntimeBackendAnchor { .. }
            | AgentRunRuntimeSurfaceQueryError::BackendAnchorDerivation { .. } => {
                ApiError::BadRequest(error.to_string())
            }
            AgentRunRuntimeSurfaceQueryError::Repository { message, .. } => {
                ApiError::Internal(message)
            }
        },
        AgentRunPresentationReadModelError::Domain(error) => ApiError::from(error),
        AgentRunPresentationReadModelError::SessionStore(error) => ApiError::from(error),
        AgentRunPresentationReadModelError::Io(error) => ApiError::Internal(format!(
            "读取 session presentation read model 失败: {error}"
        )),
    }
}
