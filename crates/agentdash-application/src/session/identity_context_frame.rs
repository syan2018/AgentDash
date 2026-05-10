use agentdash_domain::common::SystemPromptMode;
use agentdash_spi::hooks::{ContextFrame, ContextFrameSection, RuntimeEventSource};
use agentdash_spi::DiscoveredGuideline;

use super::context_frame::{self, ContextFramePayload};

pub(crate) struct IdentityFrameInput<'a> {
    pub base_system_prompt: &'a str,
    pub agent_system_prompt: Option<&'a str>,
    pub agent_system_prompt_mode: Option<SystemPromptMode>,
    pub user_preferences: &'a [String],
    pub discovered_guidelines: &'a [DiscoveredGuideline],
}

pub(crate) fn build_identity_context_frame(input: &IdentityFrameInput<'_>) -> Option<ContextFrame> {
    let effective_prompt = resolve_identity_prompt(input);
    if effective_prompt.trim().is_empty()
        && input.user_preferences.is_empty()
        && input.discovered_guidelines.is_empty()
    {
        return None;
    }
    let payload = IdentityContextFrame {
        base_prompt: input.base_system_prompt.trim().to_string(),
        agent_prompt: input
            .agent_system_prompt
            .map(str::trim)
            .filter(|prompt| !prompt.is_empty())
            .map(ToString::to_string),
        mode: resolve_identity_mode(
            input.base_system_prompt,
            input.agent_system_prompt,
            input.agent_system_prompt_mode,
        ),
        effective_prompt,
        user_preferences: input.user_preferences.to_vec(),
        discovered_guidelines: input.discovered_guidelines.to_vec(),
    };
    Some(context_frame::build_context_frame(&payload))
}

pub(crate) fn resolve_identity_prompt(input: &IdentityFrameInput<'_>) -> String {
    let base = input.base_system_prompt.trim();
    let agent = input
        .agent_system_prompt
        .map(str::trim)
        .filter(|prompt| !prompt.is_empty());
    match (input.agent_system_prompt_mode, agent) {
        (Some(SystemPromptMode::Override), Some(agent_prompt)) => agent_prompt.to_string(),
        (_, Some(agent_prompt)) if base.is_empty() => agent_prompt.to_string(),
        (_, Some(agent_prompt)) => format!("{base}\n\n{agent_prompt}"),
        _ => base.to_string(),
    }
}

fn resolve_identity_mode(
    base_system_prompt: &str,
    agent_system_prompt: Option<&str>,
    system_prompt_mode: Option<SystemPromptMode>,
) -> String {
    let has_base = !base_system_prompt.trim().is_empty();
    let has_agent = agent_system_prompt
        .map(str::trim)
        .is_some_and(|prompt| !prompt.is_empty());
    match (system_prompt_mode, has_base, has_agent) {
        (Some(SystemPromptMode::Override), _, true) => "override".to_string(),
        (_, true, true) => "append".to_string(),
        (_, false, true) => "agent_only".to_string(),
        _ => "base_only".to_string(),
    }
}

#[derive(Debug, Clone)]
struct IdentityContextFrame {
    base_prompt: String,
    agent_prompt: Option<String>,
    mode: String,
    effective_prompt: String,
    user_preferences: Vec<String>,
    discovered_guidelines: Vec<DiscoveredGuideline>,
}

impl ContextFramePayload for IdentityContextFrame {
    fn id(&self, created_at_ms: i64) -> String {
        format!("identity-{created_at_ms}")
    }

    fn kind(&self) -> &'static str {
        "identity"
    }

    fn source(&self) -> RuntimeEventSource {
        RuntimeEventSource::RuntimeContextUpdate
    }

    fn delivery_status(&self) -> String {
        "prepared_for_connector".to_string()
    }

    fn delivery_channel(&self) -> &'static str {
        "connector_context"
    }

    fn message_role(&self) -> &'static str {
        "system"
    }

    fn sections(&self) -> Vec<ContextFrameSection> {
        vec![ContextFrameSection::Identity {
            title: "Identity".to_string(),
            summary: "Connector 启动时使用的稳定 system identity。".to_string(),
            base_prompt: self.base_prompt.clone(),
            agent_prompt: self.agent_prompt.clone(),
            mode: self.mode.clone(),
            effective_prompt: self.effective_prompt.clone(),
        }]
    }

    fn rendered_text(&self) -> String {
        let mut parts = vec![format!("## Identity\n\n{}", self.effective_prompt)];
        if !self.user_preferences.is_empty() {
            let prefs = self
                .user_preferences
                .iter()
                .map(|p| format!("- {p}"))
                .collect::<Vec<_>>()
                .join("\n");
            parts.push(format!("## User Preferences\n\n{prefs}"));
        }
        if !self.discovered_guidelines.is_empty() {
            let guidelines = self
                .discovered_guidelines
                .iter()
                .filter(|g| !g.content.trim().is_empty())
                .map(|g| format!("### {}\n\n{}", g.path, g.content))
                .collect::<Vec<_>>()
                .join("\n\n");
            if !guidelines.is_empty() {
                parts.push(format!("## Project Guidelines\n\n{guidelines}"));
            }
        }
        parts.join("\n\n")
    }
}

#[cfg(test)]
mod tests {
    use super::{IdentityFrameInput, resolve_identity_prompt};

    #[test]
    fn resolve_identity_prompt_handles_append_override_and_agent_only() {
        let append_prompt = resolve_identity_prompt(&IdentityFrameInput {
            base_system_prompt: "base",
            agent_system_prompt: Some("agent"),
            agent_system_prompt_mode: None,
            user_preferences: &[],
            discovered_guidelines: &[],
        });
        assert!(append_prompt.contains("base"));
        assert!(append_prompt.contains("agent"));

        let override_prompt = resolve_identity_prompt(&IdentityFrameInput {
            base_system_prompt: "base",
            agent_system_prompt: Some("agent"),
            agent_system_prompt_mode: Some(agentdash_domain::common::SystemPromptMode::Override),
            user_preferences: &[],
            discovered_guidelines: &[],
        });
        assert_eq!(override_prompt, "agent");

        let agent_only_prompt = resolve_identity_prompt(&IdentityFrameInput {
            base_system_prompt: "",
            agent_system_prompt: Some("agent only"),
            agent_system_prompt_mode: None,
            user_preferences: &[],
            discovered_guidelines: &[],
        });
        assert_eq!(agent_only_prompt, "agent only");
    }
}
