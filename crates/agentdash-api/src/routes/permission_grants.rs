use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use agentdash_application::permission::PermissionGrantService;
use agentdash_domain::permission::PermissionGrant;

use crate::{app_state::AppState, auth::CurrentUser, rpc::ApiError};

// ── DTOs ──

#[derive(Serialize)]
pub struct PermissionGrantDto {
    pub id: String,
    pub run_id: String,
    pub effect_frame_id: Option<String>,
    pub source_runtime_session_id: String,
    pub requested_paths: Vec<String>,
    pub reason: String,
    pub grant_scope: String,
    pub expires_at: Option<String>,
    pub scope_escalation_intent: Option<serde_json::Value>,
    pub status: String,
    pub policy_decision: Option<serde_json::Value>,
    pub approved_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

fn grant_to_dto(grant: &PermissionGrant) -> PermissionGrantDto {
    PermissionGrantDto {
        id: grant.id.to_string(),
        run_id: grant.run_id.to_string(),
        effect_frame_id: grant.effect_frame_id.map(|id| id.to_string()),
        source_runtime_session_id: grant.source_runtime_session_id.clone(),
        requested_paths: grant
            .requested_paths
            .iter()
            .map(|p| p.to_qualified_string())
            .collect(),
        reason: grant.reason.clone(),
        grant_scope: grant.grant_scope.as_str().to_string(),
        expires_at: grant.expires_at.map(|t| t.to_rfc3339()),
        scope_escalation_intent: grant
            .scope_escalation_intent
            .as_ref()
            .and_then(|v| serde_json::to_value(v).ok()),
        status: grant.status.as_str().to_string(),
        policy_decision: grant
            .policy_decision
            .as_ref()
            .and_then(|v| serde_json::to_value(v).ok()),
        approved_by: grant.approved_by.clone(),
        created_at: grant.created_at.to_rfc3339(),
        updated_at: grant.updated_at.to_rfc3339(),
    }
}

pub fn router() -> axum::Router<Arc<AppState>> {
    axum::Router::new()
        .route("/permission-grants", axum::routing::get(list_grants))
        .route("/permission-grants/{id}", axum::routing::get(get_grant))
        .route(
            "/permission-grants/{id}/approve",
            axum::routing::post(approve_grant),
        )
        .route(
            "/permission-grants/{id}/reject",
            axum::routing::post(reject_grant),
        )
        .route(
            "/permission-grants/{id}/revoke",
            axum::routing::post(revoke_grant),
        )
}

// ── Query params ──

#[derive(Deserialize)]
pub struct ListGrantsQuery {
    pub effect_frame_id: Option<String>,
    pub run_id: Option<String>,
    pub status: Option<String>,
}

// ── Handlers ──

/// GET /permission-grants
pub async fn list_grants(
    State(state): State<Arc<AppState>>,
    CurrentUser(_current_user): CurrentUser,
    Query(query): Query<ListGrantsQuery>,
) -> Result<Json<Vec<PermissionGrantDto>>, ApiError> {
    let grants = if let Some(frame_id) = &query.effect_frame_id {
        let frame_uuid: Uuid = frame_id
            .parse()
            .map_err(|_| ApiError::BadRequest(format!("invalid effect_frame_id: {frame_id}")))?;
        state
            .repos
            .permission_grant_repo
            .list_active_by_frame(frame_uuid)
            .await?
    } else if let Some(run_id) = &query.run_id {
        let run_uuid: Uuid = run_id
            .parse()
            .map_err(|_| ApiError::BadRequest(format!("invalid run_id: {run_id}")))?;
        state
            .repos
            .permission_grant_repo
            .list_active_by_run(run_uuid)
            .await?
    } else {
        return Err(ApiError::BadRequest(
            "effect_frame_id or run_id query param required".to_string(),
        ));
    };

    Ok(Json(grants.iter().map(grant_to_dto).collect()))
}

/// GET /permission-grants/:id
pub async fn get_grant(
    State(state): State<Arc<AppState>>,
    CurrentUser(_current_user): CurrentUser,
    Path(grant_id): Path<String>,
) -> Result<Json<PermissionGrantDto>, ApiError> {
    let id: Uuid = grant_id
        .parse()
        .map_err(|_| ApiError::BadRequest(format!("invalid grant_id: {grant_id}")))?;

    let grant = state
        .repos
        .permission_grant_repo
        .find_by_id(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("grant not found: {grant_id}")))?;

    Ok(Json(grant_to_dto(&grant)))
}

/// POST /permission-grants/:id/approve
pub async fn approve_grant(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(grant_id): Path<String>,
) -> Result<Json<PermissionGrantDto>, ApiError> {
    let id: Uuid = grant_id
        .parse()
        .map_err(|_| ApiError::BadRequest(format!("invalid grant_id: {grant_id}")))?;

    let result = PermissionGrantService::new(
        state.repos.permission_grant_repo.clone(),
        state.repos.agent_frame_repo.clone(),
    )
    .approve(id, &current_user.user_id)
    .await?;

    Ok(Json(grant_to_dto(&result.grant)))
}

/// POST /permission-grants/:id/reject
pub async fn reject_grant(
    State(state): State<Arc<AppState>>,
    CurrentUser(_current_user): CurrentUser,
    Path(grant_id): Path<String>,
) -> Result<Json<PermissionGrantDto>, ApiError> {
    let id: Uuid = grant_id
        .parse()
        .map_err(|_| ApiError::BadRequest(format!("invalid grant_id: {grant_id}")))?;

    let grant = PermissionGrantService::new(
        state.repos.permission_grant_repo.clone(),
        state.repos.agent_frame_repo.clone(),
    )
    .reject(id)
    .await?;

    Ok(Json(grant_to_dto(&grant)))
}

/// POST /permission-grants/:id/revoke
pub async fn revoke_grant(
    State(state): State<Arc<AppState>>,
    CurrentUser(_current_user): CurrentUser,
    Path(grant_id): Path<String>,
) -> Result<Json<PermissionGrantDto>, ApiError> {
    let id: Uuid = grant_id
        .parse()
        .map_err(|_| ApiError::BadRequest(format!("invalid grant_id: {grant_id}")))?;

    let result = PermissionGrantService::new(
        state.repos.permission_grant_repo.clone(),
        state.repos.agent_frame_repo.clone(),
    )
    .revoke(id)
    .await?;

    Ok(Json(grant_to_dto(&result.grant)))
}
