use std::sync::Arc;
use std::time::Duration;

use agentdash_contracts::llm_provider::{
    CodexOAuthFlowStatusDto, CodexOAuthStatusResponse, CreateLlmProviderRequest,
    DeleteLlmProviderUserCredentialResponse, EffectiveLlmProviderDto, LlmProviderAdminDto,
    ProbeLlmProviderModelDto, ProbeLlmProviderModelsRequest, ReorderLlmProvidersRequest,
    StartCodexOAuthResponse, UpdateLlmProviderRequest, UpsertLlmProviderUserCredentialRequest,
};
use agentdash_domain::llm_provider::{
    LlmCredentialMode, LlmCredentialSource, LlmCredentialVerificationStatus, LlmProvider,
    LlmProviderUserCredential, WireProtocol, mask_secret, provider_allows_empty_api_key,
    resolve_effective_credential, resolve_global_credential,
};
use axum::{
    Json,
    extract::{Path, State},
};
use base64::Engine;
use serde::Deserialize;
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

enum CodexOAuthCredentialTarget {
    GlobalProvider,
    UserByok { user_id: String },
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
) -> Result<Json<Vec<LlmProviderAdminDto>>, ApiError> {
    require_system_access(&current_user)?;
    let providers = state
        .repos
        .llm_provider_repo
        .list_all()
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let response = providers
        .into_iter()
        .map(|provider| admin_provider_dto(provider, &state))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Json(response))
}

