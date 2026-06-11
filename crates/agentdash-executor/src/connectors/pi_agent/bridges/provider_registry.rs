use std::collections::HashSet;
use std::sync::Arc;

use super::AnthropicBridge;
use super::OpenAiCodexResponsesBridge;
use super::OpenAiCompletionsBridge;
use super::OpenAiResponsesBridge;
use agentdash_agent::LlmBridge;
use agentdash_domain::llm_provider::{
    LlmCredentialMode, LlmCredentialSource, LlmProvider, LlmProviderCredentialRepository,
    LlmProviderRepository, LlmSecretCodec, WireProtocol, provider_allows_empty_api_key,
    resolve_effective_credential,
};
use agentdash_spi::AuthIdentity;
use futures::future::BoxFuture;
use tokio::sync::RwLock;

pub(crate) type BridgeFactory = Arc<dyn Fn(&str) -> Arc<dyn LlmBridge> + Send + Sync>;

pub(crate) const CONTEXT_WINDOW_STANDARD: u64 = 200_000;

type ModelListFuture = BoxFuture<'static, Result<Vec<ModelMeta>, String>>;

#[derive(Debug, Clone)]
pub struct ModelMeta {
    pub id: String,
    pub name: String,
    pub reasoning: bool,
    pub supports_image: bool,
    pub context_window: u64,
    pub blocked: bool,
    /// true = 来自 API 动态发现；false = 仅来自 models JSON 配置
    pub discovered: bool,
    pub source: ModelProfileSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelProfileSource {
    Discovered,
    Configured,
    Default,
}

impl ModelProfileSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Discovered => "discovered",
            Self::Configured => "configured",
            Self::Default => "default",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelDiscoveryStatus {
    NotSupported,
    Ok,
    Failed(String),
    SkippedUnavailable,
}

impl ModelDiscoveryStatus {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::NotSupported => "not_supported",
            Self::Ok => "ok",
            Self::Failed(_) => "failed",
            Self::SkippedUnavailable => "skipped_unavailable",
        }
    }

    pub fn message(&self) -> Option<&str> {
        match self {
            Self::Failed(message) => Some(message.as_str()),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ModelCatalogSnapshot {
    pub models: Vec<ModelMeta>,
    pub discovery_status: ModelDiscoveryStatus,
}

impl ModelMeta {
    pub(crate) fn from_id(id: impl Into<String>) -> Self {
        let id = id.into();
        Self {
            name: format_model_name(&id),
            reasoning: true,
            supports_image: true,
            context_window: CONTEXT_WINDOW_STANDARD,
            blocked: false,
            discovered: true,
            source: ModelProfileSource::Discovered,
            id,
        }
    }

    fn fallback(id: &str) -> Self {
        let mut model = Self::from_id(id.to_string());
        model.discovered = false;
        model.source = ModelProfileSource::Default;
        model
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
struct StoredModelMeta {
    id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    reasoning: Option<bool>,
    #[serde(default)]
    supports_image: Option<bool>,
    #[serde(default)]
    context_window: Option<u64>,
}

impl From<StoredModelMeta> for ModelMeta {
    fn from(value: StoredModelMeta) -> Self {
        Self {
            name: value
                .name
                .filter(|item| !item.trim().is_empty())
                .unwrap_or_else(|| format_model_name(&value.id)),
            reasoning: value.reasoning.unwrap_or(true),
            supports_image: value.supports_image.unwrap_or(true),
            context_window: value.context_window.unwrap_or(CONTEXT_WINDOW_STANDARD),
            blocked: false,
            discovered: false,
            source: ModelProfileSource::Configured,
            id: value.id,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OpenAiWireApi {
    Responses,
    Completions,
}

impl OpenAiWireApi {
    fn from_setting(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "responses" => Some(Self::Responses),
            "completions" => Some(Self::Completions),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Responses => "responses",
            Self::Completions => "completions",
        }
    }
}

#[derive(Clone)]
pub(crate) struct BuiltProviderEntry {
    pub entry: ProviderEntry,
    pub default_bridge: Arc<dyn LlmBridge>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderCallProfile {
    pub credential_mode: LlmCredentialMode,
    pub credential_source: LlmCredentialSource,
    pub protocol: WireProtocol,
    pub base_url: Option<String>,
    pub discovery_url: Option<String>,
    pub resolved_wire_api: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderModelResolveError {
    EmptyModelSelection,
    UnknownModel { model_id: String },
    BlockedModel { model_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderUnavailableReason {
    Disabled,
    MissingCredential {
        credential_mode: LlmCredentialMode,
        has_identity: bool,
    },
    CredentialResolutionFailed(String),
    InvalidWireApi(String),
    InvalidModels,
    InvalidBlockedModels,
}

#[derive(Clone)]
pub struct EffectiveLlmProfileCatalog {
    pub providers: Vec<EffectiveLlmProviderProfile>,
}

impl EffectiveLlmProfileCatalog {
    pub(crate) fn available_entries(&self) -> Vec<BuiltProviderEntry> {
        self.providers
            .iter()
            .filter_map(|provider| provider.built_entry.clone())
            .collect()
    }

    pub(crate) fn unavailable_entries(&self) -> Vec<UnavailableProviderEntry> {
        self.providers
            .iter()
            .filter_map(|provider| {
                provider
                    .unavailable_reason
                    .clone()
                    .map(|reason| UnavailableProviderEntry {
                        provider_id: provider.provider.slug.clone(),
                        reason,
                    })
            })
            .collect()
    }
}

#[derive(Clone)]
pub struct EffectiveLlmProviderProfile {
    pub provider: LlmProvider,
    pub executable: bool,
    pub credential_source: LlmCredentialSource,
    pub unavailable_reason: Option<ProviderUnavailableReason>,
    pub call_profile: Option<ProviderCallProfile>,
    pub default_model: Option<String>,
    pub models: Vec<ModelMeta>,
    pub discovery_status: ModelDiscoveryStatus,
    pub(crate) built_entry: Option<BuiltProviderEntry>,
}

#[derive(Debug, Clone)]
pub struct UnavailableProviderEntry {
    pub provider_id: String,
    pub reason: ProviderUnavailableReason,
}

#[derive(Clone)]
pub(crate) struct ProviderEntry {
    pub provider_id: String,
    pub provider_name: String,
    pub default_model: String,
    call_profile: ProviderCallProfile,
    bridge_factory: BridgeFactory,
    list_models: Option<Arc<dyn Fn() -> ModelListFuture + Send + Sync>>,
    configured_models: Vec<ModelMeta>,
    blocked_models: HashSet<String>,
    models_cache: Arc<RwLock<Option<ModelCatalogSnapshot>>>,
}

struct ProviderEntryDraft {
    provider_id: String,
    provider_name: String,
    default_model: String,
    call_profile: ProviderCallProfile,
    bridge_factory: BridgeFactory,
    list_models: Option<Arc<dyn Fn() -> ModelListFuture + Send + Sync>>,
    configured_models: Vec<ModelMeta>,
    blocked_models: HashSet<String>,
}

impl ProviderEntry {
    fn new(draft: ProviderEntryDraft) -> Self {
        Self {
            provider_id: draft.provider_id,
            provider_name: draft.provider_name,
            default_model: draft.default_model,
            call_profile: draft.call_profile,
            bridge_factory: draft.bridge_factory,
            list_models: draft.list_models,
            configured_models: draft.configured_models,
            blocked_models: draft.blocked_models,
            models_cache: Arc::new(RwLock::new(None)),
        }
    }

    pub(crate) fn create_bridge(&self, model_id: &str) -> Arc<dyn LlmBridge> {
        (self.bridge_factory)(model_id)
    }

    pub(crate) fn call_profile(&self) -> &ProviderCallProfile {
        &self.call_profile
    }

    #[cfg(test)]
    pub(crate) fn new_for_test(
        provider_id: impl Into<String>,
        provider_name: impl Into<String>,
        default_model: impl Into<String>,
        bridge_factory: BridgeFactory,
        configured_models: Vec<ModelMeta>,
    ) -> Self {
        Self::new(ProviderEntryDraft {
            provider_id: provider_id.into(),
            provider_name: provider_name.into(),
            default_model: default_model.into(),
            call_profile: ProviderCallProfile {
                credential_mode: LlmCredentialMode::GlobalOnly,
                credential_source: LlmCredentialSource::None,
                protocol: WireProtocol::Anthropic,
                base_url: None,
                discovery_url: None,
                resolved_wire_api: None,
            },
            bridge_factory,
            list_models: None,
            configured_models,
            blocked_models: HashSet::new(),
        })
    }

    async fn load_model_catalog(&self) -> ModelCatalogSnapshot {
        if let Some(cached) = self.models_cache.read().await.clone() {
            return cached;
        }

        let (discovered_models, discovery_status) = if let Some(list_models) = &self.list_models {
            match list_models().await {
                Ok(models) => (models, ModelDiscoveryStatus::Ok),
                Err(error) => {
                    tracing::warn!(
                        "PiAgentConnector: provider={} 动态获取模型失败: {}",
                        self.provider_id,
                        error
                    );
                    (Vec::new(), ModelDiscoveryStatus::Failed(error))
                }
            }
        } else {
            (Vec::new(), ModelDiscoveryStatus::NotSupported)
        };

        // Merge configured_models into discovered: override attributes for matching
        // IDs (keeping discovered=true), append truly custom IDs (discovered=false)
        let mut models = discovered_models;
        for custom in &self.configured_models {
            if let Some(existing) = models.iter_mut().find(|m| m.id == custom.id) {
                existing.name = custom.name.clone();
                existing.reasoning = custom.reasoning;
                existing.supports_image = custom.supports_image;
                existing.context_window = custom.context_window;
                // discovered stays true — it was found via API
            } else {
                let mut entry = custom.clone();
                entry.discovered = false;
                entry.source = ModelProfileSource::Configured;
                models.push(entry);
            }
        }

        // If still empty, synthesize a single-entry fallback from default_model
        if models.is_empty() && !self.default_model.is_empty() {
            models.push(ModelMeta::fallback(&self.default_model));
        }

        // Always ensure default_model is present in the list
        if !self.default_model.is_empty() && !models.iter().any(|m| m.id == self.default_model) {
            models.insert(0, ModelMeta::fallback(&self.default_model));
        }

        dedup_models(&mut models);
        for model in &mut models {
            model.blocked = self.blocked_models.contains(model.id.as_str());
        }

        let snapshot = ModelCatalogSnapshot {
            models,
            discovery_status,
        };
        let mut cache = self.models_cache.write().await;
        *cache = Some(snapshot.clone());
        snapshot
    }

    pub(crate) async fn load_models_with_block_state(&self) -> Vec<ModelMeta> {
        self.load_model_catalog().await.models
    }

    pub async fn load_model_profile_snapshot(&self) -> ModelCatalogSnapshot {
        self.load_model_catalog().await
    }

    pub(crate) async fn resolve_model(
        &self,
        model_id: Option<&str>,
    ) -> Result<ModelMeta, ProviderModelResolveError> {
        let requested_model = model_id
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .unwrap_or(self.default_model.as_str())
            .trim();

        if requested_model.is_empty() {
            return Err(ProviderModelResolveError::EmptyModelSelection);
        }

        let Some(model) = self
            .load_model_catalog()
            .await
            .models
            .iter()
            .find(|model| model.id == requested_model)
            .cloned()
        else {
            return Err(ProviderModelResolveError::UnknownModel {
                model_id: requested_model.to_string(),
            });
        };

        if model.blocked {
            return Err(ProviderModelResolveError::BlockedModel {
                model_id: requested_model.to_string(),
            });
        }

        Ok(model)
    }
}

pub(crate) async fn build_provider_entries_from_db(
    repo: &dyn LlmProviderRepository,
    credential_repo: Option<&dyn LlmProviderCredentialRepository>,
    secret_codec: &dyn LlmSecretCodec,
    identity: Option<&AuthIdentity>,
) -> Vec<BuiltProviderEntry> {
    let catalog =
        build_effective_profile_catalog_from_db(repo, credential_repo, secret_codec, identity)
            .await;
    catalog.available_entries()
}

pub async fn build_effective_profile_catalog_from_db(
    repo: &dyn LlmProviderRepository,
    credential_repo: Option<&dyn LlmProviderCredentialRepository>,
    secret_codec: &dyn LlmSecretCodec,
    identity: Option<&AuthIdentity>,
) -> EffectiveLlmProfileCatalog {
    let providers = match repo.list_all().await {
        Ok(list) => list,
        Err(e) => {
            tracing::error!("PiAgentConnector: 从 DB 读取 LLM providers 失败: {e}");
            return EffectiveLlmProfileCatalog {
                providers: Vec::new(),
            };
        }
    };

    let mut profiles = Vec::with_capacity(providers.len());
    for db_provider in providers {
        profiles.push(
            build_effective_provider_profile(db_provider, credential_repo, secret_codec, identity)
                .await,
        );
    }

    EffectiveLlmProfileCatalog {
        providers: profiles,
    }
}

pub async fn build_effective_provider_profile(
    db_provider: LlmProvider,
    credential_repo: Option<&dyn LlmProviderCredentialRepository>,
    secret_codec: &dyn LlmSecretCodec,
    identity: Option<&AuthIdentity>,
) -> EffectiveLlmProviderProfile {
    if !db_provider.enabled {
        return EffectiveLlmProviderProfile {
            provider: db_provider,
            executable: false,
            credential_source: LlmCredentialSource::None,
            unavailable_reason: Some(ProviderUnavailableReason::Disabled),
            call_profile: None,
            default_model: None,
            models: Vec::new(),
            discovery_status: ModelDiscoveryStatus::SkippedUnavailable,
            built_entry: None,
        };
    }

    match build_provider_entry_from_db(&db_provider, credential_repo, secret_codec, identity).await
    {
        Ok(entry) => {
            let snapshot = entry.entry.load_model_profile_snapshot().await;
            EffectiveLlmProviderProfile {
                provider: db_provider,
                executable: true,
                credential_source: entry.entry.call_profile().credential_source,
                unavailable_reason: None,
                call_profile: Some(entry.entry.call_profile().clone()),
                default_model: Some(entry.entry.default_model.clone())
                    .filter(|model| !model.trim().is_empty()),
                models: snapshot.models,
                discovery_status: snapshot.discovery_status,
                built_entry: Some(entry),
            }
        }
        Err(reason) => EffectiveLlmProviderProfile {
            provider: db_provider,
            executable: false,
            credential_source: LlmCredentialSource::None,
            unavailable_reason: Some(reason),
            call_profile: None,
            default_model: None,
            models: Vec::new(),
            discovery_status: ModelDiscoveryStatus::SkippedUnavailable,
            built_entry: None,
        },
    }
}

async fn build_provider_entry_from_db(
    db_provider: &LlmProvider,
    credential_repo: Option<&dyn LlmProviderCredentialRepository>,
    secret_codec: &dyn LlmSecretCodec,
    identity: Option<&AuthIdentity>,
) -> Result<BuiltProviderEntry, ProviderUnavailableReason> {
    let credential = match resolve_effective_credential(
        db_provider,
        credential_repo,
        secret_codec,
        identity.map(|user| user.user_id.as_str()),
    )
    .await
    {
        Ok(credential) => credential,
        Err(error) => {
            tracing::error!(
                provider = %db_provider.slug,
                error = %error,
                "PiAgentConnector: Provider 凭据解析失败"
            );
            return Err(ProviderUnavailableReason::CredentialResolutionFailed(
                error.to_string(),
            ));
        }
    };
    let credential_source = credential
        .as_ref()
        .map(|credential| credential.source)
        .unwrap_or(LlmCredentialSource::None);
    let api_key = credential
        .map(|credential| credential.api_key)
        .unwrap_or_default();
    if api_key.is_empty() && !provider_allows_empty_api_key(db_provider) {
        tracing::warn!(
            provider = %db_provider.slug,
            mode = %db_provider.credential_mode,
            "PiAgentConnector: Provider 当前身份缺少可用凭据，已从可执行列表隐藏"
        );
        return Err(ProviderUnavailableReason::MissingCredential {
            credential_mode: db_provider.credential_mode,
            has_identity: identity.is_some(),
        });
    }

    let base_url = if db_provider.base_url.is_empty() {
        None
    } else {
        Some(db_provider.base_url.clone())
    };

    let default_model = db_provider.default_model.clone();

    let openai_wire_api = if matches!(db_provider.protocol, WireProtocol::OpenaiCompatible) {
        let wire_api_setting = if db_provider.wire_api.is_empty() {
            None
        } else {
            Some(db_provider.wire_api.as_str())
        };
        match resolve_openai_wire_api(wire_api_setting, base_url.as_deref()) {
            Ok(api) => Some(api),
            Err(err) => {
                tracing::error!(
                    "PiAgentConnector: provider={} wire_api 配置错误: {err}",
                    db_provider.slug
                );
                return Err(ProviderUnavailableReason::InvalidWireApi(err));
            }
        }
    } else {
        None
    };

    let bridge_factory = build_bridge_factory_by_protocol(
        db_provider.protocol,
        api_key.clone(),
        base_url.clone(),
        openai_wire_api,
    );
    let default_bridge = bridge_factory(&default_model);

    let configured_models = match parse_model_list(&db_provider.models) {
        Some(models) => models,
        None => {
            tracing::error!(
                "PiAgentConnector: provider={} models 字段解析失败: {:?}",
                db_provider.slug,
                db_provider.models
            );
            return Err(ProviderUnavailableReason::InvalidModels);
        }
    };
    let blocked_models: HashSet<String> = match parse_string_list(&db_provider.blocked_models) {
        Some(list) => list.into_iter().collect(),
        None => {
            tracing::error!(
                "PiAgentConnector: provider={} blocked_models 字段解析失败: {:?}",
                db_provider.slug,
                db_provider.blocked_models
            );
            return Err(ProviderUnavailableReason::InvalidBlockedModels);
        }
    };

    let discovery_url = if db_provider.discovery_url.is_empty() {
        None
    } else {
        Some(db_provider.discovery_url.clone())
    };
    let call_profile = ProviderCallProfile {
        credential_mode: db_provider.credential_mode,
        credential_source,
        protocol: db_provider.protocol,
        base_url: base_url.clone(),
        discovery_url: discovery_url.clone(),
        resolved_wire_api: openai_wire_api.map(|api| api.as_str().to_string()),
    };

    let list_models =
        build_model_lister_by_protocol(db_provider.protocol, api_key, base_url, discovery_url);

    let provider_id = db_provider.slug.clone();
    tracing::info!(
        "PiAgentConnector: provider={} ({}) 已注册（protocol={}, default_model={}{}）",
        db_provider.name,
        provider_id,
        db_provider.protocol,
        default_model,
        openai_wire_api
            .map(|wa| format!(", wire_api={}", wa.as_str()))
            .unwrap_or_default()
    );

    Ok(BuiltProviderEntry {
        entry: ProviderEntry::new(ProviderEntryDraft {
            provider_id,
            provider_name: db_provider.name.clone(),
            default_model,
            call_profile,
            bridge_factory,
            list_models,
            configured_models,
            blocked_models,
        }),
        default_bridge,
    })
}

fn build_bridge_factory_by_protocol(
    protocol: WireProtocol,
    api_key: String,
    base_url: Option<String>,
    openai_wire_api: Option<OpenAiWireApi>,
) -> BridgeFactory {
    match protocol {
        WireProtocol::Anthropic => {
            let base = base_url;
            Arc::new(move |model_id: &str| {
                Arc::new(AnthropicBridge::new(&api_key, model_id, base.as_deref()))
                    as Arc<dyn LlmBridge>
            })
        }
        WireProtocol::Gemini => {
            // Gemini 走 OpenAI 兼容端点（Completions API）
            let base = base_url.unwrap_or_else(|| {
                "https://generativelanguage.googleapis.com/v1beta/openai".to_string()
            });
            Arc::new(move |model_id: &str| {
                Arc::new(OpenAiCompletionsBridge::new(
                    &api_key,
                    model_id,
                    Some(&base),
                )) as Arc<dyn LlmBridge>
            })
        }
        WireProtocol::OpenaiCompatible => {
            let wire_api = openai_wire_api.unwrap_or(OpenAiWireApi::Responses);
            let base = base_url;
            Arc::new(move |model_id: &str| match wire_api {
                OpenAiWireApi::Responses => Arc::new(OpenAiResponsesBridge::new(
                    &api_key,
                    model_id,
                    base.as_deref(),
                )) as Arc<dyn LlmBridge>,
                OpenAiWireApi::Completions => Arc::new(OpenAiCompletionsBridge::new(
                    &api_key,
                    model_id,
                    base.as_deref(),
                )) as Arc<dyn LlmBridge>,
            })
        }
        WireProtocol::OpenaiCodex => {
            let base = base_url;
            Arc::new(move |model_id: &str| {
                Arc::new(OpenAiCodexResponsesBridge::new(
                    &api_key,
                    model_id,
                    base.as_deref(),
                )) as Arc<dyn LlmBridge>
            })
        }
    }
}

fn build_model_lister_by_protocol(
    protocol: WireProtocol,
    api_key: String,
    base_url: Option<String>,
    discovery_url: Option<String>,
) -> Option<Arc<dyn Fn() -> ModelListFuture + Send + Sync>> {
    match protocol {
        WireProtocol::Anthropic => None, // Anthropic has no models list API
        WireProtocol::OpenaiCodex => None, // ChatGPT Codex 后端不提供稳定 models list
        WireProtocol::Gemini => Some(Arc::new(move || {
            let api_key = api_key.clone();
            Box::pin(async move { list_gemini_models(&api_key).await })
        })),
        WireProtocol::OpenaiCompatible => {
            // Use discovery_url if provided, else base_url, else default OpenAI
            let effective_url = discovery_url
                .or(base_url)
                .unwrap_or_else(|| "https://api.openai.com/v1".to_string());
            Some(Arc::new(move || {
                let api_key = api_key.clone();
                let url = effective_url.clone();
                Box::pin(async move { list_openai_compatible_models(&url, &api_key).await })
            }))
        }
    }
}

fn resolve_openai_wire_api(
    configured_value: Option<&str>,
    base_url: Option<&str>,
) -> Result<OpenAiWireApi, String> {
    if let Some(value) = configured_value {
        return OpenAiWireApi::from_setting(value).ok_or_else(|| {
            format!("无法识别 wire_api 设置 '{value}'，合法值: responses | completions")
        });
    }

    Ok(if is_official_openai_base_url(base_url) {
        OpenAiWireApi::Responses
    } else {
        OpenAiWireApi::Completions
    })
}

fn is_official_openai_base_url(base_url: Option<&str>) -> bool {
    let Some(base_url) = base_url.map(str::trim).filter(|value| !value.is_empty()) else {
        return true;
    };
    let normalized = base_url.trim_end_matches('/').to_ascii_lowercase();
    normalized == "https://api.openai.com/v1" || normalized == "https://api.openai.com"
}

fn parse_model_list(value: &serde_json::Value) -> Option<Vec<ModelMeta>> {
    match value {
        serde_json::Value::Array(items) => {
            let mut models = Vec::new();
            for item in items {
                match item {
                    serde_json::Value::String(id) => models.push(ModelMeta::from_id(id.clone())),
                    serde_json::Value::Object(_) => {
                        let parsed =
                            serde_json::from_value::<StoredModelMeta>(item.clone()).ok()?;
                        models.push(parsed.into());
                    }
                    _ => return None,
                }
            }
            Some(models)
        }
        serde_json::Value::String(text) => {
            if text.trim().is_empty() {
                Some(Vec::new())
            } else {
                serde_json::from_str::<serde_json::Value>(text)
                    .ok()
                    .and_then(|parsed| parse_model_list(&parsed))
            }
        }
        _ => None,
    }
}

fn parse_string_list(value: &serde_json::Value) -> Option<Vec<String>> {
    match value {
        serde_json::Value::Array(items) => Some(
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::trim))
                .filter(|item| !item.is_empty())
                .map(ToOwned::to_owned)
                .collect(),
        ),
        serde_json::Value::String(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return Some(Vec::new());
            }
            if trimmed.starts_with('[') {
                serde_json::from_str::<serde_json::Value>(trimmed)
                    .ok()
                    .and_then(|parsed| parse_string_list(&parsed))
            } else {
                Some(
                    trimmed
                        .lines()
                        .flat_map(|line| line.split(','))
                        .map(str::trim)
                        .filter(|item| !item.is_empty())
                        .map(ToOwned::to_owned)
                        .collect(),
                )
            }
        }
        _ => None,
    }
}

#[derive(Debug, serde::Deserialize)]
struct OpenAiModelsResponse {
    data: Vec<OpenAiModel>,
}

#[derive(Debug, serde::Deserialize)]
struct OpenAiModel {
    id: String,
}

async fn list_openai_compatible_models(
    base_url: &str,
    api_key: &str,
) -> Result<Vec<ModelMeta>, String> {
    let client = reqwest::Client::new();
    let url = format!("{}/models", base_url.trim_end_matches('/'));

    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {api_key}"))
        .send()
        .await
        .map_err(|error| format!("请求失败: {error}"))?;

    if !response.status().is_success() {
        return Err(format!("API 错误: {}", response.status()));
    }

    let body: OpenAiModelsResponse = response
        .json()
        .await
        .map_err(|error| format!("解析失败: {error}"))?;

    let mut models = body
        .data
        .into_iter()
        .map(|model| ModelMeta::from_id(model.id))
        .collect::<Vec<_>>();
    dedup_models(&mut models);

    if models.is_empty() {
        return Err("API 返回空模型列表".to_string());
    }

    Ok(models)
}

#[derive(Debug, serde::Deserialize)]
struct GeminiModelsResponse {
    models: Vec<GeminiModel>,
}

#[derive(Debug, serde::Deserialize)]
struct GeminiModel {
    name: String,
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    input_token_limit: Option<u64>,
}

async fn list_gemini_models(api_key: &str) -> Result<Vec<ModelMeta>, String> {
    let client = reqwest::Client::new();
    let url = format!("https://generativelanguage.googleapis.com/v1beta/models?key={api_key}");

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|error| format!("请求失败: {error}"))?;

    if !response.status().is_success() {
        return Err(format!("API 错误: {}", response.status()));
    }

    let body: GeminiModelsResponse = response
        .json()
        .await
        .map_err(|error| format!("解析失败: {error}"))?;

    let mut models = body
        .models
        .into_iter()
        .filter(|model| model.name.starts_with("models/"))
        .map(|model| {
            let id = model.name.trim_start_matches("models/").to_string();
            let display_name = if model.display_name.trim().is_empty() {
                format_model_name(&id)
            } else {
                model.display_name
            };
            ModelMeta {
                name: display_name,
                reasoning: true,
                supports_image: true,
                context_window: model.input_token_limit.unwrap_or(CONTEXT_WINDOW_STANDARD),
                blocked: false,
                discovered: true,
                source: ModelProfileSource::Discovered,
                id,
            }
        })
        .collect::<Vec<_>>();
    dedup_models(&mut models);

    if models.is_empty() {
        return Err("API 返回空模型列表".to_string());
    }

    Ok(models)
}

fn dedup_models(models: &mut Vec<ModelMeta>) {
    let mut seen = HashSet::new();
    models.retain(|model| seen.insert(model.id.clone()));
}

fn format_model_name(model_id: &str) -> String {
    model_id
        .split(['-', '_'])
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => {
                    let mut display = first.to_uppercase().collect::<String>();
                    display.push_str(&chars.as_str().to_ascii_lowercase());
                    display
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

// ─── Public probe API ───

/// 探测结果：简化的模型条目，只包含 id 和 name
#[derive(Debug, Clone, serde::Serialize)]
pub struct ProbeModelResult {
    pub id: String,
    pub name: String,
}

/// 用给定 credentials 实时探测远端可用模型列表。
/// 返回 Ok(vec) 表示成功，Err(string) 表示失败原因。
pub async fn probe_models_for_protocol(
    protocol: WireProtocol,
    api_key: &str,
    base_url: Option<&str>,
    discovery_url: Option<&str>,
) -> Result<Vec<ProbeModelResult>, String> {
    match protocol {
        WireProtocol::OpenaiCompatible => {
            let effective_url = discovery_url
                .or(base_url)
                .unwrap_or("https://api.openai.com/v1");
            let models = list_openai_compatible_models(effective_url, api_key).await?;
            Ok(models
                .into_iter()
                .map(|m| ProbeModelResult {
                    name: m.name,
                    id: m.id,
                })
                .collect())
        }
        WireProtocol::Gemini => {
            let models = list_gemini_models(api_key).await?;
            Ok(models
                .into_iter()
                .map(|m| ProbeModelResult {
                    name: m.name,
                    id: m.id,
                })
                .collect())
        }
        WireProtocol::Anthropic => Ok(vec![]),
        WireProtocol::OpenaiCodex => Ok(vec![]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn official_openai_base_url_defaults_to_responses() {
        assert_eq!(
            resolve_openai_wire_api(None, Some("https://api.openai.com/v1")).unwrap(),
            OpenAiWireApi::Responses
        );
        assert_eq!(
            resolve_openai_wire_api(None, None).unwrap(),
            OpenAiWireApi::Responses
        );
    }

    #[test]
    fn custom_openai_compatible_base_url_defaults_to_completions() {
        assert_eq!(
            resolve_openai_wire_api(None, Some("https://right.codes/codex/v1")).unwrap(),
            OpenAiWireApi::Completions
        );
    }

    #[test]
    fn explicit_wire_api_setting_wins_over_base_url_default() {
        assert_eq!(
            resolve_openai_wire_api(Some("responses"), Some("https://right.codes/codex/v1"))
                .unwrap(),
            OpenAiWireApi::Responses
        );
        assert_eq!(
            resolve_openai_wire_api(Some("completions"), Some("https://api.openai.com/v1"))
                .unwrap(),
            OpenAiWireApi::Completions
        );
    }

    #[test]
    fn unrecognized_wire_api_value_returns_error() {
        let result = resolve_openai_wire_api(Some("invalid_value"), None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("合法值"));
    }
}
