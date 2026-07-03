use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::Response;

use agentdash_contracts::auth::{
    AuthStartRequest, AuthStartResponse, CurrentUser as CurrentUserResponse, LoginCredentials,
    LoginFieldDescriptor, LoginMetadata, LoginMode, LoginResponse,
};
use agentdash_diagnostics::{DiagnosticErrorContext, Subsystem, diag_error};
use agentdash_integration_api::{
    AuthCallbackRequest, AuthStartRequest as ProviderAuthStartRequest,
    LoginCredentials as ProviderLoginCredentials, LoginMetadata as ProviderLoginMetadata,
    LoginMode as ProviderLoginMode,
};
use serde_json::Value;

use crate::app_state::AppState;
use crate::auth::{CurrentUser, map_auth_error, persist_identity_snapshot_or_service_unavailable};
use crate::dto::{OidcCallbackQuery, RevokeTokenRequest, TokenQuery};
use crate::rpc::ApiError;

const TOKEN_FRAGMENT_PARAM: &str = "agentdash_access_token";
const DESKTOP_REDIRECT_ORIGINS: &[&str] = &[
    "http://tauri.localhost",
    "https://tauri.localhost",
    "tauri://localhost",
];

/// POST /api/auth/login — 用户提交凭证换取 token + 身份
pub async fn login(
    State(state): State<Arc<AppState>>,
    Json(credentials): Json<LoginCredentials>,
) -> Result<Json<LoginResponse>, ApiError> {
    let provider = state
        .auth_provider
        .as_ref()
        .ok_or_else(|| ApiError::ServiceUnavailable("认证提供者未初始化".to_string()))?;

    let credentials = ProviderLoginCredentials {
        username: credentials.username,
        password: credentials.password,
        extra: credentials.extra.unwrap_or(Value::Null),
    };

    let response = provider.login(&credentials).await.map_err(map_auth_error)?;

    persist_identity_snapshot_or_service_unavailable(state.as_ref(), &response.identity).await?;

    state
        .services
        .auth_session_service
        .save_login_session(&response.access_token, &response.identity)
        .await
        .map_err(|err| {
            let context = DiagnosticErrorContext::new("auth.login", "save_session");
            diag_error!(
                Error,
                Subsystem::Auth,
                context = &context,
                error = &err,
                route = "/api/auth/login",
                user_id = %response.identity.user_id,
                auth_mode = %response.identity.auth_mode,
                "认证登录会话落库失败"
            );
            ApiError::ServiceUnavailable(format!("认证会话落库失败: {err}"))
        })?;

    let identity = response.identity;
    Ok(Json(LoginResponse {
        access_token: response.access_token,
        identity: CurrentUserResponse::from(identity),
    }))
}

pub fn router() -> axum::Router<std::sync::Arc<crate::app_state::AppState>> {
    axum::Router::new()
        .route("/auth/logout", axum::routing::post(logout))
        .route("/auth/revoke", axum::routing::post(revoke_token))
}

pub fn public_router() -> axum::Router<std::sync::Arc<crate::app_state::AppState>> {
    axum::Router::new()
        .route("/auth/login", axum::routing::post(login))
        .route("/auth/oidc/start", axum::routing::post(start_oidc_login))
        .route("/auth/oidc/callback", axum::routing::get(oidc_callback))
        .route("/auth/metadata", axum::routing::get(metadata))
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
        .start_login(&ProviderAuthStartRequest {
            return_to: request.return_to,
        })
        .await
        .map_err(map_auth_error)?;
    Ok(Json(AuthStartResponse {
        auth_url: response.auth_url,
        state: response.state,
        expires_at_epoch_seconds: response.expires_at_epoch_seconds,
    }))
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

    persist_identity_snapshot_or_service_unavailable(state.as_ref(), &response.identity).await?;

    state
        .services
        .auth_session_service
        .save_login_session(&response.access_token, &response.identity)
        .await
        .map_err(|err| {
            let context = DiagnosticErrorContext::new("auth.oidc_callback", "save_session");
            diag_error!(
                Error,
                Subsystem::Auth,
                context = &context,
                error = &err,
                route = "/api/auth/oidc/callback",
                user_id = %response.identity.user_id,
                auth_mode = %response.identity.auth_mode,
                "OIDC 登录会话落库失败"
            );
            ApiError::ServiceUnavailable(format!("认证会话落库失败: {err}"))
        })?;

    let redirect_to =
        oidc_callback_redirect(response.redirect_to.as_deref(), &response.access_token);
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
            HeaderValue::from_str(&cookie).map_err(|err| {
                let context =
                    DiagnosticErrorContext::new("auth.oidc_callback", "build_cookie_header");
                diag_error!(
                    Error,
                    Subsystem::Auth,
                    context = &context,
                    error = &err,
                    route = "/api/auth/oidc/callback",
                    "生成登录 Cookie 失败"
                );
                ApiError::ServiceUnavailable(format!("生成登录 Cookie 失败: {err}"))
            })?,
        )
        .body(axum::body::Body::empty())
        .map_err(|err| {
            let context =
                DiagnosticErrorContext::new("auth.oidc_callback", "build_redirect_response");
            diag_error!(
                Error,
                Subsystem::Auth,
                context = &context,
                error = &err,
                route = "/api/auth/oidc/callback",
                "生成登录跳转响应失败"
            );
            ApiError::ServiceUnavailable(format!("生成登录跳转响应失败: {err}"))
        })
}

