use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, State},
};
use uuid::Uuid;

use agentdash_contracts::core::UnboundBindingResponse;
use agentdash_domain::workflow::{LifecycleRunLink, RunLinkRole, RunLinkSubjectKind};

use crate::{
    app_state::AppState,
    auth::{CurrentUser, ProjectPermission, load_story_and_project_with_permission},
    dto::{CreateStorySessionRequest, SessionBindingResponse, StorySessionDetailResponse},
    rpc::ApiError,
    session_construction::build_session_context_plan,
};

pub fn router() -> axum::Router<std::sync::Arc<crate::app_state::AppState>> {
    axum::Router::new()
        .route(
            "/stories/{id}/sessions",
            axum::routing::get(list_story_sessions).post(create_story_session),
        )
        .route(
            "/stories/{id}/sessions/{session_id}",
            axum::routing::get(get_story_session).delete(unbind_story_session),
        )
}

pub async fn list_story_sessions(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(story_id): Path<String>,
) -> Result<Json<Vec<SessionBindingResponse>>, ApiError> {
    let story_uuid = parse_story_id(&story_id)?;
    let (story, _) = load_story_and_project_with_permission(
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
    let run_ids: Vec<Uuid> = links.iter().map(|link| link.run_id).collect();
    let runs = state.repos.lifecycle_run_repo.list_by_ids(&run_ids).await?;

    let mut responses = Vec::new();
    for run in runs {
        let Some(session_id) = run.session_id else {
            continue;
        };
        let meta = state
            .services
            .session_core
            .get_session_meta(&session_id)
            .await?;
        responses.push(SessionBindingResponse {
            id: session_id.clone(),
            project_id: story.project_id.to_string(),
            session_id,
            owner_type: "story".to_string(),
            owner_id: story.id.to_string(),
            label: "companion".to_string(),
            created_at: run.created_at.to_rfc3339(),
            session_title: meta.as_ref().map(|item| item.title.clone()),
            session_updated_at: meta.as_ref().map(|item| item.updated_at),
        });
    }

    responses.sort_by_key(|r| std::cmp::Reverse(r.session_updated_at));
    Ok(Json(responses))
}

pub async fn get_story_session(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((story_id, session_id)): Path<(String, String)>,
) -> Result<Json<StorySessionDetailResponse>, ApiError> {
    let story_uuid = parse_story_id(&story_id)?;
    let (story, _) = load_story_and_project_with_permission(
        state.as_ref(),
        &current_user,
        story_uuid,
        ProjectPermission::View,
    )
    .await?;
    let meta = state
        .services
        .session_core
        .get_session_meta(&session_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Session {session_id} 不存在")))?;
    let story_project_id = story.project_id.to_string();
    if meta.project_id.as_deref() != Some(story_project_id.as_str()) {
        return Err(ApiError::NotFound(format!(
            "Session {session_id} 不属于 Story {story_id}"
        )));
    }
    let context_projection = build_session_context_plan(&state, &current_user, &session_id)
        .await?
        .map(|plan| plan.context_projection);

    Ok(Json(StorySessionDetailResponse {
        binding_id: session_id.clone(),
        session_id,
        label: "companion".to_string(),
        session_title: Some(meta.title),
        last_activity: Some(meta.updated_at),
        vfs: context_projection
            .as_ref()
            .and_then(|projection| projection.vfs.clone()),
        runtime_surface: context_projection
            .as_ref()
            .and_then(|projection| projection.runtime_surface.clone()),
        context_snapshot: context_projection.and_then(|projection| projection.context_snapshot),
    }))
}

pub async fn create_story_session(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(story_id): Path<String>,
    Json(req): Json<CreateStorySessionRequest>,
) -> Result<Json<SessionBindingResponse>, ApiError> {
    let story_uuid = parse_story_id(&story_id)?;
    let (story, _) = load_story_and_project_with_permission(
        state.as_ref(),
        &current_user,
        story_uuid,
        ProjectPermission::Edit,
    )
    .await?;
    let session_id =
        if let Some(session_id) = req.session_id.filter(|value| !value.trim().is_empty()) {
            session_id
        } else {
            let title = req
                .title
                .unwrap_or_else(|| format!("Story: {}", story.title));
            state.services.session_core.create_session(&title).await?.id
        };

    let meta = state
        .services
        .session_core
        .update_session_meta(&session_id, |meta| {
            meta.project_id = Some(story.project_id.to_string());
        })
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Session {session_id} 不存在")))?;
    state
        .services
        .session_core
        .mark_owner_bootstrap_pending(&session_id)
        .await?;
    crate::routes::acp_sessions::ensure_freeform_lifecycle_run(
        state.as_ref(),
        story.project_id,
        &session_id,
    )
    .await?;
    attach_story_link_for_session(state.as_ref(), story.id, &session_id).await?;

    Ok(Json(SessionBindingResponse {
        id: session_id.clone(),
        project_id: story.project_id.to_string(),
        session_id,
        owner_type: "story".to_string(),
        owner_id: story.id.to_string(),
        label: req.label.unwrap_or_else(|| "companion".to_string()),
        created_at: chrono::Utc::now().to_rfc3339(),
        session_title: Some(meta.title),
        session_updated_at: Some(meta.updated_at),
    }))
}

pub async fn unbind_story_session(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((story_id, session_id)): Path<(String, String)>,
) -> Result<Json<UnboundBindingResponse>, ApiError> {
    let story_uuid = parse_story_id(&story_id)?;
    load_story_and_project_with_permission(
        state.as_ref(),
        &current_user,
        story_uuid,
        ProjectPermission::Edit,
    )
    .await?;

    let runs = state
        .repos
        .lifecycle_run_repo
        .list_by_session(&session_id)
        .await?;
    for run in runs {
        let links = state
            .repos
            .lifecycle_run_link_repo
            .list_by_run(run.id)
            .await?;
        for link in links {
            if link.subject_kind == RunLinkSubjectKind::Story
                && link.subject_id == story_uuid
                && link.role == RunLinkRole::Subject
            {
                state.repos.lifecycle_run_link_repo.delete(link.id).await?;
            }
        }
    }

    Ok(Json(UnboundBindingResponse {
        unbound: true,
        binding_id: session_id,
    }))
}

async fn attach_story_link_for_session(
    state: &AppState,
    story_id: Uuid,
    session_id: &str,
) -> Result<(), ApiError> {
    let runs = state
        .repos
        .lifecycle_run_repo
        .list_by_session(session_id)
        .await?;
    let Some(run) = agentdash_application::workflow::select_active_run(runs) else {
        return Ok(());
    };
    let links = state
        .repos
        .lifecycle_run_link_repo
        .list_by_run(run.id)
        .await?;
    if links.iter().any(|link| {
        link.subject_kind == RunLinkSubjectKind::Story
            && link.subject_id == story_id
            && link.role == RunLinkRole::Subject
    }) {
        return Ok(());
    }
    let link = LifecycleRunLink::new(
        run.id,
        RunLinkSubjectKind::Story,
        story_id,
        RunLinkRole::Subject,
    );
    state.repos.lifecycle_run_link_repo.create(&link).await?;
    Ok(())
}

fn parse_story_id(story_id: &str) -> Result<Uuid, ApiError> {
    story_id
        .parse()
        .map_err(|_| ApiError::BadRequest(format!("无效的 story_id: {story_id}")))
}
