use std::sync::Arc;

use agentdash_agent_runtime_host::CompleteAgentHostRepository;
use agentdash_contracts::project_agent::{
    ExecutionProfileDiscoveryResponse, ExecutionProfileDto, ExecutionProfileModelDto,
    ExecutionProfileModelSelectorDto, ExecutionProfileOptionsDto, ExecutionProfileProviderDto,
};
use agentdash_llm_provider::{ProviderUnavailableReason, build_effective_profile_catalog_from_db};
use axum::{
    Json,
    body::{Body, Bytes},
    extract::{Query, State},
    response::IntoResponse,
};
use serde::Deserialize;

use crate::{app_state::AppState, auth::CurrentUser, rpc::ApiError};

pub const MANAGED_EXECUTION_PROFILE_ID: &str = "PI_AGENT";
pub const CODEX_EXECUTION_PROFILE_ID: &str = "CODEX";
const CODEX_DEFINITION_ID: &str = "builtin.codex-app-server";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExecutionProfileRegistration {
    BuiltIn,
    StaticService(&'static str),
}

#[derive(Debug, Deserialize)]
pub struct ExecutionProfileOptionsQuery {
    pub executor: String,
}

pub async fn is_known_execution_profile(
    state: &AppState,
    profile_id: &str,
) -> Result<bool, ApiError> {
    let Some(registration) = execution_profile_registration(profile_id) else {
        return Ok(false);
    };
    let ExecutionProfileRegistration::StaticService(definition_id) = registration else {
        return Ok(true);
    };
    let snapshot = state
        .services
        .complete_agent
        .host_repository
        .load()
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    Ok(snapshot
        .facts
        .service_instances
        .values()
        .any(|descriptor| descriptor.definition_id.as_str() == definition_id))
}

fn execution_profile_registration(profile_id: &str) -> Option<ExecutionProfileRegistration> {
    match profile_id {
        MANAGED_EXECUTION_PROFILE_ID => Some(ExecutionProfileRegistration::BuiltIn),
        CODEX_EXECUTION_PROFILE_ID => Some(ExecutionProfileRegistration::StaticService(
            CODEX_DEFINITION_ID,
        )),
        _ => None,
    }
}

fn managed_profile(provider_available: bool) -> ExecutionProfileDto {
    ExecutionProfileDto {
        id: MANAGED_EXECUTION_PROFILE_ID.to_string(),
        name: "Managed Agent".to_string(),
        available: provider_available,
        unavailable_reason: (!provider_available)
            .then(|| "没有可执行的 LLM Provider，请先配置并启用 Provider 凭据".to_string()),
    }
}

fn codex_profile(codex_registered: bool) -> ExecutionProfileDto {
    ExecutionProfileDto {
        id: CODEX_EXECUTION_PROFILE_ID.to_string(),
        name: "Codex App Server".to_string(),
        available: codex_registered,
        unavailable_reason: (!codex_registered)
            .then(|| "内置 Codex App Server Runtime Integration 未注册".to_string()),
    }
}

fn provider_unavailable_reason(reason: &ProviderUnavailableReason) -> String {
    match reason {
        ProviderUnavailableReason::Disabled => "Provider 已禁用".to_string(),
        ProviderUnavailableReason::MissingCredential { .. } => "Provider 凭据未配置".to_string(),
        ProviderUnavailableReason::CredentialResolutionFailed(reason) => {
            format!("Provider 凭据解析失败：{reason}")
        }
        ProviderUnavailableReason::InvalidWireApi(reason) => {
            format!("Provider wire API 配置无效：{reason}")
        }
        ProviderUnavailableReason::InvalidModels => "Provider 模型配置无效".to_string(),
        ProviderUnavailableReason::InvalidBlockedModels => "Provider 屏蔽模型配置无效".to_string(),
    }
}

pub async fn discover_execution_profiles(
    State(state): State<Arc<AppState>>,
    CurrentUser(identity): CurrentUser,
) -> Result<Json<ExecutionProfileDiscoveryResponse>, ApiError> {
    let catalog = build_effective_profile_catalog_from_db(
        state.repos.llm_provider_repo.as_ref(),
        Some(state.repos.llm_provider_credential_repo.as_ref()),
        state.secrets.llm_provider_secret.as_ref(),
        Some(&identity),
    )
    .await;
    let provider_available = catalog.providers.iter().any(|provider| provider.executable);
    Ok(Json(ExecutionProfileDiscoveryResponse {
        executors: vec![
            managed_profile(provider_available),
            codex_profile(is_known_execution_profile(&state, CODEX_EXECUTION_PROFILE_ID).await?),
        ],
    }))
}

pub async fn stream_execution_profile_options(
    State(state): State<Arc<AppState>>,
    CurrentUser(identity): CurrentUser,
    Query(query): Query<ExecutionProfileOptionsQuery>,
) -> Result<impl IntoResponse, ApiError> {
    if !is_known_execution_profile(&state, query.executor.trim()).await? {
        return Err(ApiError::BadRequest(format!(
            "未知 execution profile: {}",
            query.executor.trim()
        )));
    }
    let catalog = build_effective_profile_catalog_from_db(
        state.repos.llm_provider_repo.as_ref(),
        Some(state.repos.llm_provider_credential_repo.as_ref()),
        state.secrets.llm_provider_secret.as_ref(),
        Some(&identity),
    )
    .await;
    let uses_provider_catalog = query.executor.trim() == MANAGED_EXECUTION_PROFILE_ID;
    let default_model = uses_provider_catalog
        .then(|| {
            catalog
                .providers
                .iter()
                .find(|provider| provider.executable)
                .and_then(|provider| provider.default_model.clone())
        })
        .flatten();
    let providers = catalog
        .providers
        .iter()
        .filter(|_| uses_provider_catalog)
        .map(|profile| ExecutionProfileProviderDto {
            id: profile.provider.slug.clone(),
            name: profile.provider.name.clone(),
            executable: profile.executable,
            unavailable_reason: profile
                .unavailable_reason
                .as_ref()
                .map(provider_unavailable_reason),
        })
        .collect();
    let models = catalog
        .providers
        .iter()
        .filter(|_| uses_provider_catalog)
        .filter(|profile| profile.executable)
        .flat_map(|profile| {
            let provider_id = profile.provider.slug.clone();
            profile
                .models
                .iter()
                .map(move |model| ExecutionProfileModelDto {
                    id: model.id.clone(),
                    name: model.name.clone(),
                    provider_id: provider_id.clone(),
                    reasoning: model.reasoning,
                    supports_image: model.supports_image,
                    context_window: u32::try_from(model.context_window).unwrap_or(u32::MAX),
                    blocked: model.blocked,
                    discovered: model.discovered,
                    source: model.source.as_str().to_string(),
                })
        })
        .collect();
    let options = ExecutionProfileOptionsDto {
        model_selector: ExecutionProfileModelSelectorDto {
            providers,
            models,
            default_model,
            agents: Vec::new(),
        },
        slash_commands: Vec::new(),
        loading_models: false,
        loading_agents: false,
        loading_slash_commands: false,
        error: None,
    };
    let messages = [
        serde_json::json!({ "Ready": true }),
        serde_json::json!({ "JsonPatch": [{ "op": "replace", "path": "/options", "value": options }] }),
        serde_json::json!({ "finished": true }),
    ];
    let mut body = Vec::new();
    for message in messages {
        serde_json::to_writer(&mut body, &message)
            .map_err(|error| ApiError::Internal(error.to_string()))?;
        body.push(b'\n');
    }
    Ok((
        [
            (
                axum::http::header::CONTENT_TYPE,
                "application/x-ndjson; charset=utf-8",
            ),
            (axum::http::header::CACHE_CONTROL, "no-cache, no-transform"),
        ],
        Body::from(Bytes::from(body)),
    ))
}

pub fn router() -> axum::Router<Arc<AppState>> {
    axum::Router::new()
        .route(
            "/agents/discovery",
            axum::routing::get(discover_execution_profiles),
        )
        .route(
            "/agents/discovered-options/stream",
            axum::routing::get(stream_execution_profile_options),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn managed_profile_is_visible_with_provider_diagnostic() {
        let profile = managed_profile(false);
        assert_eq!(profile.id, MANAGED_EXECUTION_PROFILE_ID);
        assert!(!profile.available);
        assert!(
            profile
                .unavailable_reason
                .as_deref()
                .unwrap()
                .contains("LLM Provider")
        );
    }

    #[test]
    fn managed_profile_is_available_when_provider_exists() {
        let profile = managed_profile(true);
        assert!(profile.available);
        assert!(profile.unavailable_reason.is_none());
    }

    #[test]
    fn codex_profile_is_projected_independently_from_native_provider_availability() {
        assert!(codex_profile(true).available);
        let unavailable = codex_profile(false);
        assert!(!unavailable.available);
        assert!(unavailable.unavailable_reason.is_some());
    }

    #[test]
    fn execution_profiles_declare_their_registration_lifecycle() {
        assert_eq!(
            execution_profile_registration(MANAGED_EXECUTION_PROFILE_ID),
            Some(ExecutionProfileRegistration::BuiltIn)
        );
        assert_eq!(
            execution_profile_registration(CODEX_EXECUTION_PROFILE_ID),
            Some(ExecutionProfileRegistration::StaticService(
                CODEX_DEFINITION_ID
            ))
        );
        assert_eq!(execution_profile_registration("unknown"), None);
    }
}
