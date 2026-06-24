use crate::embedded_skill::{EmbeddedSkillBundle, EmbeddedSkillFile, EmbeddedSkillFileKind};

pub const MEMORY_MANAGER_SKILL_NAME: &str = "memory-manager";
pub const MEMORY_MANAGER_SKILL_PATH: &str = "skills/memory-manager/SKILL.md";

const MEMORY_MANAGER_SKILL_CONTENT: &str = include_str!("skills/memory-manager/SKILL.md");

const MEMORY_MANAGER_BUNDLE_FILES: &[EmbeddedSkillFile] = &[EmbeddedSkillFile {
    relative_path: "SKILL.md",
    content: MEMORY_MANAGER_SKILL_CONTENT,
    kind: EmbeddedSkillFileKind::Skill,
}];

pub const MEMORY_MANAGER_BUNDLE: EmbeddedSkillBundle = EmbeddedSkillBundle {
    name: MEMORY_MANAGER_SKILL_NAME,
    root_path: "skills/memory-manager",
    entry_path: "SKILL.md",
    files: MEMORY_MANAGER_BUNDLE_FILES,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_manager_bundle_is_valid() {
        MEMORY_MANAGER_BUNDLE
            .validate()
            .expect("memory-manager bundle should be valid");

        let skill = MEMORY_MANAGER_BUNDLE.files[0].content;
        assert!(skill.contains("agent://MEMORY.md"));
        assert!(skill.contains("topics/*.md"));
        assert!(skill.contains("fs.apply_patch"));
        assert!(!skill.contains("memory.read"));
        assert!(!skill.contains("memory.write"));
    }
}
