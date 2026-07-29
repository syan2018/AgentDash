use crate::embedded_skill::{EmbeddedSkillBundle, EmbeddedSkillFile, EmbeddedSkillFileKind};

pub const CANVAS_SYSTEM_SKILL_NAME: &str = "canvas-system";
pub const CANVAS_SYSTEM_SKILL_PATH: &str = "skills/canvas-system/SKILL.md";

const CANVAS_SYSTEM_SKILL_CONTENT: &str = include_str!("skills/canvas-system/SKILL.md");
const CANVAS_SYSTEM_AUTHORING_REFERENCE_CONTENT: &str =
    include_str!("skills/canvas-system/references/authoring.md");
const CANVAS_SYSTEM_INTERACTION_RUNTIME_REFERENCE_CONTENT: &str =
    include_str!("skills/canvas-system/references/interaction-runtime.md");
const CANVAS_SYSTEM_PRESENTATION_QUALITY_REFERENCE_CONTENT: &str =
    include_str!("skills/canvas-system/references/presentation-quality.md");
const CANVAS_SYSTEM_RUNTIME_BRIDGE_REFERENCE_CONTENT: &str =
    include_str!("skills/canvas-system/references/runtime-bridge.md");
const CANVAS_SYSTEM_RUNTIME_ACTIONS_REFERENCE_CONTENT: &str =
    include_str!("skills/canvas-system/references/runtime-actions.md");
const CANVAS_SYSTEM_VFS_ASSETS_REFERENCE_CONTENT: &str =
    include_str!("skills/canvas-system/references/vfs-assets.md");
const CANVAS_SYSTEM_INTERACTION_STATE_REFERENCE_CONTENT: &str =
    include_str!("skills/canvas-system/references/interaction-state.md");
const CANVAS_SYSTEM_AGENT_SUBMIT_REFERENCE_CONTENT: &str =
    include_str!("skills/canvas-system/references/agent-submit.md");
const CANVAS_SYSTEM_AGENT_SIDE_INTERFACES_REFERENCE_CONTENT: &str =
    include_str!("skills/canvas-system/references/agent-side-interfaces.md");

const CANVAS_SYSTEM_BUNDLE_FILES: &[EmbeddedSkillFile] = &[
    EmbeddedSkillFile {
        relative_path: "SKILL.md",
        content: CANVAS_SYSTEM_SKILL_CONTENT,
        kind: EmbeddedSkillFileKind::Skill,
    },
    EmbeddedSkillFile {
        relative_path: "references/authoring.md",
        content: CANVAS_SYSTEM_AUTHORING_REFERENCE_CONTENT,
        kind: EmbeddedSkillFileKind::Reference,
    },
    EmbeddedSkillFile {
        relative_path: "references/interaction-runtime.md",
        content: CANVAS_SYSTEM_INTERACTION_RUNTIME_REFERENCE_CONTENT,
        kind: EmbeddedSkillFileKind::Reference,
    },
    EmbeddedSkillFile {
        relative_path: "references/presentation-quality.md",
        content: CANVAS_SYSTEM_PRESENTATION_QUALITY_REFERENCE_CONTENT,
        kind: EmbeddedSkillFileKind::Reference,
    },
    EmbeddedSkillFile {
        relative_path: "references/runtime-bridge.md",
        content: CANVAS_SYSTEM_RUNTIME_BRIDGE_REFERENCE_CONTENT,
        kind: EmbeddedSkillFileKind::Reference,
    },
    EmbeddedSkillFile {
        relative_path: "references/runtime-actions.md",
        content: CANVAS_SYSTEM_RUNTIME_ACTIONS_REFERENCE_CONTENT,
        kind: EmbeddedSkillFileKind::Reference,
    },
    EmbeddedSkillFile {
        relative_path: "references/vfs-assets.md",
        content: CANVAS_SYSTEM_VFS_ASSETS_REFERENCE_CONTENT,
        kind: EmbeddedSkillFileKind::Reference,
    },
    EmbeddedSkillFile {
        relative_path: "references/interaction-state.md",
        content: CANVAS_SYSTEM_INTERACTION_STATE_REFERENCE_CONTENT,
        kind: EmbeddedSkillFileKind::Reference,
    },
    EmbeddedSkillFile {
        relative_path: "references/agent-submit.md",
        content: CANVAS_SYSTEM_AGENT_SUBMIT_REFERENCE_CONTENT,
        kind: EmbeddedSkillFileKind::Reference,
    },
    EmbeddedSkillFile {
        relative_path: "references/agent-side-interfaces.md",
        content: CANVAS_SYSTEM_AGENT_SIDE_INTERFACES_REFERENCE_CONTENT,
        kind: EmbeddedSkillFileKind::Reference,
    },
];

pub const CANVAS_SYSTEM_BUNDLE: EmbeddedSkillBundle = EmbeddedSkillBundle {
    name: CANVAS_SYSTEM_SKILL_NAME,
    root_path: "skills/canvas-system",
    entry_path: "SKILL.md",
    files: CANVAS_SYSTEM_BUNDLE_FILES,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canvas_system_bundle_is_valid_and_complete() {
        CANVAS_SYSTEM_BUNDLE
            .validate()
            .expect("canvas-system bundle should be valid");
        assert_eq!(CANVAS_SYSTEM_BUNDLE.files.len(), 10);
    }
}
