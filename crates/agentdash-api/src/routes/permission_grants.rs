use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, Query, State},
};
use uuid::Uuid;

use agentdash_contracts::permission::{
    ListPermissionGrantsQuery, PermissionGrantResponse, PermissionGrantScopeDto,
    PermissionGrantStatusDto, PermissionGrantStatusGroupDto, PermissionGrantVfsAccessRuleDto,
    PermissionGrantVfsOperationDto, PermissionGrantVfsPathScopeDto, PolicyDecisionDto,
    PolicyOutcomeDto, ScopeEscalationIntentDto,
};
use agentdash_domain::permission::{
    GrantStatus, PermissionGrant, PermissionGrantStatusFilter, PermissionGrantVfsAccessRule,
    PermissionGrantVfsOperation, PermissionGrantVfsPathScope, PolicyOutcome,
};

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

fn vfs_operation_to_dto(operation: PermissionGrantVfsOperation) -> PermissionGrantVfsOperationDto {
    match operation {
        PermissionGrantVfsOperation::Read => PermissionGrantVfsOperationDto::Read,
        PermissionGrantVfsOperation::List => PermissionGrantVfsOperationDto::List,
        PermissionGrantVfsOperation::Search => PermissionGrantVfsOperationDto::Search,
        PermissionGrantVfsOperation::Write => PermissionGrantVfsOperationDto::Write,
        PermissionGrantVfsOperation::Exec => PermissionGrantVfsOperationDto::Exec,
        PermissionGrantVfsOperation::ApplyPatch => PermissionGrantVfsOperationDto::ApplyPatch,
    }
}

fn vfs_path_scope_to_dto(scope: &PermissionGrantVfsPathScope) -> PermissionGrantVfsPathScopeDto {
    match scope {
        PermissionGrantVfsPathScope::All => PermissionGrantVfsPathScopeDto::All,
        PermissionGrantVfsPathScope::Prefix(prefix) => {
            PermissionGrantVfsPathScopeDto::Prefix(prefix.clone())
        }
    }
}

fn vfs_access_rule_to_dto(rule: &PermissionGrantVfsAccessRule) -> PermissionGrantVfsAccessRuleDto {
    PermissionGrantVfsAccessRuleDto {
        surface_ref: rule.surface_ref.clone(),
        mount_id: rule.mount_id.clone(),
        path_scope: vfs_path_scope_to_dto(&rule.path_scope),
        operations: rule
            .operations
            .iter()
            .copied()
            .map(vfs_operation_to_dto)
            .collect(),
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
        requested_paths: grant
            .requested_paths
            .iter()
            .map(|p| p.to_qualified_string())
            .collect(),
        requested_vfs_access: grant
            .requested_vfs_access
            .iter()
            .map(vfs_access_rule_to_dto)
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