fn oidc_post_login_redirect() -> String {
    oidc_post_login_redirect_from_env(
        std::env::var("AGENTDASH_OIDC_POST_LOGIN_REDIRECT").ok(),
        std::env::var("AGENTDASH_WEB_BASE_URL").ok(),
    )
}

fn oidc_post_login_redirect_from_env(primary: Option<String>, web_base: Option<String>) -> String {
    primary
        .or(web_base)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "http://127.0.0.1:5380/".to_string())
}

fn oidc_callback_redirect(provider_redirect_to: Option<&str>, access_token: &str) -> String {
    let fallback = oidc_post_login_redirect();
    let allowed_origins = oidc_allowed_redirect_origins(&fallback);
    oidc_callback_redirect_from_parts(
        provider_redirect_to,
        access_token,
        &fallback,
        &allowed_origins,
    )
}

fn oidc_callback_redirect_from_parts(
    provider_redirect_to: Option<&str>,
    access_token: &str,
    fallback: &str,
    allowed_origins: &[String],
) -> String {
    safe_return_to_url(provider_redirect_to, allowed_origins)
        .map(|url| redirect_with_access_token_fragment(url, access_token))
        .unwrap_or_else(|| fallback.to_string())
}

fn oidc_allowed_redirect_origins(fallback: &str) -> Vec<String> {
    let mut origins = Vec::new();
    for value in [
        Some(fallback.to_string()),
        std::env::var("AGENTDASH_OIDC_POST_LOGIN_REDIRECT").ok(),
        std::env::var("AGENTDASH_WEB_BASE_URL").ok(),
        std::env::var("AGENTDASH_PUBLIC_ORIGIN").ok(),
        std::env::var("AGENTDASH_PUBLIC_BASE_URL").ok(),
    ]
    .into_iter()
    .flatten()
    {
        push_allowed_origin(&mut origins, &value);
    }
    for origin in DESKTOP_REDIRECT_ORIGINS {
        push_allowed_origin(&mut origins, origin);
    }
    origins
}

fn push_allowed_origin(origins: &mut Vec<String>, value: &str) {
    let Some(origin) = parse_url_origin(value) else {
        return;
    };
    if !origins
        .iter()
        .any(|existing| existing.eq_ignore_ascii_case(&origin))
    {
        origins.push(origin);
    }
}

fn safe_return_to_url(return_to: Option<&str>, allowed_origins: &[String]) -> Option<url::Url> {
    let value = return_to?.trim();
    if value.is_empty() {
        return None;
    }
    let url = url::Url::parse(value).ok()?;
    let origin = url_origin(&url)?;
    allowed_origins
        .iter()
        .any(|allowed| allowed.eq_ignore_ascii_case(&origin))
        .then_some(url)
}

fn parse_url_origin(value: &str) -> Option<String> {
    let url = url::Url::parse(value.trim()).ok()?;
    url_origin(&url)
}

fn url_origin(url: &url::Url) -> Option<String> {
    if !matches!(url.scheme(), "http" | "https" | "tauri") {
        return None;
    }
    if !url.username().is_empty() || url.password().is_some() {
        return None;
    }
    let host = url.host()?;
    let mut origin = format!("{}://{}", url.scheme(), host);
    if let Some(port) = url.port() {
        origin.push(':');
        origin.push_str(&port.to_string());
    }
    Some(origin)
}

fn redirect_with_access_token_fragment(mut url: url::Url, access_token: &str) -> String {
    let mut pairs = url
        .fragment()
        .map(|fragment| {
            url::form_urlencoded::parse(fragment.as_bytes())
                .into_owned()
                .filter(|(key, _)| key != TOKEN_FRAGMENT_PARAM)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    pairs.push((TOKEN_FRAGMENT_PARAM.to_string(), access_token.to_string()));

    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    serializer.extend_pairs(
        pairs
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str())),
    );
    let fragment = serializer.finish();
    url.set_fragment(Some(&fragment));
    url.to_string()
}

