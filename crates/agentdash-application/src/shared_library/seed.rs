use serde_json::json;

use agentdash_domain::DomainError;
use agentdash_domain::embedded_skill::EmbeddedSkillFileKind;
use agentdash_domain::shared_library::{
    BuiltinSeed, LibraryAssetType, SkillTemplateFilePayload, SkillTemplatePayload, seed_digest,
};
use agentdash_domain::skill_asset::SkillAssetFileKind;

use crate::mcp_preset::list_builtin_mcp_preset_templates;
use crate::skill_asset::list_builtin_skill_asset_templates;
use crate::workflow::list_builtin_workflow_templates;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuiltinAssetVersion {
    pub asset_type: LibraryAssetType,
    pub key: &'static str,
    pub version: &'static str,
}

const BUILTIN_ASSET_VERSIONS: &[BuiltinAssetVersion] = &[
    BuiltinAssetVersion {
        asset_type: LibraryAssetType::AgentTemplate,
        key: "pi_agent_general",
        version: "1.0.0",
    },
    BuiltinAssetVersion {
        asset_type: LibraryAssetType::McpServerTemplate,
        key: "filesystem",
        version: "1.0.0",
    },
    BuiltinAssetVersion {
        asset_type: LibraryAssetType::McpServerTemplate,
        key: "fetch",
        version: "1.0.0",
    },
    BuiltinAssetVersion {
        asset_type: LibraryAssetType::WorkflowTemplate,
        key: "trellis_dag_task",
        version: "1.0.1",
    },
    BuiltinAssetVersion {
        asset_type: LibraryAssetType::WorkflowTemplate,
        key: "builtin_workflow_admin",
        version: "1.0.1",
    },
    BuiltinAssetVersion {
        asset_type: LibraryAssetType::SkillTemplate,
        key: "canvas-system",
        version: "1.0.4",
    },
    BuiltinAssetVersion {
        asset_type: LibraryAssetType::SkillTemplate,
        key: "companion-system",
        version: "1.0.0",
    },
    BuiltinAssetVersion {
        asset_type: LibraryAssetType::SkillTemplate,
        key: "routine-memory",
        version: "1.0.0",
    },
];

pub fn builtin_library_seeds() -> Result<Vec<BuiltinSeed>, DomainError> {
    let mut seeds = Vec::new();
    seeds.push(agent_template_seed()?);
    seeds.extend(mcp_server_template_seeds()?);
    seeds.extend(workflow_template_seeds()?);
    seeds.extend(skill_template_seeds()?);
    Ok(seeds)
}

pub fn builtin_source_ref(asset_type: LibraryAssetType, key: &str) -> String {
    format!("builtin:{}:{key}", asset_type.as_str())
}

fn builtin_asset_version(
    asset_type: LibraryAssetType,
    key: &str,
) -> Result<&'static str, DomainError> {
    BUILTIN_ASSET_VERSIONS
        .iter()
        .find(|item| item.asset_type == asset_type && item.key == key)
        .map(|item| item.version)
        .ok_or_else(|| {
            DomainError::InvalidConfig(format!(
                "builtin asset version 缺失: {}:{}",
                asset_type.as_str(),
                key
            ))
        })
}

fn agent_template_seed() -> Result<BuiltinSeed, DomainError> {
    let asset_type = LibraryAssetType::AgentTemplate;
    let key = "pi_agent_general";
    let payload = json!({
        "config": {
            "executor": "PI_AGENT",
            "system_prompt": "你是 AgentDash 内置通用 Agent，优先遵循当前 Project 的上下文与任务约束。",
            "system_prompt_mode": "append",
            "capability_directives": [
                { "add": "workflow_management" }
            ]
        }
    });
    Ok(BuiltinSeed {
        asset_type,
        key: key.to_string(),
        display_name: "Pi Agent General".to_string(),
        description: Some("平台内置通用 Agent 模板".to_string()),
        version: builtin_asset_version(asset_type, key)?.to_string(),
        source_ref: builtin_source_ref(asset_type, key),
        payload_digest: seed_digest(&payload)?,
        payload,
    })
}

fn mcp_server_template_seeds() -> Result<Vec<BuiltinSeed>, DomainError> {
    let templates = list_builtin_mcp_preset_templates()
        .map_err(|error| DomainError::InvalidConfig(error.to_string()))?;
    templates
        .into_iter()
        .map(|template| {
            let asset_type = LibraryAssetType::McpServerTemplate;
            let payload = json!({
                "transport": template.transport,
                "route_policy": template.route_policy,
                "capabilities": []
            });
            let version = builtin_asset_version(asset_type, &template.key)?.to_string();
            let source_ref = builtin_source_ref(asset_type, &template.key);
            Ok(BuiltinSeed {
                asset_type,
                key: template.key,
                display_name: template.display_name,
                description: template.description,
                version,
                source_ref,
                payload_digest: seed_digest(&payload)?,
                payload,
            })
        })
        .collect()
}

