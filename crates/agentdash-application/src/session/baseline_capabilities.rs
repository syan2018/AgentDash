use agentdash_spi::hooks::HookSessionRuntimeAccess;
use agentdash_spi::session_capabilities::{
    CompanionAgentEntry, SessionBaselineCapabilities, SkillEntry,
};
use agentdash_spi::skill::SkillRef;

const COMPANION_AGENTS_SLOT: &str = "companion_agents";

/// 从 hook snapshot + 已发现 skills 构建统一的 session baseline capabilities。
pub fn build_session_baseline_capabilities(
    hook_session: Option<&dyn HookSessionRuntimeAccess>,
    skills: &[SkillRef],
) -> SessionBaselineCapabilities {
    let companion_agents = extract_companion_agents(hook_session);
    let skill_entries = skills
        .iter()
        .map(|s| SkillEntry {
            name: s.name.clone(),
            description: s.description.clone(),
            file_path: s.file_path.to_string_lossy().to_string(),
            disable_model_invocation: s.disable_model_invocation,
        })
        .collect();
    SessionBaselineCapabilities {
        companion_agents,
        skills: skill_entries,
    }
}

fn extract_companion_agents(
    hook_session: Option<&dyn HookSessionRuntimeAccess>,
) -> Vec<CompanionAgentEntry> {
    let Some(hook_session) = hook_session else {
        return Vec::new();
    };
    let snapshot = hook_session.snapshot();
    for injection in &snapshot.injections {
        if injection.slot == COMPANION_AGENTS_SLOT {
            return parse_companion_agents_from_markdown(&injection.content);
        }
    }
    Vec::new()
}

/// 从 `build_companion_agents_injection` 生成的 markdown 中解析结构化条目。
///
/// 格式：`- **{name}** (executor: \`{executor}\`): {display}`
fn parse_companion_agents_from_markdown(content: &str) -> Vec<CompanionAgentEntry> {
    let mut entries = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("- **") else {
            continue;
        };
        let Some(name_end) = rest.find("**") else {
            continue;
        };
        let name = rest[..name_end].to_string();
        let after_name = &rest[name_end + 2..];
        let executor = after_name
            .find('`')
            .and_then(|start| {
                let s = &after_name[start + 1..];
                s.find('`').map(|end| s[..end].to_string())
            })
            .unwrap_or_default();
        let display = after_name
            .find("):")
            .map(|pos| after_name[pos + 2..].trim().to_string())
            .unwrap_or_else(|| name.clone());
        entries.push(CompanionAgentEntry {
            name,
            executor,
            display_name: display,
        });
    }
    entries
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use agentdash_spi::hooks::SessionHookSnapshot;
    use agentdash_spi::{HookInjection, NoopExecutionHookProvider};

    use crate::session::hook_runtime::HookSessionRuntime;

    use super::*;

    #[test]
    fn build_capabilities_from_hook_and_skills() {
        let snapshot = SessionHookSnapshot {
            session_id: "sess-cap".to_string(),
            injections: vec![HookInjection {
                slot: "companion_agents".to_string(),
                content: "## Companion Agents\n\n- **agent** (executor: `PI_AGENT`): Agent\n- **reviewer** (executor: `PI_AGENT`): Code Reviewer".to_string(),
                source: "builtin:companion_agents".to_string(),
            }],
            ..SessionHookSnapshot::default()
        };
        let runtime = HookSessionRuntime::new(
            "sess-cap".to_string(),
            Arc::new(NoopExecutionHookProvider),
            snapshot,
        );
        let skills = vec![SkillRef {
            name: "test-skill".to_string(),
            description: "A test skill".to_string(),
            file_path: "/workspace/skills/test/SKILL.md".into(),
            base_dir: "/workspace/skills/test".into(),
            disable_model_invocation: false,
        }];

        let caps = build_session_baseline_capabilities(Some(&runtime), &skills);
        assert_eq!(caps.companion_agents.len(), 2);
        assert_eq!(caps.companion_agents[0].name, "agent");
        assert_eq!(caps.companion_agents[0].executor, "PI_AGENT");
        assert_eq!(caps.companion_agents[0].display_name, "Agent");
        assert_eq!(caps.companion_agents[1].name, "reviewer");
        assert_eq!(caps.companion_agents[1].display_name, "Code Reviewer");

        assert_eq!(caps.skills.len(), 1);
        assert_eq!(caps.skills[0].name, "test-skill");
        assert!(!caps.is_empty());
    }

    #[test]
    fn build_capabilities_without_hook_session() {
        let caps = build_session_baseline_capabilities(None, &[]);
        assert!(caps.is_empty());
    }

    #[test]
    fn parse_companion_markdown_handles_edge_cases() {
        let entries = parse_companion_agents_from_markdown("no companion lines here");
        assert!(entries.is_empty());

        let entries =
            parse_companion_agents_from_markdown("- **solo** (executor: `CODEX`): Solo Agent");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "solo");
        assert_eq!(entries[0].executor, "CODEX");
        assert_eq!(entries[0].display_name, "Solo Agent");
    }
}
