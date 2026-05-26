use std::collections::HashSet;
use std::convert::Infallible;
use std::io;
use std::sync::Arc;
use std::time::Duration;

use axum::{
    Json,
    body::{Body, Bytes},
    extract::{Path, Query, State},
    http::HeaderMap,
    response::IntoResponse,
};
use serde::Deserialize;
use tokio::time::MissedTickBehavior;

use crate::bootstrap::session_construction_provider::decode_construction_runtime_error;
use crate::session_use_cases::context_query::build_session_context_plan;
use crate::{app_state::AppState, rpc::ApiError};
use agentdash_application::session::construction::SessionConstructionPlan;
use agentdash_application::session::context::SessionContextSnapshot;
use agentdash_application::session::{
    LaunchCommand, SessionExecutionState, SessionForkRequest, SessionLineageRelationKind,
    SessionMeta, SessionProjectionRollbackRequest as ApplicationProjectionRollbackRequest,
    TitleSource, UserPromptInput,
};
use agentdash_contracts::session::{
    CreateSessionForkRequest, RollbackSessionProjectionRequest, SessionEventResponse,
    SessionEventsPageResponse, SessionForkChildSessionResponse, SessionForkResponse,
    SessionLineageRelationKindDto, SessionLineageViewResponse, SessionNdjsonEnvelope,
    SessionProjectionRollbackResponse, SessionProjectionViewResponse,
};
use agentdash_domain::session_binding::{SessionBinding, SessionOwnerType};

use agentdash_plugin_api::AuthIdentity;
use agentdash_spi::HookSessionRuntimeSnapshot;
use serde::Serialize;

use crate::auth::{
    CurrentUser, ProjectPermission, load_project_with_permission,
    load_story_and_project_with_permission, load_task_story_project_with_permission,
};

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
                .session_core
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
        .session_core
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
    pub project_id: uuid::Uuid,
}

