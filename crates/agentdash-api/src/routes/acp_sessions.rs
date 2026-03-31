use std::collections::HashSet;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::{
    Json,
    body::{Body, Bytes},
    extract::{Path, Query, State},
    http::HeaderMap,
    response::IntoResponse,
    response::sse::{Event, KeepAlive, Sse},
};
use futures::stream::Stream;
use serde::Deserialize;
use tokio::time::MissedTickBehavior;

use crate::{app_state::AppState, rpc::ApiError};
use agentdash_application::address_space::SessionMountTarget;
use agentdash_application::project::context_builder::{
    ProjectContextBuildInput, build_project_context_markdown, build_project_owner_prompt_blocks,
};
use agentdash_application::session::{
    PromptSessionRequest, SessionExecutionState, SessionMeta, UserPromptInput,
};
use agentdash_application::session_context::SessionContextSnapshot;
use agentdash_application::story::context_builder::{
    StoryContextBuildInput, build_story_context_markdown, build_story_owner_prompt_blocks,
};
use agentdash_domain::{
    project::Project, session_binding::SessionOwnerType, story::Story, workspace::Workspace,
};
use agentdash_mcp::injection::McpInjectionConfig;
use agentdash_plugin_api::AuthIdentity;
use agentdash_spi::HookSessionRuntimeSnapshot;
use serde::Serialize;

use super::project_agents::{resolve_project_agent_bridge_async, resolve_project_workspace};
use crate::auth::{
    CurrentUser, ProjectPermission, load_project_with_permission,
    load_story_and_project_with_permission, load_task_story_project_with_permission,
};
use crate::routes::{project_sessions, story_sessions, task_execution};
use crate::runtime_bridge::acp_mcp_servers_to_runtime;
use crate::task_agent_context::resolve_workspace_declared_sources;
use agentdash_application::session_context::apply_workspace_defaults;

const ACP_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(20);

#[derive(Debug, Deserialize)]
pub struct NdjsonStreamQuery {
    pub since_id: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct ListSessionsQuery {
    pub owner_type: Option<String>,
    pub owner_id: Option<String>,
    /// 为 true 时排除已绑定到 Story/Task 的会话，仅返回独立会话
    pub exclude_bound: Option<bool>,
}

pub async fn list_sessions(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Query(query): Query<ListSessionsQuery>,
) -> Result<Json<Vec<SessionMeta>>, ApiError> {
    if let (Some(owner_type_str), Some(owner_id_str)) = (&query.owner_type, &query.owner_id) {
        let owner_type = SessionOwnerType::from_str_loose(owner_type_str)
            .ok_or_else(|| ApiError::BadRequest(format!("无效的 owner_type: {owner_type_str}")))?;
        let owner_id: uuid::Uuid = owner_id_str
            .parse()
            .map_err(|_| ApiError::BadRequest(format!("无效的 owner_id: {owner_id_str}")))?;
        authorize_owner_scope(&state, &current_user, owner_type, owner_id).await?;

        let bindings = state
            .repos
            .session_binding_repo
            .list_by_owner(owner_type, owner_id)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;

        let mut sessions = Vec::with_capacity(bindings.len());
        for binding in &bindings {
            if let Ok(Some(meta)) = state
                .services
                .session_hub
                .get_session_meta(&binding.session_id)
                .await
            {
                sessions.push(meta);
            }
        }
        return Ok(Json(sessions));
    }

    let mut sessions = state
        .services
        .session_hub
        .list_sessions()
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let exclude_bound = query.exclude_bound.unwrap_or(false);
    let mut visible_sessions = Vec::with_capacity(sessions.len());
    for session in sessions.drain(..) {
        let bindings = state
            .repos
            .session_binding_repo
            .list_by_session(&session.id)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;

        if bindings.is_empty() {
            visible_sessions.push(session);
            continue;
        }

        if exclude_bound {
            continue;
        }

        ensure_bindings_permission(
            state.as_ref(),
            &current_user,
            &bindings,
            ProjectPermission::View,
        )
        .await?;
        visible_sessions.push(session);
    }

    Ok(Json(visible_sessions))
}

#[derive(Debug, Deserialize)]
pub struct CreateSessionRequest {
    #[serde(default)]
    pub title: Option<String>,
}

pub async fn create_session(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateSessionRequest>,
) -> Result<Json<SessionMeta>, ApiError> {
    let title = req.title.unwrap_or_else(|| "新会话".to_string());
    let meta = state
        .services
        .session_hub
        .create_session(&title)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(meta))
}

