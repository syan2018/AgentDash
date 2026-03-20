use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, State},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    app_state::AppState,
    routes::task_execution::{
        SessionEffectiveContext, SessionProjectDefaults, SessionStoryOverrides,
    },
    rpc::ApiError,
    session_plan::{
        resolve_effective_session_composition, summarize_runtime_policy,
        summarize_tool_visibility,
    },
};
use agentdash_domain::{project::Project, workspace::Workspace};
use agentdash_domain::session_binding::{SessionBinding, SessionOwnerType};
use agentdash_mcp::injection::McpInjectionConfig;

#[derive(Debug, Serialize)]
pub struct StorySessionContextSnapshot {
    pub project_defaults: SessionProjectDefaults,
    pub story_overrides: SessionStoryOverrides,
    pub effective: SessionEffectiveContext,
}

#[derive(Debug, Serialize)]
pub struct StorySessionDetailResponse {
    pub binding_id: String,
    pub session_id: String,
    pub label: String,
    pub session_title: Option<String>,
    pub last_activity: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address_space: Option<agentdash_executor::ExecutionAddressSpace>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_snapshot: Option<StorySessionContextSnapshot>,
}

#[derive(Debug)]
struct BuiltStorySessionContextResponse {
    address_space: Option<agentdash_executor::ExecutionAddressSpace>,
    context_snapshot: Option<StorySessionContextSnapshot>,
}

