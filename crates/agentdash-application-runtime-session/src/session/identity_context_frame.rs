use agentdash_domain::common::SystemPromptMode;
use agentdash_spi::hooks::{ContextFrame, ContextFrameSection, RuntimeEventSource};

use super::context_frame::{self, ContextFramePayload};

/// identity 帧输入。
///
/// 仅承载系统身份（base + agent prompt）。用户偏好与项目指引已迁出至
/// `guidelines_context_frame`，以消除同一份系统提示词在 `effective_prompt`
/// 与 `rendered_text` 之间的双写。
pub(crate) struct IdentityFrameInput<'a> {
    pub base_system_prompt: &'a str,
    pub agent_system_prompt: Option<&'a str>,
    pub agent_system_prompt_mode: Option<SystemPromptMode>,
}

pub(crate) fn build_identity_context_frame(input: &IdentityFrameInput<'_>) -> Option<ContextFrame> {
    let effective_prompt = resolve_identity_prompt(input);
    if effective_prompt.trim().is_empty() {
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
        // 单一真相源：identity 帧仅承载身份提示词本身，且为「原样」文本——
        // 不再包裹 `## Identity` 等 markdown 脚手架，保持与历史上 connector 直接
        // 使用 effective_prompt 投递给模型的行为一致（无 AGENTS.md/偏好时，系统
        // 提示词与改造前逐字节相同）。偏好/指引由独立 guidelines 帧承载。
        self.effective_prompt.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::{IdentityFrameInput, build_identity_context_frame, resolve_identity_prompt};

    #[test]
    fn resolve_identity_prompt_handles_append_override_and_agent_only() {
        let append_prompt = resolve_identity_prompt(&IdentityFrameInput {
            base_system_prompt: "base",
            agent_system_prompt: Some("agent"),
            agent_system_prompt_mode: None,
        });
        assert!(append_prompt.contains("base"));
        assert!(append_prompt.contains("agent"));

        let override_prompt = resolve_identity_prompt(&IdentityFrameInput {
            base_system_prompt: "base",
            agent_system_prompt: Some("agent"),
            agent_system_prompt_mode: Some(agentdash_domain::common::SystemPromptMode::Override),
        });
        assert_eq!(override_prompt, "agent");

        let agent_only_prompt = resolve_identity_prompt(&IdentityFrameInput {
            base_system_prompt: "",
            agent_system_prompt: Some("agent only"),
            agent_system_prompt_mode: None,
        });
        assert_eq!(agent_only_prompt, "agent only");
    }

    #[test]
    fn identity_frame_rendered_text_only_carries_identity() {
        let frame = build_identity_context_frame(&IdentityFrameInput {
            base_system_prompt: "base identity",
            agent_system_prompt: None,
            agent_system_prompt_mode: None,
        })
        .expect("identity frame");
        // 原样身份提示词，不含任何 markdown 脚手架或偏好/指引段。
        assert_eq!(frame.rendered_text, "base identity");
        assert!(!frame.rendered_text.contains("## User Preferences"));
        assert!(!frame.rendered_text.contains("## Project Guidelines"));
    }
}
