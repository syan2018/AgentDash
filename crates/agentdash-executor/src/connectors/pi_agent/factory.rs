use std::sync::Arc;

use agentdash_agent::LlmBridge;
use agentdash_domain::llm_provider::LlmProviderRepository;

use super::bridges::provider_registry::build_provider_entries_from_db;
use super::connector::PiAgentConnector;

pub struct NoopBridge;

#[async_trait::async_trait]
impl LlmBridge for NoopBridge {
    async fn stream_complete(
        &self,
        _request: agentdash_agent::BridgeRequest,
    ) -> std::pin::Pin<Box<dyn futures::Stream<Item = agentdash_agent::StreamChunk> + Send>> {
        Box::pin(tokio_stream::empty())
    }
}

/// 从 `LlmProviderRepository` 和 `SettingsRepository` 构建 `PiAgentConnector`。
///
/// Provider 列表从 `llm_providers` DB 表加载。
/// settings_repo 用于读取以下配置：
/// - `agent.pi.base_system_prompt`：覆盖内置 Layer 0 system prompt
/// - `agent.pi.user_preferences`：JSON 数组，用户偏好提示（Layer 2）
/// - `agent.pi.system_prompt`：向后兼容旧键（当 user_preferences 不存在时回退为单条偏好）
///
/// 按 sort_order，首个完成注册的 provider 的首个模型作为默认 bridge。
pub async fn build_pi_agent_connector(
    settings: &dyn agentdash_domain::settings::SettingsRepository,
    llm_provider_repo: &dyn LlmProviderRepository,
) -> Option<PiAgentConnector> {
    let system_prompt = read_setting_str(settings, "agent.pi.base_system_prompt")
        .await
        .or_else(|| std::env::var("PI_AGENT_SYSTEM_PROMPT").ok())
        .unwrap_or_else(|| super::system_prompt::DEFAULT_SYSTEM_PROMPT.to_string());

    let user_preferences = read_user_preferences(settings).await;

    let providers = build_provider_entries_from_db(llm_provider_repo).await;

    let (global_default_bridge, global_default_model) = if let Some(provider) = providers.first() {
        (
            provider.default_bridge.clone(),
            provider.entry.default_model.clone(),
        )
    } else {
        tracing::warn!(
            "PiAgentConnector: 启动时未检测到任何 LLM provider 配置，将以动态占位模式注册"
        );
        (Arc::new(NoopBridge) as Arc<dyn LlmBridge>, String::new())
    };

    let mut connector = PiAgentConnector::new(global_default_bridge, system_prompt);
    connector.set_user_preferences(user_preferences);

    for provider in providers {
        connector.add_provider(provider.entry);
    }

    if connector.provider_count() == 0 {
        tracing::info!("PiAgentConnector 已初始化（动态占位模式，等待 provider 配置）");
    } else {
        tracing::info!(
            "PiAgentConnector 已初始化（默认模型：{}，provider 数量：{}）",
            global_default_model,
            connector.provider_count()
        );
    }
    Some(connector)
}

/// 从 settings 读取用户偏好提示列表。
///
/// 优先读取 `agent.pi.user_preferences`（JSON 数组），
/// 若不存在则回退到旧 `agent.pi.system_prompt`（当作单条偏好）。
async fn read_user_preferences(
    settings: &dyn agentdash_domain::settings::SettingsRepository,
) -> Vec<String> {
    let scope = agentdash_domain::settings::SettingScope::system();

    if let Ok(Some(setting)) = settings.get(&scope, "agent.pi.user_preferences").await {
        if let Some(arr) = setting.value.as_array() {
            let prefs: Vec<String> = arr
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .filter(|s| !s.trim().is_empty())
                .collect();
            if !prefs.is_empty() {
                return prefs;
            }
        }
    }

    // 向后兼容：旧 agent.pi.system_prompt 键当作单条偏好
    if let Some(legacy) = read_setting_str(settings, "agent.pi.system_prompt").await {
        return vec![legacy];
    }

    Vec::new()
}

async fn read_setting_str(
    repo: &dyn agentdash_domain::settings::SettingsRepository,
    key: &str,
) -> Option<String> {
    repo.get(&agentdash_domain::settings::SettingScope::system(), key)
        .await
        .ok()
        .flatten()
        .and_then(|s| s.value.as_str().map(String::from))
        .filter(|s| !s.is_empty())
}
