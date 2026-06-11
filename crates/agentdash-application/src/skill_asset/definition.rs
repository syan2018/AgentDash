use agentdash_domain::canvas::CANVAS_SYSTEM_BUNDLE;
use agentdash_domain::companion::COMPANION_SYSTEM_BUNDLE;
use agentdash_domain::embedded_skill::EmbeddedSkillBundle;
use agentdash_domain::routine::ROUTINE_MEMORY_BUNDLE;
use agentdash_domain::workspace_module::WORKSPACE_MODULE_SYSTEM_BUNDLE;

#[derive(Debug, Clone, Copy)]
pub struct BuiltinSkillAssetTemplate {
    pub builtin_key: &'static str,
    pub display_name: &'static str,
    pub bundle: &'static EmbeddedSkillBundle,
}

const BUILTIN_SKILL_TEMPLATES: &[BuiltinSkillAssetTemplate] = &[
    BuiltinSkillAssetTemplate {
        builtin_key: "canvas-system",
        display_name: "Canvas System",
        bundle: &CANVAS_SYSTEM_BUNDLE,
    },
    BuiltinSkillAssetTemplate {
        builtin_key: "workspace-module-system",
        display_name: "Workspace Module System",
        bundle: &WORKSPACE_MODULE_SYSTEM_BUNDLE,
    },
    BuiltinSkillAssetTemplate {
        builtin_key: "companion-system",
        display_name: "Companion System",
        bundle: &COMPANION_SYSTEM_BUNDLE,
    },
    BuiltinSkillAssetTemplate {
        builtin_key: "routine-memory",
        display_name: "Routine Memory",
        bundle: &ROUTINE_MEMORY_BUNDLE,
    },
];

pub fn list_builtin_skill_asset_templates() -> Vec<BuiltinSkillAssetTemplate> {
    BUILTIN_SKILL_TEMPLATES.to_vec()
}

pub fn get_builtin_skill_asset_template(key: &str) -> Option<BuiltinSkillAssetTemplate> {
    BUILTIN_SKILL_TEMPLATES
        .iter()
        .copied()
        .find(|template| template.builtin_key == key)
}
