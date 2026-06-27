use crate::embedded_skill::{EmbeddedSkillBundle, EmbeddedSkillFile, EmbeddedSkillFileKind};

pub const WORKSPACE_MODULE_SYSTEM_SKILL_NAME: &str = "workspace-module-system";
pub const WORKSPACE_MODULE_SYSTEM_SKILL_PATH: &str = "skills/workspace-module-system/SKILL.md";

const WORKSPACE_MODULE_SYSTEM_SKILL_CONTENT: &str =
    include_str!("skills/workspace-module-system/SKILL.md");
const WORKSPACE_MODULE_SYSTEM_BUNDLE_FILES: &[EmbeddedSkillFile] = &[EmbeddedSkillFile {
    relative_path: "SKILL.md",
    content: WORKSPACE_MODULE_SYSTEM_SKILL_CONTENT,
    kind: EmbeddedSkillFileKind::Skill,
}];

pub const WORKSPACE_MODULE_SYSTEM_BUNDLE: EmbeddedSkillBundle = EmbeddedSkillBundle {
    name: WORKSPACE_MODULE_SYSTEM_SKILL_NAME,
    root_path: "skills/workspace-module-system",
    entry_path: "SKILL.md",
    files: WORKSPACE_MODULE_SYSTEM_BUNDLE_FILES,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_module_system_bundle_is_valid() {
        WORKSPACE_MODULE_SYSTEM_BUNDLE
            .validate()
            .expect("workspace-module-system bundle should be valid");
    }
}
