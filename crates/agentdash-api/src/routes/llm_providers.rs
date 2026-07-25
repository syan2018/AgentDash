use agentdash_diagnostics::{DiagnosticErrorContext, Subsystem, diag_error};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use agentdash_application::llm_provider::{
    CreateLlmProviderInput, UpdateLlmProviderInput, create_llm_provider, delete_llm_provider,
    get_llm_provider, list_llm_providers, reorder_llm_providers, update_llm_provider,
};
use agentdash_contracts::llm_provider::{
    CodexOAuthFlowStatusDto, CodexOAuthStatusResponse, CompleteCodexOAuthRequest,
    CreateLlmProviderRequest, DeleteLlmProviderResponse, DeleteLlmProviderUserCredentialResponse,
    EffectiveLlmModelProfileDto, EffectiveLlmProviderDto, FailCodexOAuthRequest,
    LlmCredentialModeDto, LlmProviderAdminDto, LlmProviderProtocol, PrepareCodexOAuthRequest,
    ProbeLlmProviderModelDto, ProbeLlmProviderModelsRequest, ReorderLlmProvidersRequest,
    ReorderLlmProvidersResponse, StartCodexOAuthResponse, UpdateLlmProviderRequest,
    UpsertLlmProviderUserCredentialRequest,
};
use agentdash_domain::llm_provider::{
    LlmCredentialMode, LlmCredentialSource, LlmCredentialVerificationStatus, LlmProvider,
    LlmProviderUserCredential, WireProtocol, mask_secret, resolve_effective_credential,
    resolve_global_credential,
};
use axum::{
    Json,
    extract::{Path, State},
};
use base64::Engine;
use chrono::{DateTime, Utc};
use tokio::sync::Mutex;
use uuid::Uuid;

use agentdash_llm_provider::{
    EffectiveLlmProviderProfile, ProviderUnavailableReason,
    build_effective_profile_catalog_from_db, build_effective_provider_profile,
};

use crate::{
    app_state::AppState,
    auth::CurrentUser,
    dto::CodexTokenResponse,
    oauth_flow::{self, LocalOAuthProviderConfig},
    rpc::ApiError,
};
use agentdash_integration_api::AuthMode;

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

#[derive(Debug, Clone, PartialEq, Eq)]
enum CodexOAuthCredentialTarget {
    GlobalProvider,
    UserByok { user_id: String },
}

type CodexOAuthFlowStore = Arc<Mutex<HashMap<String, CodexOAuthPreparedFlow>>>;

static CODEX_OAUTH_FLOWS: OnceLock<CodexOAuthFlowStore> = OnceLock::new();

#[derive(Debug, Clone)]
struct CodexOAuthPreparedFlow {
    provider_id: Uuid,
    user_id: String,
    target: CodexOAuthCredentialTarget,
    state: String,
    code_challenge: String,
    redirect_uri: String,
    expires_at: DateTime<Utc>,
    status: oauth_flow::OAuthFlowStatus,
    completion_claimed: bool,
}

pub fn router() -> axum::Router<std::sync::Arc<crate::app_state::AppState>> {
    axum::Router::new()
        .route(
            "/llm-providers",
            axum::routing::get(list_providers).post(create_provider),
        )
        .route(
            "/llm-providers/effective",
            axum::routing::get(list_effective_providers),
        )
        .route(
            "/llm-providers/reorder",
            axum::routing::post(reorder_providers),
        )
        .route(
            "/llm-providers/probe-models",
            axum::routing::post(probe_models),
        )
        .route(
            "/llm-providers/codex-oauth/{flow_id}",
            axum::routing::get(get_codex_oauth_status),
        )
        .route(
            "/llm-providers/codex-oauth/{flow_id}/cancel",
            axum::routing::post(cancel_codex_oauth),
        )
        .route(
            "/llm-providers/codex-oauth/{flow_id}/complete",
            axum::routing::post(complete_codex_oauth),
        )
        .route(
            "/llm-providers/codex-oauth/{flow_id}/fail",
            axum::routing::post(fail_codex_oauth),
        )
        .route(
            "/llm-providers/{id}",
            axum::routing::get(get_provider)
                .put(update_provider)
                .delete(delete_provider),
        )
        .route(
            "/llm-providers/{id}/user-credential",
            axum::routing::put(upsert_user_credential).delete(delete_user_credential),
        )
        .route(
            "/llm-providers/{id}/user-credential/verify",
            axum::routing::post(verify_user_credential),
        )
        .route(
            "/llm-providers/{id}/user-credential/codex-oauth/start",
            axum::routing::post(start_user_codex_oauth),
        )
        .route(
            "/llm-providers/{id}/user-credential/codex-oauth/desktop/prepare",
            axum::routing::post(prepare_user_codex_oauth),
        )
        .route(
            "/llm-providers/{id}/probe-models",
            axum::routing::post(probe_user_provider_models),
        )
        .route(
            "/llm-providers/{id}/codex-oauth/start",
            axum::routing::post(start_codex_oauth),
        )
        .route(
            "/llm-providers/{id}/codex-oauth/desktop/prepare",
            axum::routing::post(prepare_codex_oauth),
        )
}

