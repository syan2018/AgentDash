use agentdash_domain::common::SystemPromptMode;
use agentdash_spi::hooks::{ContextFrame, ContextFrameSection, RuntimeEventSource};

use super::context_frame::{self, ContextFramePayload};

/// identity 帧输入。
///
/// 仅承载系统身份（base + ProjectAgent 固定身份 + agent prompt）。用户偏好与项目
/// 指引已迁出至 `guidelines_context_frame`，以消除同一份系统提示词在
/// `effective_prompt` 与 `rendered_text` 之间的双写。
pub(crate) struct IdentityFrameInput<'a> {
    pub base_system_prompt: &'a str,
    pub agent_identity_markdown: Option<&'a str>,
    pub agent_system_prompt: Option<&'a str>,
    pub agent_system_prompt_mode: Option<SystemPromptMode>,
}

pub(crate) fn build_identity_context_frames(input: &IdentityFrameInput<'_>) -> Vec<ContextFrame> {
    identity_prompt_parts(input)
        .into_iter()
        .map(|part| context_frame::build_context_frame(&part))
        .collect()
}

#[cfg(test)]
pub(crate) fn resolve_identity_prompt(input: &IdentityFrameInput<'_>) -> String {
    identity_prompt_parts(input)
        .into_iter()
        .map(|part| part.rendered_text())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn identity_prompt_parts(input: &IdentityFrameInput<'_>) -> Vec<IdentityContextFrame> {
    let base = input.base_system_prompt.trim();
    let agent_identity = input
        .agent_identity_markdown
        .map(str::trim)
        .filter(|prompt| !prompt.is_empty());
    let agent = input
        .agent_system_prompt
        .map(str::trim)
        .filter(|prompt| !prompt.is_empty());
    let mode = resolve_identity_mode(
        input.base_system_prompt,
        input.agent_system_prompt,
        input.agent_system_prompt_mode,
    );

    let include_base = !matches!(
        (input.agent_system_prompt_mode, agent),
        (Some(SystemPromptMode::Override), Some(_))
    ) && !base.is_empty();

    let mut parts = Vec::new();
    if include_base {
        parts.push(IdentityContextFrame::new(
            IdentityContextFrameKind::SystemPrompt,
            "System Prompt",
            "Connector 的全局 system prompt。",
            base,
            None,
            mode.as_str(),
        ));
    }
    if agent_identity.is_some() || agent.is_some() {
        parts.push(IdentityContextFrame::new(
            IdentityContextFrameKind::AgentProfile,
            "Agent Identity",
            "ProjectAgent 固定身份与配置的 agent-level system prompt。",
            agent_identity.unwrap_or_default(),
            agent,
            mode.as_str(),
        ));
    }

    if let Some(first) = parts.first_mut() {
        first.include_identity_heading = true;
    }

    parts
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
enum IdentityContextFrameKind {
    SystemPrompt,
    AgentProfile,
}

impl IdentityContextFrameKind {
    fn frame_kind(&self) -> &'static str {
        match self {
            Self::SystemPrompt => "identity_system_prompt",
            Self::AgentProfile => "identity_agent_profile",
        }
    }

    fn id_slug(&self) -> &'static str {
        match self {
            Self::SystemPrompt => "system-prompt",
            Self::AgentProfile => "agent-profile",
        }
    }
}

#[derive(Debug, Clone)]
struct IdentityContextFrame {
    kind: IdentityContextFrameKind,
    title: String,
    summary: String,
    content: String,
    agent_prompt: Option<String>,
    mode: String,
    include_identity_heading: bool,
}

impl IdentityContextFrame {
    fn new(
        kind: IdentityContextFrameKind,
        title: &str,
        summary: &str,
        content: &str,
        agent_prompt: Option<&str>,
        mode: &str,
    ) -> Self {
        Self {
            kind,
            title: title.to_string(),
            summary: summary.to_string(),
            content: content.trim().to_string(),
            agent_prompt: agent_prompt.map(|prompt| prompt.trim().to_string()),
            mode: mode.to_string(),
            include_identity_heading: false,
        }
    }

    fn section_text(&self) -> String {
        match self.kind {
            IdentityContextFrameKind::SystemPrompt => {
                format!("## System Prompt\n{}", self.content)
            }
            IdentityContextFrameKind::AgentProfile => {
                let mut lines = vec!["## Agent Identity".to_string()];
                let identity_body = strip_agent_identity_heading(self.content.trim());
                if !identity_body.is_empty() {
                    lines.push(identity_body.to_string());
                }
                if let Some(agent_prompt) = self.agent_prompt.as_deref() {
                    lines.push(String::new());
                    lines.push(agent_prompt.trim().to_string());
                }
                lines.join("\n")
            }
        }
    }
}