pub async fn create_session(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Json(req): Json<CreateSessionRequest>,
) -> Result<Json<SessionMeta>, ApiError> {
    let project = load_project_with_permission(
        state.as_ref(),
        &current_user,
        req.project_id,
        ProjectPermission::Edit,
    )
    .await?;
    let title = req.title.unwrap_or_else(|| "新会话".to_string());
    let meta = state
        .services
        .session_core
        .create_session(&title)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let binding = SessionBinding::new(
        project.id,
        meta.id.clone(),
        SessionOwnerType::Project,
        project.id,
        agentdash_application::workflow::FREEFORM_SESSION_LABEL,
    );
    state.repos.session_binding_repo.create(&binding).await?;
    state
        .services
        .session_core
        .mark_owner_bootstrap_pending(&meta.id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    ensure_freeform_lifecycle_run(state.as_ref(), project.id, &meta.id).await?;
    Ok(Json(meta))
}

pub(crate) async fn ensure_freeform_lifecycle_run(
    state: &AppState,
    project_id: uuid::Uuid,
    session_id: &str,
) -> Result<(), ApiError> {
    let service = agentdash_application::workflow::FreeformLifecycleService::new(
        state.repos.workflow_definition_repo.as_ref(),
        state.repos.activity_lifecycle_definition_repo.as_ref(),
        state.repos.lifecycle_run_repo.as_ref(),
    );
    service
        .ensure_run_for_session(project_id, session_id)
        .await
        .map(|_| ())
        .map_err(ApiError::from)
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
        .session_core
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
        .session_hooks
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

fn map_session_event(
    event: agentdash_application::session::PersistedSessionEvent,
) -> SessionEventResponse {
    event.into()
}

fn stream_event_payload(
    event: agentdash_application::session::PersistedSessionEvent,
) -> SessionNdjsonEnvelope {
    SessionNdjsonEnvelope::event(event)
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
        .session_core
        .get_session_meta(&session_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("会话 {} 不存在", session_id)))?;

    let execution_state = state
        .services
        .session_core
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
        .session_eventing
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
    use agentdash_application::session::{
        ExecutionStatus, SessionBootstrapState, SessionPromptLifecycle,
        SessionRepositoryRehydrateMode, TitleSource, resolve_session_prompt_lifecycle,
    };

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
            last_execution_status: ExecutionStatus::Idle,
            last_turn_id: None,
            last_terminal_message: None,
            executor_config: None,
            executor_session_id: None,
            companion_context: None,
            tab_layout: None,
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
            last_execution_status: ExecutionStatus::Completed,
            last_turn_id: Some("t-last".to_string()),
            last_terminal_message: None,
            executor_config: None,
            executor_session_id: None,
            companion_context: None,
            tab_layout: None,
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
            last_execution_status: ExecutionStatus::Completed,
            last_turn_id: Some("t-last".to_string()),
            last_terminal_message: None,
            executor_config: None,
            executor_session_id: Some("exec-1".to_string()),
            companion_context: None,
            tab_layout: None,
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
            last_execution_status: ExecutionStatus::Completed,
            last_turn_id: Some("t-last".to_string()),
            last_terminal_message: None,
            executor_config: None,
            executor_session_id: None,
            companion_context: None,
            tab_layout: None,
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

    let Some(plan) =
        build_session_context_plan(&state, &current_user, &session_id, &bindings).await?
    else {
        return Ok(Json(SessionContextResponse::empty()));
    };

    Ok(Json(SessionContextResponse::from_construction_plan(plan)))
}

/// GET /sessions/{id}/context/projection — 返回当前模型可见上下文投影。
pub async fn get_session_context_projection(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(session_id): Path<String>,
) -> Result<Json<SessionProjectionViewResponse>, ApiError> {
    ensure_session_permission(
        state.as_ref(),
        &current_user,
        &session_id,
        ProjectPermission::View,
    )
    .await?;

    let envelope = state
        .services
        .session_eventing
        .build_agent_context_envelope(&session_id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;

    Ok(Json(SessionProjectionViewResponse::from(envelope)))
}

/// POST /sessions/{id}/fork — 基于当前模型投影创建可独立恢复的 child session。
pub async fn fork_session(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(session_id): Path<String>,
    Json(req): Json<CreateSessionForkRequest>,
) -> Result<Json<SessionForkResponse>, ApiError> {
    let parent_bindings = ensure_session_permission(
        state.as_ref(),
        &current_user,
        &session_id,
        ProjectPermission::Edit,
    )
    .await?;

    let result = state
        .services
        .session_branching
        .fork_session(SessionForkRequest {
            parent_session_id: session_id.clone(),
            title: req.title,
            fork_point_event_seq: req.fork_point_event_seq,
            fork_point_ref: req.fork_point_ref.map(Into::into),
            fork_point_compaction_id: req.fork_point_compaction_id,
            relation_kind: relation_kind_from_dto(req.relation_kind),
            metadata_json: req.metadata_json.unwrap_or_else(|| serde_json::json!({})),
        })
        .await
        .map_err(api_error_from_io)?;

    if let Err(error) = copy_parent_session_bindings_to_child(
        state.as_ref(),
        &parent_bindings,
        &result.child_session.id,
    )
    .await
    {
        let _ = state
            .services
            .session_core
            .delete_session(&result.child_session.id)
            .await;
        return Err(error);
    }

    Ok(Json(SessionForkResponse {
        parent_session_id: result.parent_session_id,
        child_session: SessionForkChildSessionResponse {
            id: result.child_session.id,
            title: result.child_session.title,
            created_at: result.child_session.created_at,
            updated_at: result.child_session.updated_at,
            last_event_seq: result.child_session.last_event_seq,
        },
        lineage: result.lineage.into(),
        child_initial_compaction_id: result.projection_commit.compaction.id,
        projection_version: result.projection_commit.head.projection_version,
        head_event_seq: result.projection_commit.head.head_event_seq,
    }))
}

/// GET /sessions/{id}/lineage — 返回当前 session 的父边、祖先与直接 children。
pub async fn get_session_lineage(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(session_id): Path<String>,
) -> Result<Json<SessionLineageViewResponse>, ApiError> {
    ensure_session_permission(
        state.as_ref(),
        &current_user,
        &session_id,
        ProjectPermission::View,
    )
    .await?;

    let view = state
        .services
        .session_branching
        .lineage_view(&session_id)
        .await
        .map_err(api_error_from_io)?;

    Ok(Json(SessionLineageViewResponse {
        session_id,
        lineage: view.lineage.map(Into::into),
        ancestors: view.ancestors.into_iter().map(Into::into).collect(),
        children: view.children.into_iter().map(Into::into).collect(),
    }))
}

/// POST /sessions/{id}/projection/rollback — 移动模型可见 projection head，不删除审计事件。
pub async fn rollback_session_projection(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(session_id): Path<String>,
    Json(req): Json<RollbackSessionProjectionRequest>,
) -> Result<Json<SessionProjectionRollbackResponse>, ApiError> {
    ensure_session_permission(
        state.as_ref(),
        &current_user,
        &session_id,
        ProjectPermission::Edit,
    )
    .await?;

    let result = state
        .services
        .session_branching
        .rollback_model_projection(ApplicationProjectionRollbackRequest {
            session_id: session_id.clone(),
            target_event_seq: req.target_event_seq,
            active_compaction_id: req.active_compaction_id,
            reason: req.reason,
        })
        .await
        .map_err(api_error_from_io)?;

    Ok(Json(SessionProjectionRollbackResponse {
        session_id,
        event: result.event.into(),
        head_event_seq: result.head.head_event_seq,
        active_compaction_id: result.head.active_compaction_id,
        projection_version: result.head.projection_version,
        updated_by_event_seq: result.head.updated_by_event_seq,
    }))
}

impl SessionContextResponse {
    fn empty() -> Self {
        Self {
            workspace_id: None,
            agent_binding: None,
            vfs: None,
            runtime_surface: None,
            context_snapshot: None,
            session_capabilities: None,
        }
    }

    fn from_construction_plan(plan: SessionConstructionPlan) -> Self {
        let projection = plan.context_projection;
        Self {
            workspace_id: projection.workspace_id.map(|id| id.to_string()),
            agent_binding: projection.agent_binding,
            vfs: projection.vfs,
            runtime_surface: projection.runtime_surface,
            context_snapshot: projection.context_snapshot,
            session_capabilities: projection.session_capabilities,
        }
    }
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
                // M1-b：Task 查询经 Story aggregate
                if let Some(story) = state
                    .repos
                    .story_repo
                    .find_by_task_id(binding.owner_id)
                    .await
                    .map_err(|e| ApiError::Internal(e.to_string()))?
                {
                    if let Some(task) = story.find_task(binding.owner_id) {
                        owner_title = Some(task.title.clone());
                        story_id = Some(task.story_id.to_string());
                        task_id = Some(task.id.to_string());
                    }
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
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub tab_layout: Option<serde_json::Value>,
}

/// GET /sessions/{id}/meta — 返回完整 session meta。
pub async fn get_session_meta(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(session_id): Path<String>,
) -> Result<Json<SessionMeta>, ApiError> {
    get_session(State(state), CurrentUser(current_user), Path(session_id)).await
}

/// PATCH /sessions/{id}/meta — 用户手动修改会话 meta。
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

    if req.title.is_none() && req.tab_layout.is_none() {
        return Err(ApiError::BadRequest(
            "必须提供 title 或 tab_layout".to_string(),
        ));
    }
    if let Some(title) = req.title.as_deref()
        && title.trim().is_empty()
    {
        return Err(ApiError::BadRequest("标题不能为空".to_string()));
    }

    let meta = state
        .services
        .session_core
        .update_session_meta(&session_id, |meta| {
            if let Some(title) = req.title.as_deref() {
                let title = title.trim();
                if !title.is_empty() {
                    meta.title = title.to_string();
                    meta.title_source = TitleSource::User;
                }
            }
            if let Some(tab_layout) = req.tab_layout.clone() {
                meta.tab_layout = Some(tab_layout);
            }
        })
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
        .session_core
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
    let turn_id = state
        .services
        .session_launch
        .launch_command(
            &session_id,
            LaunchCommand::http_prompt_input(user_input, Some(current_user)),
        )
        .await
        .map_err(|e| match e {
            agentdash_spi::ConnectorError::InvalidConfig(msg) => ApiError::BadRequest(msg),
            agentdash_spi::ConnectorError::Runtime(msg) => {
                decode_construction_runtime_error(&msg).unwrap_or(ApiError::Internal(msg))
            }
            other => ApiError::Internal(other.to_string()),
        })?;

    Ok(Json(
        serde_json::json!({ "started": true, "sessionId": session_id, "turnId": turn_id }),
    ))
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
        .session_runtime
        .cancel(&session_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let execution_state = state
        .services
        .session_core
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
        .session_control
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
        .session_control
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
        .session_control
        .respond_companion_request(&session_id, &request_id, req.payload)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(serde_json::json!({
        "responded": true,
        "session_id": session_id,
        "request_id": request_id,
    })))
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
        .session_eventing
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

        if let Some(line) = to_ndjson_line(&SessionNdjsonEnvelope::connected(seq)) {
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
                    if let Some(line) = to_ndjson_line(&SessionNdjsonEnvelope::heartbeat_now()) {
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

fn to_ndjson_line(value: &SessionNdjsonEnvelope) -> Option<Bytes> {
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

fn relation_kind_from_dto(
    value: Option<SessionLineageRelationKindDto>,
) -> SessionLineageRelationKind {
    match value.unwrap_or(SessionLineageRelationKindDto::Fork) {
        SessionLineageRelationKindDto::Fork => SessionLineageRelationKind::Fork,
        SessionLineageRelationKindDto::Companion => SessionLineageRelationKind::Companion,
        SessionLineageRelationKindDto::SpawnedAgent => SessionLineageRelationKind::SpawnedAgent,
        SessionLineageRelationKindDto::RollbackBranch => SessionLineageRelationKind::RollbackBranch,
    }
}

async fn copy_parent_session_bindings_to_child(
    state: &AppState,
    parent_bindings: &[SessionBinding],
    child_session_id: &str,
) -> Result<(), ApiError> {
    let mut created_binding_ids = Vec::with_capacity(parent_bindings.len());
    for binding in parent_bindings {
        let child_binding = SessionBinding::new(
            binding.project_id,
            child_session_id.to_string(),
            binding.owner_type,
            binding.owner_id,
            forked_binding_label(binding, child_session_id),
        );
        match state
            .repos
            .session_binding_repo
            .create(&child_binding)
            .await
        {
            Ok(()) => created_binding_ids.push(child_binding.id),
            Err(error) => {
                cleanup_session_bindings(state, created_binding_ids).await;
                return Err(ApiError::from(error));
            }
        }
    }

    let mut freeform_project_ids = HashSet::new();
    for binding in parent_bindings {
        if binding.owner_type == SessionOwnerType::Project
            && binding.label == agentdash_application::workflow::FREEFORM_SESSION_LABEL
            && freeform_project_ids.insert(binding.project_id)
            && let Err(error) =
                ensure_freeform_lifecycle_run(state, binding.project_id, child_session_id).await
        {
            cleanup_session_bindings(state, created_binding_ids).await;
            return Err(error);
        }
    }

    Ok(())
}

async fn cleanup_session_bindings(state: &AppState, binding_ids: Vec<uuid::Uuid>) {
    for binding_id in binding_ids {
        let _ = state.repos.session_binding_repo.delete(binding_id).await;
    }
}

fn forked_binding_label(binding: &SessionBinding, child_session_id: &str) -> String {
    if binding.owner_type == SessionOwnerType::Story {
        format!(
            "{}:fork:{}",
            binding.label,
            short_session_suffix(child_session_id)
        )
    } else {
        binding.label.clone()
    }
}

fn short_session_suffix(session_id: &str) -> &str {
    session_id
        .rsplit('-')
        .next()
        .filter(|value| !value.is_empty())
        .unwrap_or(session_id)
}

fn api_error_from_io(error: io::Error) -> ApiError {
    match error.kind() {
        io::ErrorKind::InvalidInput | io::ErrorKind::InvalidData => {
            ApiError::BadRequest(error.to_string())
        }
        io::ErrorKind::NotFound => ApiError::NotFound(error.to_string()),
        io::ErrorKind::AlreadyExists => ApiError::Conflict(error.to_string()),
        _ => ApiError::Internal(error.to_string()),
    }
}

// ═══════════════════════════════════════════════════════════════════
// Context Audit —— Bundle / Fragment 产出与消费的可观测轨迹（Step 10d）
// ═══════════════════════════════════════════════════════════════════

/// Content preview 的最大字节数（超过时截断）。
const CONTEXT_AUDIT_CONTENT_PREVIEW_MAX: usize = 2048;

/// `GET /sessions/{id}/context/audit` 的查询参数。
#[derive(Debug, Deserialize)]
pub struct ContextAuditQuery {
    pub since_ms: Option<u64>,
    /// scope 标签：`runtime_agent` / `title_gen` / `summarizer` / `bridge_replay` / `audit`
    pub scope: Option<String>,
    pub slot: Option<String>,
    pub source_prefix: Option<String>,
}

/// 审计事件的 HTTP DTO。
#[derive(Debug, Serialize)]
pub struct ContextAuditEventDto {
    pub event_id: uuid::Uuid,
    pub bundle_id: uuid::Uuid,
    /// session 外部 ID（session runtime 分配的 `sess-<ms>-<short>`）。
    pub session_id: String,
    /// Bundle 内部追踪 UUID（可能是占位值，与 `session_id` 不同）。
    pub bundle_session_uuid: uuid::Uuid,
    pub at_ms: u64,
    /// 触发标签（snake_case）：`session_bootstrap` / `composer_rebuild` /
    /// `hook:UserPromptSubmit` / `session_plan` / `capability` / `filter:runtime_agent`
    pub trigger: String,
    pub slot: String,
    pub label: String,
    pub source: String,
    pub order: i32,
    pub scope: Vec<String>,
    pub content_preview: String,
    pub content_hash: u64,
    pub full_content_available: bool,
}

fn parse_scope_tag(tag: &str) -> Option<agentdash_spi::FragmentScope> {
    match tag {
        "runtime_agent" => Some(agentdash_spi::FragmentScope::RuntimeAgent),
        "title_gen" => Some(agentdash_spi::FragmentScope::TitleGen),
        "summarizer" => Some(agentdash_spi::FragmentScope::Summarizer),
        "bridge_replay" => Some(agentdash_spi::FragmentScope::BridgeReplay),
        "audit" => Some(agentdash_spi::FragmentScope::Audit),
        _ => None,
    }
}

fn scope_set_to_tags(scope: agentdash_spi::FragmentScopeSet) -> Vec<String> {
    let mut tags = Vec::new();
    for (label, s) in [
        ("runtime_agent", agentdash_spi::FragmentScope::RuntimeAgent),
        ("title_gen", agentdash_spi::FragmentScope::TitleGen),
        ("summarizer", agentdash_spi::FragmentScope::Summarizer),
        ("bridge_replay", agentdash_spi::FragmentScope::BridgeReplay),
        ("audit", agentdash_spi::FragmentScope::Audit),
    ] {
        if scope.contains(s) {
            tags.push(label.to_string());
        }
    }
    tags
}

/// `GET /sessions/{id}/context/audit` —— 返回 session 的 Fragment 审计时间线。
///
/// 返回按 `at_ms` 升序的事件列表（审计总线内部已保持插入顺序）。
pub async fn get_session_context_audit(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(session_id): Path<String>,
    Query(query): Query<ContextAuditQuery>,
) -> Result<Json<Vec<ContextAuditEventDto>>, ApiError> {
    ensure_session_permission(
        state.as_ref(),
        &current_user,
        &session_id,
        ProjectPermission::View,
    )
    .await?;

    let scope = match query.scope.as_deref() {
        Some(raw) => match parse_scope_tag(raw) {
            Some(s) => Some(s),
            None => return Err(ApiError::BadRequest(format!("无效的 scope: {raw}"))),
        },
        None => None,
    };

    let filter = agentdash_application::context::AuditFilter {
        since_ms: query.since_ms,
        scope,
        slot: query.slot.clone(),
        source_prefix: query.source_prefix.clone(),
    };

    let events = state.services.audit_bus.query(&session_id, &filter);
    let dtos: Vec<ContextAuditEventDto> = events
        .into_iter()
        .map(|event| {
            let full_len = event.fragment.content.len();
            let truncated = full_len > CONTEXT_AUDIT_CONTENT_PREVIEW_MAX;
            let preview = if truncated {
                // 按字符边界截断，避免切断 UTF-8 多字节
                let mut end = CONTEXT_AUDIT_CONTENT_PREVIEW_MAX;
                while end > 0 && !event.fragment.content.is_char_boundary(end) {
                    end -= 1;
                }
                event.fragment.content[..end].to_string()
            } else {
                event.fragment.content.clone()
            };
            ContextAuditEventDto {
                event_id: event.event_id,
                bundle_id: event.bundle_id,
                session_id: event.session_id,
                bundle_session_uuid: event.bundle_session_uuid,
                at_ms: event.at_ms,
                trigger: event.trigger.as_tag(),
                slot: event.fragment.slot,
                label: event.fragment.label,
                source: event.fragment.source,
                order: event.fragment.order,
                scope: scope_set_to_tags(event.fragment.scope),
                content_preview: preview,
                content_hash: event.content_hash,
                full_content_available: truncated,
            }
        })
        .collect();

    Ok(Json(dtos))
}
