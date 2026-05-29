use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use agentdash_domain::permission::{GrantStatus, PermissionGrant};

use crate::{app_state::AppState, auth::CurrentUser, rpc::ApiError};

// ── DTOs ──

#[derive(Serialize)]
pub struct PermissionGrantDto {
    pub id: String,
    pub run_id: String,
    pub session_id: String,
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
        session_id: grant.session_id.clone(),
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

// ── Query params ──

#[derive(Deserialize)]
pub struct ListGrantsQuery {
    pub session_id: Option<String>,
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
    let grants = if let Some(session_id) = &query.session_id {
        match query.status.as_deref() {
            Some("active") | None => {
                state
                    .repos
                    .permission_grant_repo
                    .list_active_by_session(session_id)
                    .await?
            }
            Some(_status) => {
                // For specific non-active status, use active listing (extensible later)
                state
                    .repos
                    .permission_grant_repo
                    .list_active_by_session(session_id)
                    .await?
            }
        }
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
            "session_id or run_id query param required".to_string(),
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

    let mut grant = state
        .repos
        .permission_grant_repo
        .find_by_id(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("grant not found: {grant_id}")))?;

    if grant.status != GrantStatus::PendingUserApproval {
        return Err(ApiError::BadRequest(format!(
            "grant is not pending user approval (status={})",
            grant.status.as_str()
        )));
    }

    grant
        .user_approve(&current_user.user_id)
        .map_err(|e| ApiError::Internal(format!("state transition failed: {e}")))?;

    grant
        .mark_applied()
        .map_err(|e| ApiError::Internal(format!("mark_applied failed: {e}")))?;

    state.repos.permission_grant_repo.update(&grant).await?;

    // TODO(permission-system): trigger capability runtime apply here
    // The caller (frontend) should also call a session capability refresh endpoint

    Ok(Json(grant_to_dto(&grant)))
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

    let mut grant = state
        .repos
        .permission_grant_repo
        .find_by_id(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("grant not found: {grant_id}")))?;

    if grant.status != GrantStatus::PendingUserApproval {
        return Err(ApiError::BadRequest(format!(
            "grant is not pending user approval (status={})",
            grant.status.as_str()
        )));
    }

    grant
        .user_reject()
        .map_err(|e| ApiError::Internal(format!("state transition failed: {e}")))?;

    state.repos.permission_grant_repo.update(&grant).await?;

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

    let mut grant = state
        .repos
        .permission_grant_repo
        .find_by_id(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("grant not found: {grant_id}")))?;

    if !grant.status.is_active() {
        return Err(ApiError::BadRequest(format!(
            "grant is not active (status={})",
            grant.status.as_str()
        )));
    }

    grant
        .revoke()
        .map_err(|e| ApiError::Internal(format!("revoke failed: {e}")))?;

    state.repos.permission_grant_repo.update(&grant).await?;

    // TODO(permission-system): trigger capability runtime revocation here

    Ok(Json(grant_to_dto(&grant)))
}
