use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use uuid::Uuid;

use agentdash_application::backend::runner_registration::{
    RunnerRegistrationClaimError, RunnerRegistrationClaimInput, RunnerRegistrationTokenCreateInput,
    claim_runner_registration_token, create_runner_registration_token,
    list_runner_registration_tokens, revoke_runner_registration_token,
    rotate_runner_registration_token,
};
use agentdash_contracts::backend::{
    BackendShareScopeKind as BackendShareScopeKindDto, RunnerRegistrationClaimRequest,
    RunnerRegistrationClaimResponse, RunnerRegistrationTokenCreateRequest,
    RunnerRegistrationTokenCreateResponse, RunnerRegistrationTokenMetadataResponse,
    RunnerRegistrationTokenRevokeResponse, RunnerRegistrationTokenRotateResponse,
};

use crate::app_state::AppState;
use crate::auth::{CurrentUser, ProjectPermission, load_project_with_permission};
use crate::rpc::ApiError;

pub fn router() -> axum::Router<Arc<AppState>> {
    axum::Router::new()
        .route(
            "/projects/{project_id}/runner-registration-tokens",
            axum::routing::get(list_tokens).post(create_token),
        )
        .route(
            "/projects/{project_id}/runner-registration-tokens/{token_id}/revoke",
            axum::routing::post(revoke_token),
        )
        .route(
            "/projects/{project_id}/runner-registration-tokens/{token_id}/rotate",
            axum::routing::post(rotate_token),
        )
}

pub fn public_router() -> axum::Router<Arc<AppState>> {
    axum::Router::new().route(
        "/local-runtime/runner/claim",
        axum::routing::post(claim_runner),
    )
}

async fn create_token(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(project_id): Path<String>,
    Json(req): Json<RunnerRegistrationTokenCreateRequest>,
) -> Result<Json<RunnerRegistrationTokenCreateResponse>, ApiError> {
    let project_id = parse_project_id(&project_id)?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Edit,
    )
    .await?;

    let created = create_runner_registration_token(
        &state.repos,
        RunnerRegistrationTokenCreateInput {
            project_id,
            name: req.name,
            created_by_user_id: current_user.user_id.clone(),
            expires_at: req.expires_at,
            default_capability_slot: req.default_capability_slot,
            machine_policy: req.machine_policy,
        },
    )
    .await?;

    Ok(Json(RunnerRegistrationTokenCreateResponse {
        token: RunnerRegistrationTokenMetadataResponse::from(created.token),
        registration_token: created.registration_token,
    }))
}

async fn list_tokens(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(project_id): Path<String>,
) -> Result<Json<Vec<RunnerRegistrationTokenMetadataResponse>>, ApiError> {
    let project_id = parse_project_id(&project_id)?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Edit,
    )
    .await?;
    let tokens = list_runner_registration_tokens(&state.repos, project_id).await?;
    Ok(Json(
        tokens
            .into_iter()
            .map(RunnerRegistrationTokenMetadataResponse::from)
            .collect(),
    ))
}

async fn revoke_token(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((project_id, token_id)): Path<(String, String)>,
) -> Result<Json<RunnerRegistrationTokenRevokeResponse>, ApiError> {
    let project_id = parse_project_id(&project_id)?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Edit,
    )
    .await?;
    let token = revoke_runner_registration_token(&state.repos, project_id, token_id.trim()).await?;
    Ok(Json(RunnerRegistrationTokenRevokeResponse {
        token: RunnerRegistrationTokenMetadataResponse::from(token),
    }))
}

async fn rotate_token(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((project_id, token_id)): Path<(String, String)>,
) -> Result<Json<RunnerRegistrationTokenRotateResponse>, ApiError> {
    let project_id = parse_project_id(&project_id)?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Edit,
    )
    .await?;
    let rotated = rotate_runner_registration_token(
        &state.repos,
        project_id,
        token_id.trim(),
        current_user.user_id.clone(),
    )
    .await?;
    Ok(Json(RunnerRegistrationTokenRotateResponse {
        token: RunnerRegistrationTokenMetadataResponse::from(rotated.token),
        registration_token: rotated.registration_token,
    }))
}

