use std::collections::HashSet;
use std::sync::Arc;

use super::anthropic_bridge::AnthropicBridge;
use super::openai_completions_bridge::OpenAiCompletionsBridge;
use super::openai_responses_bridge::OpenAiResponsesBridge;
use agentdash_agent::LlmBridge;
use agentdash_domain::llm_provider::{LlmProvider, LlmProviderRepository, WireProtocol};
use futures::future::BoxFuture;
use tokio::sync::RwLock;

type BridgeFactory = Arc<dyn Fn(&str) -> Arc<dyn LlmBridge> + Send + Sync>;

pub(crate) const CONTEXT_WINDOW_STANDARD: u64 = 200_000;
pub(crate) const CONTEXT_WINDOW_LEGACY: u64 = 128_000;
pub(crate) const CONTEXT_WINDOW_1M: u64 = 1_000_000;

type ModelListFuture = BoxFuture<'static, Result<Vec<ModelMeta>, String>>;

#[derive(Debug, Clone)]
pub(crate) struct ModelMeta {
    pub id: String,
    pub name: String,
    pub reasoning: bool,
    pub context_window: u64,
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
            blocked: false,
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

pub(crate) struct BuiltProviderEntry {
    pub entry: ProviderEntry,
    pub default_bridge: Arc<dyn LlmBridge>,
}

#[derive(Clone)]
pub(crate) struct ProviderEntry {
    pub provider_id: String,
    pub provider_name: String,
    pub default_model: String,
    bridge_factory: BridgeFactory,
    list_models: Option<Arc<dyn Fn() -> ModelListFuture + Send + Sync>>,
    configured_models: Vec<ModelMeta>,
    blocked_models: HashSet<String>,
    models_cache: Arc<RwLock<Option<Vec<ModelMeta>>>>,
}

impl ProviderEntry {
    fn new(
        provider_id: impl Into<String>,
        provider_name: impl Into<String>,
        default_model: String,
        bridge_factory: BridgeFactory,
        list_models: Option<Arc<dyn Fn() -> ModelListFuture + Send + Sync>>,
        configured_models: Vec<ModelMeta>,
        blocked_models: HashSet<String>,
    ) -> Self {
        Self {
            provider_id: provider_id.into(),
            provider_name: provider_name.into(),
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

        // Start with discovered models
        let mut models = discovered_models;

        // Merge in configured_models: add any custom IDs not already present
        for custom in &self.configured_models {
            if !models.iter().any(|m| m.id == custom.id) {
                models.push(custom.clone());
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

pub(crate) async fn build_provider_entries_from_db(
    repo: &dyn LlmProviderRepository,
) -> Vec<BuiltProviderEntry> {
    let providers = match repo.list_enabled().await {
        Ok(list) => list,
        Err(e) => {
            tracing::error!("PiAgentConnector: 从 DB 读取 LLM providers 失败: {e}");
            return Vec::new();
        }
    };

    let mut result = Vec::new();
    for db_provider in providers {
        if let Some(entry) = build_provider_entry_from_db(&db_provider) {
            result.push(entry);
        }
    }
    result
}

fn build_provider_entry_from_db(db_provider: &LlmProvider) -> Option<BuiltProviderEntry> {
    let api_key = db_provider.resolve_api_key();
    // 对于非 openai_compatible + 空 api_key 的情况，Anthropic/Gemini 需要 key
    let needs_api_key = !matches!(db_provider.protocol, WireProtocol::OpenaiCompatible)
        || !db_provider.api_key.is_empty()
        || !db_provider.env_api_key.is_empty();
    if needs_api_key && api_key.is_none() {
        // 无 API key 且不是可以无 key 运行的 provider (如 Ollama)，跳过
        // 但 openai_compatible 可能是无 key 的本地端点
        if !matches!(db_provider.protocol, WireProtocol::OpenaiCompatible) {
            return None;
        }
    }
    let api_key = api_key.unwrap_or_default();

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
                return None;
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
            return None;
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
            return None;
        }
    };

    let discovery_url = if db_provider.discovery_url.is_empty() {
        None
    } else {
        Some(db_provider.discovery_url.clone())
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

    Some(BuiltProviderEntry {
        entry: ProviderEntry::new(
            provider_id,
            db_provider.name.clone(),
            default_model,
            bridge_factory,
            list_models,
            configured_models,
            blocked_models,
        ),
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
                Arc::new(AnthropicBridge::new(
                    &api_key,
                    model_id,
                    base.as_deref(),
                )) as Arc<dyn LlmBridge>
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

    // 现代模型普遍支持 extended thinking，只有明确已知的旧模型关闭
    let reasoning = !is_known_non_reasoning(&id_lower);

    let context_window = infer_context_window(&id_lower);

    ModelMeta {
        id: model_id.to_string(),
        name: format_model_name(model_id),
        reasoning,
        context_window,
        blocked: false,
    }
}

fn is_known_non_reasoning(model_id: &str) -> bool {
    // GPT-3.5 / GPT-4 base (非 o 系列) 的早期版本
    if model_id.contains("4o") {
        return true;
    }
    false
}

fn infer_context_window(model_id: &str) -> u64 {
    if model_id.contains("gemini")
        || model_id.contains("opus")
        || model_id.contains("sonnet")
    {
        return CONTEXT_WINDOW_1M;
    }

    if model_id.contains("deepseek") || model_id.contains("grok") || model_id.contains("4o") {
        return CONTEXT_WINDOW_LEGACY;
    }
    CONTEXT_WINDOW_STANDARD
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
