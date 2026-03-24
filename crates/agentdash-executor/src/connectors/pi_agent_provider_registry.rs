use std::collections::HashSet;
use std::sync::Arc;

use agentdash_agent::{LlmBridge, RigBridge};
use agentdash_domain::settings::{SettingScope, SettingsRepository};
use futures::future::BoxFuture;
use rig::client::CompletionClient as _;
use tokio::sync::RwLock;

pub(crate) const CONTEXT_WINDOW_STANDARD: u64 = 200_000;
pub(crate) const CONTEXT_WINDOW_LEGACY: u64 = 128_000;
pub(crate) const CONTEXT_WINDOW_1M: u64 = 1_048_576;
pub(crate) const MAX_TOKENS_STANDARD: u64 = 16_384;
pub(crate) const MAX_TOKENS_LARGE_REASONING: u64 = 100_000;
pub(crate) const MAX_TOKENS_GEMINI: u64 = 65_536;
pub(crate) const MAX_TOKENS_LEGACY: u64 = 8_192;

type ModelListFuture = BoxFuture<'static, Result<Vec<ModelMeta>, String>>;

#[derive(Debug, Clone)]
pub(crate) struct ModelMeta {
    pub id: String,
    pub name: String,
    pub reasoning: bool,
    pub context_window: u64,
    pub max_tokens: u64,
    pub blocked: bool,
}

impl ModelMeta {
    pub(crate) fn from_id(id: impl Into<String>) -> Self {
        let id = id.into();
        let inferred = infer_model_meta(&id);
        Self {
            name: format_model_name(&id),
            id,
            ..inferred
        }
    }

