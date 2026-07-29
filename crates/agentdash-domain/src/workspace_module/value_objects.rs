use crate::embedded_skill::{EmbeddedSkillBundle, EmbeddedSkillFile, EmbeddedSkillFileKind};

pub const WORKSPACE_MODULE_SYSTEM_SKILL_NAME: &str = "workspace-module-system";
pub const WORKSPACE_MODULE_SYSTEM_SKILL_PATH: &str = "skills/workspace-module-system/SKILL.md";

const WORKSPACE_MODULE_SYSTEM_SKILL_CONTENT: &str =
    include_str!("skills/workspace-module-system/SKILL.md");
const WORKSPACE_MODULE_OPERATION_SCRIPTS_REFERENCE_CONTENT: &str =
    include_str!("skills/workspace-module-system/references/operation-scripts.md");
const WORKSPACE_MODULE_SURFACE_DIAGNOSTICS_REFERENCE_CONTENT: &str =
    include_str!("skills/workspace-module-system/references/surface-diagnostics.md");
const WORKSPACE_MODULE_SYSTEM_BUNDLE_FILES: &[EmbeddedSkillFile] = &[
    EmbeddedSkillFile {
        relative_path: "SKILL.md",
        content: WORKSPACE_MODULE_SYSTEM_SKILL_CONTENT,
        kind: EmbeddedSkillFileKind::Skill,
    },
    EmbeddedSkillFile {
        relative_path: "references/operation-scripts.md",
        content: WORKSPACE_MODULE_OPERATION_SCRIPTS_REFERENCE_CONTENT,
        kind: EmbeddedSkillFileKind::Reference,
    },
    EmbeddedSkillFile {
        relative_path: "references/surface-diagnostics.md",
        content: WORKSPACE_MODULE_SURFACE_DIAGNOSTICS_REFERENCE_CONTENT,
        kind: EmbeddedSkillFileKind::Reference,
    },
];

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
        assert_eq!(WORKSPACE_MODULE_SYSTEM_BUNDLE.files.len(), 3);
        assert!(
            WORKSPACE_MODULE_SYSTEM_SKILL_CONTENT
                .contains("workspace_module_invoke(module_id, operation_key, input)")
        );
        assert!(WORKSPACE_MODULE_SYSTEM_SKILL_CONTENT.contains("references/operation-scripts.md"));
        assert!(
            WORKSPACE_MODULE_OPERATION_SCRIPTS_REFERENCE_CONTENT
                .contains("ops.invoke_all([{operation, input}, ...])")
        );
        assert!(
            WORKSPACE_MODULE_OPERATION_SCRIPTS_REFERENCE_CONTENT
                .contains("namespace:provider_key:operation_key:v<contract_version>")
        );
    }
}