fn workflow_template_seeds() -> Result<Vec<BuiltinSeed>, DomainError> {
    let templates = list_builtin_workflow_templates().map_err(DomainError::InvalidConfig)?;
    templates
        .into_iter()
        .map(|template| {
            let asset_type = LibraryAssetType::WorkflowTemplate;
            let payload = json!({
                "template": template,
                "schema_version": "1.0.0"
            });
            let key = payload["template"]["key"]
                .as_str()
                .unwrap_or("workflow_template")
                .to_string();
            let version = builtin_asset_version(asset_type, &key)?.to_string();
            Ok(BuiltinSeed {
                asset_type,
                source_ref: builtin_source_ref(asset_type, &key),
                key,
                display_name: payload["template"]["name"]
                    .as_str()
                    .unwrap_or("Workflow Template")
                    .to_string(),
                description: payload["template"]["description"]
                    .as_str()
                    .map(ToString::to_string),
                version,
                payload_digest: seed_digest(&payload)?,
                payload,
            })
        })
        .collect()
}

fn skill_template_seeds() -> Result<Vec<BuiltinSeed>, DomainError> {
    list_builtin_skill_asset_templates()
        .into_iter()
        .map(|template| {
            let asset_type = LibraryAssetType::SkillTemplate;
            template
                .bundle
                .validate()
                .map_err(|error| DomainError::InvalidConfig(error.to_string()))?;
            let files = template
                .bundle
                .files
                .iter()
                .map(|file| SkillTemplateFilePayload {
                    path: file.relative_path.to_string(),
                    content: file.content.to_string(),
                    kind: embedded_skill_kind_to_asset_kind(file.kind),
                })
                .collect::<Vec<_>>();
            let payload = serde_json::to_value(SkillTemplatePayload {
                files,
                disable_model_invocation: false,
            })
            .map_err(DomainError::Serialization)?;

            Ok(BuiltinSeed {
                asset_type,
                key: template.builtin_key.to_string(),
                display_name: template.display_name.to_string(),
                description: Some(format!("内置 Skill 模板: {}", template.bundle.name)),
                version: builtin_asset_version(asset_type, template.builtin_key)?.to_string(),
                source_ref: builtin_source_ref(asset_type, template.builtin_key),
                payload_digest: seed_digest(&payload)?,
                payload,
            })
        })
        .collect()
}

fn embedded_skill_kind_to_asset_kind(kind: EmbeddedSkillFileKind) -> SkillAssetFileKind {
    match kind {
        EmbeddedSkillFileKind::Skill => SkillAssetFileKind::Skill,
        EmbeddedSkillFileKind::Reference => SkillAssetFileKind::Reference,
        EmbeddedSkillFileKind::Script => SkillAssetFileKind::Script,
        EmbeddedSkillFileKind::Asset => SkillAssetFileKind::Asset,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use super::*;

    #[test]
    fn builtin_library_seeds_cover_all_template_types() {
        let seeds = builtin_library_seeds().expect("load seeds");
        let types = seeds
            .iter()
            .map(|seed| seed.asset_type)
            .collect::<HashSet<_>>();

        assert!(types.contains(&LibraryAssetType::AgentTemplate));
        assert!(types.contains(&LibraryAssetType::McpServerTemplate));
        assert!(types.contains(&LibraryAssetType::WorkflowTemplate));
        assert!(types.contains(&LibraryAssetType::SkillTemplate));
        for seed in seeds {
            seed.validate().expect("builtin seed payload must validate");
            assert!(seed.payload_digest.starts_with("sha256:"));
            assert_eq!(
                seed.source_ref,
                builtin_source_ref(seed.asset_type, &seed.key)
            );
        }
    }

    #[test]
    fn builtin_asset_versions_cover_all_seeds() {
        let seeds = builtin_library_seeds().expect("load seeds");
        let versions = BUILTIN_ASSET_VERSIONS
            .iter()
            .map(|item| ((item.asset_type, item.key), item.version))
            .collect::<HashMap<_, _>>();

        for seed in &seeds {
            assert!(
                versions.contains_key(&(seed.asset_type, seed.key.as_str())),
                "builtin asset version manifest 缺少 {}:{}",
                seed.asset_type.as_str(),
                seed.key
            );
        }
        assert_eq!(
            versions.len(),
            seeds.len(),
            "builtin asset version manifest 不能包含未使用的资产版本"
        );
    }

    #[test]
    fn builtin_canvas_system_skill_template_uses_skill_asset_relative_paths() {
        let seed = builtin_library_seeds()
            .expect("load seeds")
            .into_iter()
            .find(|seed| {
                seed.asset_type == LibraryAssetType::SkillTemplate && seed.key == "canvas-system"
            })
            .expect("canvas-system skill template seed");
        let payload = serde_json::from_value::<SkillTemplatePayload>(seed.payload)
            .expect("skill template payload should be typed");

        assert!(payload.files.iter().any(|file| file.path == "SKILL.md"));
        assert!(
            payload
                .files
                .iter()
                .any(|file| file.path == "references/runtime-bridge.md")
        );
        assert!(
            payload
                .files
                .iter()
                .all(|file| !file.path.starts_with("skills/canvas-system/")),
            "SkillTemplate payload paths are SkillAsset-root relative; the skill_asset_fs provider adds skills/<key>/ during projection"
        );
    }
}