    fn fallback(id: &str) -> Self {
        Self::from_id(id.to_string())
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
    context_window: Option<u64>,
    #[serde(default)]
    max_tokens: Option<u64>,
}

impl From<StoredModelMeta> for ModelMeta {
    fn from(value: StoredModelMeta) -> Self {
        let inferred = infer_model_meta(&value.id);
        Self {
            id: value.id,
            name: value
                .name
                .filter(|item| !item.trim().is_empty())
                .unwrap_or_else(|| inferred.name.clone()),
            reasoning: value.reasoning.unwrap_or(inferred.reasoning),
            context_window: value.context_window.unwrap_or(inferred.context_window),
            max_tokens: value.max_tokens.unwrap_or(inferred.max_tokens),
            blocked: false,
        }
    }
}

#[derive(Clone, Copy)]
enum BridgeKind {
    Anthropic,
    Gemini,
    DeepSeek,
    Groq,
    Xai,
    OpenAi,
}

#[derive(Clone, Copy)]
enum DiscoveryKind {
    ConfigOnly,
    GeminiApi,
    OpenAiCompatibleFixed(&'static str),
    OpenAiCompatibleFromBaseUrl { default_base_url: &'static str },
}

struct ProviderSpec {
    provider_id: &'static str,
    provider_name: &'static str,
    api_key_setting_key: &'static str,
    api_key_env_var: &'static str,
    default_model: &'static str,
    default_model_setting_key: Option<&'static str>,
    base_url_setting_key: Option<&'static str>,
    base_url_env_var: Option<&'static str>,
    models_setting_key: &'static str,
    blocked_models_setting_key: &'static str,
    bridge_kind: BridgeKind,
    discovery_kind: DiscoveryKind,
}

const PROVIDER_SPECS: &[ProviderSpec] = &[
    ProviderSpec {
        provider_id: "anthropic",
        provider_name: "Anthropic Claude",
        api_key_setting_key: "llm.anthropic.api_key",
        api_key_env_var: "ANTHROPIC_API_KEY",
        default_model: rig::providers::anthropic::completion::CLAUDE_4_SONNET,
        default_model_setting_key: None,
        base_url_setting_key: None,
        base_url_env_var: None,
        models_setting_key: "llm.anthropic.models",
        blocked_models_setting_key: "llm.anthropic.blocked_models",
        bridge_kind: BridgeKind::Anthropic,
        discovery_kind: DiscoveryKind::ConfigOnly,
    },
    ProviderSpec {
        provider_id: "gemini",
        provider_name: "Google Gemini",
        api_key_setting_key: "llm.gemini.api_key",
        api_key_env_var: "GEMINI_API_KEY",
        default_model: "gemini-2.5-flash",
        default_model_setting_key: None,
        base_url_setting_key: None,
        base_url_env_var: None,
        models_setting_key: "llm.gemini.models",
        blocked_models_setting_key: "llm.gemini.blocked_models",
        bridge_kind: BridgeKind::Gemini,
        discovery_kind: DiscoveryKind::GeminiApi,
    },
    ProviderSpec {
        provider_id: "deepseek",
        provider_name: "DeepSeek",
        api_key_setting_key: "llm.deepseek.api_key",
        api_key_env_var: "DEEPSEEK_API_KEY",
        default_model: "deepseek-chat",
        default_model_setting_key: None,
        base_url_setting_key: None,
        base_url_env_var: None,
        models_setting_key: "llm.deepseek.models",
        blocked_models_setting_key: "llm.deepseek.blocked_models",
        bridge_kind: BridgeKind::DeepSeek,
        discovery_kind: DiscoveryKind::OpenAiCompatibleFixed("https://api.deepseek.com/v1"),
    },
    ProviderSpec {
        provider_id: "groq",
        provider_name: "Groq",
        api_key_setting_key: "llm.groq.api_key",
        api_key_env_var: "GROQ_API_KEY",
        default_model: "llama-3.3-70b-versatile",
        default_model_setting_key: None,
        base_url_setting_key: None,
        base_url_env_var: None,
        models_setting_key: "llm.groq.models",
        blocked_models_setting_key: "llm.groq.blocked_models",
        bridge_kind: BridgeKind::Groq,
        discovery_kind: DiscoveryKind::OpenAiCompatibleFixed("https://api.groq.com/openai/v1"),
    },
    ProviderSpec {
        provider_id: "xai",
        provider_name: "xAI (Grok)",
        api_key_setting_key: "llm.xai.api_key",
        api_key_env_var: "XAI_API_KEY",
        default_model: "grok-3",
        default_model_setting_key: None,
        base_url_setting_key: None,
        base_url_env_var: None,
        models_setting_key: "llm.xai.models",
        blocked_models_setting_key: "llm.xai.blocked_models",
        bridge_kind: BridgeKind::Xai,
        discovery_kind: DiscoveryKind::OpenAiCompatibleFixed("https://api.x.ai/v1"),
    },
    ProviderSpec {
        provider_id: "openai",
        provider_name: "OpenAI",
        api_key_setting_key: "llm.openai.api_key",
        api_key_env_var: "OPENAI_API_KEY",
        default_model: "gpt-4o",
        default_model_setting_key: Some("llm.openai.default_model"),
        base_url_setting_key: Some("llm.openai.base_url"),
        base_url_env_var: Some("OPENAI_BASE_URL"),
        models_setting_key: "llm.openai.models",
        blocked_models_setting_key: "llm.openai.blocked_models",
        bridge_kind: BridgeKind::OpenAi,
        discovery_kind: DiscoveryKind::OpenAiCompatibleFromBaseUrl {
            default_base_url: "https://api.openai.com/v1",
        },
    },
];

pub(crate) struct BuiltProviderEntry {
    pub entry: ProviderEntry,
    pub default_bridge: Arc<dyn LlmBridge>,
}

#[derive(Clone)]
pub(crate) struct ProviderEntry {
    pub provider_id: &'static str,
    pub provider_name: &'static str,
    pub default_model: String,
    bridge_factory: Arc<dyn Fn(&str) -> Arc<dyn LlmBridge> + Send + Sync>,
    list_models: Option<Arc<dyn Fn() -> ModelListFuture + Send + Sync>>,
    configured_models: Vec<ModelMeta>,
    blocked_models: HashSet<String>,
    models_cache: Arc<RwLock<Option<Vec<ModelMeta>>>>,
}

impl ProviderEntry {
    fn new(
        provider_id: &'static str,
        provider_name: &'static str,
        default_model: String,
        bridge_factory: Arc<dyn Fn(&str) -> Arc<dyn LlmBridge> + Send + Sync>,
        list_models: Option<Arc<dyn Fn() -> ModelListFuture + Send + Sync>>,
        configured_models: Vec<ModelMeta>,
        blocked_models: HashSet<String>,
    ) -> Self {
        Self {
            provider_id,
            provider_name,
            default_model,
            bridge_factory,
            list_models,
            configured_models,
            blocked_models,
            models_cache: Arc::new(RwLock::new(None)),
        }
    }

    pub(crate) fn create_bridge(&self, model_id: &str) -> Arc<dyn LlmBridge> {
        (self.bridge_factory)(model_id)
    }

    async fn load_models_raw(&self) -> Vec<ModelMeta> {
        if let Some(cached) = self.models_cache.read().await.clone() {
            return cached;
        }

        let discovered_models = if let Some(list_models) = &self.list_models {
            match list_models().await {
                Ok(models) => models,
                Err(error) => {
                    tracing::warn!(
                        "PiAgentConnector: provider={} 动态获取模型失败: {}",
                        self.provider_id,
                        error
                    );
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        };

        let mut models = if !discovered_models.is_empty() {
            discovered_models
        } else if !self.configured_models.is_empty() {
            self.configured_models.clone()
        } else {
            vec![ModelMeta::fallback(&self.default_model)]
        };
        dedup_models(&mut models);
        for model in &mut models {
            model.blocked = self.blocked_models.contains(model.id.as_str());
        }

        let mut cache = self.models_cache.write().await;
        *cache = Some(models.clone());
        models
    }

    pub(crate) async fn load_models_with_block_state(&self) -> Vec<ModelMeta> {
        self.load_models_raw().await
    }

    pub(crate) async fn supports_model(&self, model_id: &str) -> bool {
        self.load_models_raw()
            .await
            .iter()
            .any(|model| model.id == model_id)
    }
}

pub(crate) async fn build_provider_entries(
    settings: &dyn SettingsRepository,
) -> Vec<BuiltProviderEntry> {
    let mut providers = Vec::new();
    for spec in PROVIDER_SPECS {
        if let Some(entry) = build_provider_entry(settings, spec).await {
            providers.push(entry);
        }
    }
    providers
}

async fn build_provider_entry(
    settings: &dyn SettingsRepository,
    spec: &ProviderSpec,
) -> Option<BuiltProviderEntry> {
    let api_key = read_setting_str(settings, spec.api_key_setting_key)
        .await
        .or_else(|| std::env::var(spec.api_key_env_var).ok())?;

    let base_url = match (spec.base_url_setting_key, spec.base_url_env_var) {
        (Some(setting_key), env_key) => read_setting_str(settings, setting_key)
            .await
            .or_else(|| env_key.and_then(|key| std::env::var(key).ok())),
        (None, _) => None,
    };

    let default_model = match spec.default_model_setting_key {
        Some(setting_key) => read_setting_str(settings, setting_key)
            .await
            .unwrap_or_else(|| spec.default_model.to_string()),
        None => spec.default_model.to_string(),
    };

    let configured_models = read_model_list(settings, spec.models_setting_key).await;
    let blocked_models = read_string_list(settings, spec.blocked_models_setting_key)
        .await
        .into_iter()
        .collect::<HashSet<_>>();

    let bridge_factory = build_bridge_factory(spec.bridge_kind, api_key.clone(), base_url.clone());
    let default_bridge = bridge_factory(&default_model);
    let list_models = build_model_lister(spec.discovery_kind, api_key, base_url);

    tracing::info!(
        "PiAgentConnector: provider={} 已注册（default_model={}）",
        spec.provider_id,
        default_model
    );

    Some(BuiltProviderEntry {
        entry: ProviderEntry::new(
            spec.provider_id,
            spec.provider_name,
            default_model,
            bridge_factory,
            list_models,
            configured_models,
            blocked_models,
        ),
        default_bridge,
    })
}

fn build_bridge_factory(
    kind: BridgeKind,
    api_key: String,
    base_url: Option<String>,
) -> Arc<dyn Fn(&str) -> Arc<dyn LlmBridge> + Send + Sync> {
    match kind {
        BridgeKind::Anthropic => {
            let client = rig::providers::anthropic::Client::new(&api_key);
            Arc::new(move |model_id: &str| {
                Arc::new(RigBridge::new(client.completion_model(model_id))) as Arc<dyn LlmBridge>
            })
        }
        BridgeKind::Gemini => {
            let client = rig::providers::gemini::Client::new(&api_key);
            Arc::new(move |model_id: &str| {
                Arc::new(RigBridge::new(client.completion_model(model_id))) as Arc<dyn LlmBridge>
            })
        }
        BridgeKind::DeepSeek => {
            let client = rig::providers::deepseek::Client::new(&api_key);
            Arc::new(move |model_id: &str| {
                Arc::new(RigBridge::new(client.completion_model(model_id))) as Arc<dyn LlmBridge>
            })
        }
        BridgeKind::Groq => {
            let client = rig::providers::groq::Client::new(&api_key);
            Arc::new(move |model_id: &str| {
                Arc::new(RigBridge::new(client.completion_model(model_id))) as Arc<dyn LlmBridge>
            })
        }
        BridgeKind::Xai => {
            let client = rig::providers::xai::Client::new(&api_key);
            Arc::new(move |model_id: &str| {
                Arc::new(RigBridge::new(client.completion_model(model_id))) as Arc<dyn LlmBridge>
            })
        }
        BridgeKind::OpenAi => {
            let mut builder = rig::providers::openai::Client::builder(&api_key);
            let base_url_owned = base_url;
            if let Some(ref url) = base_url_owned {
                builder = builder.base_url(url);
            }
            let client = builder.build();
            Arc::new(move |model_id: &str| {
                Arc::new(RigBridge::new(client.completion_model(model_id))) as Arc<dyn LlmBridge>
            })
        }
    }
}

fn build_model_lister(
    discovery: DiscoveryKind,
    api_key: String,
    base_url: Option<String>,
) -> Option<Arc<dyn Fn() -> ModelListFuture + Send + Sync>> {
    match discovery {
        DiscoveryKind::ConfigOnly => None,
        DiscoveryKind::GeminiApi => Some(Arc::new(move || {
            let api_key = api_key.clone();
            Box::pin(async move { list_gemini_models(&api_key).await })
        })),
        DiscoveryKind::OpenAiCompatibleFixed(url) => Some(Arc::new(move || {
            let api_key = api_key.clone();
            let url = url.to_string();
            Box::pin(async move { list_openai_compatible_models(&url, &api_key).await })
        })),
        DiscoveryKind::OpenAiCompatibleFromBaseUrl { default_base_url } => {
            Some(Arc::new(move || {
                let api_key = api_key.clone();
                let api_base = base_url
                    .clone()
                    .unwrap_or_else(|| default_base_url.to_string());
                Box::pin(async move { list_openai_compatible_models(&api_base, &api_key).await })
            }))
        }
    }
}

async fn read_setting_value(repo: &dyn SettingsRepository, key: &str) -> Option<serde_json::Value> {
    repo.get(&SettingScope::system(), key)
        .await
        .ok()
        .flatten()
        .map(|setting| setting.value)
}

async fn read_setting_str(repo: &dyn SettingsRepository, key: &str) -> Option<String> {
    read_setting_value(repo, key)
        .await
        .and_then(|value| value.as_str().map(str::trim).map(ToOwned::to_owned))
        .filter(|value| !value.is_empty())
}

async fn read_model_list(repo: &dyn SettingsRepository, key: &str) -> Vec<ModelMeta> {
    let Some(value) = read_setting_value(repo, key).await else {
        return Vec::new();
    };

    parse_model_list(&value).unwrap_or_default()
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

async fn read_string_list(repo: &dyn SettingsRepository, key: &str) -> Vec<String> {
    let Some(value) = read_setting_value(repo, key).await else {
        return Vec::new();
    };
    parse_string_list(&value).unwrap_or_default()
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
        .map(|model| infer_model_meta(&model.id))
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
    #[serde(default)]
    output_token_limit: Option<u64>,
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
            let inferred = infer_model_meta(&id);
            ModelMeta {
                id,
                name: if model.display_name.trim().is_empty() {
                    inferred.name.clone()
                } else {
                    model.display_name
                },
                context_window: model.input_token_limit.unwrap_or(inferred.context_window),
                max_tokens: model.output_token_limit.unwrap_or(inferred.max_tokens),
                ..inferred
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

fn infer_model_meta(model_id: &str) -> ModelMeta {
    let id_lower = model_id.to_ascii_lowercase();
    let reasoning = id_lower.contains("reasoning")
        || id_lower.contains("thinking")
        || id_lower.starts_with("o1")
        || id_lower.starts_with("o3")
        || id_lower.starts_with("o4")
        || id_lower.contains("qwq")
        || id_lower.contains("r1")
        || id_lower.contains("reasoner");

    let context_window = infer_context_window(&id_lower);
    let max_tokens = infer_max_tokens(&id_lower, reasoning);

    ModelMeta {
        id: model_id.to_string(),
        name: format_model_name(model_id),
        reasoning,
        context_window,
        max_tokens,
        blocked: false,
    }
}

fn infer_context_window(model_id: &str) -> u64 {
    if model_id.contains("1m") || model_id.contains("1000000") || model_id.contains("1048576") {
        return CONTEXT_WINDOW_1M;
    }
    if model_id.contains("200k") || model_id.contains("200000") {
        return CONTEXT_WINDOW_STANDARD;
    }
    if model_id.contains("128k") || model_id.contains("128000") {
        return CONTEXT_WINDOW_LEGACY;
    }
    if model_id.contains("gemini") {
        return CONTEXT_WINDOW_1M;
    }
    if model_id.contains("claude") || model_id.contains("gpt-4") || model_id.starts_with('o') {
        return CONTEXT_WINDOW_STANDARD;
    }
    if model_id.contains("deepseek") || model_id.contains("grok") {
        return CONTEXT_WINDOW_LEGACY;
    }
    CONTEXT_WINDOW_STANDARD
}

fn infer_max_tokens(model_id: &str, reasoning: bool) -> u64 {
    if model_id.contains("gemini") {
        return MAX_TOKENS_GEMINI;
    }
    if reasoning && (model_id.starts_with('o') || model_id.contains("reasoner")) {
        return MAX_TOKENS_LARGE_REASONING;
    }
    if model_id.contains("claude") || model_id.contains("deepseek") {
        return MAX_TOKENS_LEGACY;
    }
    MAX_TOKENS_STANDARD
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
