use agentdash_domain::canvas::CANVAS_SYSTEM_BUNDLE;
use agentdash_domain::embedded_skill::EmbeddedSkillBundle;

#[derive(Debug, Clone, Copy)]
pub struct BuiltinSkillAssetTemplate {
    pub builtin_key: &'static str,
    pub display_name: &'static str,
    pub bundle: &'static EmbeddedSkillBundle,
}

const BUILTIN_SKILL_TEMPLATES: &[BuiltinSkillAssetTemplate] = &[BuiltinSkillAssetTemplate {
    builtin_key: "canvas-system",
    display_name: "Canvas System",
    bundle: &CANVAS_SYSTEM_BUNDLE,
}];

pub fn list_builtin_skill_asset_templates() -> Vec<BuiltinSkillAssetTemplate> {
    BUILTIN_SKILL_TEMPLATES.to_vec()
}

pub fn get_builtin_skill_asset_template(key: &str) -> Option<BuiltinSkillAssetTemplate> {
    BUILTIN_SKILL_TEMPLATES
        .iter()
        .copied()
        .find(|template| template.builtin_key == key)
}
