use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use agentdash_domain::DomainError;
use agentdash_domain::embedded_skill::EmbeddedSkillFileKind;
use agentdash_domain::shared_library::{
    BuiltinSeed, LibraryAssetType, SkillTemplateFilePayload, SkillTemplatePayload,
};
use agentdash_domain::skill_asset::SkillAssetFileKind;

use crate::mcp_preset::list_builtin_mcp_preset_templates;
use crate::skill_asset::list_builtin_skill_asset_templates;
use crate::workflow::list_builtin_workflow_templates;

const BUILTIN_VERSION: &str = "1.0.0";

pub fn builtin_library_seeds() -> Result<Vec<BuiltinSeed>, DomainError> {
    let mut seeds = Vec::new();
    seeds.push(agent_template_seed()?);
    seeds.extend(mcp_server_template_seeds()?);
    seeds.extend(workflow_template_seeds()?);
    seeds.extend(skill_template_seeds()?);
    Ok(seeds)
}

pub fn seed_digest(payload: &Value) -> Result<String, DomainError> {
    let bytes = serde_json::to_vec(payload).map_err(DomainError::Serialization)?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(format!("sha256:{:x}", hasher.finalize()))
}

fn agent_template_seed() -> Result<BuiltinSeed, DomainError> {
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
        asset_type: LibraryAssetType::AgentTemplate,
        key: "pi_agent_general".to_string(),
        display_name: "Pi Agent General".to_string(),
        description: Some("平台内置通用 Agent 模板".to_string()),
        version: BUILTIN_VERSION.to_string(),
        payload_digest: seed_digest(&payload)?,
        payload,
    })
}

fn mcp_server_template_seeds() -> Result<Vec<BuiltinSeed>, DomainError> {
    let templates = list_builtin_mcp_preset_templates().map_err(DomainError::InvalidConfig)?;
    templates
        .into_iter()
        .map(|template| {
            let payload = json!({
                "transport": template.transport,
                "route_policy": template.route_policy,
                "capabilities": []
            });
            Ok(BuiltinSeed {
                asset_type: LibraryAssetType::McpServerTemplate,
                key: template.key,
                display_name: template.display_name,
                description: template.description,
                version: BUILTIN_VERSION.to_string(),
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
            let payload = json!({
                "template": template,
                "schema_version": BUILTIN_VERSION
            });
            Ok(BuiltinSeed {
                asset_type: LibraryAssetType::WorkflowTemplate,
                key: payload["template"]["key"]
                    .as_str()
                    .unwrap_or("workflow_template")
                    .to_string(),
                display_name: payload["template"]["name"]
                    .as_str()
                    .unwrap_or("Workflow Template")
                    .to_string(),
                description: payload["template"]["description"]
                    .as_str()
                    .map(ToString::to_string),
                version: BUILTIN_VERSION.to_string(),
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
            template
                .bundle
                .validate()
                .map_err(|error| DomainError::InvalidConfig(error.to_string()))?;
            let files = template
                .bundle
                .materialized_files()
                .into_iter()
                .map(|file| SkillTemplateFilePayload {
                    path: file.path,
                    content: file.content,
                    kind: embedded_skill_kind_to_asset_kind(file.kind),
                })
                .collect::<Vec<_>>();
            let payload = serde_json::to_value(SkillTemplatePayload {
                files,
                disable_model_invocation: false,
            })
            .map_err(DomainError::Serialization)?;

            Ok(BuiltinSeed {
                asset_type: LibraryAssetType::SkillTemplate,
                key: template.builtin_key.to_string(),
                display_name: template.display_name.to_string(),
                description: Some(format!("内置 Skill 模板: {}", template.bundle.name)),
                version: BUILTIN_VERSION.to_string(),
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
    use std::collections::HashSet;

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
        }
    }
}
