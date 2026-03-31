use std::sync::Arc;

use axum::Json;
use axum::extract::State;

use agentdash_plugin_api::{LoginCredentials, LoginMetadata, LoginResponse};

use crate::app_state::AppState;
use crate::auth::map_auth_error;
use crate::rpc::ApiError;

/// POST /api/auth/login — 用户提交凭证换取 token + 身份
pub async fn login(
    State(state): State<Arc<AppState>>,
    Json(credentials): Json<LoginCredentials>,
) -> Result<Json<LoginResponse>, ApiError> {
    let provider = state
        .auth_provider
        .as_ref()
        .ok_or_else(|| ApiError::ServiceUnavailable("认证提供者未初始化".to_string()))?;

    let response = provider
        .login(&credentials)
        .await
        .map_err(map_auth_error)?;

    Ok(Json(response))
}

/// GET /api/auth/metadata — 返回登录方式描述（不需要认证）
pub async fn metadata(
    State(state): State<Arc<AppState>>,
) -> Result<Json<LoginMetadata>, ApiError> {
    let provider = state
        .auth_provider
        .as_ref()
        .ok_or_else(|| ApiError::ServiceUnavailable("认证提供者未初始化".to_string()))?;

    Ok(Json(provider.login_metadata()))
}
