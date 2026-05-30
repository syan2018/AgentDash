use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use agentdash_application::session::SessionExecutionState;

use crate::{
    app_state::AppState,
    auth::{CurrentUser, ProjectPermission, load_project_with_permission},
    rpc::ApiError,
};

#[derive(Debug, Serialize)]
pub struct ProjectSessionDetailResponse {
    pub session_id: String,
    pub session_title: Option<String>,
    pub last_activity: Option<i64>,
}

pub async fn get_project_session(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((project_id, session_id)): Path<(String, String)>,
) -> Result<Json<ProjectSessionDetailResponse>, ApiError> {
    let project_uuid = Uuid::parse_str(&project_id)
        .map_err(|_| ApiError::BadRequest(format!("无效的 project_id: {project_id}")))?;

    let _project = load_project_with_permission(
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
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Session {session_id} 不存在")))?;

    Ok(Json(ProjectSessionDetailResponse {
        session_id: meta.id,
        session_title: Some(meta.title),
        last_activity: Some(meta.updated_at),
    }))
}

// ─── Project Sessions 聚合 API ────────────────────────────────────────────────

/// 项目级 Session 聚合条目
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ProjectSessionEntry {
    pub session_id: String,
    pub session_title: Option<String>,
    pub last_activity: Option<i64>,
    pub execution_status: String,
    pub agent_key: Option<String>,
    pub agent_display_name: Option<String>,
    pub parent_session_id: Option<String>,
    pub parent_relation_kind: Option<String>,
}

/// GET /api/projects/{project_id}/sessions 查询参数
#[derive(Debug, Deserialize)]
pub struct ListProjectSessionsQuery {
    pub status: Option<String>,
    pub limit: Option<i64>,
}

/// GET /api/projects/{project_id}/sessions
///
/// TODO(permission-system): 项目 session 列表基于 SessionMeta.project_id 查询
pub async fn list_project_sessions(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(project_id): Path<String>,
    Query(_query): Query<ListProjectSessionsQuery>,
) -> Result<Json<Vec<ProjectSessionEntry>>, ApiError> {
    let project_uuid = Uuid::parse_str(&project_id)
        .map_err(|_| ApiError::BadRequest(format!("无效的 project_id: {project_id}")))?;

    let _project = load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_uuid,
        ProjectPermission::View,
    )
    .await?;

    // TODO(permission-system): 基于 SessionMeta.project_id 实现列表查询
    Ok(Json(vec![]))
}

fn execution_state_to_str(state: Option<&SessionExecutionState>) -> &'static str {
    match state {
        Some(SessionExecutionState::Running { .. }) => "running",
        Some(SessionExecutionState::Completed { .. }) => "completed",
        Some(SessionExecutionState::Failed { .. }) => "failed",
        Some(SessionExecutionState::Interrupted { .. }) => "interrupted",
        _ => "idle",
    }
}

#[cfg(test)]
mod list_project_sessions_tests {
    use super::*;

    #[test]
    fn project_session_entry_serializes_as_snake_case() {
        let value = serde_json::to_value(ProjectSessionEntry {
            session_id: "sess-1".to_string(),
            session_title: Some("Test".to_string()),
            last_activity: Some(1711234567890),
            execution_status: "idle".to_string(),
            agent_key: Some("claude-code".to_string()),
            agent_display_name: None,
            parent_session_id: None,
            parent_relation_kind: None,
        })
        .expect("serialize ProjectSessionEntry");

        assert!(value.get("session_id").is_some());
        assert!(value.get("execution_status").is_some());
        assert!(value.get("agent_key").is_some());
        assert!(value.get("parent_session_id").is_some());
        assert!(value.get("parent_relation_kind").is_some());
        assert!(value.get("sessionId").is_none());
        assert!(value.get("executionStatus").is_none());
        assert!(value.get("agentKey").is_none());
        assert!(value.get("parentSessionId").is_none());
        assert!(value.get("parentRelationKind").is_none());
    }
}