pub async fn get_session(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(session_id): Path<String>,
) -> Result<Json<SessionMeta>, ApiError> {
    ensure_session_permission(
        state.as_ref(),
        &current_user,
        &session_id,
        ProjectPermission::View,
    )
    .await?;
    let meta = state
        .services
        .session_hub
        .get_session_meta(&session_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("会话 {} 不存在", session_id)))?;
    Ok(Json(meta))
}

pub async fn get_session_hook_runtime(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(session_id): Path<String>,
) -> Result<Json<HookSessionRuntimeSnapshot>, ApiError> {
    ensure_session_permission(
        state.as_ref(),
        &current_user,
        &session_id,
        ProjectPermission::View,
    )
    .await?;
    let runtime = state
        .services
        .session_hub
        .ensure_hook_session_runtime(&session_id, None)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?
        .ok_or_else(|| {
            ApiError::NotFound(format!(
                "session {} 当前没有可用的 hook runtime",
                session_id
            ))
        })?;
    Ok(Json(runtime.runtime_snapshot()))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct SessionExecutionStateResponse {
    pub session_id: String,
    pub status: String,
    pub turn_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

pub async fn get_session_state(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(session_id): Path<String>,
) -> Result<Json<SessionExecutionStateResponse>, ApiError> {
    ensure_session_permission(
        state.as_ref(),
        &current_user,
        &session_id,
        ProjectPermission::View,
    )
    .await?;
    state
        .services
        .session_hub
        .get_session_meta(&session_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("会话 {} 不存在", session_id)))?;

    let execution_state = state
        .services
        .session_hub
        .inspect_session_execution_state(&session_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let response = match execution_state {
        SessionExecutionState::Idle => SessionExecutionStateResponse {
            session_id,
            status: "idle".to_string(),
            turn_id: None,
            message: None,
        },
        SessionExecutionState::Running { turn_id } => SessionExecutionStateResponse {
            session_id,
            status: "running".to_string(),
            turn_id,
            message: None,
        },
        SessionExecutionState::Completed { turn_id } => SessionExecutionStateResponse {
            session_id,
            status: "completed".to_string(),
            turn_id: Some(turn_id),
            message: None,
        },
        SessionExecutionState::Failed { turn_id, message } => SessionExecutionStateResponse {
            session_id,
            status: "failed".to_string(),
            turn_id: Some(turn_id),
            message,
        },
        SessionExecutionState::Interrupted { turn_id, message } => SessionExecutionStateResponse {
            session_id,
            status: "interrupted".to_string(),
            turn_id,
            message,
        },
    };

    Ok(Json(response))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct SessionBindingOwnerResponse {
    pub id: String,
    pub session_id: String,
    pub owner_type: String,
    pub owner_id: String,
    pub label: String,
    pub created_at: String,
    pub owner_title: Option<String>,
    pub project_id: String,
    pub story_id: Option<String>,
    pub task_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_binding_owner_response_serializes_as_snake_case() {
        let value = serde_json::to_value(SessionBindingOwnerResponse {
            id: "binding-1".to_string(),
            session_id: "sess-1".to_string(),
            owner_type: "project".to_string(),
            owner_id: "project-1".to_string(),
            label: "project_agent:default".to_string(),
            created_at: "2026-03-20T00:00:00Z".to_string(),
            owner_title: Some("AgentDash".to_string()),
            project_id: "project-1".to_string(),
            story_id: None,
            task_id: None,
        })
        .expect("serialize session binding owner response");

        assert!(value.get("session_id").is_some());
        assert!(value.get("owner_title").is_some());
        assert!(value.get("project_id").is_some());
        assert!(value.get("sessionId").is_none());
        assert!(value.get("ownerTitle").is_none());
        assert!(value.get("projectId").is_none());
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct SessionContextResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_binding: Option<agentdash_domain::task::AgentBinding>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address_space: Option<agentdash_spi::AddressSpace>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_snapshot: Option<SessionContextSnapshot>,
}

/// GET /sessions/{id}/context — 按会话绑定统一返回 workspace / agent_binding / address_space / snapshot
pub async fn get_session_context(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(session_id): Path<String>,
) -> Result<Json<SessionContextResponse>, ApiError> {
    let bindings = ensure_session_permission(
        state.as_ref(),
        &current_user,
        &session_id,
        ProjectPermission::View,
    )
    .await?;

    let Some(primary) = pick_primary_session_binding(&bindings) else {
        return Ok(Json(SessionContextResponse {
            workspace_id: None,
            agent_binding: None,
            address_space: None,
            context_snapshot: None,
        }));
    };

    match primary.owner_type {
        SessionOwnerType::Task => {
            let task_id = primary.owner_id;
            let (task, _, _) = load_task_story_project_with_permission(
                state.as_ref(),
                &current_user,
                task_id,
                ProjectPermission::View,
            )
            .await?;
            let result = state
                .services
                .task_lifecycle_service
                .get_task_session(task_id)
                .await
                .map_err(task_execution::map_task_execution_error)?;
            let built_context =
                agentdash_application::task::context_builder::build_task_session_context(
                    &state.repos,
                    &state.services.address_space_service,
                    state.config.mcp_base_url.as_deref(),
                    task_id,
                )
                .await;
            Ok(Json(SessionContextResponse {
                workspace_id: task.workspace_id.map(|id| id.to_string()),
                agent_binding: Some(result.agent_binding),
                address_space: built_context
                    .as_ref()
                    .and_then(|context| context.address_space.clone()),
                context_snapshot: built_context.and_then(|context| context.context_snapshot),
            }))
        }
        SessionOwnerType::Story => {
            let story_id = primary.owner_id;
            let (story, _) = load_story_and_project_with_permission(
                state.as_ref(),
                &current_user,
                story_id,
                ProjectPermission::View,
            )
            .await?;
            let built_context =
                story_sessions::build_story_session_context_response(&state, &story, &session_id)
                    .await;
            Ok(Json(SessionContextResponse {
                workspace_id: None,
                agent_binding: None,
                address_space: built_context
                    .as_ref()
                    .and_then(|context| context.address_space.clone()),
                context_snapshot: built_context.and_then(|context| context.context_snapshot),
            }))
        }
        SessionOwnerType::Project => {
            let project_id = primary.owner_id;
            let project = load_project_with_permission(
                state.as_ref(),
                &current_user,
                project_id,
                ProjectPermission::View,
            )
            .await?;
            let built_context = project_sessions::build_project_session_context_response(
                &state,
                &project,
                &session_id,
                &primary.label,
            )
            .await;
            Ok(Json(SessionContextResponse {
                workspace_id: None,
                agent_binding: None,
                address_space: built_context
                    .as_ref()
                    .and_then(|context| context.address_space.clone()),
                context_snapshot: built_context.and_then(|context| context.context_snapshot),
            }))
        }
    }
}

fn pick_primary_session_binding(
    bindings: &[agentdash_domain::session_binding::SessionBinding],
) -> Option<&agentdash_domain::session_binding::SessionBinding> {
    // 与 `SessionPage.tsx` 中 `sessionOwnerBinding` 一致：project → story → task → 首个
    bindings
        .iter()
        .find(|b| b.owner_type == SessionOwnerType::Project)
        .or_else(|| {
            bindings
                .iter()
                .find(|b| b.owner_type == SessionOwnerType::Story)
        })
        .or_else(|| {
            bindings
                .iter()
                .find(|b| b.owner_type == SessionOwnerType::Task)
        })
        .or_else(|| bindings.first())
}

pub async fn get_session_bindings(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(session_id): Path<String>,
) -> Result<Json<Vec<SessionBindingOwnerResponse>>, ApiError> {
    let bindings = ensure_session_permission(
        state.as_ref(),
        &current_user,
        &session_id,
        ProjectPermission::View,
    )
    .await?;

    let mut responses = Vec::with_capacity(bindings.len());
    for binding in bindings {
        let mut owner_title = None;
        let project_id = binding.project_id.to_string();
        let mut story_id = None;
        let mut task_id = None;

        match binding.owner_type {
            SessionOwnerType::Project => {
                if let Some(project) = state
                    .repos
                    .project_repo
                    .get_by_id(binding.owner_id)
                    .await
                    .map_err(|e| ApiError::Internal(e.to_string()))?
                {
                    owner_title = Some(project.name);
                }
            }
            SessionOwnerType::Story => {
                if let Some(story) = state
                    .repos
                    .story_repo
                    .get_by_id(binding.owner_id)
                    .await
                    .map_err(|e| ApiError::Internal(e.to_string()))?
                {
                    owner_title = Some(story.title);
                    story_id = Some(story.id.to_string());
                }
            }
            SessionOwnerType::Task => {
                if let Some(task) = state
                    .repos
                    .task_repo
                    .get_by_id(binding.owner_id)
                    .await
                    .map_err(|e| ApiError::Internal(e.to_string()))?
                {
                    owner_title = Some(task.title);
                    story_id = Some(task.story_id.to_string());
                    task_id = Some(task.id.to_string());
                }
            }
        }

        responses.push(SessionBindingOwnerResponse {
            id: binding.id.to_string(),
            session_id: binding.session_id,
            owner_type: binding.owner_type.to_string(),
            owner_id: binding.owner_id.to_string(),
            label: binding.label,
            created_at: binding.created_at.to_rfc3339(),
            owner_title,
            project_id,
            story_id,
            task_id,
        });
    }

    Ok(Json(responses))
}

pub async fn delete_session(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(session_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let bindings = ensure_session_permission(
        state.as_ref(),
        &current_user,
        &session_id,
        ProjectPermission::Edit,
    )
    .await?;
    state
        .services
        .session_hub
        .delete_session(&session_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    for binding in bindings {
        state
            .repos
            .session_binding_repo
            .delete(binding.id)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;
    }
    Ok(Json(
        serde_json::json!({ "deleted": true, "sessionId": session_id }),
    ))
}

pub async fn prompt_session(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(session_id): Path<String>,
    Json(user_input): Json<UserPromptInput>,
) -> Result<Json<serde_json::Value>, ApiError> {
    ensure_session_permission(
        state.as_ref(),
        &current_user,
        &session_id,
        ProjectPermission::Edit,
    )
    .await?;
    let mut req = augment_prompt_request_for_owner(
        &state,
        &session_id,
        PromptSessionRequest::from_user_input(user_input),
    )
    .await?;
    req.identity = Some(current_user);
    let turn_id = state
        .services
        .session_hub
        .start_prompt(&session_id, req)
        .await
        .map_err(|e| match &e {
            agentdash_spi::ConnectorError::InvalidConfig(_) => ApiError::BadRequest(e.to_string()),
            _ => ApiError::Internal(e.to_string()),
        })?;

    Ok(Json(
        serde_json::json!({ "started": true, "sessionId": session_id, "turnId": turn_id }),
    ))
}

async fn augment_prompt_request_for_owner(
    state: &Arc<AppState>,
    session_id: &str,
    req: PromptSessionRequest,
) -> Result<PromptSessionRequest, ApiError> {
    let bindings = state
        .repos
        .session_binding_repo
        .list_by_session(session_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    if let Some(binding) = bindings
        .iter()
        .find(|binding| binding.owner_type == SessionOwnerType::Story)
    {
        let story = state
            .repos
            .story_repo
            .get_by_id(binding.owner_id)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?
            .ok_or_else(|| ApiError::NotFound(format!("Story {} 不存在", binding.owner_id)))?;
        let project = state
            .repos
            .project_repo
            .get_by_id(story.project_id)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?
            .ok_or_else(|| ApiError::NotFound(format!("Project {} 不存在", story.project_id)))?;
        let workspace = resolve_project_workspace(state, &project).await?;

        return build_story_owner_prompt_request(state, req, &story, &project, workspace.as_ref())
            .await;
    }

    if let Some(binding) = bindings
        .iter()
        .find(|binding| binding.owner_type == SessionOwnerType::Project)
    {
        let project = state
            .repos
            .project_repo
            .get_by_id(binding.owner_id)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?
            .ok_or_else(|| ApiError::NotFound(format!("Project {} 不存在", binding.owner_id)))?;

        return build_project_owner_prompt_request(state, req, &project, &binding.label).await;
    }

    Ok(req)
}

/// Shared finalization step for all owner-type prompt augmentations.
///
/// Applies workspace defaults, assigns address space / MCP / flow capabilities,
/// and injects system context + prompt blocks into the request.
fn finalize_augmented_request(
    req: &mut PromptSessionRequest,
    context_markdown: String,
    prompt_blocks: Vec<serde_json::Value>,
    workspace: Option<&Workspace>,
    address_space: Option<agentdash_spi::AddressSpace>,
    effective_mcp_servers: Vec<agent_client_protocol::McpServer>,
    flow_capabilities: agentdash_spi::FlowCapabilities,
) {
    req.user_input.prompt = None;
    req.user_input.prompt_blocks = Some(prompt_blocks);
    req.system_context = Some(context_markdown);

    apply_workspace_defaults(
        &mut req.user_input.working_dir,
        &mut req.workspace_root,
        workspace,
    );
    if req.address_space.is_none() {
        req.address_space = address_space;
    }
    req.mcp_servers = effective_mcp_servers;
    req.flow_capabilities = Some(flow_capabilities);
}

async fn build_story_owner_prompt_request(
    state: &Arc<AppState>,
    mut req: PromptSessionRequest,
    story: &Story,
    project: &Project,
    workspace: Option<&Workspace>,
) -> Result<PromptSessionRequest, ApiError> {
    let address_space = match req.address_space.clone() {
        Some(address_space) => Some(address_space),
        None => {
            let agent_type = req
                .user_input
                .executor_config
                .as_ref()
                .map(|config| config.executor.as_str())
                .or(project.config.default_agent_type.as_deref());
            Some(
                state
                    .services
                    .address_space_service
                    .build_address_space(
                        project,
                        Some(story),
                        workspace,
                        SessionMountTarget::Story,
                        agent_type,
                    )
                    .map_err(ApiError::BadRequest)?,
            )
        }
    };
    let effective_agent_type = req
        .user_input
        .executor_config
        .as_ref()
        .map(|config| config.executor.as_str())
        .or(project.config.default_agent_type.as_deref());
    let mut effective_mcp_servers = req.mcp_servers.clone();
    let base_url = state
        .config
        .mcp_base_url
        .clone()
        .unwrap_or_else(|| "http://127.0.0.1:3001".to_string());
    effective_mcp_servers
        .push(McpInjectionConfig::for_story(base_url, project.id, story.id).to_acp_mcp_server());
    let runtime_address_space = address_space.clone();
    let runtime_mcp_servers = acp_mcp_servers_to_runtime(&effective_mcp_servers);

    let resolved_workspace_sources =
        resolve_workspace_declared_sources(state, &story.context.source_refs, workspace, 60)
            .await
            .map_err(ApiError::BadRequest)?;

    let (context_markdown, _) = build_story_context_markdown(StoryContextBuildInput {
        story,
        project,
        workspace,
        address_space: runtime_address_space.as_ref(),
        mcp_servers: &runtime_mcp_servers,
        effective_agent_type,
        workspace_source_fragments: resolved_workspace_sources.fragments,
        workspace_source_warnings: resolved_workspace_sources.warnings,
    });
    let prompt_blocks = build_story_owner_prompt_blocks(
        story.id,
        context_markdown.clone(),
        req.user_input.prompt.take(),
        req.user_input.prompt_blocks.take(),
    );

    finalize_augmented_request(
        &mut req,
        context_markdown,
        prompt_blocks,
        workspace,
        address_space,
        effective_mcp_servers,
        agentdash_spi::FlowCapabilities {
            workflow_artifact: true,
            companion_dispatch: true,
            companion_complete: true,
            resolve_hook_action: true,
        },
    );

    Ok(req)
}

async fn build_project_owner_prompt_request(
    state: &Arc<AppState>,
    mut req: PromptSessionRequest,
    project: &Project,
    binding_label: &str,
) -> Result<PromptSessionRequest, ApiError> {
    let agent_key = binding_label
        .trim()
        .strip_prefix("project_agent:")
        .unwrap_or_default();
    let project_agent = resolve_project_agent_bridge_async(state, project.id, agent_key)
        .await
        .ok_or_else(|| ApiError::NotFound(format!("Project Agent `{agent_key}` 不存在")))?;
    let workspace = resolve_project_workspace(state, project).await?;

    let effective_executor_config = req
        .user_input
        .executor_config
        .clone()
        .unwrap_or_else(|| project_agent.executor_config.clone());
    let effective_agent_type = Some(effective_executor_config.executor.as_str());
    let address_space = match req.address_space.clone() {
        Some(address_space) => Some(address_space),
        None => Some(
            state
                .services
                .address_space_service
                .build_address_space(
                    project,
                    None,
                    workspace.as_ref(),
                    SessionMountTarget::Project,
                    effective_agent_type,
                )
                .map_err(ApiError::BadRequest)?,
        ),
    };

    let mut effective_mcp_servers = req.mcp_servers.clone();
    let base_url = state
        .config
        .mcp_base_url
        .clone()
        .unwrap_or_else(|| "http://127.0.0.1:3001".to_string());
    effective_mcp_servers
        .push(McpInjectionConfig::for_relay(base_url, project.id).to_acp_mcp_server());
    // Inject Http/SSE MCP servers from the preset config
    effective_mcp_servers.extend(project_agent.preset_mcp_servers.iter().cloned());
    // Inject Stdio MCP servers (deserialized from relay JSON decls)
    for decl in &project_agent.preset_stdio_mcp_decls {
        if let Ok(server) = serde_json::from_value::<agent_client_protocol::McpServer>(decl.clone())
        {
            effective_mcp_servers.push(server);
        }
    }
    let runtime_address_space = address_space.clone();
    let runtime_mcp_servers = acp_mcp_servers_to_runtime(&effective_mcp_servers);

    let (context_markdown, _) = build_project_context_markdown(ProjectContextBuildInput {
        project,
        workspace: workspace.as_ref(),
        address_space: runtime_address_space.as_ref(),
        mcp_servers: &runtime_mcp_servers,
        effective_agent_type,
        preset_name: project_agent.preset_name.as_deref(),
        agent_display_name: project_agent.display_name.as_str(),
    });
    let prompt_blocks = build_project_owner_prompt_blocks(
        project.id,
        context_markdown.clone(),
        req.user_input.prompt.take(),
        req.user_input.prompt_blocks.take(),
    );

    if req.user_input.executor_config.is_none() {
        req.user_input.executor_config = Some(effective_executor_config);
    }

    finalize_augmented_request(
        &mut req,
        context_markdown,
        prompt_blocks,
        workspace.as_ref(),
        address_space,
        effective_mcp_servers,
        agentdash_spi::FlowCapabilities {
            workflow_artifact: false,
            companion_dispatch: false,
            companion_complete: false,
            resolve_hook_action: true,
        },
    );

    Ok(req)
}

pub async fn cancel_session(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(session_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    ensure_session_permission(
        state.as_ref(),
        &current_user,
        &session_id,
        ProjectPermission::Edit,
    )
    .await?;
    state
        .services
        .session_hub
        .cancel(&session_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let execution_state = state
        .services
        .session_hub
        .inspect_session_execution_state(&session_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let state_payload = match execution_state {
        SessionExecutionState::Idle => serde_json::json!({ "status": "idle" }),
        SessionExecutionState::Running { turn_id } => {
            serde_json::json!({ "status": "running", "turn_id": turn_id })
        }
        SessionExecutionState::Completed { turn_id } => {
            serde_json::json!({ "status": "completed", "turn_id": turn_id })
        }
        SessionExecutionState::Failed { turn_id, message } => {
            serde_json::json!({ "status": "failed", "turn_id": turn_id, "message": message })
        }
        SessionExecutionState::Interrupted { turn_id, message } => {
            serde_json::json!({ "status": "interrupted", "turn_id": turn_id, "message": message })
        }
    };

    Ok(Json(serde_json::json!({
        "cancelled": true,
        "sessionId": session_id,
        "state": state_payload,
    })))
}

#[derive(Debug, Deserialize)]
pub struct RejectToolApprovalRequest {
    #[serde(default)]
    pub reason: Option<String>,
}

pub async fn approve_tool_call(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((session_id, tool_call_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    ensure_session_permission(
        state.as_ref(),
        &current_user,
        &session_id,
        ProjectPermission::Edit,
    )
    .await?;
    state
        .services
        .session_hub
        .approve_tool_call(&session_id, &tool_call_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(serde_json::json!({
        "approved": true,
        "sessionId": session_id,
        "toolCallId": tool_call_id,
    })))
}

pub async fn reject_tool_call(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((session_id, tool_call_id)): Path<(String, String)>,
    Json(req): Json<RejectToolApprovalRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    ensure_session_permission(
        state.as_ref(),
        &current_user,
        &session_id,
        ProjectPermission::Edit,
    )
    .await?;
    state
        .services
        .session_hub
        .reject_tool_call(&session_id, &tool_call_id, req.reason.clone())
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(serde_json::json!({
        "rejected": true,
        "sessionId": session_id,
        "toolCallId": tool_call_id,
    })))
}

/// ACP 会话流（Streaming HTTP / SSE）
pub async fn acp_session_stream_sse(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(session_id): Path<String>,
    headers: HeaderMap,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    ensure_session_permission(
        state.as_ref(),
        &current_user,
        &session_id,
        ProjectPermission::View,
    )
    .await?;
    let last_event_id = headers
        .get("last-event-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

    tracing::info!(
        session_id = %session_id,
        last_event_id = last_event_id,
        "ACP 会话流连接建立（SSE）"
    );

    let (history, mut rx) = state
        .services
        .session_hub
        .subscribe_with_history(&session_id)
        .await;
    let start_index = std::cmp::min(last_event_id as usize, history.len());
    let replayed = history.len().saturating_sub(start_index);
    tracing::info!(
        session_id = %session_id,
        replayed_count = replayed,
        history_total = history.len(),
        "ACP 会话流历史补发完成（SSE）"
    );

    let stream = async_stream::stream! {
        for (i, n) in history.iter().enumerate().skip(start_index) {
            let id = (i as u64) + 1;
            if let Ok(json) = serde_json::to_string(n) {
                yield Ok(Event::default().id(id.to_string()).data(json));
            }
        }

        let mut seq = history.len() as u64;
        loop {
            match rx.recv().await {
                Ok(n) => {
                    seq += 1;
                    if let Ok(json) = serde_json::to_string(&n) {
                        yield Ok(Event::default().id(seq.to_string()).data(json));
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(
                        session_id = %session_id,
                        lagged = n,
                        "ACP 会话流订阅落后，部分消息被跳过（SSE）"
                    );
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    tracing::info!(
                        session_id = %session_id,
                        last_seq = seq,
                        "ACP 会话流连接关闭：广播通道关闭（SSE）"
                    );
                    break;
                }
            }
        }
    };

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

/// ACP 会话流（Fetch Streaming / NDJSON）
pub async fn acp_session_stream_ndjson(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(session_id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<NdjsonStreamQuery>,
) -> Result<impl IntoResponse, ApiError> {
    ensure_session_permission(
        state.as_ref(),
        &current_user,
        &session_id,
        ProjectPermission::View,
    )
    .await?;
    let resume_from = parse_ndjson_resume_from(&headers, query.since_id);
    tracing::info!(
        session_id = %session_id,
        resume_from = resume_from,
        "ACP 会话流连接建立（NDJSON）"
    );

    let (history, mut rx) = state
        .services
        .session_hub
        .subscribe_with_history(&session_id)
        .await;
    let start_index = std::cmp::min(resume_from as usize, history.len());
    let replayed = history.len().saturating_sub(start_index);
    tracing::info!(
        session_id = %session_id,
        replayed_count = replayed,
        history_total = history.len(),
        "ACP 会话流历史补发完成（NDJSON）"
    );

    let stream = async_stream::stream! {
        let mut seq = history.len() as u64;
        if let Some(line) = to_ndjson_line(&serde_json::json!({
            "type": "connected",
            "last_event_id": seq,
        })) {
            yield Ok::<Bytes, Infallible>(line);
        }

        for (i, n) in history.iter().enumerate().skip(start_index) {
            let id = (i as u64) + 1;
            if let Some(line) = to_ndjson_line(&serde_json::json!({
                "type": "notification",
                "id": id,
                "notification": n,
            })) {
                yield Ok::<Bytes, Infallible>(line);
            }
        }

        let mut heartbeat_tick = tokio::time::interval(ACP_HEARTBEAT_INTERVAL);
        heartbeat_tick.set_missed_tick_behavior(MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                next = rx.recv() => {
                    match next {
                        Ok(n) => {
                            seq += 1;
                            if let Some(line) = to_ndjson_line(&serde_json::json!({
                                "type": "notification",
                                "id": seq,
                                "notification": n,
                            })) {
                                yield Ok::<Bytes, Infallible>(line);
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!(
                                session_id = %session_id,
                                lagged = n,
                                "ACP 会话流订阅落后，部分消息被跳过（NDJSON）"
                            );
                            continue;
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            tracing::info!(
                                session_id = %session_id,
                                last_seq = seq,
                                "ACP 会话流连接关闭：广播通道关闭（NDJSON）"
                            );
                            break;
                        }
                    }
                }
                _ = heartbeat_tick.tick() => {
                    if let Some(line) = to_ndjson_line(&serde_json::json!({
                        "type": "heartbeat",
                        "timestamp": chrono::Utc::now().timestamp_millis(),
                    })) {
                        yield Ok::<Bytes, Infallible>(line);
                    }
                }
            }
        }
    };

    Ok((
        [
            (
                axum::http::header::CONTENT_TYPE,
                "application/x-ndjson; charset=utf-8",
            ),
            (axum::http::header::CACHE_CONTROL, "no-cache, no-transform"),
            (axum::http::header::CONNECTION, "keep-alive"),
            (axum::http::header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        Body::from_stream(stream),
    ))
}

fn parse_ndjson_resume_from(headers: &HeaderMap, query_since_id: Option<u64>) -> u64 {
    headers
        .get("x-stream-since-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .or(query_since_id)
        .unwrap_or(0)
}

fn to_ndjson_line(value: &serde_json::Value) -> Option<Bytes> {
    match serde_json::to_vec(value) {
        Ok(mut bytes) => {
            bytes.push(b'\n');
            Some(Bytes::from(bytes))
        }
        Err(err) => {
            tracing::error!(error = %err, "序列化 ACP NDJSON 消息失败");
            None
        }
    }
}

async fn authorize_owner_scope(
    state: &Arc<AppState>,
    current_user: &AuthIdentity,
    owner_type: SessionOwnerType,
    owner_id: uuid::Uuid,
) -> Result<(), ApiError> {
    match owner_type {
        SessionOwnerType::Project => {
            load_project_with_permission(
                state.as_ref(),
                current_user,
                owner_id,
                ProjectPermission::View,
            )
            .await?;
        }
        SessionOwnerType::Story => {
            load_story_and_project_with_permission(
                state.as_ref(),
                current_user,
                owner_id,
                ProjectPermission::View,
            )
            .await?;
        }
        SessionOwnerType::Task => {
            load_task_story_project_with_permission(
                state.as_ref(),
                current_user,
                owner_id,
                ProjectPermission::View,
            )
            .await?;
        }
    }
    Ok(())
}

async fn ensure_session_permission(
    state: &AppState,
    current_user: &AuthIdentity,
    session_id: &str,
    permission: ProjectPermission,
) -> Result<Vec<agentdash_domain::session_binding::SessionBinding>, ApiError> {
    let bindings = state
        .repos
        .session_binding_repo
        .list_by_session(session_id)
        .await?;
    ensure_bindings_permission(state, current_user, &bindings, permission).await?;
    Ok(bindings)
}

async fn ensure_bindings_permission(
    state: &AppState,
    current_user: &AuthIdentity,
    bindings: &[agentdash_domain::session_binding::SessionBinding],
    permission: ProjectPermission,
) -> Result<(), ApiError> {
    let mut visited_project_ids = HashSet::new();
    for binding in bindings {
        if visited_project_ids.insert(binding.project_id) {
            load_project_with_permission(state, current_user, binding.project_id, permission)
                .await?;
        }
    }
    Ok(())
}
