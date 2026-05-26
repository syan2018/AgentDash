use agentdash_domain::common::{AgentConfig, ThinkingLevel};

const CODEX_EXECUTOR_ID: &str = "CODEX";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodexPermissionPolicy {
    Auto,
    Supervised,
    Plan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexExecutorConfig {
    pub model_id: Option<String>,
    pub agent_id: Option<String>,
    pub reasoning_id: Option<String>,
    pub permission_policy: Option<CodexPermissionPolicy>,
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

fn parse_permission_policy(raw: Option<&str>) -> Option<CodexPermissionPolicy> {
    let raw = raw?.trim();
    if raw.is_empty() {
        return None;
    }

    match raw.replace('-', "_").to_ascii_uppercase().as_str() {
        "AUTO" | "AUTONOMOUS" | "NEVER" => Some(CodexPermissionPolicy::Auto),
        "SUPERVISED" | "UNLESS_TRUSTED" => Some(CodexPermissionPolicy::Supervised),
        "PLAN" | "ON_REQUEST" => Some(CodexPermissionPolicy::Plan),
        _ => None,
    }
}

pub fn to_codex_config(config: &AgentConfig) -> Option<CodexExecutorConfig> {
    if normalize_executor_id(&config.executor) != CODEX_EXECUTOR_ID {
        return None;
    }

    Some(CodexExecutorConfig {
        model_id: config.model_id.clone(),
        agent_id: config.agent_id.clone(),
        reasoning_id: config.thinking_level.map(map_thinking_level),
        permission_policy: parse_permission_policy(config.permission_policy.as_deref()),
    })
}

#[cfg(test)]
mod tests {
    use super::{CodexPermissionPolicy, to_codex_config};
    use agentdash_domain::common::{AgentConfig, ThinkingLevel};

    #[test]
    fn accepts_codex_executor_alias() {
        let mut config = AgentConfig::new("codex");
        config.model_id = Some("gpt-5.3-codex".to_string());
        config.thinking_level = Some(ThinkingLevel::High);
        config.permission_policy = Some("plan".to_string());

        let parsed = to_codex_config(&config).expect("codex executor should be accepted");
        assert_eq!(parsed.model_id.as_deref(), Some("gpt-5.3-codex"));
        assert_eq!(parsed.reasoning_id.as_deref(), Some("high"));
        assert_eq!(parsed.permission_policy, Some(CodexPermissionPolicy::Plan));
    }

    #[test]
    fn rejects_non_codex_executor() {
        let config = AgentConfig::new("claude_code");
        assert!(to_codex_config(&config).is_none());
    }
}
