use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, State},
};
use uuid::Uuid;

use agentdash_contracts::core::UnboundBindingResponse;

use crate::{
    app_state::AppState,
    auth::{CurrentUser, ProjectPermission, load_story_and_project_with_permission},
    dto::{CreateStorySessionRequest, SessionBindingResponse, StorySessionDetailResponse},
    rpc::ApiError,
    session_construction::build_session_context_plan,
};
use agentdash_domain::session_binding::{SessionBinding, SessionOwnerType};

/// GET /stories/{id}/sessions
pub async fn list_story_sessions(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(story_id): Path<String>,
) -> Result<Json<Vec<SessionBindingResponse>>, ApiError> {
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

    let bindings = state
        .repos
        .session_binding_repo
        .list_by_owner(SessionOwnerType::Story, story_uuid)
        .await?;

    let mut responses: Vec<SessionBindingResponse> = Vec::with_capacity(bindings.len());
    for binding in &bindings {
        let mut resp = SessionBindingResponse::from_binding(binding);
        if let Ok(Some(meta)) = state
            .services
            .session_core
            .get_session_meta(&binding.session_id)
            .await
        {
            resp.session_title = Some(meta.title);
            resp.session_updated_at = Some(meta.updated_at);
        }
        responses.push(resp);
    }

    Ok(Json(responses))
}

pub fn router() -> axum::Router<std::sync::Arc<crate::app_state::AppState>> {
    axum::Router::new()
        .route(
            "/stories/{id}/sessions",
            axum::routing::get(list_story_sessions).post(create_story_session),
        )
        .route(
            "/stories/{id}/sessions/{binding_id}",
            axum::routing::get(get_story_session).delete(unbind_story_session),
        )
}

/// GET /stories/{id}/sessions/{binding_id}
pub async fn get_story_session(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((story_id, binding_id)): Path<(String, String)>,
) -> Result<Json<StorySessionDetailResponse>, ApiError> {
    let story_uuid: Uuid = story_id
        .parse()
        .map_err(|_| ApiError::BadRequest(format!("无效的 story_id: {story_id}")))?;
    let binding_uuid: Uuid = binding_id
        .parse()
        .map_err(|_| ApiError::BadRequest(format!("无效的 binding_id: {binding_id}")))?;

    let (_story, _) = load_story_and_project_with_permission(
        state.as_ref(),
        &current_user,
        story_uuid,
        ProjectPermission::View,
    )
    .await?;

    let bindings = state
        .repos
        .session_binding_repo
        .list_by_owner(SessionOwnerType::Story, story_uuid)
        .await?;
    let binding = bindings
        .into_iter()
        .find(|item| item.id == binding_uuid)
        .ok_or_else(|| ApiError::NotFound(format!("Story Session binding {binding_id} 不存在")))?;

    let meta = state
        .services
        .session_core
        .get_session_meta(&binding.session_id)
        .await?;
    let context_bindings = state
        .repos
        .session_binding_repo
        .list_by_session(&binding.session_id)
        .await
        .map_err(ApiError::from)?;
    let context_projection = build_session_context_plan(
        &state,
        &current_user,
        &binding.session_id,
        &context_bindings,
    )
    .await?
    .map(|plan| plan.context_projection);
    let response_session_id = binding.session_id.clone();

    Ok(Json(StorySessionDetailResponse {
        binding_id,
        session_id: response_session_id.clone(),
        label: binding.label,
        session_title: meta.as_ref().map(|item| item.title.clone()),
        last_activity: meta.as_ref().map(|item| item.updated_at),
        vfs: context_projection
            .as_ref()
            .and_then(|projection| projection.vfs.clone()),
        runtime_surface: context_projection
            .as_ref()
            .and_then(|projection| projection.runtime_surface.clone()),
        context_snapshot: context_projection.and_then(|projection| projection.context_snapshot),
    }))
}

