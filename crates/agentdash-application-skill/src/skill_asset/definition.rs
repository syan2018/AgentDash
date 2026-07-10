use agentdash_domain::agent::MEMORY_MANAGER_BUNDLE;
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
    BuiltinSkillAssetTemplate {
        builtin_key: "memory-manager",
        display_name: "Memory Manager",
        bundle: &MEMORY_MANAGER_BUNDLE,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skill_asset::parse_skill_metadata;

    #[test]
    fn memory_manager_template_is_registered() {
        let template =
            get_builtin_skill_asset_template("memory-manager").expect("memory-manager template");

        assert_eq!(template.display_name, "Memory Manager");
        assert_eq!(template.bundle.name, "memory-manager");
        assert_eq!(template.bundle.entry_path, "SKILL.md");
        let skill = template
            .bundle
            .files
            .iter()
            .find(|file| file.relative_path == "SKILL.md")
            .expect("memory-manager SKILL.md");
        let meta = parse_skill_metadata(skill.content).expect("valid skill frontmatter");
        assert_eq!(meta.name, "memory-manager");
        assert!(meta.description.contains("agent://"));
        assert!(
            list_builtin_skill_asset_templates()
                .iter()
                .any(|template| template.builtin_key == "memory-manager")
        );
    }
}
