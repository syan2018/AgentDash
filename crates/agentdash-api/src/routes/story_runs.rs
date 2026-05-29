use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, State},
};
use uuid::Uuid;

use agentdash_contracts::workflow::{
    LifecycleRunLinkDto, StoryRunOverviewDto, StoryRunsResponse,
};
use agentdash_domain::workflow::{LifecycleRunLink, RunLinkRole, RunLinkSubjectKind};

use crate::{
    app_state::AppState,
    auth::{CurrentUser, ProjectPermission, load_story_and_project_with_permission},
    rpc::ApiError,
};

fn link_to_dto(link: &LifecycleRunLink) -> LifecycleRunLinkDto {
    LifecycleRunLinkDto {
        id: link.id.to_string(),
        run_id: link.run_id.to_string(),
        subject_kind: link.subject_kind.as_str().to_string(),
        subject_id: link.subject_id.to_string(),
        role: link.role.as_str().to_string(),
        metadata: link.metadata.clone(),
        created_at: link.created_at.to_rfc3339(),
    }
}

/// GET /stories/{story_id}/runs
///
/// 返回与 Story 关联的所有 LifecycleRun（通过 LifecycleRunLink 查询）。
pub async fn list_story_runs(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(story_id): Path<String>,
) -> Result<Json<StoryRunsResponse>, ApiError> {
    let story_uuid: Uuid = story_id
        .parse()
        .map_err(|_| ApiError::BadRequest(format!("无效的 story_id: {story_id}")))?;

    load_story_and_project_with_permission(
        state.as_ref(),
        &current_user,
        story_uuid,
        ProjectPermission::View,
    )
    .await?;

    let links = state
        .repos
        .lifecycle_run_link_repo
        .list_by_subject(RunLinkSubjectKind::Story, story_uuid)
        .await?;

    if links.is_empty() {
        return Ok(Json(StoryRunsResponse {
            story_id,
            runs: Vec::new(),
        }));
    }

    let run_ids: Vec<Uuid> = links.iter().map(|l| l.run_id).collect();
    let runs = state.repos.lifecycle_run_repo.list_by_ids(&run_ids).await?;

    let mut run_overviews: Vec<StoryRunOverviewDto> = runs
        .iter()
        .map(|run| {
            let run_links: Vec<LifecycleRunLinkDto> = links
                .iter()
                .filter(|l| l.run_id == run.id)
                .map(link_to_dto)
                .collect();

            StoryRunOverviewDto {
                id: run.id.to_string(),
                lifecycle_id: run.lifecycle_id.to_string(),
                status: run.status,
                session_id: run.session_id.clone(),
                created_at: run.created_at.to_rfc3339(),
                updated_at: run.updated_at.to_rfc3339(),
                last_activity_at: run.last_activity_at.to_rfc3339(),
                links: run_links,
            }
        })
        .collect();

    run_overviews.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    Ok(Json(StoryRunsResponse {
        story_id,
        runs: run_overviews,
    }))
}

/// GET /stories/{story_id}/runs/active
///
/// 返回 Story 当前活跃的 LifecycleRun（如果有）。
pub async fn get_active_story_run(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(story_id): Path<String>,
) -> Result<Json<Option<StoryRunOverviewDto>>, ApiError> {
    let story_uuid: Uuid = story_id
        .parse()
        .map_err(|_| ApiError::BadRequest(format!("无效的 story_id: {story_id}")))?;

    load_story_and_project_with_permission(
        state.as_ref(),
        &current_user,
        story_uuid,
        ProjectPermission::View,
    )
    .await?;

    let links = state
        .repos
        .lifecycle_run_link_repo
        .list_by_subject_and_role(RunLinkSubjectKind::Story, story_uuid, RunLinkRole::Subject)
        .await?;

    if links.is_empty() {
        return Ok(Json(None));
    }

    let run_ids: Vec<Uuid> = links.iter().map(|l| l.run_id).collect();
    let runs = state.repos.lifecycle_run_repo.list_by_ids(&run_ids).await?;

    let active_run = runs.iter().find(|r| {
        r.status == agentdash_domain::workflow::LifecycleRunStatus::Running
            || r.status == agentdash_domain::workflow::LifecycleRunStatus::Ready
    });

    let result = active_run.map(|run| {
        let run_links: Vec<LifecycleRunLinkDto> = links
            .iter()
            .filter(|l| l.run_id == run.id)
            .map(link_to_dto)
            .collect();

        StoryRunOverviewDto {
            id: run.id.to_string(),
            lifecycle_id: run.lifecycle_id.to_string(),
            status: run.status,
            session_id: run.session_id.clone(),
            created_at: run.created_at.to_rfc3339(),
            updated_at: run.updated_at.to_rfc3339(),
            last_activity_at: run.last_activity_at.to_rfc3339(),
            links: run_links,
        }
    });

    Ok(Json(result))
}
