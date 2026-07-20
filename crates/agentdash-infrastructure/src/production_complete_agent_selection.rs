use std::sync::Arc;

use agentdash_agent_runtime_host::{CompleteAgentAvailability, CompleteAgentLiveCatalog};
use agentdash_agent_service_api::{AgentPayloadDigest, AgentServiceInstanceId};
use agentdash_application_agentrun::agent_run::{
    AgentRunProductRuntimeProvisioningError, ProductCredentialScopeRef, ProductExecutionProfileRef,
};
use agentdash_domain::{
    common::{AgentConfig, ThinkingLevel},
    llm_provider::{LlmProviderCredentialRepository, LlmProviderRepository, LlmSecretCodec},
};
use agentdash_integration_native_agent::{
    bridge_dash_execution_dependencies, native_complete_agent_registration,
};
use agentdash_llm_provider::{
    ProviderCredentialScope, resolve_effective_bridge_with_model_for_scope,
};
use async_trait::async_trait;
use sha2::{Digest, Sha256};

use crate::{
    CompleteAgentComposition, CompleteAgentServiceSelectionCatalog, CompleteAgentServiceSelector,
    VerifiedCompleteAgentSelection, persistence::postgres::PostgresDashCompleteAgentStore,
};

const DASH_PROFILE_KEY: &str = "pi_agent";
const CODEX_PROFILE_KEY: &str = "codex";

/// Production selector for the three Complete Agent placement families.
///
/// Dash instances are materialized per immutable Product execution profile and credential scope.
/// Codex uses the pinned in-process instance and admits only configuration that the static service
/// can actually apply. Other keys are resolved from exact, independently registered placement
/// selections.
pub struct ProductionCompleteAgentServiceSelector {
    complete_agent: Arc<CompleteAgentComposition>,
    exact: Arc<CompleteAgentServiceSelectionCatalog>,
    codex_instance_id: AgentServiceInstanceId,
    dash_store: Arc<PostgresDashCompleteAgentStore>,
    provider_repository: Arc<dyn LlmProviderRepository>,
    credential_repository: Arc<dyn LlmProviderCredentialRepository>,
    secret_codec: Arc<dyn LlmSecretCodec>,
}

impl ProductionCompleteAgentServiceSelector {
    pub fn new(
        complete_agent: Arc<CompleteAgentComposition>,
        exact: Arc<CompleteAgentServiceSelectionCatalog>,
        codex_instance_id: AgentServiceInstanceId,
        dash_store: Arc<PostgresDashCompleteAgentStore>,
        provider_repository: Arc<dyn LlmProviderRepository>,
        credential_repository: Arc<dyn LlmProviderCredentialRepository>,
        secret_codec: Arc<dyn LlmSecretCodec>,
    ) -> Self {
        Self {
            complete_agent,
            exact,
            codex_instance_id,
            dash_store,
            provider_repository,
            credential_repository,
            secret_codec,
        }
    }

    async fn select_dash(
        &self,
        profile: &ProductExecutionProfileRef,
        config: AgentConfig,
    ) -> Result<VerifiedCompleteAgentSelection, AgentRunProductRuntimeProvisioningError> {
        if config
            .thinking_level
            .is_some_and(|level| level != ThinkingLevel::Off)
        {
            return Err(incompatible(
                "Dash Complete Agent does not yet expose a verified thinking-level configuration boundary",
            ));
        }
        let provider_id = config
            .provider_id
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| incompatible("PI_AGENT execution requires an explicit provider_id"))?;
        let model_id = config
            .model_id
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                incompatible(
                    "PI_AGENT execution requires an explicit model_id so the immutable profile \
                     identifies the actual provider model",
                )
            })?;
        let scope = credential_scope(profile.credential_scope.as_ref())?;
        let resolved = resolve_effective_bridge_with_model_for_scope(
            self.provider_repository.as_ref(),
            Some(self.credential_repository.as_ref()),
            self.secret_codec.as_ref(),
            &scope,
            provider_id,
            Some(model_id),
        )
        .await
        .map_err(
            |error| AgentRunProductRuntimeProvisioningError::Incompatible {
                reason: error.to_string(),
            },
        )?;
        if model_id != resolved.model.id {
            return Err(incompatible(format!(
                "resolved Dash model `{}` does not match requested model `{model_id}`",
                resolved.model.id
            )));
        }

        let instance_id = dash_instance_id(profile)?;
        let contribution = native_complete_agent_registration(
            instance_id.clone(),
            bridge_dash_execution_dependencies(resolved.bridge),
            self.complete_agent.host_callbacks(),
            self.dash_store.clone(),
        )
        .map_err(|error| failed(error.to_string()))?;
        let selection = self
            .complete_agent
            .register_contribution(contribution)
            .await
            .map_err(|error| failed(error.to_string()))?;
        Ok(VerifiedCompleteAgentSelection {
            target: selection.target,
            verified_product_profile_digest: profile.profile_digest.clone(),
        })
    }

    async fn select_codex(
        &self,
        profile: &ProductExecutionProfileRef,
        config: &AgentConfig,
    ) -> Result<VerifiedCompleteAgentSelection, AgentRunProductRuntimeProvisioningError> {
        if config.provider_id.is_some()
            || config.model_id.is_some()
            || config.agent_id.is_some()
            || config.thinking_level.is_some()
        {
            return Err(incompatible(
                "CODEX profile requests per-binding configuration that the pinned Codex service does not expose",
            ));
        }
        let availability = self
            .complete_agent
            .live_catalog
            .availability(&self.codex_instance_id)
            .await;
        let CompleteAgentAvailability::Available { attachment_id } = availability else {
            return Err(incompatible(
                availability
                    .unavailable_reason()
                    .unwrap_or("Codex Complete Agent 当前不可用"),
            ));
        };
        let selection = self
            .complete_agent
            .live_catalog
            .resolve(&attachment_id)
            .await
            .ok_or_else(|| incompatible("Codex live attachment 已退出当前进程"))?;
        Ok(VerifiedCompleteAgentSelection {
            target: selection.target,
            verified_product_profile_digest: profile.profile_digest.clone(),
        })
    }
}

