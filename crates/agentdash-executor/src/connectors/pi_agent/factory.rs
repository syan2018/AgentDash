use agentdash_diagnostics::{Subsystem, diag};
use std::sync::Arc;

use agentdash_agent::LlmBridge;
use agentdash_domain::llm_provider::{
    LlmProviderCredentialRepository, LlmProviderRepository, LlmSecretCodec,
};

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
///
/// 按 sort_order，首个完成注册的 provider 的首个模型作为默认 bridge。
pub async fn build_pi_agent_connector(
    settings: &dyn agentdash_domain::settings::SettingsRepository,
    llm_provider_repo: &dyn LlmProviderRepository,
    credential_repo: &dyn LlmProviderCredentialRepository,
    secret_codec: &dyn LlmSecretCodec,
) -> Option<PiAgentConnector> {
    let system_prompt = read_setting_str(settings, "agent.pi.base_system_prompt")
        .await
        .or_else(|| std::env::var("PI_AGENT_SYSTEM_PROMPT").ok())
        .unwrap_or_else(|| super::system_prompt::DEFAULT_SYSTEM_PROMPT.to_string());

    let providers = build_provider_entries_from_db(
        llm_provider_repo,
        Some(credential_repo),
        secret_codec,
        None,
    )
    .await;

    let (global_default_bridge, global_default_model) = if let Some(provider) = providers.first() {
        (
            provider.default_bridge.clone(),
            provider.entry.default_model.clone(),
        )
    } else {
        diag!(
            Warn,
            Subsystem::AgentRun,
            "PiAgentConnector: 启动时未检测到任何 LLM provider 配置，将以动态占位模式注册"
        );
        (Arc::new(NoopBridge) as Arc<dyn LlmBridge>, String::new())
    };

    let mut connector = PiAgentConnector::new(global_default_bridge, system_prompt);

    for provider in providers {
        connector.add_provider(provider.entry);
    }

    if connector.provider_count() == 0 {
        diag!(
            Info,
            Subsystem::AgentRun,
            "PiAgentConnector 已初始化（动态占位模式，等待 provider 配置）"
        );
    } else {
        diag!(
            Info,
            Subsystem::AgentRun,
            "PiAgentConnector 已初始化（默认模型：{}，provider 数量：{}）",
            global_default_model,
            connector.provider_count()
        );
    }
    Some(connector)
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