/// 返回给前端的 Session 绑定信息（含 Session 元数据）
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionBindingResponse {
    pub id: String,
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
    Path(story_id): Path<String>,
) -> Result<Json<Vec<SessionBindingResponse>>, ApiError> {
    let story_uuid: Uuid = story_id
        .parse()
        .map_err(|_| ApiError::BadRequest(format!("无效的 story_id: {story_id}")))?;

    let bindings = state
        .session_binding_repo
        .list_by_owner(SessionOwnerType::Story, story_uuid)
        .await?;

    let mut responses: Vec<SessionBindingResponse> = Vec::with_capacity(bindings.len());
    for binding in &bindings {
        let mut resp = SessionBindingResponse::from_binding(binding);
        if let Ok(Some(meta)) = state
            .executor_hub
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
    Path((story_id, binding_id)): Path<(String, String)>,
) -> Result<Json<StorySessionDetailResponse>, ApiError> {
    let story_uuid: Uuid = story_id
        .parse()
        .map_err(|_| ApiError::BadRequest(format!("无效的 story_id: {story_id}")))?;
    let binding_uuid: Uuid = binding_id
        .parse()
        .map_err(|_| ApiError::BadRequest(format!("无效的 binding_id: {binding_id}")))?;

    let story = state
        .story_repo
        .get_by_id(story_uuid)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Story {story_id} 不存在")))?;

    let bindings = state
        .session_binding_repo
        .list_by_owner(SessionOwnerType::Story, story_uuid)
        .await?;
    let binding = bindings
        .into_iter()
        .find(|item| item.id == binding_uuid)
        .ok_or_else(|| ApiError::NotFound(format!("Story Session binding {binding_id} 不存在")))?;

    let meta = state
        .executor_hub
        .get_session_meta(&binding.session_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let built_context = build_story_session_context_response(&state, &story).await;

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
#[serde(rename_all = "camelCase")]
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

/// POST /stories/{id}/sessions
pub async fn create_story_session(
    State(state): State<Arc<AppState>>,
    Path(story_id): Path<String>,
    Json(req): Json<CreateStorySessionRequest>,
) -> Result<Json<SessionBindingResponse>, ApiError> {
    let story_uuid: Uuid = story_id
        .parse()
        .map_err(|_| ApiError::BadRequest(format!("无效的 story_id: {story_id}")))?;

    state
        .story_repo
        .get_by_id(story_uuid)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Story {story_id} 不存在")))?;

    let label = req.label.unwrap_or_else(|| "companion".to_string());

    let session_id = match (req.session_id, req.title) {
        (Some(_), Some(_)) => {
            return Err(ApiError::BadRequest(
                "session_id 与 title 不能同时提供".into(),
            ));
        }
        (Some(sid), None) => {
            state
                .executor_hub
                .get_session_meta(&sid)
                .await
                .map_err(|e| ApiError::Internal(e.to_string()))?
                .ok_or_else(|| ApiError::NotFound(format!("Session {sid} 不存在")))?;
            sid
        }
        (None, title) => {
            let title = title.unwrap_or_else(|| "Story 伴随会话".to_string());
            let meta = state
                .executor_hub
                .create_session(&title)
                .await
                .map_err(|e| ApiError::Internal(e.to_string()))?;
            meta.id
        }
    };

    let binding = SessionBinding::new(
        session_id.clone(),
        SessionOwnerType::Story,
        story_uuid,
        &label,
    );
    state.session_binding_repo.create(&binding).await?;

    let mut resp = SessionBindingResponse::from_binding(&binding);
    if let Ok(Some(meta)) = state.executor_hub.get_session_meta(&session_id).await {
        resp.session_title = Some(meta.title);
        resp.session_updated_at = Some(meta.updated_at);
    }

    Ok(Json(resp))
}

/// DELETE /stories/{id}/sessions/{binding_id}
pub async fn unbind_story_session(
    State(state): State<Arc<AppState>>,
    Path((story_id, binding_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let _story_uuid: Uuid = story_id
        .parse()
        .map_err(|_| ApiError::BadRequest(format!("无效的 story_id: {story_id}")))?;
    let binding_uuid: Uuid = binding_id
        .parse()
        .map_err(|_| ApiError::BadRequest(format!("无效的 binding_id: {binding_id}")))?;

    state.session_binding_repo.delete(binding_uuid).await?;

    Ok(Json(serde_json::json!({
        "unbound": true,
        "binding_id": binding_id,
    })))
}

async fn build_story_session_context_response(
    state: &Arc<AppState>,
    story: &agentdash_domain::story::Story,
) -> Option<BuiltStorySessionContextResponse> {
    let project = state.project_repo.get_by_id(story.project_id).await.ok()??;
    let workspace = resolve_story_workspace(state, &project).await.ok()?;
    let default_agent_type = normalize_optional_string(project.config.default_agent_type.clone());
    let address_space = state
        .address_space_service
        .build_story_address_space(
            &project,
            story,
            workspace.as_ref(),
            default_agent_type.as_deref(),
        )
        .ok();
    let effective_mount_policy = story
        .context
        .mount_policy_override
        .clone()
        .unwrap_or_else(|| project.config.mount_policy.clone());
    let effective_session_composition = resolve_effective_session_composition(&project, Some(story));
    let mcp_servers = state
        .mcp_base_url
        .as_ref()
        .map(|base_url| {
            vec![McpInjectionConfig::for_story(base_url.clone(), story.project_id, story.id)
                .to_acp_mcp_server()]
        })
        .unwrap_or_default();
    let tool_visibility = summarize_tool_visibility(address_space.as_ref(), &mcp_servers);
    let runtime_policy = summarize_runtime_policy(
        workspace.is_some(),
        address_space.as_ref(),
        &mcp_servers,
        &tool_visibility.tool_names,
    );

    Some(BuiltStorySessionContextResponse {
        address_space,
        context_snapshot: Some(StorySessionContextSnapshot {
            project_defaults: SessionProjectDefaults {
                default_agent_type,
                context_containers: project.config.context_containers.clone(),
                mount_policy: project.config.mount_policy.clone(),
                session_composition: project.config.session_composition.clone(),
            },
            story_overrides: SessionStoryOverrides {
                context_containers: story.context.context_containers.clone(),
                disabled_container_ids: story.context.disabled_container_ids.clone(),
                mount_policy_override: story.context.mount_policy_override.clone(),
                session_composition_override: story.context.session_composition_override.clone(),
            },
            effective: SessionEffectiveContext {
                mount_policy: effective_mount_policy,
                session_composition: effective_session_composition,
                tool_visibility,
                runtime_policy,
            },
        }),
    })
}

async fn resolve_story_workspace(
    state: &Arc<AppState>,
    project: &Project,
) -> Result<Option<Workspace>, ApiError> {
    if let Some(workspace_id) = project.config.default_workspace_id {
        return state
            .workspace_repo
            .get_by_id(workspace_id)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()));
    }

    let workspaces = state
        .workspace_repo
        .list_by_project(project.id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(workspaces.into_iter().next())
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value.and_then(|item| {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}
