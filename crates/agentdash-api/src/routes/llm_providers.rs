use std::sync::Arc;
use std::time::Duration;

use agentdash_domain::llm_provider::{LlmProvider, WireProtocol};
use axum::{
    Json,
    extract::{Path, State},
};
use base64::Engine;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    app_state::AppState,
    auth::CurrentUser,
    oauth_flow::{self, LocalOAuthProviderConfig},
    rpc::ApiError,
};
use agentdash_plugin_api::AuthMode;

const CODEX_OAUTH_CALLBACK_HOST: &str = "127.0.0.1";
const CODEX_OAUTH_CALLBACK_PORT: u16 = 1455;
const CODEX_OAUTH_REDIRECT_URI: &str = "http://localhost:1455/auth/callback";
const CODEX_OAUTH_AUTHORIZE_URL: &str = "https://auth.openai.com/oauth/authorize";
const CODEX_OAUTH_TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
const CODEX_OAUTH_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const CODEX_OAUTH_SCOPE: &str = "openid profile email offline_access";
const CODEX_JWT_CLAIM_PATH: &str = "https://api.openai.com/auth";
const CODEX_OAUTH_TIMEOUT_SECS: u64 = 5 * 60;
const CODEX_OAUTH_EXTRA_AUTHORIZE_PARAMS: &[(&str, &str)] = &[
    ("id_token_add_organizations", "true"),
    ("codex_cli_simplified_flow", "true"),
    ("originator", "agentdash"),
];

// ─── Response / Request types ───

