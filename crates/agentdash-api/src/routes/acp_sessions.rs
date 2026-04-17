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
use agentdash_application::vfs::SessionMountTarget;
use agentdash_application::canvas::append_visible_canvas_mounts;
use agentdash_application::project::context_builder::{
    ProjectContextBuildInput, build_project_context_markdown, build_project_owner_prompt_blocks,
};
use agentdash_application::session::context::SessionContextSnapshot;
use agentdash_application::session::{
    PromptSessionRequest, SessionBootstrapAction, SessionExecutionState, SessionMeta,
    SessionPromptLifecycle, SessionRepositoryRehydrateMode, UserPromptInput,
    resolve_session_prompt_lifecycle,
};
use agentdash_application::story::context_builder::{
    StoryContextBuildInput, build_story_context_markdown, build_story_owner_prompt_blocks,
};
use agentdash_application::task::execution::ExecutionPhase;
use agentdash_application::task::gateway::{TaskTurnServices, prepare_task_turn_context};
use agentdash_domain::{
    project::Project, session_binding::SessionOwnerType, story::Story, workspace::Workspace,
};
use agentdash_mcp::injection::McpInjectionConfig;
use agentdash_plugin_api::AuthIdentity;
use agentdash_spi::HookSessionRuntimeSnapshot;
use serde::Serialize;

use super::project_agents::{
    parse_project_agent_session_label, resolve_project_agent_bridge_async,
    resolve_project_workspace,
};
use crate::auth::{
    CurrentUser, ProjectPermission, load_project_with_permission,
    load_story_and_project_with_permission, load_task_story_project_with_permission,
};
use crate::routes::{project_sessions, story_sessions, task_execution};
use crate::routes::vfs_surfaces::build_surface_summary;
use crate::runtime_bridge::{acp_mcp_servers_to_runtime, runtime_mcp_servers_to_acp};
use crate::task_agent_context::resolve_workspace_declared_sources;
use agentdash_application::session::context::apply_workspace_defaults;

const ACP_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(20);

