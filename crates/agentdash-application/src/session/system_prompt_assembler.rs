//! Core system prompt 组装器。
//!
//! 这里仅负责稳定身份定义。用户偏好、项目规则、workspace、skills、hook runtime
//! 等动态上下文必须由独立 `ContextFrame` 投递和持久化。

use agentdash_domain::common::SystemPromptMode;

/// assembler 的全部输入——只在 application 层组装，不穿透到 connector。
pub struct SystemPromptInput<'a> {
    pub base_system_prompt: &'a str,
    pub agent_system_prompt: Option<&'a str>,
    pub agent_system_prompt_mode: Option<SystemPromptMode>,
}

/// 组装稳定 core system prompt 文本。
pub fn assemble_system_prompt(input: &SystemPromptInput) -> String {
    let mut sections: Vec<String> = Vec::new();

    // ── Identity: base prompt + agent-level override/append ──
    {
        let agent_sp = input.agent_system_prompt.filter(|s| !s.trim().is_empty());

        let identity = match (input.agent_system_prompt_mode, agent_sp) {
            (Some(SystemPromptMode::Override), Some(sp)) => sp.to_string(),
            (_, Some(sp)) if input.base_system_prompt.trim().is_empty() => sp.to_string(),
            (_, Some(sp)) => format!("{}\n\n{sp}", input.base_system_prompt),
            _ => input.base_system_prompt.to_string(),
        };

        sections.push(format!("## Identity\n\n{identity}"));
    }

    sections.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_system_prompt_only_renders_identity() {
        let prompt = assemble_system_prompt(&SystemPromptInput {
            base_system_prompt: "base",
            agent_system_prompt: Some("agent"),
            agent_system_prompt_mode: None,
        });

        assert!(prompt.contains("## Identity"));
        assert!(prompt.contains("base"));
        assert!(prompt.contains("agent"));
        assert!(!prompt.contains("## Workspace"));
        assert!(!prompt.contains("## Hooks"));
        assert!(!prompt.contains("<available_skills>"));
        assert!(!prompt.contains("## Project Context"));

        let agent_only = assemble_system_prompt(&SystemPromptInput {
            base_system_prompt: "",
            agent_system_prompt: Some("agent only"),
            agent_system_prompt_mode: None,
        });
        assert!(agent_only.contains("agent only"));
        assert!(!agent_only.contains("\n\n\n"));
    }
}
