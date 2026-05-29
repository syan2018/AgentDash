use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, State},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    app_state::AppState,
    auth::{CurrentUser, ProjectPermission, load_story_and_project_with_permission},
    rpc::ApiError,
};

#[derive(Debug, Serialize)]
pub struct StorySessionDetailResponse {
    pub session_id: String,
    pub session_title: Option<String>,
    pub last_activity: Option<i64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct StorySessionSummaryResponse {
    pub session_id: String,
    pub session_title: Option<String>,
    pub session_updated_at: Option<i64>,
}

/// GET /stories/{id}/sessions
pub async fn list_story_sessions(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(story_id): Path<String>,
) -> Result<Json<Vec<StorySessionSummaryResponse>>, ApiError> {
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

    // TODO: session_binding 已移除，需要通过 SessionMeta.project_id + story 关联查询
    Ok(Json(vec![]))
}

/// GET /stories/{id}/sessions/{session_id}
pub async fn get_story_session(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((story_id, session_id)): Path<(String, String)>,
) -> Result<Json<StorySessionDetailResponse>, ApiError> {
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

    let meta = state
        .services
        .session_core
        .get_session_meta(&session_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Session {session_id} 不存在")))?;

    Ok(Json(StorySessionDetailResponse {
        session_id: meta.id,
        session_title: Some(meta.title),
        last_activity: Some(meta.updated_at),
    }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateStorySessionRequest {
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
}

/// POST /stories/{id}/sessions
pub async fn create_story_session(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(story_id): Path<String>,
    Json(req): Json<CreateStorySessionRequest>,
) -> Result<Json<StorySessionSummaryResponse>, ApiError> {
    let story_uuid: Uuid = story_id
        .parse()
        .map_err(|_| ApiError::BadRequest(format!("无效的 story_id: {story_id}")))?;

    let (story, _) = load_story_and_project_with_permission(
        state.as_ref(),
        &current_user,
        story_uuid,
        ProjectPermission::Edit,
    )
    .await?;

    let created_new_session = req.session_id.is_none();
    let session_id = match (req.session_id, req.title) {
        (Some(_), Some(_)) => {
            return Err(ApiError::BadRequest(
                "session_id 与 title 不能同时提供".into(),
            ));
        }
        (Some(sid), None) => {
            state
                .services
                .session_core
                .get_session_meta(&sid)
                .await?
                .ok_or_else(|| ApiError::NotFound(format!("Session {sid} 不存在")))?;
            sid
        }
        (None, title) => {
            let title = title.unwrap_or_else(|| "Story 伴随会话".to_string());
            let meta = state
                .services
                .session_core
                .create_session(&title)
                .await
                .map_err(|e| ApiError::Internal(e.to_string()))?;
            meta.id
        }
    };

    if created_new_session {
        state
            .services
            .session_core
            .mark_owner_bootstrap_pending(&session_id)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;
        crate::routes::acp_sessions::ensure_freeform_lifecycle_run(
            state.as_ref(),
            story.project_id,
            &session_id,
        )
        .await?;
    }

    let meta = state
        .services
        .session_core
        .get_session_meta(&session_id)
        .await?;

    Ok(Json(StorySessionSummaryResponse {
        session_id,
        session_title: meta.as_ref().map(|m| m.title.clone()),
        session_updated_at: meta.as_ref().map(|m| m.updated_at),
    }))
}

/// DELETE /stories/{id}/sessions/{session_id}
pub async fn unbind_story_session(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((story_id, session_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let story_uuid: Uuid = story_id
        .parse()
        .map_err(|_| ApiError::BadRequest(format!("无效的 story_id: {story_id}")))?;
    load_story_and_project_with_permission(
        state.as_ref(),
        &current_user,
        story_uuid,
        ProjectPermission::Edit,
    )
    .await?;

    // TODO: session_binding 已移除，解绑逻辑需基于新的 session-story 关联方式实现
    let _ = &session_id;

    Ok(Json(serde_json::json!({
        "unbound": true,
        "session_id": session_id,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn story_session_summary_serializes_as_snake_case() {
        let value = serde_json::to_value(StorySessionSummaryResponse {
            session_id: "sess-1".to_string(),
            session_title: Some("title".to_string()),
            session_updated_at: Some(1),
        })
        .expect("serialize story session summary");

        assert!(value.get("session_id").is_some());
        assert!(value.get("session_title").is_some());
        assert!(value.get("session_updated_at").is_some());
        assert!(value.get("sessionId").is_none());
        assert!(value.get("sessionTitle").is_none());
    }

    #[test]
    fn create_story_session_request_deserializes_from_snake_case() {
        let request: CreateStorySessionRequest = serde_json::from_value(serde_json::json!({
            "session_id": "sess-1",
        }))
        .expect("deserialize story session request");

        assert_eq!(request.session_id.as_deref(), Some("sess-1"));
    }
}
