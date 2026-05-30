use uuid::Uuid;

use agentdash_domain::llm_provider::{
    LlmCredentialMode, LlmProvider, LlmSecretCodec, WireProtocol,
};

use crate::ApplicationError;
use crate::repository_set::RepositorySet;

#[derive(Debug, Clone)]
pub struct CreateLlmProviderInput {
    pub name: String,
    pub slug: String,
    pub protocol: WireProtocol,
    pub credential_mode: Option<LlmCredentialMode>,
    pub global_api_key: Option<String>,
    pub base_url: Option<String>,
    pub wire_api: Option<String>,
    pub default_model: Option<String>,
    pub models: Option<serde_json::Value>,
    pub blocked_models: Option<serde_json::Value>,
    pub env_api_key: Option<String>,
    pub discovery_url: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Default)]
pub struct UpdateLlmProviderInput {
    pub name: Option<String>,
    pub protocol: Option<WireProtocol>,
    pub credential_mode: Option<LlmCredentialMode>,
    pub global_api_key: Option<String>,
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

pub async fn list_llm_providers(
    repos: &RepositorySet,
) -> Result<Vec<LlmProvider>, ApplicationError> {
    repos
        .llm_provider_repo
        .list_all()
        .await
        .map_err(ApplicationError::from)
}

pub async fn get_llm_provider(
    repos: &RepositorySet,
    id: Uuid,
) -> Result<LlmProvider, ApplicationError> {
    repos
        .llm_provider_repo
        .get_by_id(id)
        .await
        .map_err(ApplicationError::from)?
        .ok_or_else(|| ApplicationError::NotFound(format!("LLM Provider {id} 不存在")))
}

pub async fn create_llm_provider(
    repos: &RepositorySet,
    secret_codec: &dyn LlmSecretCodec,
    input: CreateLlmProviderInput,
) -> Result<LlmProvider, ApplicationError> {
    let CreateLlmProviderInput {
        name,
        slug,
        protocol,
        credential_mode,
        global_api_key,
        base_url,
        wire_api,
        default_model,
        models,
        blocked_models,
        env_api_key,
        discovery_url,
        enabled,
    } = input;
    let name = normalize_required_name(name)?;
    let slug = normalize_slug(slug)?;
    let max_sort = list_llm_providers(repos)
        .await?
        .iter()
        .map(|provider| provider.sort_order)
        .max()
        .unwrap_or(-1);

    let mut provider = LlmProvider::new(name, slug, protocol);
    provider.sort_order = max_sort + 1;
    if let Some(mode) = credential_mode {
        provider.credential_mode = mode;
    }
    if let Some(value) = global_api_key {
        provider.global_api_key_ciphertext = encrypt_optional_secret(secret_codec, &value)?;
    }
    apply_optional_provider_fields(
        &mut provider,
        UpdateLlmProviderInput {
            base_url,
            wire_api,
            default_model,
            models,
            blocked_models,
            env_api_key,
            discovery_url,
            enabled,
            ..UpdateLlmProviderInput::default()
        },
    );

    repos
        .llm_provider_repo
        .create(&provider)
        .await
        .map_err(ApplicationError::from)?;
    Ok(provider)
}

pub async fn update_llm_provider(
    repos: &RepositorySet,
    secret_codec: &dyn LlmSecretCodec,
    id: Uuid,
    input: UpdateLlmProviderInput,
) -> Result<LlmProvider, ApplicationError> {
    let UpdateLlmProviderInput {
        name,
        protocol,
        credential_mode,
        global_api_key,
        base_url,
        wire_api,
        default_model,
        models,
        blocked_models,
        env_api_key,
        discovery_url,
        sort_order,
        enabled,
    } = input;
    let mut provider = get_llm_provider(repos, id).await?;

    if let Some(name) = name {
        provider.name = normalize_required_name(name)?;
    }
    if let Some(protocol) = protocol {
        provider.protocol = protocol;
    }
    if let Some(mode) = credential_mode {
        provider.credential_mode = mode;
    }
    if let Some(api_key) = global_api_key {
        if !is_masked_placeholder(&api_key) {
            provider.global_api_key_ciphertext = encrypt_optional_secret(secret_codec, &api_key)?;
        }
    }
    apply_optional_provider_fields(
        &mut provider,
        UpdateLlmProviderInput {
            base_url,
            wire_api,
            default_model,
            models,
            blocked_models,
            env_api_key,
            discovery_url,
            sort_order,
            enabled,
            ..UpdateLlmProviderInput::default()
        },
    );
    provider.updated_at = chrono::Utc::now();

    repos
        .llm_provider_repo
        .update(&provider)
        .await
        .map_err(ApplicationError::from)?;
    Ok(provider)
}

pub async fn delete_llm_provider(repos: &RepositorySet, id: Uuid) -> Result<(), ApplicationError> {
    repos
        .llm_provider_repo
        .delete(id)
        .await
        .map_err(ApplicationError::from)
}

pub async fn reorder_llm_providers(
    repos: &RepositorySet,
    ids: &[Uuid],
) -> Result<(), ApplicationError> {
    repos
        .llm_provider_repo
        .reorder(ids)
        .await
        .map_err(ApplicationError::from)
}

fn apply_optional_provider_fields(provider: &mut LlmProvider, input: UpdateLlmProviderInput) {
    if let Some(value) = input.base_url {
        provider.base_url = value;
    }
    if let Some(value) = input.wire_api {
        provider.wire_api = value;
    }
    if let Some(value) = input.default_model {
        provider.default_model = value;
    }
    if let Some(value) = input.models {
        provider.models = value;
    }
    if let Some(value) = input.blocked_models {
        provider.blocked_models = value;
    }
    if let Some(value) = input.env_api_key {
        provider.env_api_key = value;
    }
    if let Some(value) = input.discovery_url {
        provider.discovery_url = value;
    }
    if let Some(value) = input.sort_order {
        provider.sort_order = value;
    }
    if let Some(value) = input.enabled {
        provider.enabled = value;
    }
}

fn normalize_required_name(name: String) -> Result<String, ApplicationError> {
    let trimmed = name.trim().to_string();
    if trimmed.is_empty() {
        return Err(ApplicationError::BadRequest("name 不能为空".to_string()));
    }
    Ok(trimmed)
}

fn normalize_slug(slug: String) -> Result<String, ApplicationError> {
    let slug = slug.trim().to_lowercase();
    if slug.is_empty() {
        return Err(ApplicationError::BadRequest("slug 不能为空".to_string()));
    }
    if !slug
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    {
        return Err(ApplicationError::BadRequest(
            "slug 仅允许字母、数字、- 和 _".to_string(),
        ));
    }
    Ok(slug)
}

fn encrypt_optional_secret(
    secret_codec: &dyn LlmSecretCodec,
    value: &str,
) -> Result<String, ApplicationError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    secret_codec
        .encrypt(trimmed)
        .map_err(ApplicationError::from)
}

fn is_masked_placeholder(value: &str) -> bool {
    value == "****" || (value.contains("...") && value.len() <= 11)
}