#[async_trait]
impl CompleteAgentServiceSelector for ProductionCompleteAgentServiceSelector {
    async fn select(
        &self,
        profile: &ProductExecutionProfileRef,
    ) -> Result<VerifiedCompleteAgentSelection, AgentRunProductRuntimeProvisioningError> {
        if !profile.validate() {
            return Err(invalid(
                "execution profile digest does not cover its immutable configuration",
            ));
        }
        let config: AgentConfig =
            serde_json::from_value(profile.configuration.clone()).map_err(|error| {
                invalid(format!(
                    "execution profile configuration is invalid: {error}"
                ))
            })?;
        let normalized_key = profile.profile_key.trim().to_ascii_lowercase();
        if config.executor.trim().to_ascii_lowercase() != normalized_key {
            return Err(invalid(
                "execution profile key does not match configuration.executor",
            ));
        }
        match normalized_key.as_str() {
            DASH_PROFILE_KEY => self.select_dash(profile, config).await,
            CODEX_PROFILE_KEY => self.select_codex(profile, &config).await,
            _ => self.exact.select(profile).await,
        }
    }
}

fn credential_scope(
    scope: Option<&ProductCredentialScopeRef>,
) -> Result<ProviderCredentialScope, AgentRunProductRuntimeProvisioningError> {
    let Some(scope) = scope else {
        return Ok(ProviderCredentialScope::Platform);
    };
    match scope.owner_kind.trim().to_ascii_lowercase().as_str() {
        "platform" => Ok(ProviderCredentialScope::Platform),
        "user" => Ok(ProviderCredentialScope::User {
            user_id: scope.owner_id.trim().to_owned(),
        }),
        owner => Err(invalid(format!(
            "unsupported Product credential owner kind `{owner}`"
        ))),
    }
}

fn dash_instance_id(
    profile: &ProductExecutionProfileRef,
) -> Result<AgentServiceInstanceId, AgentRunProductRuntimeProvisioningError> {
    let mut hasher = Sha256::new();
    hasher.update(b"agentdash.dash-complete-agent-instance/v1\0");
    hasher.update(profile.profile_digest.as_bytes());
    if let Some(scope) = &profile.credential_scope {
        hasher.update(b"\0");
        hasher.update(scope.owner_kind.as_bytes());
        hasher.update(b"\0");
        hasher.update(scope.owner_id.as_bytes());
        hasher.update(b"\0");
        hasher.update(scope.credential_ref.as_bytes());
    }
    let digest = format!("{:x}", hasher.finalize());
    AgentServiceInstanceId::new(format!("builtin.dash-agent.{}", &digest[..32]))
        .map_err(|error| failed(error.to_string()))
}

pub fn dash_complete_agent_verification_template()
-> Result<crate::CompleteAgentVerificationTemplate, AgentRunProductRuntimeProvisioningError> {
    let descriptor = agentdash_integration_native_agent::DashAgentCompleteService::descriptor();
    Ok(crate::CompleteAgentVerificationTemplate {
        expected_publisher_integration: "builtin.dash_agent".to_owned(),
        expected_service_version: env!("CARGO_PKG_VERSION").to_owned(),
        expected_build_digest: AgentPayloadDigest::new(format!(
            "dash-complete-agent:{}",
            env!("CARGO_PKG_VERSION")
        ))
        .map_err(|error| failed(error.to_string()))?,
        expected_profile_digest: descriptor.profile_digest,
        expected_conformance_suite_revision: "dash-complete-agent-v1".to_owned(),
        method: agentdash_agent_runtime_host::CompleteAgentVerificationMethod::PinnedBuiltin,
        verifier_identity: "agentdash-api.builtin-catalog".to_owned(),
        verifier_revision: "complete-agent-v1".to_owned(),
        evidence_digest: AgentPayloadDigest::new(format!(
            "pinned-builtin:dash-complete-agent:{}:dash-complete-agent-v1",
            env!("CARGO_PKG_VERSION")
        ))
        .map_err(|error| failed(error.to_string()))?,
    })
}

fn invalid(reason: impl Into<String>) -> AgentRunProductRuntimeProvisioningError {
    AgentRunProductRuntimeProvisioningError::InvalidRequest {
        reason: reason.into(),
    }
}

fn incompatible(reason: impl Into<String>) -> AgentRunProductRuntimeProvisioningError {
    AgentRunProductRuntimeProvisioningError::Incompatible {
        reason: reason.into(),
    }
}

fn failed(reason: impl Into<String>) -> AgentRunProductRuntimeProvisioningError {
    AgentRunProductRuntimeProvisioningError::Failed {
        reason: reason.into(),
    }
}