fn strip_agent_identity_heading(content: &str) -> &str {
    content
        .strip_prefix("## Agent Identity")
        .or_else(|| content.strip_prefix("# Agent Identity"))
        .map(str::trim)
        .unwrap_or(content)
}

impl ContextFramePayload for IdentityContextFrame {
    fn id(&self, created_at_ms: i64) -> String {
        format!("identity-{}-{created_at_ms}", self.kind.id_slug())
    }

    fn kind(&self) -> &'static str {
        self.kind.frame_kind()
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
            title: self.title.clone(),
            summary: self.summary.clone(),
            base_prompt: match self.kind {
                IdentityContextFrameKind::SystemPrompt => self.content.clone(),
                _ => String::new(),
            },
            agent_prompt: self.agent_prompt.clone(),
            mode: self.mode.clone(),
            effective_prompt: self.rendered_text(),
        }]
    }

    fn rendered_text(&self) -> String {
        // 单一真相源：identity 帧承载 connector system identity。偏好/指引由独立
        // guidelines 帧承载，ProjectAgent 固定身份与 Agent system prompt 在这里以
        // 明确 markdown section 进入 system 通道。
        let section = self.section_text();
        if self.include_identity_heading {
            format!("# Identity\n\n{section}")
        } else {
            section
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{IdentityFrameInput, build_identity_context_frames, resolve_identity_prompt};

    #[test]
    fn resolve_identity_prompt_handles_append_override_and_agent_only() {
        let append_prompt = resolve_identity_prompt(&IdentityFrameInput {
            base_system_prompt: "base",
            agent_identity_markdown: None,
            agent_system_prompt: Some("agent"),
            agent_system_prompt_mode: None,
        });
        assert!(append_prompt.contains("base"));
        assert!(append_prompt.contains("agent"));
        assert!(append_prompt.starts_with("# Identity"));
        assert!(append_prompt.contains("## System Prompt\nbase"));
        assert!(append_prompt.contains("## Agent Identity\n\nagent"));

        let override_prompt = resolve_identity_prompt(&IdentityFrameInput {
            base_system_prompt: "base",
            agent_identity_markdown: None,
            agent_system_prompt: Some("agent"),
            agent_system_prompt_mode: Some(agentdash_domain::common::SystemPromptMode::Override),
        });
        assert!(!override_prompt.contains("base"));
        assert!(override_prompt.contains("## Agent Identity\n\nagent"));

        let agent_only_prompt = resolve_identity_prompt(&IdentityFrameInput {
            base_system_prompt: "",
            agent_identity_markdown: None,
            agent_system_prompt: Some("agent only"),
            agent_system_prompt_mode: None,
        });
        assert!(agent_only_prompt.contains("## Agent Identity\n\nagent only"));
    }

    #[test]
    fn identity_frame_rendered_text_only_carries_identity() {
        let frames = build_identity_context_frames(&IdentityFrameInput {
            base_system_prompt: "base identity",
            agent_identity_markdown: None,
            agent_system_prompt: None,
            agent_system_prompt_mode: None,
        });
        assert_eq!(frames.len(), 1);
        let frame = frames.first().expect("identity frame");
        assert_eq!(frame.kind, "identity_system_prompt");
        assert_eq!(
            frame.rendered_text,
            "# Identity\n\n## System Prompt\nbase identity"
        );
        assert!(!frame.rendered_text.contains("## User Preferences"));
        assert!(!frame.rendered_text.contains("## Project Guidelines"));
    }

    #[test]
    fn identity_frame_groups_agent_identity_and_prompt_into_sections() {
        let frames = build_identity_context_frames(&IdentityFrameInput {
            base_system_prompt: "base identity",
            agent_identity_markdown: Some("## Agent Identity\n- preset: `general`"),
            agent_system_prompt: Some("agent rules"),
            agent_system_prompt_mode: None,
        });
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].kind, "identity_system_prompt");
        assert_eq!(frames[1].kind, "identity_agent_profile");

        let rendered = frames
            .iter()
            .map(|frame| frame.rendered_text.as_str())
            .collect::<Vec<_>>()
            .join("\n\n");
        assert!(rendered.starts_with("# Identity"));
        assert!(rendered.contains("## System Prompt\nbase identity"));
        assert!(rendered.contains("## Agent Identity\n- preset: `general`\n\nagent rules"));
    }
}
