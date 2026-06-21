use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, Query, State},
};
use uuid::Uuid;

use agentdash_application::{permission::PermissionGrantService, session::AgentFrameRuntimeTarget};
use agentdash_contracts::permission::{
    ListPermissionGrantsQuery, PermissionGrantResponse, PermissionGrantScopeDto,
    PermissionGrantStatusDto, PermissionGrantStatusGroupDto, PolicyDecisionDto, PolicyOutcomeDto,
    ScopeEscalationIntentDto,
};
use agentdash_domain::permission::{
    GrantStatus, PermissionGrant, PermissionGrantStatusFilter, PolicyOutcome,
};
use agentdash_domain::workflow::AgentFrame;

use crate::{app_state::AppState, auth::CurrentUser, rpc::ApiError};

// ── DTOs ──

fn grant_scope_to_dto(scope: agentdash_domain::permission::GrantScope) -> PermissionGrantScopeDto {
    match scope {
        agentdash_domain::permission::GrantScope::Turn => PermissionGrantScopeDto::Turn,
        agentdash_domain::permission::GrantScope::AgentFrame => PermissionGrantScopeDto::AgentFrame,
        agentdash_domain::permission::GrantScope::Activity => PermissionGrantScopeDto::Activity,
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

fn grant_status_to_domain(status: PermissionGrantStatusDto) -> GrantStatus {
    match status {
        PermissionGrantStatusDto::Created => GrantStatus::Created,
        PermissionGrantStatusDto::PendingPolicy => GrantStatus::PendingPolicy,
        PermissionGrantStatusDto::PendingUserApproval => GrantStatus::PendingUserApproval,
        PermissionGrantStatusDto::Approved => GrantStatus::Approved,
        PermissionGrantStatusDto::Rejected => GrantStatus::Rejected,
        PermissionGrantStatusDto::Applied => GrantStatus::Applied,
        PermissionGrantStatusDto::Failed => GrantStatus::Failed,
        PermissionGrantStatusDto::Expired => GrantStatus::Expired,
        PermissionGrantStatusDto::Revoked => GrantStatus::Revoked,
        PermissionGrantStatusDto::ScopeEscalated => GrantStatus::ScopeEscalated,
    }
}

fn status_group_to_filter(group: PermissionGrantStatusGroupDto) -> PermissionGrantStatusFilter {
    match group {
        PermissionGrantStatusGroupDto::Pending => PermissionGrantStatusFilter::Pending,
        PermissionGrantStatusGroupDto::Active => PermissionGrantStatusFilter::Active,
        PermissionGrantStatusGroupDto::Terminal => PermissionGrantStatusFilter::Terminal,
    }
}

fn policy_outcome_to_dto(outcome: PolicyOutcome) -> PolicyOutcomeDto {
    match outcome {
        PolicyOutcome::AutoApproved => PolicyOutcomeDto::AutoApproved,
        PolicyOutcome::NeedsUserApproval => PolicyOutcomeDto::NeedsUserApproval,
        PolicyOutcome::Rejected => PolicyOutcomeDto::Rejected,
    }
}

fn status_filter_from_query(
    query: &ListPermissionGrantsQuery,
) -> Result<Option<PermissionGrantStatusFilter>, ApiError> {
    match (query.status, query.status_group) {
        (Some(_), Some(_)) => Err(ApiError::BadRequest(
            "status and status_group cannot be used together".to_string(),
        )),
        (Some(status), None) => Ok(Some(PermissionGrantStatusFilter::Exact(
            grant_status_to_domain(status),
        ))),
        (None, Some(group)) => Ok(Some(status_group_to_filter(group))),
        (None, None) => Ok(None),
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
        scope_escalation_intent: grant.scope_escalation_intent.as_ref().map(|intent| {
            ScopeEscalationIntentDto {
                target_subject_kind: intent.target_subject_kind.clone(),
                unlocked_paths: intent
                    .unlocked_paths
                    .iter()
                    .map(|path| path.to_qualified_string())
                    .collect(),
            }
        }),
        status: grant_status_to_dto(grant.status),
        policy_decision: grant
            .policy_decision
            .as_ref()
            .map(|decision| PolicyDecisionDto {
                outcome: policy_outcome_to_dto(decision.outcome),
                matched_rules: decision.matched_rules.clone(),
                reason: decision.reason.clone(),
            }),
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
    let status_filter = status_filter_from_query(&query)?;

    let grants = if let Some(frame_id) = &query.effect_frame_id {
        let frame_uuid: Uuid = frame_id
            .parse()
            .map_err(|_| ApiError::BadRequest(format!("invalid effect_frame_id: {frame_id}")))?;
        state
            .repos
            .permission_grant_repo
            .list_by_frame(frame_uuid, status_filter)
            .await?
    } else if let Some(run_id) = &query.run_id {
        let run_uuid: Uuid = run_id
            .parse()
            .map_err(|_| ApiError::BadRequest(format!("invalid run_id: {run_id}")))?;
        state
            .repos
            .permission_grant_repo
            .list_by_run(run_uuid, status_filter)
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
    adopt_effect_frame_if_present(&state, &result.grant, result.effect_frame.as_ref()).await;

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
    adopt_effect_frame_if_present(&state, &result.grant, result.effect_frame.as_ref()).await;

    Ok(Json(grant_to_dto(&result.grant)))
}

async fn adopt_effect_frame_if_present(
    state: &AppState,
    grant: &PermissionGrant,
    effect_frame: Option<&AgentFrame>,
) {
    let Some(effect_frame) = effect_frame else {
        return;
    };
    if let Err(error) = state
        .services
        .session_capability
        .adopt_persisted_agent_frame_revision(AgentFrameRuntimeTarget {
            frame_id: effect_frame.id,
            delivery_runtime_session_id: grant.source_runtime_session_id.clone(),
        })
        .await
    {
        tracing::warn!(
            grant_id = %grant.id,
            effect_frame_id = %effect_frame.id,
            delivery_runtime_session_id = grant.source_runtime_session_id,
            "PermissionGrant effect frame active-runtime adoption skipped: {error}"
        );
    }
}
