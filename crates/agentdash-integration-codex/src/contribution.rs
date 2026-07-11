use std::sync::{Arc, LazyLock};

use agentdash_agent_runtime_contract::RuntimeProfile;
use agentdash_integration_api::{
    AgentDashIntegration, AgentRuntimeDriverContribution, AgentRuntimeFactoryKey,
    AgentRuntimeTrustManifest, AgentServiceBuildDigest, AgentServiceDefinition,
    AgentServiceDefinitionId, AgentServiceProvenance, AgentServiceSchemaDigest,
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

    fn agent_runtime_trust_manifests(&self) -> Vec<AgentRuntimeTrustManifest> {
        vec![codex_runtime_trust_manifest()]
    }
}

pub fn codex_runtime_contribution() -> AgentRuntimeDriverContribution {
    codex_runtime_contribution_with_launcher(Arc::new(
        crate::driver::ProductionCodexAppServerLauncher,
    ))
}

pub fn codex_runtime_contribution_with_launcher(
    launcher: Arc<dyn crate::driver::CodexAppServerLauncher>,
) -> AgentRuntimeDriverContribution {
    let profile = codex_runtime_profile();
    AgentRuntimeDriverContribution {
        definition: definition(profile),
        factory: Arc::new(CodexRuntimeDriverFactory::with_launcher(
            FACTORY_KEY.clone(),
            launcher,
        )),
    }
}

pub fn codex_runtime_trust_manifest() -> AgentRuntimeTrustManifest {
    let contribution = codex_runtime_contribution();
    AgentRuntimeTrustManifest {
        provenance: contribution.definition.provenance.clone(),
        suite_revision: "codex-app-server-runtime-v1".to_string(),
        driver_build_digest: contribution.definition.provenance.build_digest.to_string(),
        protocol_revision: CODEX_PROTOCOL_REVISION,
        verified_profile: contribution.definition.service_profile_upper_bound,
    }
}

fn definition(profile: RuntimeProfile) -> AgentServiceDefinition {
    let config_schema = json!({
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
    });
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
        config_schema_digest: AgentServiceSchemaDigest::new(
            agentdash_integration_api::agent_service_schema_digest(&config_schema),
        )
        .expect("computed schema digest is valid"),
        config_schema,
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
        let manifest = codex_runtime_trust_manifest();
        assert_eq!(manifest.provenance, contribution.definition.provenance);
        assert_eq!(manifest.protocol_revision, CODEX_PROTOCOL_REVISION);
        assert_eq!(
            manifest.verified_profile,
            contribution.definition.service_profile_upper_bound
        );
    }
}
