use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::Response;
use serde::Deserialize;

use agentdash_plugin_api::{
    AuthCallbackRequest, AuthStartRequest, AuthStartResponse, LoginCredentials, LoginMetadata,
    LoginResponse,
};

use crate::app_state::AppState;
use crate::auth::{CurrentUser, map_auth_error, persist_identity_snapshot};
use crate::rpc::ApiError;

#[derive(Debug, Deserialize, Default)]
pub struct TokenQuery {
    pub token: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RevokeTokenRequest {
    pub access_token: String,
}

#[derive(Debug, Deserialize)]
pub struct OidcCallbackQuery {
    pub code: String,
    pub state: String,
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

    if let Err(err) = persist_identity_snapshot(state.as_ref(), &response.identity).await {
        tracing::warn!(
            user_id = %response.identity.user_id,
            auth_mode = %response.identity.auth_mode,
            error = %err,
            "登录成功，但写入用户身份投影失败"
        );
    }

    Ok(Json(response))
}

/// POST /api/auth/oidc/start — 启动重定向式 OIDC 登录。
pub async fn start_oidc_login(
    State(state): State<Arc<AppState>>,
    Json(request): Json<AuthStartRequest>,
) -> Result<Json<AuthStartResponse>, ApiError> {
    let provider = state
        .auth_provider
        .as_ref()
        .ok_or_else(|| ApiError::ServiceUnavailable("认证提供者未初始化".to_string()))?;

    let response = provider
        .start_login(&request)
        .await
        .map_err(map_auth_error)?;
    Ok(Json(response))
}

/// GET /api/auth/oidc/callback — OIDC 授权码回调。
pub async fn oidc_callback(
    State(state): State<Arc<AppState>>,
    Query(query): Query<OidcCallbackQuery>,
) -> Result<Response, ApiError> {
    let provider = state
        .auth_provider
        .as_ref()
        .ok_or_else(|| ApiError::ServiceUnavailable("认证提供者未初始化".to_string()))?;

    let response = provider
        .complete_login(&AuthCallbackRequest {
            code: query.code,
            state: query.state,
        })
        .await
        .map_err(map_auth_error)?;

    state
        .services
        .auth_session_service
        .save_login_session(&response.access_token, &response.identity)
        .await
        .map_err(|e| ApiError::ServiceUnavailable(format!("认证会话落库失败: {e}")))?;

    if let Err(err) = persist_identity_snapshot(state.as_ref(), &response.identity).await {
        tracing::warn!(
            user_id = %response.identity.user_id,
            auth_mode = %response.identity.auth_mode,
            error = %err,
            "OIDC 登录成功，但写入用户身份投影失败"
        );
    }

    let redirect_to = std::env::var("AGENTDASH_OIDC_POST_LOGIN_REDIRECT")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "/".to_string());
    let cookie = format!(
        "agentdash_access_token={}; Path=/; Max-Age={}; SameSite=Lax",
        urlencoding_percent_encode(&response.access_token),
        60 * 60 * 24 * 30
    );

    Response::builder()
        .status(StatusCode::FOUND)
        .header(header::LOCATION, redirect_to)
        .header(
            header::SET_COOKIE,
            HeaderValue::from_str(&cookie)
                .map_err(|e| ApiError::ServiceUnavailable(format!("生成登录 Cookie 失败: {e}")))?,
        )
        .body(axum::body::Body::empty())
        .map_err(|e| ApiError::ServiceUnavailable(format!("生成登录跳转响应失败: {e}")))
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

fn urlencoding_percent_encode(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}
