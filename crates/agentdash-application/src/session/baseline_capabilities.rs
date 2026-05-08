use agentdash_spi::context::capability::{SessionBaselineCapabilities, SkillEntry};
use agentdash_spi::platform::skill::SkillRef;

/// 从已发现 skills 构建统一的 session baseline capabilities。
///
/// Companion agents 已迁移到 `CapabilityState.companion` 维度（由 Resolver 直接产出），
/// 不再通过 hook snapshot markdown 解析。
pub fn build_session_baseline_capabilities(skills: &[SkillRef]) -> SessionBaselineCapabilities {
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
        skills: skill_entries,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_capabilities_from_skills() {
        let skills = vec![SkillRef {
            name: "test-skill".to_string(),
            description: "A test skill".to_string(),
            file_path: "/workspace/skills/test/SKILL.md".into(),
            base_dir: "/workspace/skills/test".into(),
            disable_model_invocation: false,
        }];

        let caps = build_session_baseline_capabilities(&skills);
        assert_eq!(caps.skills.len(), 1);
        assert_eq!(caps.skills[0].name, "test-skill");
        assert!(!caps.is_empty());
    }

    #[test]
    fn build_capabilities_without_skills() {
        let caps = build_session_baseline_capabilities(&[]);
        assert!(caps.is_empty());
    }
}
