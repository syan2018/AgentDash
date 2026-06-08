use crate::embedded_skill::{EmbeddedSkillBundle, EmbeddedSkillFile, EmbeddedSkillFileKind};

pub const COMPANION_SYSTEM_SKILL_NAME: &str = "companion-system";
pub const COMPANION_SYSTEM_SKILL_PATH: &str = "skills/companion-system/SKILL.md";

const COMPANION_SYSTEM_SKILL_CONTENT: &str = include_str!("skills/companion-system/SKILL.md");
const COMPANION_SYSTEM_PAYLOAD_ENVELOPE_REFERENCE_CONTENT: &str =
    include_str!("skills/companion-system/references/payload-envelope.md");
const COMPANION_SYSTEM_CAPABILITY_GRANT_REFERENCE_CONTENT: &str =
    include_str!("skills/companion-system/references/capability-grant-request.md");
const COMPANION_SYSTEM_HUMAN_INTERACTION_REFERENCE_CONTENT: &str =
    include_str!("skills/companion-system/references/human-interaction.md");
const COMPANION_SYSTEM_RESPONSE_ADOPTION_REFERENCE_CONTENT: &str =
    include_str!("skills/companion-system/references/response-adoption.md");
const COMPANION_SYSTEM_WORKFLOW_SCRIPT_PREFLIGHT_REFERENCE_CONTENT: &str =
    include_str!("skills/companion-system/references/workflow-script-preflight.md");

const COMPANION_SYSTEM_BUNDLE_FILES: &[EmbeddedSkillFile] = &[
    EmbeddedSkillFile {
        relative_path: "SKILL.md",
        content: COMPANION_SYSTEM_SKILL_CONTENT,
        kind: EmbeddedSkillFileKind::Skill,
    },
    EmbeddedSkillFile {
        relative_path: "references/payload-envelope.md",
        content: COMPANION_SYSTEM_PAYLOAD_ENVELOPE_REFERENCE_CONTENT,
        kind: EmbeddedSkillFileKind::Reference,
    },
    EmbeddedSkillFile {
        relative_path: "references/capability-grant-request.md",
        content: COMPANION_SYSTEM_CAPABILITY_GRANT_REFERENCE_CONTENT,
        kind: EmbeddedSkillFileKind::Reference,
    },
    EmbeddedSkillFile {
        relative_path: "references/human-interaction.md",
        content: COMPANION_SYSTEM_HUMAN_INTERACTION_REFERENCE_CONTENT,
        kind: EmbeddedSkillFileKind::Reference,
    },
    EmbeddedSkillFile {
        relative_path: "references/response-adoption.md",
        content: COMPANION_SYSTEM_RESPONSE_ADOPTION_REFERENCE_CONTENT,
        kind: EmbeddedSkillFileKind::Reference,
    },
    EmbeddedSkillFile {
        relative_path: "references/workflow-script-preflight.md",
        content: COMPANION_SYSTEM_WORKFLOW_SCRIPT_PREFLIGHT_REFERENCE_CONTENT,
        kind: EmbeddedSkillFileKind::Reference,
    },
];

pub const COMPANION_SYSTEM_BUNDLE: EmbeddedSkillBundle = EmbeddedSkillBundle {
    name: COMPANION_SYSTEM_SKILL_NAME,
    root_path: "skills/companion-system",
    entry_path: "SKILL.md",
    files: COMPANION_SYSTEM_BUNDLE_FILES,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn companion_system_bundle_is_valid() {
        COMPANION_SYSTEM_BUNDLE
            .validate()
            .expect("companion-system bundle should be valid");
        assert_eq!(COMPANION_SYSTEM_BUNDLE.files.len(), 6);
    }
}
