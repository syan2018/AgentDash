use std::sync::Arc;

use agentdash_domain::llm_provider::{LlmProvider, WireProtocol};
use axum::{
    Json,
    extract::{Path, State},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{app_state::AppState, auth::CurrentUser, rpc::ApiError};
use agentdash_plugin_api::AuthMode;

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
    let protocol = WireProtocol::from_str(&req.protocol).ok_or_else(|| {
        ApiError::BadRequest(format!(
            "无效的 protocol: {}（支持: anthropic, gemini, openai_compatible）",
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
        let protocol = WireProtocol::from_str(&protocol_str)
            .ok_or_else(|| ApiError::BadRequest(format!("无效的 protocol: {protocol_str}")))?;
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

    let protocol = WireProtocol::from_str(&req.protocol)
        .ok_or_else(|| ApiError::BadRequest(format!("无效的 protocol: {}", req.protocol)))?;

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

// ─── Helpers ───

fn parse_id(id: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(id).map_err(|_| ApiError::BadRequest(format!("无效的 llm_provider id: {id}")))
}

fn is_masked_placeholder(value: &str) -> bool {
    value == "****" || (value.contains("...") && value.len() <= 11)
}
