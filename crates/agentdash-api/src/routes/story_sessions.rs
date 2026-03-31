use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, State},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use agentdash_application::bootstrap_plan::{
    BootstrapOwnerVariant, BootstrapPlanInput, build_bootstrap_plan,
    derive_session_context_snapshot,
};
use agentdash_application::session_context::{
    SessionContextSnapshot, extract_story_overrides, normalize_optional_string,
};

use crate::{
    app_state::AppState,
    auth::{CurrentUser, ProjectPermission, load_story_and_project_with_permission},
    routes::project_agents::resolve_project_workspace,
    rpc::ApiError,
    runtime_bridge::acp_mcp_servers_to_runtime,
};
use agentdash_application::address_space::{SessionMountTarget, append_canvas_mounts};
use agentdash_domain::session_binding::{SessionBinding, SessionOwnerType};
use agentdash_mcp::injection::McpInjectionConfig;

#[derive(Debug, Serialize)]
pub struct StorySessionDetailResponse {
    pub binding_id: String,
    pub session_id: String,
    pub label: String,
    pub session_title: Option<String>,
    pub last_activity: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address_space: Option<agentdash_spi::AddressSpace>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_snapshot: Option<SessionContextSnapshot>,
}

#[derive(Debug)]
pub(crate) struct BuiltStorySessionContextResponse {
    pub(crate) address_space: Option<agentdash_spi::AddressSpace>,
    pub(crate) context_snapshot: Option<SessionContextSnapshot>,
}

/// 返回给前端的 Session 绑定信息（含 Session 元数据）
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct SessionBindingResponse {
    pub id: String,
    pub project_id: String,
    pub session_id: String,
    pub owner_type: String,
    pub owner_id: String,
    pub label: String,
    pub created_at: String,
    pub session_title: Option<String>,
    pub session_updated_at: Option<i64>,
}

impl SessionBindingResponse {
    fn from_binding(binding: &SessionBinding) -> Self {
        Self {
            id: binding.id.to_string(),
            project_id: binding.project_id.to_string(),
            session_id: binding.session_id.clone(),
            owner_type: binding.owner_type.to_string(),
            owner_id: binding.owner_id.to_string(),
            label: binding.label.clone(),
            created_at: binding.created_at.to_rfc3339(),
            session_title: None,
            session_updated_at: None,
        }
    }
}

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
            .session_hub
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

    let (story, _) = load_story_and_project_with_permission(
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
        .session_hub
        .get_session_meta(&binding.session_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let built_context =
        build_story_session_context_response(&state, &story, &binding.session_id).await;

    Ok(Json(StorySessionDetailResponse {
        binding_id,
        session_id: binding.session_id,
        label: binding.label,
        session_title: meta.as_ref().map(|item| item.title.clone()),
        last_activity: meta.as_ref().map(|item| item.updated_at),
        address_space: built_context
            .as_ref()
            .and_then(|context| context.address_space.clone()),
        context_snapshot: built_context.and_then(|context| context.context_snapshot),
    }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateStorySessionRequest {
    /// 绑定已有 Session：传 session_id
    #[serde(default)]
    pub session_id: Option<String>,
    /// 新建 Session：传 title（与 session_id 互斥）
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
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

    let label = req.label.unwrap_or_else(|| "companion".to_string());

    let session_id = match (req.session_id, req.title) {
        (Some(_), Some(_)) => {
            return Err(ApiError::BadRequest(
                "session_id 与 title 不能同时提供".into(),
            ));
        }
        (Some(sid), None) => {
            state
                .services
                .session_hub
                .get_session_meta(&sid)
                .await
                .map_err(|e| ApiError::Internal(e.to_string()))?
                .ok_or_else(|| ApiError::NotFound(format!("Session {sid} 不存在")))?;
            sid
        }
        (None, title) => {
            let title = title.unwrap_or_else(|| "Story 伴随会话".to_string());
            let meta = state
                .services
                .session_hub
                .create_session(&title)
                .await
                .map_err(|e| ApiError::Internal(e.to_string()))?;
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

    let mut resp = SessionBindingResponse::from_binding(&binding);
    if let Ok(Some(meta)) = state
        .services
        .session_hub
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
) -> Result<Json<serde_json::Value>, ApiError> {
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

    Ok(Json(serde_json::json!({
        "unbound": true,
        "binding_id": binding_id,
    })))
}

pub(crate) async fn build_story_session_context_response(
    state: &Arc<AppState>,
    story: &agentdash_domain::story::Story,
    session_id: &str,
) -> Option<BuiltStorySessionContextResponse> {
    let project = state
        .repos
        .project_repo
        .get_by_id(story.project_id)
        .await
        .ok()??;
    let workspace = resolve_project_workspace(state, &project)
        .await
        .ok()
        .flatten();
    let session_meta = state
        .services
        .session_hub
        .get_session_meta(session_id)
        .await
        .ok()??;

    let connector_config = session_meta.executor_config.clone();
    let resolved_config = connector_config.clone();
    let default_agent_type = normalize_optional_string(project.config.default_agent_type.clone());
    let effective_agent_type = resolved_config
        .as_ref()
        .and_then(|c| normalize_optional_string(Some(c.executor.clone())))
        .or(default_agent_type.clone());
    let use_address_space = connector_config
        .as_ref()
        .is_some_and(|c| c.is_cloud_native())
        || (resolved_config.is_none() && default_agent_type.is_some());
    let address_space = if use_address_space {
        let mut address_space = state
            .services
            .address_space_service
            .build_address_space(
                &project,
                Some(story),
                workspace.as_ref(),
                SessionMountTarget::Story,
                effective_agent_type.as_deref(),
            )
            .ok()?;
        let canvases = state
            .repos
            .canvas_repo
            .list_by_project(project.id)
            .await
            .ok()?;
        append_canvas_mounts(&mut address_space, &canvases);
        Some(address_space)
    } else {
        None
    };
    let effective_mcp_servers = state
        .config
        .mcp_base_url
        .as_ref()
        .map(|base_url| {
            vec![
                McpInjectionConfig::for_story(base_url.clone(), story.project_id, story.id)
                    .to_acp_mcp_server(),
            ]
        })
        .unwrap_or_default();

    let executor_source = if session_meta.executor_config.is_some() {
        "session.meta.executor_config"
    } else if effective_agent_type.is_some() {
        "project.config.default_agent_type"
    } else {
        "unresolved"
    };

    let story_overrides = extract_story_overrides(story);

    let runtime_address_space = address_space.clone();

    let plan = build_bootstrap_plan(BootstrapPlanInput {
        project,
        story: Some(story.clone()),
        workspace,
        resolved_config,
        address_space: runtime_address_space,
        mcp_servers: acp_mcp_servers_to_runtime(&effective_mcp_servers),
        working_dir: None,
        workspace_root: None,
        executor_preset_name: None,
        executor_source: executor_source.to_string(),
        executor_resolution_error: None,
        owner_variant: BootstrapOwnerVariant::Story { story_overrides },
        workflow: None,
    });

    let snapshot = derive_session_context_snapshot(&plan);

    Some(BuiltStorySessionContextResponse {
        address_space: plan.address_space.clone(),
        context_snapshot: Some(snapshot),
    })
}