#[derive(Debug, Serialize)]
pub struct LlmProviderResponse {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub protocol: String,
    pub api_key: String,
    pub api_key_configured: bool,
    pub base_url: String,
    pub wire_api: String,
    pub default_model: String,
    pub models: serde_json::Value,
    pub blocked_models: serde_json::Value,
    pub env_api_key: String,
    pub discovery_url: String,
    pub sort_order: i32,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

fn mask_api_key(key: &str) -> String {
    if key.len() > 8 {
        format!("{}...{}", &key[..4], &key[key.len() - 4..])
    } else if key.is_empty() {
        String::new()
    } else {
        "****".to_string()
    }
}

impl From<LlmProvider> for LlmProviderResponse {
    fn from(p: LlmProvider) -> Self {
        let api_key_configured = !p.api_key.is_empty();
        Self {
            id: p.id.to_string(),
            name: p.name,
            slug: p.slug,
            protocol: p.protocol.as_str().to_string(),
            api_key: mask_api_key(&p.api_key),
            api_key_configured,
            base_url: p.base_url,
            wire_api: p.wire_api,
            default_model: p.default_model,
            models: p.models,
            blocked_models: p.blocked_models,
            env_api_key: p.env_api_key,
            discovery_url: p.discovery_url,
            sort_order: p.sort_order,
            enabled: p.enabled,
            created_at: p.created_at.to_rfc3339(),
            updated_at: p.updated_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateLlmProviderRequest {
    pub name: String,
    pub slug: String,
    pub protocol: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub wire_api: Option<String>,
    #[serde(default)]
    pub default_model: Option<String>,
    #[serde(default)]
    pub models: Option<serde_json::Value>,
    #[serde(default)]
    pub blocked_models: Option<serde_json::Value>,
    #[serde(default)]
    pub env_api_key: Option<String>,
    #[serde(default)]
    pub discovery_url: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateLlmProviderRequest {
    pub name: Option<String>,
    pub protocol: Option<String>,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub wire_api: Option<String>,
    pub default_model: Option<String>,
    pub models: Option<serde_json::Value>,
    pub blocked_models: Option<serde_json::Value>,
    pub env_api_key: Option<String>,
    pub discovery_url: Option<String>,
    pub sort_order: Option<i32>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct ReorderRequest {
    pub ids: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct StartCodexOAuthResponse {
    pub flow_id: String,
    pub auth_url: String,
    pub expires_at: String,
}

#[derive(Debug, Serialize)]
pub struct CodexOAuthStatusResponse {
    pub flow_id: String,
    pub status: String,
    pub message: Option<String>,
}

// ─── Access control ───

fn require_system_access(
    current_user: &agentdash_plugin_api::AuthIdentity,
) -> Result<(), ApiError> {
    if current_user.is_admin || current_user.auth_mode == AuthMode::Personal {
        return Ok(());
    }
    Err(ApiError::Forbidden(
        "企业模式下仅管理员可以管理 LLM Provider 配置".into(),
    ))
}

// ─── Handlers ───

pub async fn list_providers(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
) -> Result<Json<Vec<LlmProviderResponse>>, ApiError> {
    require_system_access(&current_user)?;
    let providers = state
        .repos
        .llm_provider_repo
        .list_all()
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(
        providers
            .into_iter()
            .map(LlmProviderResponse::from)
            .collect(),
    ))
}

pub async fn create_provider(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Json(req): Json<CreateLlmProviderRequest>,
) -> Result<Json<LlmProviderResponse>, ApiError> {
    require_system_access(&current_user)?;

    let name = req.name.trim().to_string();
    if name.is_empty() {
        return Err(ApiError::BadRequest("name 不能为空".into()));
    }
    let slug = req.slug.trim().to_lowercase();
    if slug.is_empty() {
        return Err(ApiError::BadRequest("slug 不能为空".into()));
    }
    if !slug
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(ApiError::BadRequest("slug 仅允许字母、数字、- 和 _".into()));
    }
    let protocol = req.protocol.parse::<WireProtocol>().map_err(|_| {
        ApiError::BadRequest(format!(
            "无效的 protocol: {}（支持: anthropic, gemini, openai_compatible, openai_codex）",
            req.protocol
        ))
    })?;

    // 获取当前最大 sort_order 作为新 provider 的默认排序
    let all = state
        .repos
        .llm_provider_repo
        .list_all()
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let max_sort = all.iter().map(|p| p.sort_order).max().unwrap_or(-1);

    let mut provider = LlmProvider::new(name, slug, protocol);
    provider.sort_order = max_sort + 1;
    if let Some(v) = req.api_key {
        provider.api_key = v;
    }
    if let Some(v) = req.base_url {
        provider.base_url = v;
    }
    if let Some(v) = req.wire_api {
        provider.wire_api = v;
    }
    if let Some(v) = req.default_model {
        provider.default_model = v;
    }
    if let Some(v) = req.models {
        provider.models = v;
    }
    if let Some(v) = req.blocked_models {
        provider.blocked_models = v;
    }
    if let Some(v) = req.env_api_key {
        provider.env_api_key = v;
    }
    if let Some(v) = req.discovery_url {
        provider.discovery_url = v;
    }
    if let Some(v) = req.enabled {
        provider.enabled = v;
    }

    state
        .repos
        .llm_provider_repo
        .create(&provider)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(LlmProviderResponse::from(provider)))
}

pub async fn get_provider(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<LlmProviderResponse>, ApiError> {
    require_system_access(&current_user)?;
    let id = parse_id(&id)?;
    let provider = state
        .repos
        .llm_provider_repo
        .get_by_id(id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("LLM Provider {id} 不存在")))?;
    Ok(Json(LlmProviderResponse::from(provider)))
}

pub async fn update_provider(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<UpdateLlmProviderRequest>,
) -> Result<Json<LlmProviderResponse>, ApiError> {
    require_system_access(&current_user)?;
    let id = parse_id(&id)?;
    let mut provider = state
        .repos
        .llm_provider_repo
        .get_by_id(id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("LLM Provider {id} 不存在")))?;

    if let Some(name) = req.name {
        let trimmed = name.trim().to_string();
        if trimmed.is_empty() {
            return Err(ApiError::BadRequest("name 不能为空".into()));
        }
        provider.name = trimmed;
    }
    if let Some(protocol_str) = req.protocol {
        let protocol = protocol_str
            .parse::<WireProtocol>()
            .map_err(|_| ApiError::BadRequest(format!("无效的 protocol: {protocol_str}")))?;
        provider.protocol = protocol;
    }
    if let Some(api_key) = req.api_key {
        // 跳过 masked 占位值 — 前端未修改密钥时会发送 masked 值
        if !is_masked_placeholder(&api_key) {
            provider.api_key = api_key;
        }
    }
    if let Some(v) = req.base_url {
        provider.base_url = v;
    }
    if let Some(v) = req.wire_api {
        provider.wire_api = v;
    }
    if let Some(v) = req.default_model {
        provider.default_model = v;
    }
    if let Some(v) = req.models {
        provider.models = v;
    }
    if let Some(v) = req.blocked_models {
        provider.blocked_models = v;
    }
    if let Some(v) = req.env_api_key {
        provider.env_api_key = v;
    }
    if let Some(v) = req.discovery_url {
        provider.discovery_url = v;
    }
    if let Some(v) = req.sort_order {
        provider.sort_order = v;
    }
    if let Some(v) = req.enabled {
        provider.enabled = v;
    }

    provider.updated_at = chrono::Utc::now();

    state
        .repos
        .llm_provider_repo
        .update(&provider)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(LlmProviderResponse::from(provider)))
}

pub async fn delete_provider(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_system_access(&current_user)?;
    let id = parse_id(&id)?;
    state
        .repos
        .llm_provider_repo
        .delete(id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(serde_json::json!({ "deleted": true })))
}

pub async fn reorder_providers(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Json(req): Json<ReorderRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_system_access(&current_user)?;
    let ids: Vec<Uuid> = req
        .ids
        .iter()
        .map(|s| parse_id(s))
        .collect::<Result<Vec<_>, _>>()?;
    state
        .repos
        .llm_provider_repo
        .reorder(&ids)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(serde_json::json!({ "reordered": true })))
}

// ─── Codex OAuth 登录向导 ───

pub async fn start_codex_oauth(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<StartCodexOAuthResponse>, ApiError> {
    require_system_access(&current_user)?;
    let provider_id = parse_id(&id)?;
    let provider = state
        .repos
        .llm_provider_repo
        .get_by_id(provider_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("LLM Provider {provider_id} 不存在")))?;
    if provider.protocol != WireProtocol::OpenaiCodex {
        return Err(ApiError::BadRequest(
            "只有 openai_codex Provider 支持 Codex 登录向导".into(),
        ));
    }

    let started = oauth_flow::start_local_pkce_oauth_flow(codex_oauth_config())
        .await
        .map_err(ApiError::BadRequest)?;
    let flow_id = started.flow_id.clone();
    let auth_url = started.auth_url.clone();
    let expires_at = started.expires_at;

    tokio::spawn(run_codex_oauth_token_exchange(
        state,
        provider_id,
        flow_id.clone(),
        started.verifier,
        started.code_rx,
    ));

    Ok(Json(StartCodexOAuthResponse {
        flow_id,
        auth_url,
        expires_at: expires_at.to_rfc3339(),
    }))
}

pub async fn get_codex_oauth_status(
    State(_state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(flow_id): Path<String>,
) -> Result<Json<CodexOAuthStatusResponse>, ApiError> {
    require_system_access(&current_user)?;
    let flow = oauth_flow::get_flow_status(&flow_id)
        .await
        .map_err(ApiError::NotFound)?;
    Ok(Json(CodexOAuthStatusResponse {
        flow_id: flow.flow_id,
        status: flow.status.as_str().to_string(),
        message: flow.status.message(),
    }))
}

pub async fn cancel_codex_oauth(
    State(_state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(flow_id): Path<String>,
) -> Result<Json<CodexOAuthStatusResponse>, ApiError> {
    require_system_access(&current_user)?;
    let flow = oauth_flow::cancel_flow(&flow_id, "Codex 登录已取消")
        .await
        .map_err(ApiError::NotFound)?;
    Ok(Json(CodexOAuthStatusResponse {
        flow_id: flow.flow_id,
        status: flow.status.as_str().to_string(),
        message: flow.status.message(),
    }))
}

// ─── Probe models ───

#[derive(Debug, Deserialize)]
pub struct ProbeModelsRequest {
    pub protocol: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub discovery_url: Option<String>,
    #[serde(default)]
    pub env_api_key: Option<String>,
    /// 若来自已有 provider，传 id 可回退到 DB 中保存的 api_key
    #[serde(default)]
    pub provider_id: Option<String>,
}

/// 用给定的 credentials 实时探测远端可用模型列表，不写入 DB。
pub async fn probe_models(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Json(req): Json<ProbeModelsRequest>,
) -> Result<
    Json<
        Vec<agentdash_executor::connectors::pi_agent::pi_agent_provider_registry::ProbeModelResult>,
    >,
    ApiError,
> {
    require_system_access(&current_user)?;

    let protocol = req
        .protocol
        .parse::<WireProtocol>()
        .map_err(|_| ApiError::BadRequest(format!("无效的 protocol: {}", req.protocol)))?;

    let api_key = resolve_probe_api_key(&req, &state).await;

    let base_url = req
        .base_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let discovery_url = req
        .discovery_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let models = agentdash_executor::connectors::pi_agent::pi_agent_provider_registry::probe_models_for_protocol(
        protocol,
        &api_key,
        base_url,
        discovery_url,
    )
    .await
    .map_err(|e| ApiError::BadRequest(format!("探测失败: {e}")))?;

    Ok(Json(models))
}

async fn resolve_probe_api_key(req: &ProbeModelsRequest, state: &AppState) -> String {
    if let Some(key) = &req.api_key {
        if !key.is_empty() && !is_masked_placeholder(key) {
            return key.clone();
        }
    }
    if let Some(env_key) = &req.env_api_key {
        if let Ok(val) = std::env::var(env_key.trim()) {
            if !val.is_empty() {
                return val;
            }
        }
    }
    if let Some(pid) = &req.provider_id {
        if let Ok(id) = Uuid::parse_str(pid) {
            if let Ok(Some(provider)) = state.repos.llm_provider_repo.get_by_id(id).await {
                if let Some(resolved) = provider.resolve_api_key() {
                    return resolved;
                }
            }
        }
    }
    String::new()
}

async fn run_codex_oauth_token_exchange(
    state: Arc<AppState>,
    provider_id: Uuid,
    flow_id: String,
    verifier: String,
    code_rx: tokio::sync::oneshot::Receiver<Result<String, String>>,
) {
    let code = match code_rx.await {
        Ok(Ok(code)) => code,
        Ok(Err(message)) => {
            oauth_flow::fail_flow(&flow_id, message).await;
            return;
        }
        Err(_) => {
            oauth_flow::fail_flow(&flow_id, "Codex 登录流程意外结束").await;
            return;
        }
    };

    match exchange_codex_authorization_code(&code, &verifier).await {
        Ok(credential) => match save_codex_credential(&state, provider_id, credential).await {
            Ok(()) => oauth_flow::complete_flow(&flow_id, "Codex 登录已完成").await,
            Err(e) => oauth_flow::fail_flow(&flow_id, e).await,
        },
        Err(e) => oauth_flow::fail_flow(&flow_id, e).await,
    }
}

fn codex_oauth_config() -> LocalOAuthProviderConfig {
    LocalOAuthProviderConfig {
        label: "Codex".to_string(),
        callback_host: CODEX_OAUTH_CALLBACK_HOST.to_string(),
        callback_port: CODEX_OAUTH_CALLBACK_PORT,
        callback_path: "/auth/callback".to_string(),
        authorize_url: CODEX_OAUTH_AUTHORIZE_URL.to_string(),
        client_id: CODEX_OAUTH_CLIENT_ID.to_string(),
        redirect_uri: CODEX_OAUTH_REDIRECT_URI.to_string(),
        scope: CODEX_OAUTH_SCOPE.to_string(),
        extra_authorize_params: CODEX_OAUTH_EXTRA_AUTHORIZE_PARAMS
            .iter()
            .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
            .collect(),
        timeout: Duration::from_secs(CODEX_OAUTH_TIMEOUT_SECS),
    }
}

#[derive(Debug, Deserialize)]
struct CodexTokenResponse {
    access_token: String,
    refresh_token: String,
    expires_in: i64,
}

async fn exchange_codex_authorization_code(
    code: &str,
    verifier: &str,
) -> Result<serde_json::Value, String> {
    let client = reqwest::Client::new();
    let response = client
        .post(CODEX_OAUTH_TOKEN_URL)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .form(&[
            ("grant_type", "authorization_code"),
            ("client_id", CODEX_OAUTH_CLIENT_ID),
            ("code", code),
            ("code_verifier", verifier),
            ("redirect_uri", CODEX_OAUTH_REDIRECT_URI),
        ])
        .send()
        .await
        .map_err(|e| format!("Codex token exchange 请求失败: {e}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Codex token exchange 返回 {status}: {body}"));
    }

    let token: CodexTokenResponse = response
        .json()
        .await
        .map_err(|e| format!("解析 Codex token 响应失败: {e}"))?;
    let account_id = extract_codex_account_id(&token.access_token)?;
    Ok(serde_json::json!({
        "access": token.access_token,
        "refresh": token.refresh_token,
        "expires": chrono::Utc::now().timestamp_millis() + token.expires_in * 1000,
        "accountId": account_id,
    }))
}

async fn save_codex_credential(
    state: &AppState,
    provider_id: Uuid,
    credential: serde_json::Value,
) -> Result<(), String> {
    let mut provider = state
        .repos
        .llm_provider_repo
        .get_by_id(provider_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("LLM Provider {provider_id} 不存在"))?;
    provider.api_key = credential.to_string();
    provider.updated_at = chrono::Utc::now();
    state
        .repos
        .llm_provider_repo
        .update(&provider)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn extract_codex_account_id(token: &str) -> Result<String, String> {
    let payload = token
        .split('.')
        .nth(1)
        .ok_or_else(|| "Codex access token 不是合法 JWT".to_string())?;
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .or_else(|_| base64::engine::general_purpose::URL_SAFE.decode(payload))
        .map_err(|e| format!("解码 Codex access token 失败: {e}"))?;
    let json: serde_json::Value = serde_json::from_slice(&decoded)
        .map_err(|e| format!("解析 Codex access token payload 失败: {e}"))?;
    json.get(CODEX_JWT_CLAIM_PATH)
        .and_then(|v| v.get("chatgpt_account_id"))
        .and_then(|v| v.as_str())
        .filter(|v| !v.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| "Codex access token 中缺少 chatgpt_account_id".to_string())
}

// ─── Helpers ───

fn parse_id(id: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(id).map_err(|_| ApiError::BadRequest(format!("无效的 llm_provider id: {id}")))
}

fn is_masked_placeholder(value: &str) -> bool {
    value == "****" || (value.contains("...") && value.len() <= 11)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jwt_with_account(account_id: &str) -> String {
        let payload = serde_json::json!({
            CODEX_JWT_CLAIM_PATH: {
                "chatgpt_account_id": account_id,
            }
        });
        let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&payload).unwrap());
        format!("header.{encoded}.signature")
    }

    #[test]
    fn codex_authorize_url_contains_native_login_params() {
        let url =
            oauth_flow::build_authorize_url(&codex_oauth_config(), "state", "challenge").unwrap();
        assert!(url.contains("client_id=app_EMoamEEZ73f0CkXaXp7hrann"));
        assert!(url.contains("redirect_uri=http%3A%2F%2Flocalhost%3A1455%2Fauth%2Fcallback"));
        assert!(url.contains("codex_cli_simplified_flow=true"));
        assert!(url.contains("originator=agentdash"));
    }

    #[test]
    fn extracts_codex_account_id_from_access_token() {
        let token = jwt_with_account("acct_test");
        assert_eq!(extract_codex_account_id(&token).unwrap(), "acct_test");
    }
}
