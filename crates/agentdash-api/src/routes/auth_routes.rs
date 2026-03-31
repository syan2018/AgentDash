use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;

use agentdash_plugin_api::{LoginCredentials, LoginMetadata, LoginResponse};

use crate::app_state::AppState;
use crate::auth::{CurrentUser, map_auth_error};
use crate::rpc::ApiError;

#[derive(Debug, Deserialize, Default)]
pub struct TokenQuery {
    pub token: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RevokeTokenRequest {
    pub access_token: String,
}

/// POST /api/auth/login — 用户提交凭证换取 token + 身份
pub async fn login(
    State(state): State<Arc<AppState>>,
    Json(credentials): Json<LoginCredentials>,
) -> Result<Json<LoginResponse>, ApiError> {
    let provider = state
        .auth_provider
        .as_ref()
        .ok_or_else(|| ApiError::ServiceUnavailable("认证提供者未初始化".to_string()))?;

    let response = provider.login(&credentials).await.map_err(map_auth_error)?;

    state
        .services
        .auth_session_service
        .save_login_session(&response.access_token, &response.identity)
        .await
        .map_err(|e| ApiError::ServiceUnavailable(format!("认证会话落库失败: {e}")))?;

    Ok(Json(response))
}

/// GET /api/auth/metadata — 返回登录方式描述（不需要认证）
pub async fn metadata(State(state): State<Arc<AppState>>) -> Result<Json<LoginMetadata>, ApiError> {
    let provider = state
        .auth_provider
        .as_ref()
        .ok_or_else(|| ApiError::ServiceUnavailable("认证提供者未初始化".to_string()))?;

    Ok(Json(provider.login_metadata()))
}

/// POST /api/auth/logout — 当前 token 失效（需要认证）。
pub async fn logout(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<TokenQuery>,
) -> Result<StatusCode, ApiError> {
    let token = extract_token(&headers, query.token.as_deref())
        .ok_or_else(|| ApiError::BadRequest("缺少 access token".to_string()))?;

    state
        .services
        .auth_session_service
        .revoke_token(token)
        .await
        .map_err(|e| ApiError::ServiceUnavailable(format!("注销会话失败: {e}")))?;

    Ok(StatusCode::NO_CONTENT)
}

/// POST /api/auth/revoke — 管理员撤销任意 token（需要 is_admin）。
pub async fn revoke_token(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Json(req): Json<RevokeTokenRequest>,
) -> Result<StatusCode, ApiError> {
    if !current_user.is_admin {
        return Err(ApiError::Forbidden("仅管理员可撤销其它会话".to_string()));
    }

    state
        .services
        .auth_session_service
        .revoke_token(&req.access_token)
        .await
        .map_err(|e| ApiError::ServiceUnavailable(format!("撤销会话失败: {e}")))?;

    Ok(StatusCode::NO_CONTENT)
}

fn extract_token<'a>(headers: &'a HeaderMap, query_token: Option<&'a str>) -> Option<&'a str> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| {
            v.strip_prefix("Bearer ")
                .or_else(|| v.strip_prefix("bearer "))
        })
        .or(query_token)
}