/// GET /api/auth/metadata — 返回登录方式描述（不需要认证）
pub async fn metadata(State(state): State<Arc<AppState>>) -> Result<Json<LoginMetadata>, ApiError> {
    let provider = state
        .auth_provider
        .as_ref()
        .ok_or_else(|| ApiError::ServiceUnavailable("认证提供者未初始化".to_string()))?;

    Ok(Json(map_login_metadata(provider.login_metadata())))
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
        .map_err(|err| {
            let context = DiagnosticErrorContext::new("auth.logout", "revoke_token");
            diag_error!(
                Error,
                Subsystem::Auth,
                context = &context,
                error = &err,
                route = "/api/auth/logout",
                "注销会话失败"
            );
            ApiError::ServiceUnavailable(format!("注销会话失败: {err}"))
        })?;

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
        .map_err(|err| {
            let context = DiagnosticErrorContext::new("auth.revoke", "revoke_token");
            diag_error!(
                Error,
                Subsystem::Auth,
                context = &context,
                error = &err,
                route = "/api/auth/revoke",
                admin_user_id = %current_user.user_id,
                "撤销会话失败"
            );
            ApiError::ServiceUnavailable(format!("撤销会话失败: {err}"))
        })?;

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

fn map_login_metadata(metadata: ProviderLoginMetadata) -> LoginMetadata {
    LoginMetadata {
        provider_type: metadata.provider_type,
        display_name: metadata.display_name,
        description: metadata.description,
        fields: metadata
            .fields
            .into_iter()
            .map(|field| LoginFieldDescriptor {
                name: field.name,
                label: field.label,
                field_type: field.field_type,
                placeholder: field.placeholder,
                required: field.required,
            })
            .collect(),
        login_mode: match metadata.login_mode {
            ProviderLoginMode::Form => LoginMode::Form,
            ProviderLoginMode::Redirect => LoginMode::Redirect,
        },
        start_url: metadata.start_url,
        requires_login: metadata.requires_login,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oidc_post_login_redirect_defaults_to_frontend_dev_server() {
        assert_eq!(
            oidc_post_login_redirect_from_env(None, None),
            "http://127.0.0.1:5380/"
        );
    }

    #[test]
    fn oidc_post_login_redirect_prefers_explicit_callback_target() {
        assert_eq!(
            oidc_post_login_redirect_from_env(
                Some(" https://app.example.test/ ".to_string()),
                Some("https://secondary.example.test/".to_string()),
            ),
            "https://app.example.test/"
        );
    }

    #[test]
    fn oidc_callback_redirect_rejects_external_return_to() {
        let allowed = vec![
            "http://127.0.0.1:5380".to_string(),
            "http://tauri.localhost".to_string(),
        ];

        assert_eq!(
            oidc_callback_redirect_from_parts(
                Some("https://evil.example.test/dashboard"),
                "agd_token",
                "http://127.0.0.1:5380/",
                &allowed,
            ),
            "http://127.0.0.1:5380/"
        );
    }

    #[test]
    fn oidc_callback_redirect_allows_desktop_return_to_with_fragment_token() {
        let allowed = vec!["http://tauri.localhost".to_string()];

        assert_eq!(
            oidc_callback_redirect_from_parts(
                Some("http://tauri.localhost/dashboard/agent"),
                "agd_token",
                "http://127.0.0.1:5380/",
                &allowed,
            ),
            "http://tauri.localhost/dashboard/agent#agentdash_access_token=agd_token"
        );
    }

    #[test]
    fn oidc_callback_redirect_allows_same_origin_web_return_to() {
        let allowed = vec!["https://app.example.test".to_string()];

        assert_eq!(
            oidc_callback_redirect_from_parts(
                Some("https://app.example.test/dashboard/agent?tab=story"),
                "agd token",
                "https://app.example.test/",
                &allowed,
            ),
            "https://app.example.test/dashboard/agent?tab=story#agentdash_access_token=agd+token"
        );
    }

    #[test]
    fn oidc_callback_redirect_rejects_return_to_with_credentials() {
        let allowed = vec!["https://app.example.test".to_string()];

        assert_eq!(
            oidc_callback_redirect_from_parts(
                Some("https://user:pass@app.example.test/dashboard/agent"),
                "agd_token",
                "https://app.example.test/",
                &allowed,
            ),
            "https://app.example.test/"
        );
    }
}