// ─── Access control ───

fn require_system_access(
    current_user: &agentdash_integration_api::AuthIdentity,
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
    let providers = list_llm_providers(&state.repos).await?;
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
    let provider = create_llm_provider(
        &state.repos,
        state.secrets.llm_provider_secret.as_ref(),
        CreateLlmProviderInput {
            name: req.name,
            slug: req.slug,
            protocol: llm_provider_protocol_into_domain(req.protocol),
            credential_mode: req.credential_mode.map(llm_credential_mode_into_domain),
            global_api_key: req.global_api_key,
            base_url: req.base_url,
            wire_api: req.wire_api,
            default_model: req.default_model,
            models: req.models,
            blocked_models: req.blocked_models,
            env_api_key: req.env_api_key,
            discovery_url: req.discovery_url,
            enabled: req.enabled,
        },
    )
    .await?;

    Ok(Json(admin_provider_dto(provider, &state)?))
}

pub async fn get_provider(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<LlmProviderAdminDto>, ApiError> {
    require_system_access(&current_user)?;
    let id = parse_id(&id)?;
    let provider = get_llm_provider(&state.repos, id).await?;
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
    let provider = update_llm_provider(
        &state.repos,
        state.secrets.llm_provider_secret.as_ref(),
        id,
        UpdateLlmProviderInput {
            name: req.name,
            protocol: req.protocol.map(llm_provider_protocol_into_domain),
            credential_mode: req.credential_mode.map(llm_credential_mode_into_domain),
            global_api_key: req.global_api_key,
            base_url: req.base_url,
            wire_api: req.wire_api,
            default_model: req.default_model,
            models: req.models,
            blocked_models: req.blocked_models,
            env_api_key: req.env_api_key,
            discovery_url: req.discovery_url,
            sort_order: req.sort_order,
            enabled: req.enabled,
        },
    )
    .await?;

    Ok(Json(admin_provider_dto(provider, &state)?))
}

pub async fn delete_provider(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<DeleteLlmProviderResponse>, ApiError> {
    require_system_access(&current_user)?;
    let id = parse_id(&id)?;
    delete_llm_provider(&state.repos, id).await?;
    Ok(Json(DeleteLlmProviderResponse { deleted: true }))
}

pub async fn reorder_providers(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Json(req): Json<ReorderLlmProvidersRequest>,
) -> Result<Json<ReorderLlmProvidersResponse>, ApiError> {
    require_system_access(&current_user)?;
    let ids: Vec<Uuid> = req
        .ids
        .iter()
        .map(|s| parse_id(s))
        .collect::<Result<Vec<_>, _>>()?;
    reorder_llm_providers(&state.repos, &ids).await?;
    Ok(Json(ReorderLlmProvidersResponse { reordered: true }))
}

pub async fn list_effective_providers(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
) -> Result<Json<Vec<EffectiveLlmProviderDto>>, ApiError> {
    let catalog = build_effective_profile_catalog_from_db(
        state.repos.llm_provider_repo.as_ref(),
        Some(state.repos.llm_provider_credential_repo.as_ref()),
        state.secrets.llm_provider_secret.as_ref(),
        Some(&current_user),
    )
    .await;
    let mut response = Vec::with_capacity(catalog.providers.len());
    for profile in catalog.providers {
        response.push(effective_provider_dto(profile, &state, &current_user.user_id).await?);
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
    let provider = get_llm_provider(&state.repos, provider_id).await?;
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
        .map_err(ApiError::from)?;
    let mut credential =
        LlmProviderUserCredential::new(provider.id, current_user.user_id.clone(), encrypted);
    let (status, message) = verify_user_api_key(&provider, api_key).await;
    credential.mark_verification(status, message);
    state
        .repos
        .llm_provider_credential_repo
        .upsert_for_user_provider(&credential)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(
        effective_provider_dto_for_provider(provider, &state, &current_user).await?,
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
        .map_err(ApiError::from)?;
    Ok(Json(DeleteLlmProviderUserCredentialResponse { deleted }))
}

pub async fn verify_user_credential(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<EffectiveLlmProviderDto>, ApiError> {
    let provider_id = parse_id(&id)?;
    let provider = get_llm_provider(&state.repos, provider_id).await?;
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
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::NotFound("尚未保存个人 BYOK Key".into()))?;
    let api_key = state
        .secrets
        .llm_provider_secret
        .decrypt(&credential.api_key_ciphertext)
        .map_err(ApiError::from)?;
    let (status, message) = verify_user_api_key(&provider, &api_key).await;
    credential.mark_verification(status, message);
    state
        .repos
        .llm_provider_credential_repo
        .upsert_for_user_provider(&credential)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(
        effective_provider_dto_for_provider(provider, &state, &current_user).await?,
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
    let provider = get_llm_provider(&state.repos, provider_id).await?;
    if provider.protocol != WireProtocol::OpenaiCodex {
        return Err(ApiError::BadRequest(
            "只有 openai_codex Provider 支持 Codex 登录向导".into(),
        ));
    }

    Err(ApiError::BadRequest(
        "Codex 登录需要桌面端本机回调能力，请在 AgentDash 桌面端中发起".into(),
    ))
}

pub async fn start_user_codex_oauth(
    State(state): State<Arc<AppState>>,
    CurrentUser(_current_user): CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<StartCodexOAuthResponse>, ApiError> {
    let provider_id = parse_id(&id)?;
    let provider = get_llm_provider(&state.repos, provider_id).await?;
    if provider.protocol != WireProtocol::OpenaiCodex {
        return Err(ApiError::BadRequest(
            "只有 openai_codex Provider 支持 Codex 登录向导".into(),
        ));
    }
    ensure_provider_allows_user_credential(&provider)?;

    Err(ApiError::BadRequest(
        "Codex 登录需要桌面端本机回调能力，请在 AgentDash 桌面端中发起".into(),
    ))
}

pub async fn prepare_codex_oauth(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<PrepareCodexOAuthRequest>,
) -> Result<Json<StartCodexOAuthResponse>, ApiError> {
    require_system_access(&current_user)?;
    let provider_id = parse_id(&id)?;
    let provider = get_llm_provider(&state.repos, provider_id).await?;
    ensure_codex_provider(&provider)?;
    prepare_codex_oauth_flow(
        provider_id,
        current_user.user_id,
        CodexOAuthCredentialTarget::GlobalProvider,
        req,
    )
    .await
    .map(Json)
}

pub async fn prepare_user_codex_oauth(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<PrepareCodexOAuthRequest>,
) -> Result<Json<StartCodexOAuthResponse>, ApiError> {
    let provider_id = parse_id(&id)?;
    let provider = get_llm_provider(&state.repos, provider_id).await?;
    ensure_codex_provider(&provider)?;
    ensure_provider_allows_user_credential(&provider)?;
    let user_id = current_user.user_id;
    prepare_codex_oauth_flow(
        provider_id,
        user_id.clone(),
        CodexOAuthCredentialTarget::UserByok { user_id },
        req,
    )
    .await
    .map(Json)
}

pub async fn get_codex_oauth_status(
    State(_state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(flow_id): Path<String>,
) -> Result<Json<CodexOAuthStatusResponse>, ApiError> {
    let response = codex_flow_status_for_user(&flow_id, &current_user.user_id).await?;
    Ok(Json(response))
}

pub async fn cancel_codex_oauth(
    State(_state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(flow_id): Path<String>,
) -> Result<Json<CodexOAuthStatusResponse>, ApiError> {
    let response =
        mark_codex_flow_failed_for_user(&flow_id, &current_user.user_id, "Codex 登录已取消")
            .await?;
    Ok(Json(response))
}

pub async fn complete_codex_oauth(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(flow_id): Path<String>,
    Json(req): Json<CompleteCodexOAuthRequest>,
) -> Result<Json<CodexOAuthStatusResponse>, ApiError> {
    let prepared = claim_codex_flow_for_completion(&flow_id, &current_user.user_id, &req).await?;
    let credential =
        match exchange_codex_authorization_code(&req.code, &req.code_verifier, &req.redirect_uri)
            .await
        {
            Ok(credential) => credential,
            Err(message) => {
                return Ok(Json(
                    mark_codex_flow_failed_for_user(&flow_id, &current_user.user_id, message)
                        .await?,
                ));
            }
        };

    if let Err(message) =
        save_codex_credential(&state, prepared.provider_id, prepared.target, credential).await
    {
        return Ok(Json(
            mark_codex_flow_failed_for_user(&flow_id, &current_user.user_id, message).await?,
        ));
    }

    let response =
        mark_codex_flow_completed_for_user(&flow_id, &current_user.user_id, "Codex 登录已完成")
            .await?;
    Ok(Json(response))
}

pub async fn fail_codex_oauth(
    State(_state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(flow_id): Path<String>,
    Json(req): Json<FailCodexOAuthRequest>,
) -> Result<Json<CodexOAuthStatusResponse>, ApiError> {
    let response = mark_codex_flow_failed_for_user(
        &flow_id,
        &current_user.user_id,
        sanitize_codex_oauth_status_message(&req.message),
    )
    .await?;
    Ok(Json(response))
}

async fn prepare_codex_oauth_flow(
    provider_id: Uuid,
    user_id: String,
    target: CodexOAuthCredentialTarget,
    req: PrepareCodexOAuthRequest,
) -> Result<StartCodexOAuthResponse, ApiError> {
    validate_codex_oauth_prepare_request(&req)?;
    let flow_id = Uuid::new_v4().to_string();
    let expires_at = Utc::now() + chrono::Duration::seconds(CODEX_OAUTH_TIMEOUT_SECS as i64);
    let auth_url = oauth_flow::build_authorize_url(
        &codex_oauth_config(),
        req.state.as_str(),
        req.code_challenge.as_str(),
    )
    .map_err(ApiError::BadRequest)?;

    let flow_store = codex_oauth_flows();
    flow_store.lock().await.insert(
        flow_id.clone(),
        CodexOAuthPreparedFlow {
            provider_id,
            user_id,
            target,
            state: req.state,
            code_challenge: req.code_challenge,
            redirect_uri: req.redirect_uri,
            expires_at,
            status: oauth_flow::OAuthFlowStatus::Pending,
            completion_claimed: false,
        },
    );

    Ok(StartCodexOAuthResponse {
        flow_id,
        auth_url,
        expires_at: expires_at.to_rfc3339(),
    })
}

fn validate_codex_oauth_prepare_request(req: &PrepareCodexOAuthRequest) -> Result<(), ApiError> {
    if req.state.trim().is_empty() {
        return Err(ApiError::BadRequest("Codex OAuth state 不能为空".into()));
    }
    if req.code_challenge.trim().is_empty() {
        return Err(ApiError::BadRequest(
            "Codex OAuth code_challenge 不能为空".into(),
        ));
    }
    if req.redirect_uri != CODEX_OAUTH_REDIRECT_URI {
        return Err(ApiError::BadRequest(
            "Codex OAuth redirect_uri 不匹配".into(),
        ));
    }
    Ok(())
}

async fn codex_flow_status_for_user(
    flow_id: &str,
    user_id: &str,
) -> Result<CodexOAuthStatusResponse, ApiError> {
    let flow_store = codex_oauth_flows();
    let mut flows = flow_store.lock().await;
    let flow = flows
        .get_mut(flow_id)
        .ok_or_else(|| ApiError::NotFound(format!("OAuth 登录流程 {flow_id} 不存在")))?;
    ensure_codex_flow_user(flow, user_id)?;
    expire_codex_flow_if_needed(flow);
    Ok(codex_oauth_response(flow_id, &flow.status))
}

async fn claim_codex_flow_for_completion(
    flow_id: &str,
    user_id: &str,
    req: &CompleteCodexOAuthRequest,
) -> Result<CodexOAuthPreparedFlow, ApiError> {
    let flow_store = codex_oauth_flows();
    let mut flows = flow_store.lock().await;
    let flow = flows
        .get_mut(flow_id)
        .ok_or_else(|| ApiError::NotFound(format!("OAuth 登录流程 {flow_id} 不存在")))?;
    ensure_codex_flow_user(flow, user_id)?;
    expire_codex_flow_if_needed(flow);
    if !matches!(flow.status, oauth_flow::OAuthFlowStatus::Pending) {
        return Err(ApiError::Conflict("Codex OAuth flow 已结束".into()));
    }
    if flow.completion_claimed {
        return Err(ApiError::Conflict("Codex OAuth flow 正在完成".into()));
    }
    if req.state != flow.state {
        flow.status = oauth_flow::OAuthFlowStatus::Failed {
            message: "Codex 登录 state 校验失败".to_string(),
        };
        return Err(ApiError::BadRequest("Codex 登录 state 校验失败".into()));
    }
    if req.redirect_uri != flow.redirect_uri {
        flow.status = oauth_flow::OAuthFlowStatus::Failed {
            message: "Codex 登录 redirect_uri 校验失败".to_string(),
        };
        return Err(ApiError::BadRequest(
            "Codex 登录 redirect_uri 校验失败".into(),
        ));
    }
    if req.code.trim().is_empty() {
        flow.status = oauth_flow::OAuthFlowStatus::Failed {
            message: "Codex 登录缺少授权码".to_string(),
        };
        return Err(ApiError::BadRequest("Codex 登录缺少授权码".into()));
    }
    if oauth_flow::build_pkce_challenge(&req.code_verifier) != flow.code_challenge {
        flow.status = oauth_flow::OAuthFlowStatus::Failed {
            message: "Codex 登录 PKCE 校验失败".to_string(),
        };
        return Err(ApiError::BadRequest("Codex 登录 PKCE 校验失败".into()));
    }

    flow.completion_claimed = true;
    Ok(flow.clone())
}

async fn mark_codex_flow_completed_for_user(
    flow_id: &str,
    user_id: &str,
    message: impl Into<String>,
) -> Result<CodexOAuthStatusResponse, ApiError> {
    let flow_store = codex_oauth_flows();
    let mut flows = flow_store.lock().await;
    let flow = flows
        .get_mut(flow_id)
        .ok_or_else(|| ApiError::NotFound(format!("OAuth 登录流程 {flow_id} 不存在")))?;
    ensure_codex_flow_user(flow, user_id)?;
    if matches!(flow.status, oauth_flow::OAuthFlowStatus::Pending) || flow.completion_claimed {
        flow.status = oauth_flow::OAuthFlowStatus::Completed {
            message: message.into(),
        };
        flow.completion_claimed = false;
    }
    Ok(codex_oauth_response(flow_id, &flow.status))
}

async fn mark_codex_flow_failed_for_user(
    flow_id: &str,
    user_id: &str,
    message: impl Into<String>,
) -> Result<CodexOAuthStatusResponse, ApiError> {
    let flow_store = codex_oauth_flows();
    let mut flows = flow_store.lock().await;
    let flow = flows
        .get_mut(flow_id)
        .ok_or_else(|| ApiError::NotFound(format!("OAuth 登录流程 {flow_id} 不存在")))?;
    ensure_codex_flow_user(flow, user_id)?;
    if matches!(flow.status, oauth_flow::OAuthFlowStatus::Pending) || flow.completion_claimed {
        flow.status = oauth_flow::OAuthFlowStatus::Failed {
            message: sanitize_codex_oauth_status_message(&message.into()),
        };
        flow.completion_claimed = false;
    }
    Ok(codex_oauth_response(flow_id, &flow.status))
}

fn ensure_codex_flow_user(flow: &CodexOAuthPreparedFlow, user_id: &str) -> Result<(), ApiError> {
    if flow.user_id == user_id {
        return Ok(());
    }
    Err(ApiError::Forbidden("无权访问该 Codex OAuth flow".into()))
}

fn expire_codex_flow_if_needed(flow: &mut CodexOAuthPreparedFlow) {
    if matches!(flow.status, oauth_flow::OAuthFlowStatus::Pending) && Utc::now() > flow.expires_at {
        flow.status = oauth_flow::OAuthFlowStatus::Failed {
            message: "Codex 登录已超时".to_string(),
        };
    }
}

fn codex_oauth_response(
    flow_id: &str,
    status: &oauth_flow::OAuthFlowStatus,
) -> CodexOAuthStatusResponse {
    CodexOAuthStatusResponse {
        flow_id: flow_id.to_string(),
        status: codex_oauth_status_dto(status),
        message: status.message(),
    }
}

fn codex_oauth_flows() -> CodexOAuthFlowStore {
    CODEX_OAUTH_FLOWS
        .get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
        .clone()
}

fn sanitize_codex_oauth_status_message(message: &str) -> String {
    let trimmed = message.trim();
    if trimmed.is_empty() {
        return "Codex 登录失败".to_string();
    }
    trimmed.chars().take(160).collect()
}

fn ensure_codex_provider(provider: &LlmProvider) -> Result<(), ApiError> {
    if provider.protocol == WireProtocol::OpenaiCodex {
        return Ok(());
    }
    Err(ApiError::BadRequest(
        "只有 openai_codex Provider 支持 Codex 登录向导".into(),
    ))
}

// ─── Probe models ───

/// 用给定的 credentials 实时探测远端可用模型列表，不写入 DB。
pub async fn probe_models(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Json(req): Json<ProbeLlmProviderModelsRequest>,
) -> Result<Json<Vec<ProbeLlmProviderModelDto>>, ApiError> {
    require_system_access(&current_user)?;

    let protocol = llm_provider_protocol_into_domain(req.protocol);

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

    let models = agentdash_llm_provider::probe_models_for_protocol(
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
    let provider = get_llm_provider(&state.repos, provider_id).await?;
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

    let models = agentdash_llm_provider::probe_models_for_protocol(
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
    if let Some(key) = &req.api_key
        && !key.is_empty()
        && !is_masked_placeholder(key)
    {
        return Ok(key.clone());
    }
    if let Some(env_key) = &req.env_api_key
        && let Ok(val) = std::env::var(env_key.trim())
        && !val.is_empty()
    {
        return Ok(val);
    }
    if let Some(pid) = &req.provider_id
        && let Ok(id) = Uuid::parse_str(pid)
        && let Ok(provider) = get_llm_provider(&state.repos, id).await
        && let Some(resolved) =
            resolve_global_credential(&provider, state.secrets.llm_provider_secret.as_ref())
                .map_err(ApiError::from)?
    {
        return Ok(resolved.api_key);
    }
    Ok(String::new())
}

async fn resolve_user_probe_api_key(
    req: &ProbeLlmProviderModelsRequest,
    state: &AppState,
    provider: &LlmProvider,
    user_id: &str,
) -> Result<String, ApiError> {
    if let Some(key) = &req.api_key
        && !key.is_empty()
        && !is_masked_placeholder(key)
    {
        return Ok(key.clone());
    }
    let Some(resolved) = resolve_effective_credential(
        provider,
        Some(state.repos.llm_provider_credential_repo.as_ref()),
        state.secrets.llm_provider_secret.as_ref(),
        Some(user_id),
    )
    .await
    .map_err(ApiError::from)?
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

    match agentdash_llm_provider::probe_models_for_protocol(
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

async fn exchange_codex_authorization_code(
    code: &str,
    verifier: &str,
    redirect_uri: &str,
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
            ("redirect_uri", redirect_uri),
        ])
        .send()
        .await
        .map_err(|e| format!("Codex token exchange 请求失败: {e}"))?;

    if !response.status().is_success() {
        let status = response.status();
        return Err(format!("Codex token exchange 返回 {status}"));
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

fn llm_provider_protocol_into_domain(protocol: LlmProviderProtocol) -> WireProtocol {
    match protocol {
        LlmProviderProtocol::Anthropic => WireProtocol::Anthropic,
        LlmProviderProtocol::Gemini => WireProtocol::Gemini,
        LlmProviderProtocol::OpenaiCompatible => WireProtocol::OpenaiCompatible,
        LlmProviderProtocol::OpenaiCodex => WireProtocol::OpenaiCodex,
    }
}

fn llm_credential_mode_into_domain(mode: LlmCredentialModeDto) -> LlmCredentialMode {
    match mode {
        LlmCredentialModeDto::GlobalOnly => LlmCredentialMode::GlobalOnly,
        LlmCredentialModeDto::GlobalOrUser => LlmCredentialMode::GlobalOrUser,
        LlmCredentialModeDto::UserRequired => LlmCredentialMode::UserRequired,
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
                let context =
                    DiagnosticErrorContext::new("llm_provider.admin_dto", "decrypt_global_secret");
                diag_error!(
                    Warn,
                    Subsystem::Api,
                    context = &context,
                    error = &error,
                    provider = %provider.slug,
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

async fn effective_provider_dto_for_provider(
    provider: LlmProvider,
    state: &AppState,
    current_user: &agentdash_integration_api::AuthIdentity,
) -> Result<EffectiveLlmProviderDto, ApiError> {
    let profile = build_effective_provider_profile(
        provider,
        Some(state.repos.llm_provider_credential_repo.as_ref()),
        state.secrets.llm_provider_secret.as_ref(),
        Some(current_user),
    )
    .await;
    effective_provider_dto(profile, state, &current_user.user_id).await
}

async fn effective_provider_dto(
    profile: EffectiveLlmProviderProfile,
    state: &AppState,
    user_id: &str,
) -> Result<EffectiveLlmProviderDto, ApiError> {
    let provider = profile.provider.clone();
    let user_credential = state
        .repos
        .llm_provider_credential_repo
        .get_for_user_provider(user_id, provider.id)
        .await
        .map_err(ApiError::from)?;
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

    let executable = profile.executable;
    let source = profile.credential_source;
    let status = effective_provider_status(&provider, &profile, user_api_key_configured);
    let resolved_wire_api = profile
        .call_profile
        .as_ref()
        .and_then(|call_profile| call_profile.resolved_wire_api.clone());
    let effective_models = profile
        .models
        .iter()
        .map(|model| EffectiveLlmModelProfileDto {
            id: model.id.clone(),
            name: model.name.clone(),
            provider_id: provider.slug.clone(),
            reasoning: model.reasoning,
            supports_image: model.supports_image,
            context_window: model.context_window,
            blocked: model.blocked,
            discovered: model.discovered,
            source: model.source.as_str().to_string(),
        })
        .collect();
    let model_discovery_status = profile.discovery_status.kind().to_string();
    let model_discovery_message = profile.discovery_status.message().map(ToOwned::to_owned);
    let unavailable_reason = profile
        .unavailable_reason
        .as_ref()
        .map(effective_unavailable_reason_code);

    Ok(EffectiveLlmProviderDto {
        id: provider.id.to_string(),
        name: provider.name,
        slug: provider.slug,
        protocol: provider.protocol.into(),
        credential_mode: provider.credential_mode.into(),
        base_url: provider.base_url,
        wire_api: provider.wire_api,
        resolved_wire_api,
        default_model: provider.default_model,
        models: provider.models,
        effective_models,
        model_discovery_status,
        model_discovery_message,
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
        unavailable_reason,
    })
}

fn effective_unavailable_reason_code(reason: &ProviderUnavailableReason) -> String {
    match reason {
        ProviderUnavailableReason::Disabled => "disabled".to_string(),
        ProviderUnavailableReason::MissingCredential {
            credential_mode, ..
        } => match credential_mode {
            LlmCredentialMode::GlobalOnly => "missing_global_credential".to_string(),
            LlmCredentialMode::GlobalOrUser => "missing_global_or_user_credential".to_string(),
            LlmCredentialMode::UserRequired => "missing_user_byok".to_string(),
        },
        ProviderUnavailableReason::CredentialResolutionFailed(_) => {
            "credential_resolution_failed".to_string()
        }
        ProviderUnavailableReason::InvalidWireApi(_) => "invalid_wire_api".to_string(),
        ProviderUnavailableReason::InvalidModels => "invalid_models".to_string(),
        ProviderUnavailableReason::InvalidBlockedModels => "invalid_blocked_models".to_string(),
    }
}

fn effective_provider_status(
    provider: &LlmProvider,
    profile: &EffectiveLlmProviderProfile,
    user_api_key_configured: bool,
) -> String {
    if !provider.enabled {
        return "disabled".to_string();
    }
    if profile.credential_source == LlmCredentialSource::UserByok {
        return "user_byok_active".to_string();
    }
    if matches!(
        profile.credential_source,
        LlmCredentialSource::GlobalDb | LlmCredentialSource::GlobalEnv
    ) {
        return "platform_provided".to_string();
    }
    if provider.credential_mode == LlmCredentialMode::UserRequired && !user_api_key_configured {
        return "needs_user_key".to_string();
    }
    if profile.executable {
        return "no_key_endpoint".to_string();
    }
    "unavailable".to_string()
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

fn probe_model_dto(model: agentdash_llm_provider::ProbeModelResult) -> ProbeLlmProviderModelDto {
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
    use tokio::sync::MutexGuard;

    static CODEX_OAUTH_TEST_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

    fn auth_identity(
        auth_mode: AuthMode,
        is_admin: bool,
    ) -> agentdash_integration_api::AuthIdentity {
        agentdash_integration_api::AuthIdentity {
            auth_mode,
            user_id: "oauth-owner".to_string(),
            subject: "oauth-owner".to_string(),
            display_name: None,
            email: None,
            avatar_url: None,
            groups: Vec::new(),
            is_admin,
            provider: None,
            extra: serde_json::Value::Null,
        }
    }

    #[test]
    fn global_provider_management_allows_personal_identity_without_login_session() {
        assert!(require_system_access(&auth_identity(AuthMode::Personal, false)).is_ok());
    }

    #[test]
    fn global_provider_management_requires_enterprise_admin() {
        assert!(matches!(
            require_system_access(&auth_identity(AuthMode::Enterprise, false)),
            Err(ApiError::Forbidden(_))
        ));
        assert!(require_system_access(&auth_identity(AuthMode::Enterprise, true)).is_ok());
    }

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

    async fn codex_oauth_test_guard() -> MutexGuard<'static, ()> {
        CODEX_OAUTH_TEST_LOCK
            .get_or_init(|| tokio::sync::Mutex::new(()))
            .lock()
            .await
    }

    async fn clear_codex_oauth_flows_for_test() {
        codex_oauth_flows().lock().await.clear();
    }

    fn prepare_codex_oauth_request(state: &str, verifier: &str) -> PrepareCodexOAuthRequest {
        PrepareCodexOAuthRequest {
            state: state.to_string(),
            code_challenge: oauth_flow::build_pkce_challenge(verifier),
            redirect_uri: CODEX_OAUTH_REDIRECT_URI.to_string(),
        }
    }

    fn complete_codex_oauth_request(
        code: &str,
        state: &str,
        verifier: &str,
    ) -> CompleteCodexOAuthRequest {
        CompleteCodexOAuthRequest {
            code: code.to_string(),
            state: state.to_string(),
            code_verifier: verifier.to_string(),
            redirect_uri: CODEX_OAUTH_REDIRECT_URI.to_string(),
        }
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

    #[tokio::test]
    async fn desktop_codex_flow_claim_validates_pkce_and_is_single_use() {
        let _guard = codex_oauth_test_guard().await;
        clear_codex_oauth_flows_for_test().await;

        let provider_id = Uuid::new_v4();
        let user_id = "user-1".to_string();
        let verifier = "verifier-ok";
        let flow = prepare_codex_oauth_flow(
            provider_id,
            user_id.clone(),
            CodexOAuthCredentialTarget::GlobalProvider,
            prepare_codex_oauth_request("state-ok", verifier),
        )
        .await
        .unwrap();

        let status = codex_flow_status_for_user(&flow.flow_id, &user_id)
            .await
            .unwrap();
        assert_eq!(status.status, CodexOAuthFlowStatusDto::Pending);

        let complete = complete_codex_oauth_request("code-ok", "state-ok", verifier);
        let prepared = claim_codex_flow_for_completion(&flow.flow_id, &user_id, &complete)
            .await
            .unwrap();
        assert_eq!(prepared.provider_id, provider_id);
        assert!(matches!(
            prepared.target,
            CodexOAuthCredentialTarget::GlobalProvider
        ));

        let duplicate = claim_codex_flow_for_completion(&flow.flow_id, &user_id, &complete).await;
        assert!(matches!(duplicate, Err(ApiError::Conflict(_))));

        let completed =
            mark_codex_flow_completed_for_user(&flow.flow_id, &user_id, "Codex 登录已完成")
                .await
                .unwrap();
        assert_eq!(completed.status, CodexOAuthFlowStatusDto::Completed);
    }

    #[tokio::test]
    async fn desktop_codex_flow_rejects_state_mismatch_and_exposes_failed_status() {
        let _guard = codex_oauth_test_guard().await;
        clear_codex_oauth_flows_for_test().await;

        let provider_id = Uuid::new_v4();
        let user_id = "user-2".to_string();
        let verifier = "verifier-state";
        let flow = prepare_codex_oauth_flow(
            provider_id,
            user_id.clone(),
            CodexOAuthCredentialTarget::UserByok {
                user_id: user_id.clone(),
            },
            prepare_codex_oauth_request("state-expected", verifier),
        )
        .await
        .unwrap();

        let mismatch = claim_codex_flow_for_completion(
            &flow.flow_id,
            &user_id,
            &complete_codex_oauth_request("code-ok", "state-wrong", verifier),
        )
        .await;
        assert!(matches!(mismatch, Err(ApiError::BadRequest(_))));

        let status = codex_flow_status_for_user(&flow.flow_id, &user_id)
            .await
            .unwrap();
        assert_eq!(status.status, CodexOAuthFlowStatusDto::Failed);
        assert_eq!(status.message.as_deref(), Some("Codex 登录 state 校验失败"));
    }

    #[tokio::test]
    async fn desktop_codex_flow_expires_before_completion() {
        let _guard = codex_oauth_test_guard().await;
        clear_codex_oauth_flows_for_test().await;

        let provider_id = Uuid::new_v4();
        let user_id = "user-3".to_string();
        let verifier = "verifier-expired";
        let flow = prepare_codex_oauth_flow(
            provider_id,
            user_id.clone(),
            CodexOAuthCredentialTarget::GlobalProvider,
            prepare_codex_oauth_request("state-expired", verifier),
        )
        .await
        .unwrap();

        let flow_store = codex_oauth_flows();
        let mut flows = flow_store.lock().await;
        flows.get_mut(&flow.flow_id).unwrap().expires_at =
            Utc::now() - chrono::Duration::seconds(1);
        drop(flows);

        let status = codex_flow_status_for_user(&flow.flow_id, &user_id)
            .await
            .unwrap();
        assert_eq!(status.status, CodexOAuthFlowStatusDto::Failed);
        assert_eq!(status.message.as_deref(), Some("Codex 登录已超时"));

        let completion = claim_codex_flow_for_completion(
            &flow.flow_id,
            &user_id,
            &complete_codex_oauth_request("code-ok", "state-expired", verifier),
        )
        .await;
        assert!(matches!(completion, Err(ApiError::Conflict(_))));
    }

    #[tokio::test]
    async fn desktop_codex_flow_rejects_wrong_user_and_cancel_marks_failed() {
        let _guard = codex_oauth_test_guard().await;
        clear_codex_oauth_flows_for_test().await;

        let provider_id = Uuid::new_v4();
        let owner_user_id = "user-owner".to_string();
        let flow = prepare_codex_oauth_flow(
            provider_id,
            owner_user_id.clone(),
            CodexOAuthCredentialTarget::UserByok {
                user_id: owner_user_id.clone(),
            },
            prepare_codex_oauth_request("state-owner", "verifier-owner"),
        )
        .await
        .unwrap();

        let wrong_user = codex_flow_status_for_user(&flow.flow_id, "user-other").await;
        assert!(matches!(wrong_user, Err(ApiError::Forbidden(_))));

        let cancelled =
            mark_codex_flow_failed_for_user(&flow.flow_id, &owner_user_id, "Codex 登录已取消")
                .await
                .unwrap();
        assert_eq!(cancelled.status, CodexOAuthFlowStatusDto::Failed);
        assert_eq!(cancelled.message.as_deref(), Some("Codex 登录已取消"));
    }

    #[test]
    fn maps_llm_provider_protocol_request_to_domain() {
        assert_eq!(
            llm_provider_protocol_into_domain(LlmProviderProtocol::Anthropic),
            WireProtocol::Anthropic
        );
        assert_eq!(
            llm_provider_protocol_into_domain(LlmProviderProtocol::Gemini),
            WireProtocol::Gemini
        );
        assert_eq!(
            llm_provider_protocol_into_domain(LlmProviderProtocol::OpenaiCompatible),
            WireProtocol::OpenaiCompatible
        );
        assert_eq!(
            llm_provider_protocol_into_domain(LlmProviderProtocol::OpenaiCodex),
            WireProtocol::OpenaiCodex
        );
    }

    #[test]
    fn maps_llm_credential_mode_request_to_domain() {
        assert_eq!(
            llm_credential_mode_into_domain(LlmCredentialModeDto::GlobalOnly),
            LlmCredentialMode::GlobalOnly
        );
        assert_eq!(
            llm_credential_mode_into_domain(LlmCredentialModeDto::GlobalOrUser),
            LlmCredentialMode::GlobalOrUser
        );
        assert_eq!(
            llm_credential_mode_into_domain(LlmCredentialModeDto::UserRequired),
            LlmCredentialMode::UserRequired
        );
    }
}