async fn claim_runner(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<RunnerRegistrationClaimRequest>,
) -> Result<Json<RunnerRegistrationClaimResponse>, ApiError> {
    let registration_token = resolve_registration_token(&headers, req.registration_token)?;
    let relay_ws_url = relay_ws_url_from_headers(&headers);
    let result = claim_runner_registration_token(
        &state.repos,
        RunnerRegistrationClaimInput {
            registration_token,
            machine_id: req.machine_id,
            machine_label: req.machine_label,
            runner_name: req.runner_name,
            client_version: req.client_version,
            device: req.device,
            executor_enabled: req.executor_enabled,
            capability_slot: req.capability_slot,
            relay_ws_url,
        },
    )
    .await
    .map_err(api_error_from_claim)?;

    Ok(Json(RunnerRegistrationClaimResponse {
        backend_id: result.backend_id,
        name: result.name,
        relay_ws_url: result.relay_ws_url,
        auth_token: result.auth_token,
        machine_id: result.machine_id,
        machine_label: result.machine_label,
        share_scope_kind: BackendShareScopeKindDto::from(result.share_scope_kind),
        share_scope_id: result.share_scope_id,
        capability_slot: result.capability_slot,
        registration_source: "runner_registration_token".to_string(),
        claimed_at: result.claimed_at,
    }))
}

fn parse_project_id(raw: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(raw).map_err(|_| ApiError::BadRequest("无效的 Project ID".into()))
}

fn resolve_registration_token(
    headers: &HeaderMap,
    body_token: Option<String>,
) -> Result<String, ApiError> {
    body_token
        .and_then(|value| normalize_optional_string(Some(value)))
        .or_else(|| {
            headers
                .get("authorization")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| {
                    value
                        .strip_prefix("Bearer ")
                        .or_else(|| value.strip_prefix("bearer "))
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(str::to_string)
                })
        })
        .ok_or_else(|| ApiError::Unauthorized("runner registration token 缺失".to_string()))
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value.and_then(|raw| {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn relay_ws_url_from_headers(headers: &HeaderMap) -> String {
    let host = headers
        .get("x-forwarded-host")
        .or_else(|| headers.get("host"))
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("127.0.0.1:3001");
    let proto = headers
        .get("x-forwarded-proto")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.split(',').next().unwrap_or(value).trim())
        .unwrap_or("http");
    let ws_scheme = if proto.eq_ignore_ascii_case("https") {
        "wss"
    } else {
        "ws"
    };
    format!("{ws_scheme}://{host}/ws/backend")
}

fn api_error_from_claim(error: RunnerRegistrationClaimError) -> ApiError {
    match error {
        RunnerRegistrationClaimError::MissingToken => {
            ApiError::Unauthorized("runner registration token 缺失".to_string())
        }
        RunnerRegistrationClaimError::InvalidToken => {
            ApiError::Unauthorized("runner registration token 无效".to_string())
        }
        RunnerRegistrationClaimError::ExpiredToken => {
            ApiError::Unauthorized("runner registration token 已过期".to_string())
        }
        RunnerRegistrationClaimError::RevokedToken => {
            ApiError::Forbidden("runner registration token 已撤销".to_string())
        }
        RunnerRegistrationClaimError::BadRequest(message) => ApiError::BadRequest(message),
        RunnerRegistrationClaimError::Forbidden(message) => ApiError::Forbidden(message),
        RunnerRegistrationClaimError::Conflict(message) => ApiError::Conflict(message),
        RunnerRegistrationClaimError::Internal(message) => ApiError::Internal(message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use axum::response::IntoResponse;

    #[test]
    fn public_claim_token_can_come_from_body_without_authorization_header() {
        let headers = HeaderMap::new();

        let token =
            resolve_registration_token(&headers, Some("  adrt_rtok_abc_secret  ".to_string()))
                .expect("body token should be accepted");

        assert_eq!(token, "adrt_rtok_abc_secret");
    }

    #[test]
    fn public_claim_token_can_come_from_bearer_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            "Bearer adrt_rtok_abc_secret".parse().expect("header value"),
        );

        let token =
            resolve_registration_token(&headers, None).expect("bearer token should be accepted");

        assert_eq!(token, "adrt_rtok_abc_secret");
    }

    #[test]
    fn public_claim_missing_token_maps_to_unauthorized() {
        let response = resolve_registration_token(&HeaderMap::new(), None)
            .expect_err("missing token should fail")
            .into_response();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn claim_error_mapping_keeps_runner_operator_status_classes_stable() {
        let invalid =
            api_error_from_claim(RunnerRegistrationClaimError::InvalidToken).into_response();
        assert_eq!(invalid.status(), StatusCode::UNAUTHORIZED);

        let expired =
            api_error_from_claim(RunnerRegistrationClaimError::ExpiredToken).into_response();
        assert_eq!(expired.status(), StatusCode::UNAUTHORIZED);

        let revoked =
            api_error_from_claim(RunnerRegistrationClaimError::RevokedToken).into_response();
        assert_eq!(revoked.status(), StatusCode::FORBIDDEN);

        let conflict = api_error_from_claim(RunnerRegistrationClaimError::Conflict(
            "ProjectBackendAccess 并发创建冲突".to_string(),
        ))
        .into_response();
        assert_eq!(conflict.status(), StatusCode::CONFLICT);
    }
}
