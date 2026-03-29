use std::str::FromStr;

use agentdash_domain::common::{AgentConfig, ThinkingLevel};

/// 尝试将 AgentConfig 转换为 vibe-kanban 的 AgentConfig。
/// 若 executor 字符串不是有效的 BaseCodingAgent 变体则返回 None。
pub fn to_vibe_kanban_config(config: &AgentConfig) -> Option<executors::profile::ExecutorConfig> {
    use executors::executors::BaseCodingAgent;

    let norm = config.executor.replace('-', "_").to_ascii_uppercase();
    let agent = BaseCodingAgent::from_str(&norm).ok()?;
    let permission_policy = config
        .permission_policy
        .as_deref()
        .and_then(|p| serde_json::from_value(serde_json::json!(p)).ok());

    Some(executors::profile::ExecutorConfig {
        executor: agent,
        variant: config.variant.clone(),
        model_id: config.model_id.clone(),
        agent_id: config.agent_id.clone(),
        reasoning_id: config.thinking_level.map(|level| {
            match level {
                ThinkingLevel::Off => "off",
                ThinkingLevel::Minimal => "minimal",
                ThinkingLevel::Low => "low",
                ThinkingLevel::Medium => "medium",
                ThinkingLevel::High => "high",
                ThinkingLevel::Xhigh => "xhigh",
            }
            .to_string()
        }),
        permission_policy,
    })
}

/// 是否是 AgentDash 自有 agent（不属于 vibe-kanban 执行器）
pub fn is_native_agent(config: &AgentConfig) -> bool {
    to_vibe_kanban_config(config).is_none()
}
