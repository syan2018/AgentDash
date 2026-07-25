use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, State},
};
use uuid::Uuid;

use agentdash_application_lifecycle::run_view_builder::SubjectExecutionView as SubjectExecutionReadModel;
use agentdash_contracts::workflow::{
    LifecycleRunView, ProjectActiveAgentsView, SubjectExecutionView,
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
    lifecycle_run_view_query_error, lifecycle_run_view_to_contract,
    project_active_agents_view_to_contract, subject_execution_view_to_contract,
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
    let view = state
        .services
        .lifecycle_run_views
        .lifecycle_run_view(run_id)
        .await
        .map_err(lifecycle_run_view_query_error)?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        view.run.project_id,
        ProjectPermission::Use,
    )
    .await?;
    Ok(Json(lifecycle_run_view_to_contract(view)))
}

pub async fn get_subject_execution(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((kind, id)): Path<(String, String)>,
) -> Result<Json<SubjectExecutionView>, ApiError> {
    let subject = SubjectRef::new(kind, parse_uuid(&id, "subject_id")?);
    let view = state
        .services
        .lifecycle_run_views
        .subject_execution_view(subject.clone())
        .await
        .map_err(lifecycle_run_view_query_error)?;
    authorize_subject_execution_view(&state, &current_user, &subject, &view).await?;
    Ok(Json(subject_execution_view_to_contract(view)))
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
        ProjectPermission::Use,
    )
    .await?;

    let view = state
        .services
        .lifecycle_run_views
        .project_active_agents_view(project_id)
        .await
        .map_err(lifecycle_run_view_query_error)?;
    Ok(Json(project_active_agents_view_to_contract(view)))
}

async fn authorize_subject_execution_view(
    state: &Arc<AppState>,
    current_user: &agentdash_integration_api::AuthIdentity,
    subject: &SubjectRef,
    view: &SubjectExecutionReadModel,
) -> Result<(), ApiError> {
    if let Some(project_id) = view.runs.first().map(|run| run.run.project_id) {
        load_project_with_permission(state, current_user, project_id, ProjectPermission::Use)
            .await?;
        return Ok(());
    }

    match subject.kind.as_str() {
        "project" => {
            load_project_with_permission(state, current_user, subject.id, ProjectPermission::Use)
                .await?;
            Ok(())
        }
        "story" => {
            load_story_and_project_with_permission(
                state,
                current_user,
                subject.id,
                ProjectPermission::Use,
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
                ProjectPermission::Use,
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
