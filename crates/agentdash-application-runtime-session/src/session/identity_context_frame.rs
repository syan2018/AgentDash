use agentdash_spi::hooks::{
    ContextFrame, ContextFrameSection, RuntimeContextFragmentEntry, RuntimeEventSource,
};

use super::context_frame::{self, ContextFramePayload};

/// identity 帧输入。
///
/// 仅承载系统身份（base + ProjectAgent 固定身份 + agent prompt）。用户偏好与项目
/// 指引已迁出至 `guidelines_context_frame`，identity section 只暴露结构化 fragments。
pub(crate) struct IdentityFrameInput<'a> {
    pub base_system_prompt: &'a str,
    pub agent_identity_markdown: Option<&'a str>,
    pub agent_system_prompt: Option<&'a str>,
}

pub(crate) fn build_identity_context_frames(input: &IdentityFrameInput<'_>) -> Vec<ContextFrame> {
    build_identity_context_frame(input).into_iter().collect()
}

fn build_identity_context_frame(input: &IdentityFrameInput<'_>) -> Option<ContextFrame> {
    let payload = IdentityContextFrame::from_input(input)?;
    Some(context_frame::build_context_frame(&payload))
}

#[cfg(test)]
pub(crate) fn resolve_identity_prompt(input: &IdentityFrameInput<'_>) -> String {
    IdentityContextFrame::from_input(input)
        .map(|frame| frame.rendered_text())
        .unwrap_or_default()
}

#[derive(Debug, Clone)]
struct IdentityContextFrame {
    base_prompt: String,
    agent_identity: Option<String>,
    agent_prompt: Option<String>,
}

impl IdentityContextFrame {
    fn from_input(input: &IdentityFrameInput<'_>) -> Option<Self> {
        let base = input.base_system_prompt.trim();
        let agent_identity = input
            .agent_identity_markdown
            .map(str::trim)
            .filter(|prompt| !prompt.is_empty())
            .map(ToString::to_string);
        let agent = input
            .agent_system_prompt
            .map(str::trim)
            .filter(|prompt| !prompt.is_empty())
            .map(ToString::to_string);

        if base.is_empty() && agent_identity.is_none() && agent.is_none() {
            return None;
        }

        Some(Self {
            base_prompt: base.to_string(),
            agent_identity,
            agent_prompt: agent,
        })
    }

    fn fragments(&self) -> Vec<RuntimeContextFragmentEntry> {
        let mut fragments = Vec::new();
        if !self.base_prompt.is_empty() {
            fragments.push(RuntimeContextFragmentEntry {
                slot: "identity".to_string(),
                label: "identity_system_prompt".to_string(),
                source: "connector".to_string(),
                content: self.system_prompt_section_text(),
                context_usage_kind: None,
            });
        }
        if self.agent_identity.is_some() || self.agent_prompt.is_some() {
            fragments.push(RuntimeContextFragmentEntry {
                slot: "identity".to_string(),
                label: "identity_agent_profile".to_string(),
                source: "project_agent".to_string(),
                content: self.agent_identity_section_text(),
                context_usage_kind: None,
            });
        }
        fragments
    }

    fn system_prompt_section_text(&self) -> String {
        format!("## System Prompt\n{}", self.base_prompt)
    }

    fn agent_identity_section_text(&self) -> String {
        let mut lines = vec!["## Agent Identity".to_string()];
        let identity_body = self
            .agent_identity
            .as_deref()
            .map(strip_agent_identity_heading)
            .unwrap_or_default();
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

fn strip_agent_identity_heading(content: &str) -> &str {
    content
        .strip_prefix("## Agent Identity")
        .or_else(|| content.strip_prefix("# Agent Identity"))
        .map(str::trim)
        .unwrap_or(content)
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
            summary: "Connector 的全局 system prompt、ProjectAgent 固定身份与 agent-level system prompt。".to_string(),
            fragments: self.fragments(),
        }]
    }

    fn rendered_text(&self) -> String {
        // 单一真相源：identity 帧承载 connector system identity。偏好/指引由独立
        // guidelines 帧承载，ProjectAgent 固定身份与 Agent system prompt 在这里以
        // 明确 markdown section 进入 system 通道。
        let body = self
            .fragments()
            .into_iter()
            .map(|fragment| fragment.content)
            .collect::<Vec<_>>()
            .join("\n\n");
        format!("# Identity\n\n{body}")
    }
}

#[cfg(test)]
mod tests {
    use super::{IdentityFrameInput, build_identity_context_frames, resolve_identity_prompt};
    use agentdash_spi::hooks::ContextFrameSection;

    #[test]
    fn resolve_identity_prompt_handles_append_and_agent_only() {
        let append_prompt = resolve_identity_prompt(&IdentityFrameInput {
            base_system_prompt: "base",
            agent_identity_markdown: None,
            agent_system_prompt: Some("agent"),
        });
        assert!(append_prompt.contains("base"));
        assert!(append_prompt.contains("agent"));
        assert!(append_prompt.starts_with("# Identity"));
        assert!(append_prompt.contains("## System Prompt\nbase"));
        assert!(append_prompt.contains("## Agent Identity\n\nagent"));

        let agent_only_prompt = resolve_identity_prompt(&IdentityFrameInput {
            base_system_prompt: "",
            agent_identity_markdown: None,
            agent_system_prompt: Some("agent only"),
        });
        assert!(agent_only_prompt.contains("## Agent Identity\n\nagent only"));
    }

    #[test]
    fn identity_frame_rendered_text_only_carries_identity() {
        let frames = build_identity_context_frames(&IdentityFrameInput {
            base_system_prompt: "base identity",
            agent_identity_markdown: None,
            agent_system_prompt: None,
        });
        assert_eq!(frames.len(), 1);
        let frame = frames.first().expect("identity frame");
        assert_eq!(frame.kind, "identity");
        assert_eq!(
            frame.rendered_text,
            "# Identity\n\n## System Prompt\nbase identity"
        );
        assert!(!frame.rendered_text.contains("## User Preferences"));
        assert!(!frame.rendered_text.contains("## Project Guidelines"));
    }

    #[test]
    fn identity_frame_groups_agent_identity_and_prompt_into_fragments() {
        let frames = build_identity_context_frames(&IdentityFrameInput {
            base_system_prompt: "base identity",
            agent_identity_markdown: Some("## Agent Identity\n- preset: `general`"),
            agent_system_prompt: Some("agent rules"),
        });
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].kind, "identity");
        assert!(frames[0].rendered_text.starts_with("# Identity"));
        assert!(
            frames[0]
                .rendered_text
                .contains("## System Prompt\nbase identity")
        );
        assert!(
            frames[0]
                .rendered_text
                .contains("## Agent Identity\n- preset: `general`\n\nagent rules")
        );

        let Some(ContextFrameSection::Identity { fragments, .. }) = frames[0].sections.first()
        else {
            panic!("identity section");
        };
        assert_eq!(fragments.len(), 2);
        assert_eq!(fragments[0].label, "identity_system_prompt");
        assert_eq!(fragments[1].label, "identity_agent_profile");
    }
}
