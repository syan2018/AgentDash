use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, Query, State},
};
use uuid::Uuid;

use agentdash_application::permission::PermissionGrantService;
use agentdash_contracts::permission::{
    ListPermissionGrantsQuery, PermissionGrantResponse, PermissionGrantScopeDto,
    PermissionGrantStatusDto,
};
use agentdash_domain::permission::PermissionGrant;

use crate::{app_state::AppState, auth::CurrentUser, rpc::ApiError};

// ── DTOs ──

fn grant_scope_to_dto(scope: agentdash_domain::permission::GrantScope) -> PermissionGrantScopeDto {
    match scope {
        agentdash_domain::permission::GrantScope::Turn => PermissionGrantScopeDto::Turn,
        agentdash_domain::permission::GrantScope::Session => PermissionGrantScopeDto::Session,
        agentdash_domain::permission::GrantScope::WorkflowStep => {
            PermissionGrantScopeDto::WorkflowStep
        }
    }
}

fn grant_status_to_dto(
    status: agentdash_domain::permission::GrantStatus,
) -> PermissionGrantStatusDto {
    match status {
        agentdash_domain::permission::GrantStatus::Created => PermissionGrantStatusDto::Created,
        agentdash_domain::permission::GrantStatus::PendingPolicy => {
            PermissionGrantStatusDto::PendingPolicy
        }
        agentdash_domain::permission::GrantStatus::PendingUserApproval => {
            PermissionGrantStatusDto::PendingUserApproval
        }
        agentdash_domain::permission::GrantStatus::Approved => PermissionGrantStatusDto::Approved,
        agentdash_domain::permission::GrantStatus::Rejected => PermissionGrantStatusDto::Rejected,
        agentdash_domain::permission::GrantStatus::Applied => PermissionGrantStatusDto::Applied,
        agentdash_domain::permission::GrantStatus::Failed => PermissionGrantStatusDto::Failed,
        agentdash_domain::permission::GrantStatus::Expired => PermissionGrantStatusDto::Expired,
        agentdash_domain::permission::GrantStatus::Revoked => PermissionGrantStatusDto::Revoked,
        agentdash_domain::permission::GrantStatus::ScopeEscalated => {
            PermissionGrantStatusDto::ScopeEscalated
        }
    }
}

fn grant_to_dto(grant: &PermissionGrant) -> PermissionGrantResponse {
    PermissionGrantResponse {
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
        grant_scope: grant_scope_to_dto(grant.grant_scope),
        expires_at: grant.expires_at.map(|t| t.to_rfc3339()),
        scope_escalation_intent: grant
            .scope_escalation_intent
            .as_ref()
            .and_then(|v| serde_json::to_value(v).ok()),
        status: grant_status_to_dto(grant.status),
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

// ── Handlers ──

/// GET /permission-grants
pub async fn list_grants(
    State(state): State<Arc<AppState>>,
    CurrentUser(_current_user): CurrentUser,
    Query(query): Query<ListPermissionGrantsQuery>,
) -> Result<Json<Vec<PermissionGrantResponse>>, ApiError> {
    let mut grants = if let Some(frame_id) = &query.effect_frame_id {
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

    if let Some(status) = query.status {
        grants.retain(|grant| grant_status_to_dto(grant.status) == status);
    }

    Ok(Json(grants.iter().map(grant_to_dto).collect()))
}

/// GET /permission-grants/:id
pub async fn get_grant(
    State(state): State<Arc<AppState>>,
    CurrentUser(_current_user): CurrentUser,
    Path(grant_id): Path<String>,
) -> Result<Json<PermissionGrantResponse>, ApiError> {
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
) -> Result<Json<PermissionGrantResponse>, ApiError> {
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
) -> Result<Json<PermissionGrantResponse>, ApiError> {
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
) -> Result<Json<PermissionGrantResponse>, ApiError> {
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