fn normalize_story_root_label(raw: Option<String>) -> Result<String, ApiError> {
    match raw.as_deref().map(str::trim) {
        None | Some("") | Some("companion") => Ok("companion".to_string()),
        Some(other) => Err(ApiError::BadRequest(format!(
            "Story root session 仅支持 label=companion，收到: {other}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_binding_response_serializes_as_snake_case() {
        let value = serde_json::to_value(SessionBindingResponse {
            id: "binding-1".to_string(),
            project_id: "project-1".to_string(),
            session_id: "sess-1".to_string(),
            owner_type: "story".to_string(),
            owner_id: "story-1".to_string(),
            label: "companion".to_string(),
            created_at: "2026-03-20T00:00:00Z".to_string(),
            session_title: Some("title".to_string()),
            session_updated_at: Some(1),
        })
        .expect("serialize session binding response");

        assert!(value.get("session_id").is_some());
        assert!(value.get("project_id").is_some());
        assert!(value.get("session_title").is_some());
        assert!(value.get("session_updated_at").is_some());
        assert!(value.get("sessionId").is_none());
        assert!(value.get("sessionTitle").is_none());
        assert!(value.get("sessionUpdatedAt").is_none());
    }

    #[test]
    fn create_story_session_request_deserializes_from_snake_case() {
        let request: CreateStorySessionRequest = serde_json::from_value(serde_json::json!({
            "session_id": "sess-1",
            "label": "companion"
        }))
        .expect("deserialize story session request");

        assert_eq!(request.session_id.as_deref(), Some("sess-1"));
        assert_eq!(request.label.as_deref(), Some("companion"));
    }

    #[test]
    fn normalize_story_root_label_accepts_default_and_companion() {
        assert_eq!(
            normalize_story_root_label(None).expect("default label"),
            "companion"
        );
        assert_eq!(
            normalize_story_root_label(Some("companion".to_string())).expect("companion label"),
            "companion"
        );
        assert_eq!(
            normalize_story_root_label(Some("  ".to_string())).expect("blank label"),
            "companion"
        );
    }

    #[test]
    fn normalize_story_root_label_rejects_non_companion() {
        let err = normalize_story_root_label(Some("review".to_string())).expect_err("reject");
        match err {
            ApiError::BadRequest(message) => {
                assert!(message.contains("label=companion"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }
}

/// POST /stories/{id}/sessions
pub async fn create_story_session(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(story_id): Path<String>,
    Json(req): Json<CreateStorySessionRequest>,
) -> Result<Json<SessionBindingResponse>, ApiError> {
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

    let label = normalize_story_root_label(req.label)?;

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
                .map_err(ApiError::from)?;
            meta.id
        }
    };
    let binding = SessionBinding::new(
        story.project_id,
        session_id.clone(),
        SessionOwnerType::Story,
        story_uuid,
        &label,
    );
    state.repos.session_binding_repo.create(&binding).await?;
    if created_new_session {
        state
            .services
            .session_core
            .mark_owner_bootstrap_pending(&session_id)
            .await
            .map_err(ApiError::from)?;
        crate::routes::acp_sessions::ensure_freeform_lifecycle_run(
            state.as_ref(),
            story.project_id,
            &session_id,
        )
        .await?;
    }

    let mut resp = SessionBindingResponse::from_binding(&binding);
    if let Ok(Some(meta)) = state
        .services
        .session_core
        .get_session_meta(&session_id)
        .await
    {
        resp.session_title = Some(meta.title);
        resp.session_updated_at = Some(meta.updated_at);
    }

    Ok(Json(resp))
}

/// DELETE /stories/{id}/sessions/{binding_id}
pub async fn unbind_story_session(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((story_id, binding_id)): Path<(String, String)>,
) -> Result<Json<UnboundBindingResponse>, ApiError> {
    let story_uuid: Uuid = story_id
        .parse()
        .map_err(|_| ApiError::BadRequest(format!("无效的 story_id: {story_id}")))?;
    let binding_uuid: Uuid = binding_id
        .parse()
        .map_err(|_| ApiError::BadRequest(format!("无效的 binding_id: {binding_id}")))?;
    load_story_and_project_with_permission(
        state.as_ref(),
        &current_user,
        story_uuid,
        ProjectPermission::Edit,
    )
    .await?;

    let binding_exists = state
        .repos
        .session_binding_repo
        .list_by_owner(SessionOwnerType::Story, story_uuid)
        .await?
        .into_iter()
        .any(|binding| binding.id == binding_uuid);
    if !binding_exists {
        return Err(ApiError::NotFound(format!(
            "Story Session binding {binding_id} 不存在"
        )));
    }

    state
        .repos
        .session_binding_repo
        .delete(binding_uuid)
        .await?;

    Ok(Json(UnboundBindingResponse {
        unbound: true,
        binding_id,
    }))
}
