use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, Query, State},
};
use uuid::Uuid;

use agentdash_application::session::SessionExecutionState;

use crate::{
    app_state::AppState,
    auth::{CurrentUser, ProjectPermission, load_project_with_permission},
    dto::{ListProjectSessionsQuery, ProjectSessionDetailResponse, ProjectSessionEntry},
    rpc::ApiError,
    session_construction::build_session_context_plan,
};

pub fn router() -> axum::Router<std::sync::Arc<crate::app_state::AppState>> {
    axum::Router::new()
        .route(
            "/projects/{id}/sessions",
            axum::routing::get(list_project_sessions),
        )
        .route(
            "/projects/{id}/sessions/{session_id}",
            axum::routing::get(get_project_session),
        )
}

pub async fn get_project_session(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((project_id, session_id)): Path<(String, String)>,
) -> Result<Json<ProjectSessionDetailResponse>, ApiError> {
    let project_uuid = parse_project_id(&project_id)?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_uuid,
        ProjectPermission::View,
    )
    .await?;

    let meta = state
        .services
        .session_core
        .get_session_meta(&session_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Session {session_id} 不存在")))?;
    ensure_session_belongs_project(&meta, project_uuid)?;

    let context_projection = build_session_context_plan(&state, &current_user, &session_id)
        .await?
        .map(|plan| plan.context_projection);

    Ok(Json(ProjectSessionDetailResponse {
        binding_id: session_id.clone(),
        session_id,
        label: agentdash_application::workflow::FREEFORM_SESSION_LABEL.to_string(),
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

pub async fn list_project_sessions(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(project_id): Path<String>,
    Query(query): Query<ListProjectSessionsQuery>,
) -> Result<Json<Vec<ProjectSessionEntry>>, ApiError> {
    let project_uuid = parse_project_id(&project_id)?;
    let project = load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_uuid,
        ProjectPermission::View,
    )
    .await?;

    let sessions = state.services.session_core.list_sessions().await?;
    let session_ids: Vec<String> = sessions
        .iter()
        .filter(|session| session_project_id(session) == Some(project_uuid))
        .map(|session| session.id.clone())
        .collect();
    let status_map = state
        .services
        .session_core
        .inspect_execution_states_bulk(&session_ids)
        .await
        .map_err(|error| ApiError::Internal(format!("批量读取 session 执行状态失败: {error}")))?;
    let status_filter: Option<Vec<String>> = query.status.as_deref().map(|raw| {
        raw.split(',')
            .map(|part| part.trim().to_ascii_lowercase())
            .filter(|part| !part.is_empty())
            .collect()
    });
    let limit = query.limit.unwrap_or(50).clamp(1, 500) as usize;

    let mut entries: Vec<ProjectSessionEntry> = sessions
        .into_iter()
        .filter(|session| session_project_id(session) == Some(project_uuid))
        .filter_map(|session| {
            let execution_status = execution_state_to_str(status_map.get(&session.id));
            if let Some(filter) = &status_filter
                && !filter.contains(&execution_status.to_string())
            {
                return None;
            }
            let parent_session_id = session
                .companion_context
                .as_ref()
                .map(|context| context.parent_session_id.clone());
            Some(ProjectSessionEntry {
                session_id: session.id,
                session_title: Some(session.title),
                last_activity: Some(session.updated_at),
                execution_status: execution_status.to_string(),
                owner_type: "project".to_string(),
                owner_id: project_uuid.to_string(),
                owner_title: Some(project.name.clone()),
                story_id: None,
                story_title: None,
                agent_key: None,
                agent_display_name: None,
                parent_session_id,
                parent_relation_kind: None,
            })
        })
        .collect();

    entries.sort_by_key(|e| std::cmp::Reverse(e.last_activity));
    entries.truncate(limit);
    Ok(Json(entries))
}

fn parse_project_id(project_id: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(project_id)
        .map_err(|_| ApiError::BadRequest(format!("无效的 project_id: {project_id}")))
}

fn session_project_id(meta: &agentdash_spi::session_persistence::SessionMeta) -> Option<Uuid> {
    meta.project_id
        .as_deref()
        .and_then(|project_id| Uuid::parse_str(project_id).ok())
}

fn ensure_session_belongs_project(
    meta: &agentdash_spi::session_persistence::SessionMeta,
    project_id: Uuid,
) -> Result<(), ApiError> {
    if session_project_id(meta) == Some(project_id) {
        return Ok(());
    }
    Err(ApiError::NotFound(format!(
        "Session {} 不属于 Project {}",
        meta.id, project_id
    )))
}

fn execution_state_to_str(state: Option<&SessionExecutionState>) -> &'static str {
    match state {
        Some(SessionExecutionState::Running { .. }) => "running",
        Some(SessionExecutionState::Completed { .. }) => "completed",
        Some(SessionExecutionState::Failed { .. }) => "failed",
        Some(SessionExecutionState::Interrupted { .. }) => "interrupted",
        Some(SessionExecutionState::Idle) | None => "idle",
    }
}
