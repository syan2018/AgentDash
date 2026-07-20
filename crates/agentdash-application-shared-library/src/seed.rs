use serde_json::json;

use agentdash_application_workflow::BuiltinWorkflowTemplateBundle;
use agentdash_domain::DomainError;
use agentdash_domain::shared_library::{BuiltinSeed, LibraryAssetType, seed_digest};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuiltinAssetVersion {
    pub asset_type: LibraryAssetType,
    pub key: &'static str,
    pub version: &'static str,
}

const BUILTIN_ASSET_VERSIONS: &[BuiltinAssetVersion] = &[
    BuiltinAssetVersion {
        asset_type: LibraryAssetType::AgentTemplate,
        key: "general",
        version: "1.1.0",
    },
    BuiltinAssetVersion {
        asset_type: LibraryAssetType::WorkflowTemplate,
        key: "trellis_dag_task",
        version: "1.0.2",
    },
    BuiltinAssetVersion {
        asset_type: LibraryAssetType::WorkflowTemplate,
        key: "builtin_workflow_admin",
        version: "1.0.2",
    },
];

#[derive(Debug, Clone, Default)]
pub struct BuiltinLibrarySeedProviderInput {
    pub workflow_templates: Vec<WorkflowTemplateLibrarySeed>,
}

pub type WorkflowTemplateLibrarySeed = BuiltinWorkflowTemplateBundle;

pub fn builtin_library_seeds(
    input: BuiltinLibrarySeedProviderInput,
) -> Result<Vec<BuiltinSeed>, DomainError> {
    let mut seeds = Vec::new();
    seeds.push(agent_template_seed()?);
    seeds.extend(mcp_server_template_seeds()?);
    seeds.extend(workflow_template_seeds(input.workflow_templates)?);
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
    let key = "general";
    let payload = json!({
        "config": {
            "executor": "PI_AGENT",
            "system_prompt": "你是 Dash，AgentDash 平台的内置通用 Agent。优先遵循当前 Project 的上下文与任务约束；如未明确指定工作方式，则依据 base system prompt 的通用原则行事。有工具可用时积极使用工具完成任务，而非仅提供建议。",
            "capability_directives": [
                { "add": "workflow_management" }
            ]
        },
        "builtin": true
    });
    Ok(BuiltinSeed {
        asset_type,
        key: key.to_string(),
        display_name: "Dash".to_string(),
        description: Some("平台内置通用 Agent 模板".to_string()),
        version: builtin_asset_version(asset_type, key)?.to_string(),
        source_ref: builtin_source_ref(asset_type, key),
        payload_digest: seed_digest(&payload)?,
        payload,
    })
}

fn mcp_server_template_seeds() -> Result<Vec<BuiltinSeed>, DomainError> {
    Ok(vec![])
}

fn workflow_template_seeds(
    templates: Vec<WorkflowTemplateLibrarySeed>,
) -> Result<Vec<BuiltinSeed>, DomainError> {
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

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use super::*;

    #[test]
    fn builtin_library_seeds_cover_marketplace_template_types() {
        let seeds = builtin_library_seeds(test_seed_input()).expect("load seeds");
        let types = seeds
            .iter()
            .map(|seed| seed.asset_type)
            .collect::<HashSet<_>>();

        assert!(types.contains(&LibraryAssetType::AgentTemplate));
        assert!(types.contains(&LibraryAssetType::WorkflowTemplate));
        assert!(!types.contains(&LibraryAssetType::SkillTemplate));
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
        let seeds = builtin_library_seeds(test_seed_input()).expect("load seeds");
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

    fn test_seed_input() -> BuiltinLibrarySeedProviderInput {
        BuiltinLibrarySeedProviderInput {
            workflow_templates: vec![
                workflow_seed("trellis_dag_task", "Trellis DAG Task"),
                workflow_seed("builtin_workflow_admin", "Builtin Workflow Admin"),
            ],
        }
    }

    fn workflow_seed(key: &str, name: &str) -> WorkflowTemplateLibrarySeed {
        serde_json::from_value(json!({
            "key": key,
            "name": name,
            "description": "",
            "workflows": [],
            "graph": {
                "key": key,
                "name": name,
                "description": "",
                "entry_activity_key": "plan",
                "activities": [{
                    "key": "plan",
                    "executor": {
                        "kind": "human",
                        "type": "approval",
                        "form_schema_key": "approval"
                    },
                    "input_ports": [],
                    "output_ports": []
                }],
                "transitions": []
            }
        }))
        .expect("workflow seed")
    }
}
