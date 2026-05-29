use crate::embedded_skill::{EmbeddedSkillBundle, EmbeddedSkillFile, EmbeddedSkillFileKind};

pub const ROUTINE_MEMORY_SKILL_NAME: &str = "routine-memory";
pub const ROUTINE_MEMORY_SKILL_PATH: &str = "skills/routine-memory/SKILL.md";

const ROUTINE_MEMORY_SKILL_CONTENT: &str = include_str!("skills/routine-memory/SKILL.md");
const ROUTINE_MEMORY_MODEL_CONTENT: &str =
    include_str!("skills/routine-memory/references/memory-model.md");

const ROUTINE_MEMORY_BUNDLE_FILES: &[EmbeddedSkillFile] = &[
    EmbeddedSkillFile {
        relative_path: "SKILL.md",
        content: ROUTINE_MEMORY_SKILL_CONTENT,
        kind: EmbeddedSkillFileKind::Skill,
    },
    EmbeddedSkillFile {
        relative_path: "references/memory-model.md",
        content: ROUTINE_MEMORY_MODEL_CONTENT,
        kind: EmbeddedSkillFileKind::Reference,
    },
];

pub const ROUTINE_MEMORY_BUNDLE: EmbeddedSkillBundle = EmbeddedSkillBundle {
    name: ROUTINE_MEMORY_SKILL_NAME,
    root_path: "skills/routine-memory",
    entry_path: "SKILL.md",
    files: ROUTINE_MEMORY_BUNDLE_FILES,
};