pub async fn create_provider(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Json(req): Json<CreateLlmProviderRequest>,
) -> Result<Json<LlmProviderAdminDto>, ApiError> {
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
    let protocol: WireProtocol = req.protocol.into();

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
    if let Some(mode) = req.credential_mode {
        provider.credential_mode = mode.into();
    }
    if let Some(v) = req.global_api_key {
        provider.global_api_key_ciphertext = encrypt_optional_secret(&state, &v)?;
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

    Ok(Json(admin_provider_dto(provider, &state)?))
}

pub async fn get_provider(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<LlmProviderAdminDto>, ApiError> {
    require_system_access(&current_user)?;
    let id = parse_id(&id)?;
    let provider = state
        .repos
        .llm_provider_repo
        .get_by_id(id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("LLM Provider {id} 不存在")))?;
    Ok(Json(admin_provider_dto(provider, &state)?))
}

pub async fn update_provider(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<UpdateLlmProviderRequest>,
) -> Result<Json<LlmProviderAdminDto>, ApiError> {
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
    if let Some(protocol) = req.protocol {
        provider.protocol = protocol.into();
    }
    if let Some(mode) = req.credential_mode {
        provider.credential_mode = mode.into();
    }
    if let Some(api_key) = req.global_api_key {
        if !is_masked_placeholder(&api_key) {
            provider.global_api_key_ciphertext = encrypt_optional_secret(&state, &api_key)?;
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

    Ok(Json(admin_provider_dto(provider, &state)?))
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
    Json(req): Json<ReorderLlmProvidersRequest>,
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

pub async fn list_effective_providers(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
) -> Result<Json<Vec<EffectiveLlmProviderDto>>, ApiError> {
    let providers = state
        .repos
        .llm_provider_repo
        .list_all()
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let mut response = Vec::with_capacity(providers.len());
    for provider in providers {
        response.push(effective_provider_dto(provider, &state, &current_user.user_id).await?);
    }
    Ok(Json(response))
}

pub async fn upsert_user_credential(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<UpsertLlmProviderUserCredentialRequest>,
) -> Result<Json<EffectiveLlmProviderDto>, ApiError> {
    let provider_id = parse_id(&id)?;
    let provider = state
        .repos
        .llm_provider_repo
        .get_by_id(provider_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("LLM Provider {provider_id} 不存在")))?;
    ensure_provider_allows_user_credential(&provider)?;
    if provider.protocol == WireProtocol::OpenaiCodex {
        return Err(ApiError::BadRequest(
            "ChatGPT Codex 需要通过 OAuth 登录保存个人凭据".into(),
        ));
    }
    let api_key = req.api_key.trim();
    if api_key.is_empty() {
        return Err(ApiError::BadRequest("api_key 不能为空".into()));
    }
    let encrypted = state
        .secrets
        .llm_provider_secret
        .encrypt(api_key)
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let mut credential =
        LlmProviderUserCredential::new(provider.id, current_user.user_id.clone(), encrypted);
    let (status, message) = verify_user_api_key(&provider, api_key).await;
    credential.mark_verification(status, message);
    state
        .repos
        .llm_provider_credential_repo
        .upsert_for_user_provider(&credential)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(
        effective_provider_dto(provider, &state, &current_user.user_id).await?,
    ))
}

pub async fn delete_user_credential(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<DeleteLlmProviderUserCredentialResponse>, ApiError> {
    let provider_id = parse_id(&id)?;
    let deleted = state
        .repos
        .llm_provider_credential_repo
        .delete_for_user_provider(&current_user.user_id, provider_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(DeleteLlmProviderUserCredentialResponse { deleted }))
}

pub async fn verify_user_credential(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<EffectiveLlmProviderDto>, ApiError> {
    let provider_id = parse_id(&id)?;
    let provider = state
        .repos
        .llm_provider_repo
        .get_by_id(provider_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("LLM Provider {provider_id} 不存在")))?;
    ensure_provider_allows_user_credential(&provider)?;
    if provider.protocol == WireProtocol::OpenaiCodex {
        return Err(ApiError::BadRequest(
            "ChatGPT Codex 凭据通过 OAuth 登录验证".into(),
        ));
    }
    let mut credential = state
        .repos
        .llm_provider_credential_repo
        .get_for_user_provider(&current_user.user_id, provider_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound("尚未保存个人 BYOK Key".into()))?;
    let api_key = state
        .secrets
        .llm_provider_secret
        .decrypt(&credential.api_key_ciphertext)
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let (status, message) = verify_user_api_key(&provider, &api_key).await;
    credential.mark_verification(status, message);
    state
        .repos
        .llm_provider_credential_repo
        .upsert_for_user_provider(&credential)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(
        effective_provider_dto(provider, &state, &current_user.user_id).await?,
    ))
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
        CodexOAuthCredentialTarget::GlobalProvider,
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

pub async fn start_user_codex_oauth(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<StartCodexOAuthResponse>, ApiError> {
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
    ensure_provider_allows_user_credential(&provider)?;

    let started = oauth_flow::start_local_pkce_oauth_flow(codex_oauth_config())
        .await
        .map_err(ApiError::BadRequest)?;
    let flow_id = started.flow_id.clone();
    let auth_url = started.auth_url.clone();
    let expires_at = started.expires_at;

    tokio::spawn(run_codex_oauth_token_exchange(
        state,
        provider_id,
        CodexOAuthCredentialTarget::UserByok {
            user_id: current_user.user_id,
        },
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
    CurrentUser(_current_user): CurrentUser,
    Path(flow_id): Path<String>,
) -> Result<Json<CodexOAuthStatusResponse>, ApiError> {
    let flow = oauth_flow::get_flow_status(&flow_id)
        .await
        .map_err(ApiError::NotFound)?;
    Ok(Json(CodexOAuthStatusResponse {
        flow_id: flow.flow_id,
        status: codex_oauth_status_dto(&flow.status),
        message: flow.status.message(),
    }))
}

pub async fn cancel_codex_oauth(
    State(_state): State<Arc<AppState>>,
    CurrentUser(_current_user): CurrentUser,
    Path(flow_id): Path<String>,
) -> Result<Json<CodexOAuthStatusResponse>, ApiError> {
    let flow = oauth_flow::cancel_flow(&flow_id, "Codex 登录已取消")
        .await
        .map_err(ApiError::NotFound)?;
    Ok(Json(CodexOAuthStatusResponse {
        flow_id: flow.flow_id,
        status: codex_oauth_status_dto(&flow.status),
        message: flow.status.message(),
    }))
}

// ─── Probe models ───

/// 用给定的 credentials 实时探测远端可用模型列表，不写入 DB。
pub async fn probe_models(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Json(req): Json<ProbeLlmProviderModelsRequest>,
) -> Result<Json<Vec<ProbeLlmProviderModelDto>>, ApiError> {
    require_system_access(&current_user)?;

    let protocol: WireProtocol = req.protocol.into();

    let api_key = resolve_admin_probe_api_key(&req, &state).await?;

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

    Ok(Json(models.into_iter().map(probe_model_dto).collect()))
}

pub async fn probe_user_provider_models(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<ProbeLlmProviderModelsRequest>,
) -> Result<Json<Vec<ProbeLlmProviderModelDto>>, ApiError> {
    let provider_id = parse_id(&id)?;
    let provider = state
        .repos
        .llm_provider_repo
        .get_by_id(provider_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("LLM Provider {provider_id} 不存在")))?;
    ensure_provider_allows_user_credential(&provider)?;
    let protocol = provider.protocol;
    let api_key =
        resolve_user_probe_api_key(&req, &state, &provider, &current_user.user_id).await?;
    let base_url = req
        .base_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or_else(|| (!provider.base_url.trim().is_empty()).then_some(provider.base_url.as_str()));
    let discovery_url = req
        .discovery_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or_else(|| {
            (!provider.discovery_url.trim().is_empty()).then_some(provider.discovery_url.as_str())
        });

    let models = agentdash_executor::connectors::pi_agent::pi_agent_provider_registry::probe_models_for_protocol(
        protocol,
        &api_key,
        base_url,
        discovery_url,
    )
    .await
    .map_err(|e| ApiError::BadRequest(format!("探测失败: {e}")))?;

    Ok(Json(models.into_iter().map(probe_model_dto).collect()))
}

async fn resolve_admin_probe_api_key(
    req: &ProbeLlmProviderModelsRequest,
    state: &AppState,
) -> Result<String, ApiError> {
    if let Some(key) = &req.api_key {
        if !key.is_empty() && !is_masked_placeholder(key) {
            return Ok(key.clone());
        }
    }
    if let Some(env_key) = &req.env_api_key {
        if let Ok(val) = std::env::var(env_key.trim()) {
            if !val.is_empty() {
                return Ok(val);
            }
        }
    }
    if let Some(pid) = &req.provider_id {
        if let Ok(id) = Uuid::parse_str(pid) {
            if let Ok(Some(provider)) = state.repos.llm_provider_repo.get_by_id(id).await {
                if let Some(resolved) =
                    resolve_global_credential(&provider, state.secrets.llm_provider_secret.as_ref())
                        .map_err(|e| ApiError::Internal(e.to_string()))?
                {
                    return Ok(resolved.api_key);
                }
            }
        }
    }
    Ok(String::new())
}

async fn resolve_user_probe_api_key(
    req: &ProbeLlmProviderModelsRequest,
    state: &AppState,
    provider: &LlmProvider,
    user_id: &str,
) -> Result<String, ApiError> {
    if let Some(key) = &req.api_key {
        if !key.is_empty() && !is_masked_placeholder(key) {
            return Ok(key.clone());
        }
    }
    let Some(resolved) = resolve_effective_credential(
        provider,
        Some(state.repos.llm_provider_credential_repo.as_ref()),
        state.secrets.llm_provider_secret.as_ref(),
        Some(user_id),
    )
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?
    else {
        return Ok(String::new());
    };
    if resolved.source != LlmCredentialSource::UserByok {
        return Err(ApiError::Forbidden(
            "普通用户探测模型需要提交临时 Key 或先保存个人 BYOK Key".into(),
        ));
    }
    Ok(resolved.api_key)
}

async fn verify_user_api_key(
    provider: &LlmProvider,
    api_key: &str,
) -> (LlmCredentialVerificationStatus, String) {
    let base_url = (!provider.base_url.trim().is_empty()).then_some(provider.base_url.as_str());
    let discovery_url =
        (!provider.discovery_url.trim().is_empty()).then_some(provider.discovery_url.as_str());

    match agentdash_executor::connectors::pi_agent::pi_agent_provider_registry::probe_models_for_protocol(
        provider.protocol,
        api_key,
        base_url,
        discovery_url,
    )
    .await
    {
        Ok(models) => (
            LlmCredentialVerificationStatus::Verified,
            format!("验证通过，发现 {} 个模型", models.len()),
        ),
        Err(error) => (
            LlmCredentialVerificationStatus::Failed,
            format!("验证失败: {error}"),
        ),
    }
}

async fn run_codex_oauth_token_exchange(
    state: Arc<AppState>,
    provider_id: Uuid,
    target: CodexOAuthCredentialTarget,
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
        Ok(credential) => {
            match save_codex_credential(&state, provider_id, target, credential).await {
                Ok(()) => oauth_flow::complete_flow(&flow_id, "Codex 登录已完成").await,
                Err(e) => oauth_flow::fail_flow(&flow_id, e).await,
            }
        }
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
    target: CodexOAuthCredentialTarget,
    credential: serde_json::Value,
) -> Result<(), String> {
    let encrypted = state
        .secrets
        .llm_provider_secret
        .encrypt(&credential.to_string())
        .map_err(|e| e.to_string())?;
    match target {
        CodexOAuthCredentialTarget::GlobalProvider => {
            let mut provider = state
                .repos
                .llm_provider_repo
                .get_by_id(provider_id)
                .await
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("LLM Provider {provider_id} 不存在"))?;
            provider.global_api_key_ciphertext = encrypted;
            provider.updated_at = chrono::Utc::now();
            state
                .repos
                .llm_provider_repo
                .update(&provider)
                .await
                .map_err(|e| e.to_string())?;
        }
        CodexOAuthCredentialTarget::UserByok { user_id } => {
            let mut credential = LlmProviderUserCredential::new(provider_id, user_id, encrypted);
            credential.mark_verification(
                LlmCredentialVerificationStatus::Verified,
                "ChatGPT OAuth 已验证",
            );
            state
                .repos
                .llm_provider_credential_repo
                .upsert_for_user_provider(&credential)
                .await
                .map_err(|e| e.to_string())?;
        }
    }
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

fn codex_oauth_status_dto(status: &oauth_flow::OAuthFlowStatus) -> CodexOAuthFlowStatusDto {
    match status {
        oauth_flow::OAuthFlowStatus::Pending => CodexOAuthFlowStatusDto::Pending,
        oauth_flow::OAuthFlowStatus::Completed { .. } => CodexOAuthFlowStatusDto::Completed,
        oauth_flow::OAuthFlowStatus::Failed { .. } => CodexOAuthFlowStatusDto::Failed,
    }
}

fn credential_preview(protocol: WireProtocol, secret: &str) -> String {
    if protocol == WireProtocol::OpenaiCodex {
        return "ChatGPT OAuth".to_string();
    }
    mask_secret(secret)
}

fn admin_provider_dto(
    provider: LlmProvider,
    state: &AppState,
) -> Result<LlmProviderAdminDto, ApiError> {
    let global =
        match resolve_global_credential(&provider, state.secrets.llm_provider_secret.as_ref()) {
            Ok(global) => global,
            Err(error) => {
                tracing::warn!(
                    provider = %provider.slug,
                    error = %error,
                    "LLM Provider 全局密钥无法解密，管理员需要重新保存"
                );
                None
            }
        };
    let global_api_key_configured =
        global.is_some() || !provider.global_api_key_ciphertext.trim().is_empty();
    let global_api_key_preview = global
        .as_ref()
        .map(|credential| credential_preview(provider.protocol, &credential.api_key))
        .filter(|preview| !preview.is_empty());
    let global_api_key_source = if !provider.global_api_key_ciphertext.trim().is_empty() {
        LlmCredentialSource::GlobalDb
    } else {
        global
            .map(|credential| credential.source)
            .unwrap_or(LlmCredentialSource::None)
    };

    Ok(LlmProviderAdminDto {
        id: provider.id.to_string(),
        name: provider.name,
        slug: provider.slug,
        protocol: provider.protocol.into(),
        credential_mode: provider.credential_mode.into(),
        global_api_key_configured,
        global_api_key_preview,
        global_api_key_source: global_api_key_source.into(),
        base_url: provider.base_url,
        wire_api: provider.wire_api,
        default_model: provider.default_model,
        models: provider.models,
        blocked_models: provider.blocked_models,
        env_api_key: provider.env_api_key,
        discovery_url: provider.discovery_url,
        sort_order: provider.sort_order,
        enabled: provider.enabled,
        created_at: provider.created_at.to_rfc3339(),
        updated_at: provider.updated_at.to_rfc3339(),
    })
}

async fn effective_provider_dto(
    provider: LlmProvider,
    state: &AppState,
    user_id: &str,
) -> Result<EffectiveLlmProviderDto, ApiError> {
    let user_credential = state
        .repos
        .llm_provider_credential_repo
        .get_for_user_provider(user_id, provider.id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let user_api_key_preview = user_credential
        .as_ref()
        .and_then(|credential| {
            state
                .secrets
                .llm_provider_secret
                .decrypt(&credential.api_key_ciphertext)
                .ok()
        })
        .map(|secret| credential_preview(provider.protocol, &secret))
        .filter(|preview| !preview.is_empty());
    let user_api_key_configured = user_credential.is_some();
    let user_credential_verification_status = user_credential
        .as_ref()
        .map(|credential| credential.verification_status)
        .unwrap_or(LlmCredentialVerificationStatus::Unverified);
    let user_credential_verification_message = user_credential
        .as_ref()
        .map(|credential| credential.verification_message.trim().to_string())
        .filter(|message| !message.is_empty());
    let user_credential_verified_at = user_credential
        .as_ref()
        .and_then(|credential| credential.verified_at)
        .map(|verified_at| verified_at.to_rfc3339());

    let resolved = match resolve_effective_credential(
        &provider,
        Some(state.repos.llm_provider_credential_repo.as_ref()),
        state.secrets.llm_provider_secret.as_ref(),
        Some(user_id),
    )
    .await
    {
        Ok(resolved) => resolved,
        Err(error) => {
            tracing::warn!(
                provider = %provider.slug,
                user_id = %user_id,
                error = %error,
                "LLM Provider 用户视角密钥无法解析"
            );
            None
        }
    };
    let allows_empty = provider_allows_empty_api_key(&provider);
    let executable = provider.enabled && (resolved.is_some() || allows_empty);
    let source = resolved
        .as_ref()
        .map(|credential| credential.source)
        .unwrap_or(LlmCredentialSource::None);
    let status = effective_provider_status(&provider, executable, source, user_api_key_configured);

    Ok(EffectiveLlmProviderDto {
        id: provider.id.to_string(),
        name: provider.name,
        slug: provider.slug,
        protocol: provider.protocol.into(),
        credential_mode: provider.credential_mode.into(),
        base_url: provider.base_url,
        wire_api: provider.wire_api,
        default_model: provider.default_model,
        models: provider.models,
        blocked_models: provider.blocked_models,
        discovery_url: provider.discovery_url,
        enabled: provider.enabled,
        executable,
        effective_api_key_source: source.into(),
        user_api_key_configured,
        user_credential_verification_status: user_credential_verification_status.into(),
        user_api_key_preview,
        user_credential_verification_message,
        user_credential_verified_at,
        status,
    })
}

fn effective_provider_status(
    provider: &LlmProvider,
    executable: bool,
    source: LlmCredentialSource,
    user_api_key_configured: bool,
) -> String {
    if !provider.enabled {
        return "disabled".to_string();
    }
    if source == LlmCredentialSource::UserByok {
        return "user_byok_active".to_string();
    }
    if matches!(
        source,
        LlmCredentialSource::GlobalDb | LlmCredentialSource::GlobalEnv
    ) {
        return "platform_provided".to_string();
    }
    if provider.credential_mode == LlmCredentialMode::UserRequired && !user_api_key_configured {
        return "needs_user_key".to_string();
    }
    if executable {
        return "no_key_endpoint".to_string();
    }
    "unavailable".to_string()
}

fn encrypt_optional_secret(state: &AppState, value: &str) -> Result<String, ApiError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    state
        .secrets
        .llm_provider_secret
        .encrypt(trimmed)
        .map_err(|e| ApiError::Internal(e.to_string()))
}

fn ensure_provider_allows_user_credential(provider: &LlmProvider) -> Result<(), ApiError> {
    if matches!(
        provider.credential_mode,
        LlmCredentialMode::GlobalOrUser | LlmCredentialMode::UserRequired
    ) {
        return Ok(());
    }
    Err(ApiError::Forbidden(
        "该 Provider 由平台统一管理，不允许配置个人 BYOK Key".into(),
    ))
}

fn probe_model_dto(
    model: agentdash_executor::connectors::pi_agent::pi_agent_provider_registry::ProbeModelResult,
) -> ProbeLlmProviderModelDto {
    ProbeLlmProviderModelDto {
        id: model.id,
        name: model.name,
    }
}

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
