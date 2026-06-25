use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, State},
};
use uuid::Uuid;

use agentdash_application_lifecycle::run_view_builder;
use agentdash_application_lifecycle::run_view_builder::LifecycleRunStatusView;
use agentdash_contracts::workflow::SubjectExecutionView;
use agentdash_domain::workflow::SubjectRef;

use crate::{
    app_state::AppState,
    auth::{CurrentUser, ProjectPermission, load_story_and_project_with_permission},
    rpc::ApiError,
};

use super::lifecycle_contracts::subject_execution_view_to_contract;

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
    let view = run_view_builder::build_subject_execution_view(&state.repos, subject).await?;
    Ok(Json(subject_execution_view_to_contract(view)))
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
    let view = run_view_builder::build_subject_execution_view(&state.repos, subject).await?;
    let has_active_run = view.runs.iter().any(|run| {
        matches!(
            run.status,
            LifecycleRunStatusView::Ready | LifecycleRunStatusView::Running
        )
    });

    if has_active_run {
        Ok(Json(Some(subject_execution_view_to_contract(view))))
    } else {
        Ok(Json(None))
    }
}

fn parse_story_id(story_id: &str) -> Result<Uuid, ApiError> {
    story_id
        .parse()
        .map_err(|_| ApiError::BadRequest(format!("无效的 story_id: {story_id}")))
}