#[derive(Debug, Deserialize)]
pub struct NdjsonStreamQuery {
    pub since_id: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct SessionEventsQuery {
    pub after_seq: Option<u64>,
    pub limit: Option<u32>,
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
        let owner_type = owner_type_str
            .parse::<SessionOwnerType>()
            .map_err(|_| ApiError::BadRequest(format!("无效的 owner_type: {owner_type_str}")))?;
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

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct SessionEventResponse {
    pub session_id: String,
    pub event_seq: u64,
    pub occurred_at_ms: i64,
    pub committed_at_ms: i64,
    pub session_update_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry_index: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    pub notification: agent_client_protocol::SessionNotification,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct SessionEventsPageResponse {
    pub snapshot_seq: u64,
    pub events: Vec<SessionEventResponse>,
    pub has_more: bool,
    pub next_after_seq: u64,
}

fn map_session_event(
    event: agentdash_application::session::PersistedSessionEvent,
) -> SessionEventResponse {
    SessionEventResponse {
        session_id: event.session_id,
        event_seq: event.event_seq,
        occurred_at_ms: event.occurred_at_ms,
        committed_at_ms: event.committed_at_ms,
        session_update_type: event.session_update_type,
        turn_id: event.turn_id,
        entry_index: event.entry_index,
        tool_call_id: event.tool_call_id,
        notification: event.notification,
    }
}

fn stream_event_payload(
    event: agentdash_application::session::PersistedSessionEvent,
) -> serde_json::Value {
    let mapped = map_session_event(event);
    serde_json::json!({
        "type": "event",
        "session_id": mapped.session_id,
        "event_seq": mapped.event_seq,
        "occurred_at_ms": mapped.occurred_at_ms,
        "committed_at_ms": mapped.committed_at_ms,
        "session_update_type": mapped.session_update_type,
        "turn_id": mapped.turn_id,
        "entry_index": mapped.entry_index,
        "tool_call_id": mapped.tool_call_id,
        "notification": mapped.notification,
    })
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

pub async fn list_session_events(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(session_id): Path<String>,
    Query(query): Query<SessionEventsQuery>,
) -> Result<Json<SessionEventsPageResponse>, ApiError> {
    ensure_session_permission(
        state.as_ref(),
        &current_user,
        &session_id,
        ProjectPermission::View,
    )
    .await?;

    let after_seq = query.after_seq.unwrap_or(0);
    let limit = query.limit.unwrap_or(500).clamp(1, 2_000);
    let page = state
        .services
        .session_hub
        .list_event_page(&session_id, after_seq, limit)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;

    Ok(Json(SessionEventsPageResponse {
        snapshot_seq: page.snapshot_seq,
        events: page.events.into_iter().map(map_session_event).collect(),
        has_more: page.has_more,
        next_after_seq: page.next_after_seq,
    }))
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
    use agentdash_application::session::{SessionBootstrapState, TitleSource};

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

    #[test]
    fn session_prompt_lifecycle_kind_marks_pending_as_owner_bootstrap() {
        let meta = SessionMeta {
            id: "sess-1".to_string(),
            title: "测试".to_string(),
            title_source: TitleSource::Auto,
            created_at: 1,
            updated_at: 1,
            last_event_seq: 0,
            last_execution_status: "idle".to_string(),
            last_turn_id: None,
            last_terminal_message: None,
            executor_config: None,
            executor_session_id: None,
            companion_context: None,
            visible_canvas_mount_ids: Vec::new(),
            bootstrap_state: SessionBootstrapState::Pending,
        };

        assert_eq!(
            resolve_session_prompt_lifecycle(&meta, false, false),
            SessionPromptLifecycle::OwnerBootstrap
        );
    }

    #[test]
    fn session_prompt_lifecycle_kind_requires_repository_rehydrate_after_cold_restart() {
        let meta = SessionMeta {
            id: "sess-2".to_string(),
            title: "测试".to_string(),
            title_source: TitleSource::Auto,
            created_at: 1,
            updated_at: 1,
            last_event_seq: 12,
            last_execution_status: "completed".to_string(),
            last_turn_id: Some("t-last".to_string()),
            last_terminal_message: None,
            executor_config: None,
            executor_session_id: None,
            companion_context: None,
            visible_canvas_mount_ids: Vec::new(),
            bootstrap_state: SessionBootstrapState::Bootstrapped,
        };

        assert_eq!(
            resolve_session_prompt_lifecycle(&meta, false, false),
            SessionPromptLifecycle::RepositoryRehydrate(
                SessionRepositoryRehydrateMode::SystemContext,
            )
        );
        assert_eq!(
            resolve_session_prompt_lifecycle(&meta, true, false),
            SessionPromptLifecycle::Plain
        );
    }

    #[test]
    fn session_prompt_lifecycle_prefers_executor_follow_up_when_available() {
        let meta = SessionMeta {
            id: "sess-3".to_string(),
            title: "测试".to_string(),
            title_source: TitleSource::Auto,
            created_at: 1,
            updated_at: 1,
            last_event_seq: 5,
            last_execution_status: "completed".to_string(),
            last_turn_id: Some("t-last".to_string()),
            last_terminal_message: None,
            executor_config: None,
            executor_session_id: Some("exec-1".to_string()),
            companion_context: None,
            visible_canvas_mount_ids: Vec::new(),
            bootstrap_state: SessionBootstrapState::Bootstrapped,
        };

        assert_eq!(
            resolve_session_prompt_lifecycle(&meta, false, true),
            SessionPromptLifecycle::Plain
        );
    }

    #[test]
    fn session_prompt_lifecycle_uses_executor_state_restore_when_supported() {
        let meta = SessionMeta {
            id: "sess-4".to_string(),
            title: "测试".to_string(),
            title_source: TitleSource::Auto,
            created_at: 1,
            updated_at: 1,
            last_event_seq: 7,
            last_execution_status: "completed".to_string(),
            last_turn_id: Some("t-last".to_string()),
            last_terminal_message: None,
            executor_config: None,
            executor_session_id: None,
            companion_context: None,
            visible_canvas_mount_ids: Vec::new(),
            bootstrap_state: SessionBootstrapState::Bootstrapped,
        };

        assert_eq!(
            resolve_session_prompt_lifecycle(&meta, false, true),
            SessionPromptLifecycle::RepositoryRehydrate(
                SessionRepositoryRehydrateMode::ExecutorState,
            )
        );
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
    pub vfs: Option<agentdash_spi::Vfs>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_surface: Option<agentdash_application::vfs::ResolvedVfsSurface>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_snapshot: Option<SessionContextSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_capabilities: Option<agentdash_spi::SessionBaselineCapabilities>,
}

async fn try_build_session_capabilities(
    state: &AppState,
    session_id: &str,
    vfs: Option<&agentdash_spi::Vfs>,
) -> Option<agentdash_spi::SessionBaselineCapabilities> {
    let hook_runtime = state
        .services
        .session_hub
        .ensure_hook_session_runtime(session_id, None)
        .await
        .ok()
        .flatten();

    let skills = if let Some(space) = vfs {
        let result = agentdash_application::skill::load_skills_from_vfs(
            &state.services.vfs_service,
            space,
        )
        .await;
        result.skills
    } else {
        Vec::new()
    };

    let caps =
        agentdash_application::session::baseline_capabilities::build_session_baseline_capabilities(
            hook_runtime
                .as_ref()
                .map(|rt| rt.as_ref() as &dyn agentdash_spi::hooks::HookSessionRuntimeAccess),
            &skills,
        );

    if caps.is_empty() { None } else { Some(caps) }
}

/// GET /sessions/{id}/context — 按会话绑定统一返回 workspace / agent_binding / vfs / snapshot
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
            vfs: None,
            runtime_surface: None,
            context_snapshot: None,
            session_capabilities: None,
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
            let session_meta = if let Some(session_id) = result.session_id.as_deref() {
                state
                    .services
                    .session_hub
                    .get_session_meta(session_id)
                    .await
                    .map_err(|error| ApiError::Internal(error.to_string()))?
            } else {
                None
            };
            let built_context =
                agentdash_application::task::context_builder::build_task_session_context(
                    &state.repos,
                    &state.services.vfs_service,
                    state.config.mcp_base_url.as_deref(),
                    task_id,
                    session_meta.as_ref(),
                )
                .await;
            let resolved_vfs = built_context
                .as_ref()
                .and_then(|context| context.vfs.clone());
            let capabilities = try_build_session_capabilities(
                &state,
                &session_id,
                resolved_vfs.as_ref(),
            )
            .await;
            let runtime_surface = if let Some(space) = resolved_vfs.as_ref() {
                Some(
                    build_surface_summary(
                        &state,
                        &agentdash_application::vfs::ResolvedVfsSurfaceSource::SessionRuntime {
                            session_id: session_id.clone(),
                        },
                        space,
                    )
                    .await?,
                )
            } else {
                None
            };
            Ok(Json(SessionContextResponse {
                workspace_id: task.workspace_id.map(|id| id.to_string()),
                agent_binding: Some(result.agent_binding),
                vfs: resolved_vfs,
                runtime_surface,
                context_snapshot: built_context.and_then(|context| context.context_snapshot),
                session_capabilities: capabilities,
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
                    .await?;
            let resolved_vfs = built_context
                .as_ref()
                .and_then(|context| context.vfs.clone());
            let capabilities = try_build_session_capabilities(
                &state,
                &session_id,
                resolved_vfs.as_ref(),
            )
            .await;
            let runtime_surface = if let Some(space) = resolved_vfs.as_ref() {
                Some(
                    build_surface_summary(
                        &state,
                        &agentdash_application::vfs::ResolvedVfsSurfaceSource::SessionRuntime {
                            session_id: session_id.clone(),
                        },
                        space,
                    )
                    .await?,
                )
            } else {
                None
            };
            Ok(Json(SessionContextResponse {
                workspace_id: None,
                agent_binding: None,
                vfs: resolved_vfs,
                runtime_surface,
                context_snapshot: built_context.and_then(|context| context.context_snapshot),
                session_capabilities: capabilities,
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
            .await?;
            let capabilities = try_build_session_capabilities(
                &state,
                &session_id,
                built_context.vfs.as_ref(),
            )
            .await;
            let runtime_surface = if let Some(space) = built_context.vfs.as_ref() {
                Some(
                    build_surface_summary(
                        &state,
                        &agentdash_application::vfs::ResolvedVfsSurfaceSource::SessionRuntime {
                            session_id: session_id.clone(),
                        },
                        space,
                    )
                    .await?,
                )
            } else {
                None
            };
            Ok(Json(SessionContextResponse {
                workspace_id: None,
                agent_binding: None,
                vfs: built_context.vfs.clone(),
                runtime_surface,
                context_snapshot: built_context.context_snapshot,
                session_capabilities: capabilities,
            }))
        }
    }
}

pub(crate) fn pick_primary_session_binding(
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

#[derive(Debug, Deserialize)]
pub struct UpdateSessionMetaRequest {
    pub title: String,
}

/// PATCH /sessions/{id}/meta — 用户手动修改会话标题
pub async fn update_session_meta(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(session_id): Path<String>,
    Json(req): Json<UpdateSessionMetaRequest>,
) -> Result<Json<SessionMeta>, ApiError> {
    ensure_session_permission(
        state.as_ref(),
        &current_user,
        &session_id,
        ProjectPermission::Edit,
    )
    .await?;

    let title = req.title.trim();
    if title.is_empty() {
        return Err(ApiError::BadRequest("标题不能为空".to_string()));
    }

    let meta = state
        .services
        .session_hub
        .set_user_title(&session_id, title)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("会话 {} 不存在", session_id)))?;

    Ok(Json(meta))
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
    let meta = state
        .services
        .session_hub
        .get_session_meta(session_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("会话 {} 不存在", session_id)))?;
    let bindings = state
        .repos
        .session_binding_repo
        .list_by_session(session_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let visible_canvas_mount_ids = meta.visible_canvas_mount_ids.clone();
    let effective_executor = req
        .user_input
        .executor_config
        .clone()
        .or_else(|| meta.executor_config.clone());
    let has_live_runtime = state
        .services
        .session_hub
        .has_live_runtime(session_id)
        .await;
    let supports_repository_restore = effective_executor.as_ref().is_some_and(|config| {
        state
            .services
            .connector
            .supports_repository_restore(config.executor.as_str())
    });
    let lifecycle_kind =
        resolve_session_prompt_lifecycle(&meta, has_live_runtime, supports_repository_restore);

    if let Some(binding) = bindings
        .iter()
        .find(|binding| binding.owner_type == SessionOwnerType::Task)
    {
        return build_task_owner_prompt_request(
            state,
            session_id,
            req,
            binding.owner_id,
            &meta,
            lifecycle_kind,
            &visible_canvas_mount_ids,
        )
        .await;
    }

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

        return build_story_owner_prompt_request(
            state,
            session_id,
            req,
            &story,
            &project,
            workspace.as_ref(),
            &meta,
            lifecycle_kind,
            &visible_canvas_mount_ids,
        )
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

        return build_project_owner_prompt_request(
            state,
            session_id,
            req,
            &project,
            &binding.label,
            &meta,
            lifecycle_kind,
            &visible_canvas_mount_ids,
        )
        .await;
    }

    if let SessionPromptLifecycle::RepositoryRehydrate(
        SessionRepositoryRehydrateMode::SystemContext,
    ) = lifecycle_kind
    {
        let continuation_context = state
            .services
            .session_hub
            .build_continuation_system_context(session_id, None)
            .await
            .map_err(|error| ApiError::Internal(error.to_string()))?;
        return Ok(apply_plain_lifecycle_request(
            req,
            continuation_context,
            SessionBootstrapAction::None,
        )?);
    }

    Ok(req)
}

fn finalize_augmented_request(
    req: &mut PromptSessionRequest,
    system_context: Option<String>,
    prompt_blocks: Vec<serde_json::Value>,
    workspace: Option<&Workspace>,
    vfs: Option<agentdash_spi::Vfs>,
    effective_mcp_servers: Vec<agent_client_protocol::McpServer>,
    flow_capabilities: agentdash_spi::FlowCapabilities,
    bootstrap_action: SessionBootstrapAction,
) {
    req.user_input.prompt_blocks = Some(prompt_blocks);
    req.system_context = system_context;
    req.bootstrap_action = bootstrap_action;

    apply_workspace_defaults(
        &mut req.user_input.working_dir,
        &mut req.vfs,
        workspace,
    );
    if req.vfs.is_none() {
        req.vfs = vfs;
    }
    req.mcp_servers = effective_mcp_servers;
    req.flow_capabilities = Some(flow_capabilities);
}

fn apply_plain_lifecycle_request(
    mut req: PromptSessionRequest,
    system_context: Option<String>,
    bootstrap_action: SessionBootstrapAction,
) -> Result<PromptSessionRequest, ApiError> {
    let user_prompt_blocks = req
        .user_input
        .prompt_blocks
        .take()
        .ok_or_else(|| ApiError::BadRequest("必须提供 promptBlocks".to_string()))?;
    req.user_input.prompt_blocks = Some(user_prompt_blocks);
    req.system_context = system_context;
    req.bootstrap_action = bootstrap_action;
    Ok(req)
}

async fn build_story_owner_prompt_request(
    state: &Arc<AppState>,
    session_id: &str,
    mut req: PromptSessionRequest,
    story: &Story,
    project: &Project,
    workspace: Option<&Workspace>,
    _meta: &SessionMeta,
    lifecycle_kind: SessionPromptLifecycle,
    visible_canvas_mount_ids: &[String],
) -> Result<PromptSessionRequest, ApiError> {
    let effective_executor_config = req
        .user_input
        .executor_config
        .clone()
        .or_else(|| {
            project
                .config
                .default_agent_type
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(agentdash_spi::AgentConfig::new)
        })
        .ok_or_else(|| {
            ApiError::BadRequest(
                "Story owner prompt 缺少 executor_config，且 project 没有 default_agent_type"
                    .to_string(),
            )
        })?;
    let mut vfs = match req.vfs.clone() {
        Some(vfs) => Some(vfs),
        None => Some(
            state
                .services
                .vfs_service
                .build_vfs(
                    project,
                    Some(story),
                    workspace,
                    SessionMountTarget::Story,
                    Some(effective_executor_config.executor.as_str()),
                )
                .map_err(ApiError::BadRequest)?,
        ),
    };
    if let Some(space) = vfs.as_mut() {
        append_visible_canvas_mounts(
            state.repos.canvas_repo.as_ref(),
            project.id,
            space,
            visible_canvas_mount_ids,
        )
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    }
    let effective_agent_type = Some(effective_executor_config.executor.as_str());
    let mut effective_mcp_servers = req.mcp_servers.clone();
    let base_url = required_mcp_base_url(state.as_ref())?;
    effective_mcp_servers
        .push(McpInjectionConfig::for_story(base_url, project.id, story.id).to_acp_mcp_server());
    let runtime_vfs = vfs.clone();
    let runtime_mcp_servers = acp_mcp_servers_to_runtime(&effective_mcp_servers);

    let resolved_workspace_sources =
        resolve_workspace_declared_sources(state, &story.context.source_refs, workspace, 60)
            .await
            .map_err(ApiError::BadRequest)?;

    let (context_markdown, _) = build_story_context_markdown(StoryContextBuildInput {
        story,
        project,
        workspace,
        vfs: runtime_vfs.as_ref(),
        mcp_servers: &runtime_mcp_servers,
        effective_agent_type,
        workspace_source_fragments: resolved_workspace_sources.fragments,
        workspace_source_warnings: resolved_workspace_sources.warnings,
    });
    let user_prompt_blocks = req
        .user_input
        .prompt_blocks
        .take()
        .ok_or_else(|| ApiError::BadRequest("必须提供 promptBlocks".to_string()))?;
    let (system_context, prompt_blocks, bootstrap_action) = match lifecycle_kind {
        SessionPromptLifecycle::OwnerBootstrap => (
            Some(context_markdown.clone()),
            build_story_owner_prompt_blocks(story.id, context_markdown.clone(), user_prompt_blocks),
            SessionBootstrapAction::OwnerContext,
        ),
        SessionPromptLifecycle::RepositoryRehydrate(
            SessionRepositoryRehydrateMode::SystemContext,
        ) => (
            state
                .services
                .session_hub
                .build_continuation_system_context(session_id, Some(&context_markdown))
                .await
                .map_err(|error| ApiError::Internal(error.to_string()))?,
            user_prompt_blocks,
            SessionBootstrapAction::None,
        ),
        SessionPromptLifecycle::RepositoryRehydrate(
            SessionRepositoryRehydrateMode::ExecutorState,
        ) => (
            Some(context_markdown.clone()),
            user_prompt_blocks,
            SessionBootstrapAction::None,
        ),
        SessionPromptLifecycle::Plain => (None, user_prompt_blocks, SessionBootstrapAction::None),
    };
    req.user_input.executor_config = Some(effective_executor_config);

    finalize_augmented_request(
        &mut req,
        system_context,
        prompt_blocks,
        workspace,
        vfs,
        effective_mcp_servers,
        agentdash_spi::FlowCapabilities::all(),
        bootstrap_action,
    );

    Ok(req)
}

fn required_mcp_base_url(state: &AppState) -> Result<String, ApiError> {
    state
        .config
        .mcp_base_url
        .clone()
        .ok_or_else(|| ApiError::Internal("服务端未配置 MCP relay base_url".to_string()))
}

async fn build_project_owner_prompt_request(
    state: &Arc<AppState>,
    session_id: &str,
    mut req: PromptSessionRequest,
    project: &Project,
    binding_label: &str,
    _meta: &SessionMeta,
    lifecycle_kind: SessionPromptLifecycle,
    visible_canvas_mount_ids: &[String],
) -> Result<PromptSessionRequest, ApiError> {
    let agent_key = parse_project_agent_session_label(binding_label).ok_or_else(|| {
        ApiError::BadRequest(format!("无效的项目 Agent session label: {binding_label}"))
    })?;
    let project_agent = resolve_project_agent_bridge_async(state, project.id, agent_key)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Project Agent `{agent_key}` 不存在")))?;
    let workspace = resolve_project_workspace(state, project).await?;

    let effective_executor_config = match req.user_input.executor_config.clone() {
        Some(mut user_ec) => {
            // 前端传入的 executor_config 可能只包含 model 选择等字段，
            // 需要从 preset 补全 agent 级配置（system_prompt, tool_clusters 等）
            let preset_ec = &project_agent.executor_config;
            if user_ec.system_prompt.is_none() {
                user_ec.system_prompt = preset_ec.system_prompt.clone();
            }
            if user_ec.system_prompt_mode.is_none() {
                user_ec.system_prompt_mode = preset_ec.system_prompt_mode;
            }
            if user_ec.tool_clusters.is_none() {
                user_ec.tool_clusters = preset_ec.tool_clusters.clone();
            }
            user_ec
        }
        None => project_agent.executor_config.clone(),
    };
    let effective_agent_type = Some(effective_executor_config.executor.as_str());
    let mut vfs = match req.vfs.clone() {
        Some(vfs) => Some(vfs),
        None => Some(
            state
                .services
                .vfs_service
                .build_vfs(
                    project,
                    None,
                    workspace.as_ref(),
                    SessionMountTarget::Project,
                    effective_agent_type,
                )
                .map_err(ApiError::BadRequest)?,
        ),
    };
    if let Some(space) = vfs.as_mut() {
        append_visible_canvas_mounts(
            state.repos.canvas_repo.as_ref(),
            project.id,
            space,
            visible_canvas_mount_ids,
        )
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    }
    let mut effective_mcp_servers = req.mcp_servers.clone();
    let base_url = required_mcp_base_url(state.as_ref())?;
    effective_mcp_servers
        .push(McpInjectionConfig::for_relay(base_url, project.id).to_acp_mcp_server());
    effective_mcp_servers.extend(project_agent.preset_mcp_servers.iter().cloned());
    req.relay_mcp_server_names
        .extend(project_agent.relay_mcp_server_names.iter().cloned());
    let runtime_vfs = vfs.clone();
    let runtime_mcp_servers = acp_mcp_servers_to_runtime(&effective_mcp_servers);

    let (context_markdown, _) = build_project_context_markdown(ProjectContextBuildInput {
        project,
        workspace: workspace.as_ref(),
        vfs: runtime_vfs.as_ref(),
        mcp_servers: &runtime_mcp_servers,
        effective_agent_type,
        preset_name: project_agent.preset_name.as_deref(),
        agent_display_name: project_agent.display_name.as_str(),
    });
    let user_prompt_blocks = req
        .user_input
        .prompt_blocks
        .take()
        .ok_or_else(|| ApiError::BadRequest("必须提供 promptBlocks".to_string()))?;
    let (system_context, prompt_blocks, bootstrap_action) = match lifecycle_kind {
        SessionPromptLifecycle::OwnerBootstrap => (
            Some(context_markdown.clone()),
            build_project_owner_prompt_blocks(
                project.id,
                context_markdown.clone(),
                user_prompt_blocks,
            ),
            SessionBootstrapAction::OwnerContext,
        ),
        SessionPromptLifecycle::RepositoryRehydrate(
            SessionRepositoryRehydrateMode::SystemContext,
        ) => (
            state
                .services
                .session_hub
                .build_continuation_system_context(session_id, Some(&context_markdown))
                .await
                .map_err(|error| ApiError::Internal(error.to_string()))?,
            user_prompt_blocks,
            SessionBootstrapAction::None,
        ),
        SessionPromptLifecycle::RepositoryRehydrate(
            SessionRepositoryRehydrateMode::ExecutorState,
        ) => (
            Some(context_markdown.clone()),
            user_prompt_blocks,
            SessionBootstrapAction::None,
        ),
        SessionPromptLifecycle::Plain => (None, user_prompt_blocks, SessionBootstrapAction::None),
    };

    req.user_input.executor_config = Some(effective_executor_config);

    finalize_augmented_request(
        &mut req,
        system_context,
        prompt_blocks,
        workspace.as_ref(),
        vfs,
        effective_mcp_servers,
        agentdash_spi::FlowCapabilities::from_clusters([
            agentdash_spi::ToolCluster::Read,
            agentdash_spi::ToolCluster::Write,
            agentdash_spi::ToolCluster::Execute,
            agentdash_spi::ToolCluster::Collaboration,
            agentdash_spi::ToolCluster::Canvas,
        ]),
        bootstrap_action,
    );

    Ok(req)
}

async fn build_task_owner_prompt_request(
    state: &Arc<AppState>,
    session_id: &str,
    mut req: PromptSessionRequest,
    task_id: uuid::Uuid,
    meta: &SessionMeta,
    lifecycle_kind: SessionPromptLifecycle,
    visible_canvas_mount_ids: &[String],
) -> Result<PromptSessionRequest, ApiError> {
    let task = state
        .repos
        .task_repo
        .get_by_id(task_id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Task {task_id} 不存在")))?;

    let effective_executor_config = req
        .user_input
        .executor_config
        .clone()
        .or_else(|| meta.executor_config.clone());
    let services = TaskTurnServices {
        repos: &state.repos,
        availability: state.services.backend_registry.as_ref(),
        vfs_service: state.services.vfs_service.as_ref(),
        contributor_registry: state.services.contributor_registry.as_ref(),
        mcp_base_url: state.config.mcp_base_url.as_deref(),
    };
    let prepared = prepare_task_turn_context(
        &services,
        &task,
        ExecutionPhase::Continue,
        None,
        None,
        effective_executor_config.as_ref(),
    )
    .await
    .map_err(task_execution::map_task_execution_error)?;

    let mut vfs = prepared.vfs.clone();
    if let Some(space) = vfs.as_mut() {
        append_visible_canvas_mounts(
            state.repos.canvas_repo.as_ref(),
            task.project_id,
            space,
            visible_canvas_mount_ids,
        )
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    }

    let user_prompt_blocks = req
        .user_input
        .prompt_blocks
        .take()
        .ok_or_else(|| ApiError::BadRequest("必须提供 promptBlocks".to_string()))?;
    let task_context_markdown = prepared
        .built
        .system_context
        .clone()
        .filter(|value| !value.trim().is_empty());
    let mut prompt_blocks = user_prompt_blocks;
    let mut system_context = None;
    let mut bootstrap_action = SessionBootstrapAction::None;

    match lifecycle_kind {
        SessionPromptLifecycle::OwnerBootstrap => {
            if !prepared.built.prompt_blocks.is_empty() {
                let mut next_blocks = prepared.built.prompt_blocks.clone();
                next_blocks.extend(prompt_blocks);
                prompt_blocks = next_blocks;
            }
            system_context = task_context_markdown.clone();
            bootstrap_action = SessionBootstrapAction::OwnerContext;
        }
        SessionPromptLifecycle::RepositoryRehydrate(
            SessionRepositoryRehydrateMode::SystemContext,
        ) => {
            system_context = state
                .services
                .session_hub
                .build_continuation_system_context(session_id, task_context_markdown.as_deref())
                .await
                .map_err(|error| ApiError::Internal(error.to_string()))?;
        }
        SessionPromptLifecycle::RepositoryRehydrate(
            SessionRepositoryRehydrateMode::ExecutorState,
        ) => {
            system_context = task_context_markdown.clone();
        }
        SessionPromptLifecycle::Plain => {}
    }

    if let Some(config) = effective_executor_config {
        req.user_input.executor_config = Some(config);
    }

    finalize_augmented_request(
        &mut req,
        system_context,
        prompt_blocks,
        prepared.workspace.as_ref(),
        vfs,
        runtime_mcp_servers_to_acp(&prepared.built.mcp_servers),
        agentdash_spi::FlowCapabilities::from_clusters([
            agentdash_spi::ToolCluster::Read,
            agentdash_spi::ToolCluster::Write,
            agentdash_spi::ToolCluster::Execute,
            agentdash_spi::ToolCluster::Workflow,
            agentdash_spi::ToolCluster::Collaboration,
            agentdash_spi::ToolCluster::Canvas,
        ]),
        bootstrap_action,
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

#[derive(Debug, Deserialize)]
pub struct CompanionRespondRequest {
    pub payload: serde_json::Value,
}

pub async fn respond_companion_request(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((session_id, request_id)): Path<(String, String)>,
    Json(req): Json<CompanionRespondRequest>,
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
        .respond_companion_request(&session_id, &request_id, req.payload)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(serde_json::json!({
        "responded": true,
        "session_id": session_id,
        "request_id": request_id,
    })))
}

/// ACP 会话流（Streaming HTTP / SSE）
pub async fn acp_session_stream_sse(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(session_id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<NdjsonStreamQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    ensure_session_permission(
        state.as_ref(),
        &current_user,
        &session_id,
        ProjectPermission::View,
    )
    .await?;
    let last_event_id = parse_resume_from_header(&headers, "last-event-id")?
        .or(query.since_id)
        .unwrap_or(0);

    tracing::info!(
        session_id = %session_id,
        last_event_id = last_event_id,
        "ACP 会话流连接建立（SSE）"
    );

    let subscription = state
        .services
        .session_hub
        .subscribe_after(&session_id, last_event_id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    let replayed = subscription.backlog.len();
    tracing::info!(
        session_id = %session_id,
        replayed_count = replayed,
        snapshot_seq = subscription.snapshot_seq,
        "ACP 会话流历史补发完成（SSE）"
    );

    let stream = async_stream::stream! {
        for event in subscription.backlog {
            let id = event.event_seq;
            if let Ok(json) = serde_json::to_string(&stream_event_payload(event)) {
                yield Ok(Event::default().id(id.to_string()).data(json));
            }
        }

        let mut seq = subscription.snapshot_seq;
        let mut rx = subscription.rx;
        loop {
            match rx.recv().await {
                Ok(event) => {
                    if event.event_seq <= seq {
                        continue;
                    }
                    let id = event.event_seq;
                    seq = id;
                    if let Ok(json) = serde_json::to_string(&stream_event_payload(event)) {
                        yield Ok(Event::default().id(id.to_string()).data(json));
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
    let resume_from = parse_resume_from_header(&headers, "x-stream-since-id")?
        .or(query.since_id)
        .unwrap_or(0);
    tracing::info!(
        session_id = %session_id,
        resume_from = resume_from,
        "ACP 会话流连接建立（NDJSON）"
    );

    let subscription = state
        .services
        .session_hub
        .subscribe_after(&session_id, resume_from)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    let replayed = subscription.backlog.len();
    tracing::info!(
        session_id = %session_id,
        replayed_count = replayed,
        snapshot_seq = subscription.snapshot_seq,
        "ACP 会话流历史补发完成（NDJSON）"
    );

    let stream = async_stream::stream! {
        let mut seq = resume_from;
        for event in subscription.backlog {
            seq = event.event_seq;
            if let Some(line) = to_ndjson_line(&stream_event_payload(event)) {
                yield Ok::<Bytes, Infallible>(line);
            }
        }

        if let Some(line) = to_ndjson_line(&serde_json::json!({
            "type": "connected",
            "last_event_id": seq,
        })) {
            yield Ok::<Bytes, Infallible>(line);
        }

        let mut heartbeat_tick = tokio::time::interval(ACP_HEARTBEAT_INTERVAL);
        heartbeat_tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let mut rx = subscription.rx;

        loop {
            tokio::select! {
                next = rx.recv() => {
                    match next {
                        Ok(event) => {
                            if event.event_seq <= subscription.snapshot_seq {
                                continue;
                            }
                            seq = event.event_seq;
                            if let Some(line) = to_ndjson_line(&stream_event_payload(event)) {
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

fn parse_resume_from_header(
    headers: &HeaderMap,
    header_name: &'static str,
) -> Result<Option<u64>, ApiError> {
    let Some(value) = headers.get(header_name) else {
        return Ok(None);
    };
    let raw = value
        .to_str()
        .map_err(|_| ApiError::BadRequest(format!("{header_name} 不是有效 UTF-8")))?;
    let parsed = raw
        .parse::<i64>()
        .map_err(|_| ApiError::BadRequest(format!("{header_name} 不是有效整数")))?;
    if parsed < 0 {
        return Err(ApiError::BadRequest(format!("{header_name} 不能为负数")));
    }
    Ok(Some(parsed as u64))
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

pub(crate) async fn ensure_session_permission(
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
