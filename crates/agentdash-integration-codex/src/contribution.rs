use std::sync::{Arc, LazyLock};

use agentdash_agent_runtime_contract::RuntimeProfile;
use agentdash_integration_api::{
    AgentDashIntegration, AgentRuntimeDriverContribution, AgentRuntimeFactoryKey,
    AgentServiceBuildDigest, AgentServiceDefinition, AgentServiceDefinitionId,
    AgentServiceProvenance, AgentServiceSchemaDigest,
};
use serde_json::json;

use crate::driver::{CodexRuntimeDriverFactory, codex_runtime_profile};

pub const CODEX_PROTOCOL_REVISION: u32 = 140;
pub const CODEX_APP_SERVER_PACKAGE: &str = "@openai/codex@0.140.0";

static FACTORY_KEY: LazyLock<AgentRuntimeFactoryKey> = LazyLock::new(|| {
    AgentRuntimeFactoryKey::new("builtin.codex-app-server")
        .expect("static Codex factory key is valid")
});

pub struct CodexRuntimeIntegration;

impl AgentDashIntegration for CodexRuntimeIntegration {
    fn name(&self) -> &str {
        "builtin.codex_runtime"
    }

    fn agent_runtime_drivers(&self) -> Vec<AgentRuntimeDriverContribution> {
        vec![codex_runtime_contribution()]
    }
}

pub fn codex_runtime_contribution() -> AgentRuntimeDriverContribution {
    let profile = codex_runtime_profile();
    AgentRuntimeDriverContribution {
        definition: definition(profile),
        factory: Arc::new(CodexRuntimeDriverFactory::new(FACTORY_KEY.clone())),
    }
}

fn definition(profile: RuntimeProfile) -> AgentServiceDefinition {
    AgentServiceDefinition {
        provenance: AgentServiceProvenance {
            definition_id: AgentServiceDefinitionId::new("builtin.codex-app-server")
                .expect("static definition id is valid"),
            publisher_integration: "builtin.codex_runtime".to_string(),
            service_version: CODEX_PROTOCOL_REVISION.to_string(),
            build_digest: AgentServiceBuildDigest::new(format!(
                "codex-app-server-rust-v{}",
                CODEX_PROTOCOL_REVISION
            ))
            .expect("static build digest is valid"),
        },
        factory_key: FACTORY_KEY.clone(),
        supported_protocol_revisions: vec![CODEX_PROTOCOL_REVISION],
        config_schema: json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "cwd": { "type": "string", "minLength": 1 },
                "model": { "type": "string", "minLength": 1 },
                "modelProvider": { "type": "string", "minLength": 1 },
                "baseInstructions": { "type": "string" },
                "developerInstructions": { "type": "string" },
                "runtimeWorkspaceRoots": { "type": "array", "items": { "type": "string", "minLength": 1 } },
                "artifactRoot": { "type": "string", "minLength": 1 }
            },
            "required": ["cwd", "artifactRoot"]
        }),
        config_schema_digest: AgentServiceSchemaDigest::new("sha256:codex-runtime-config-v1")
            .expect("static schema digest is valid"),
        credential_slots: Vec::new(),
        service_profile_upper_bound: profile,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contribution_revision_matches_spawned_package() {
        let contribution = codex_runtime_contribution();
        assert_eq!(
            contribution.definition.supported_protocol_revisions,
            vec![140]
        );
        assert!(CODEX_APP_SERVER_PACKAGE.ends_with("@0.140.0"));
        assert_eq!(
            contribution.definition.provenance.service_version,
            CODEX_PROTOCOL_REVISION.to_string()
        );
    }
}
