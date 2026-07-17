use agentdash_domain::common::{AgentConfig, ThinkingLevel};

const CODEX_EXECUTOR_ID: &str = "CODEX";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexExecutorConfig {
    pub model_id: Option<String>,
    pub agent_id: Option<String>,
    pub reasoning_id: Option<String>,
}

fn normalize_executor_id(executor: &str) -> String {
    executor.trim().replace('-', "_").to_ascii_uppercase()
}

fn map_thinking_level(level: ThinkingLevel) -> String {
    match level {
        ThinkingLevel::Off => "off",
        ThinkingLevel::Minimal => "minimal",
        ThinkingLevel::Low => "low",
        ThinkingLevel::Medium => "medium",
        ThinkingLevel::High => "high",
        ThinkingLevel::Xhigh => "xhigh",
    }
    .to_string()
}

pub fn to_codex_config(config: &AgentConfig) -> Option<CodexExecutorConfig> {
    if normalize_executor_id(&config.executor) != CODEX_EXECUTOR_ID {
        return None;
    }

    Some(CodexExecutorConfig {
        model_id: config.model_id.clone(),
        agent_id: config.agent_id.clone(),
        reasoning_id: config.thinking_level.map(map_thinking_level),
    })
}

#[cfg(test)]
mod tests {
    use super::to_codex_config;
    use agentdash_domain::common::{AgentConfig, ThinkingLevel};

    #[test]
    fn accepts_codex_executor_alias() {
        let mut config = AgentConfig::new("codex");
        config.model_id = Some("gpt-5.3-codex".to_string());
        config.thinking_level = Some(ThinkingLevel::High);
        let parsed = to_codex_config(&config).expect("codex executor should be accepted");
        assert_eq!(parsed.model_id.as_deref(), Some("gpt-5.3-codex"));
        assert_eq!(parsed.reasoning_id.as_deref(), Some("high"));
    }

    #[test]
    fn rejects_non_codex_executor() {
        let config = AgentConfig::new("claude_code");
        assert!(to_codex_config(&config).is_none());
    }
}
