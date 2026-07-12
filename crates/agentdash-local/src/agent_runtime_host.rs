use std::{collections::BTreeMap, path::Path, sync::Arc};

use agentdash_agent_runtime_contract::{
    HostIncarnationId, RuntimeProfile, RuntimeServiceInstanceId,
};
use agentdash_agent_runtime_host::{
    ActivateAgentServiceInstance, AgentRuntimeHostRepository, AgentServiceDefinitionRegistry,
    ConformanceEvidence, EphemeralAgentRuntimeHostRepository, IntegrationDriverHost,
    PutAgentServiceInstance, ServiceInstanceDesiredState, TrustedDriverConformanceVerifier,
    TrustedDriverManifest, TrustedDriverManifestRegistry, profile_digest,
};
use agentdash_diagnostics::{Subsystem, diag};
use agentdash_integration_api::{
    AgentRuntimeCredentialBroker, AgentRuntimeCredentialRef, AgentRuntimeCredentialSlot,
    AgentRuntimeDriverContribution, AgentRuntimePlacement, CredentialLease, CredentialResolveError,
};
use agentdash_integration_remote_runtime::RuntimeWireHostPortRouter;
use agentdash_relay::RuntimeRelayTransportDescriptor;
use chrono::Utc;

use crate::handlers::{HostRuntimeDriverEndpointResolver, RuntimeWireCommandHandler};

const LOCAL_TRANSPORT_ID: &str = "agentdash.desktop.runtime-wire";

pub(crate) struct LocalAgentRuntimeHost {
    pub handler: Arc<RuntimeWireCommandHandler>,
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn bootstrap_local_agent_runtime_host(
    backend_id: &str,
    workspace_roots: &[std::path::PathBuf],
    artifact_root: &Path,
    configured_contributions: &[AgentRuntimeDriverContribution],
    configured_manifests: &[agentdash_integration_api::AgentRuntimeTrustManifest],
    configured_instances: &[crate::runtime::LocalAgentRuntimeInstanceConfig],
    credential_broker: Arc<dyn AgentRuntimeCredentialBroker>,
    activate_instances: bool,
) -> anyhow::Result<LocalAgentRuntimeHost> {
    let (contributions, integration_manifests) = if configured_contributions.is_empty() {
        let integrations = agentdash_first_party_integrations::builtin_integrations();
        (
            integrations
                .iter()
                .flat_map(|integration| integration.agent_runtime_drivers())
                .collect::<Vec<_>>(),
            integrations
                .iter()
                .flat_map(|integration| integration.agent_runtime_trust_manifests())
                .collect::<Vec<_>>(),
        )
    } else {
        (
            configured_contributions.to_vec(),
            configured_manifests.to_vec(),
        )
    };
    anyhow::ensure!(
        !contributions.is_empty(),
        "Local Runtime has no installed Agent Runtime Integration contributions"
    );
    let manifests = trusted_manifests(&contributions, integration_manifests)?;
    let transport_profile = runtime_wire_transport_profile(&contributions)?;
    let transport_profile_digest = profile_digest(&transport_profile)?;
    let registry = AgentServiceDefinitionRegistry::collect(contributions.clone())?;
    let repository = Arc::new(EphemeralAgentRuntimeHostRepository::new());
    let host_incarnation_id = HostIncarnationId::new(uuid::Uuid::new_v4().to_string())?;
    diag!(
        Info,
        Subsystem::Relay,
        backend_id = %backend_id,
        host_incarnation_id = %host_incarnation_id,
        "Local Agent Runtime Host incarnation initialized"
    );
    let host_port_router = Arc::new(RuntimeWireHostPortRouter::default());
    let host = Arc::new(IntegrationDriverHost::new(
        registry,
        repository.clone(),
        host_port_router.host_ports(credential_broker),
        Arc::new(TrustedDriverConformanceVerifier::new(
            TrustedDriverManifestRegistry::collect(manifests.clone())?,
        )),
        backend_id,
    ));
    for (contribution, manifest) in contributions.iter().zip(manifests) {
        if !activate_instances {
            continue;
        }
        let definition = &contribution.definition;
        let configured_instance = configured_instances
            .iter()
            .find(|instance| instance.definition_id == definition.provenance.definition_id);
        anyhow::ensure!(
            configured_instance.is_some()
                || definition
                    .credential_slots
                    .iter()
                    .all(|slot| !slot.required),
            "Local Agent service {} requires configured credential references",
            definition.provenance.definition_id
        );
        let instance_id = configured_instance
            .map(|instance| instance.instance_id.clone())
            .unwrap_or(RuntimeServiceInstanceId::new(format!(
                "local-{}-{}",
                stable_coordinate(backend_id),
                stable_coordinate(definition.provenance.definition_id.as_str())
            ))?);
        let expected_revision = repository
            .load_instance(&instance_id)
            .await?
            .map(|instance| instance.revision);
        let instance = host
            .put_instance(PutAgentServiceInstance {
                id: instance_id.clone(),
                definition_id: definition.provenance.definition_id.clone(),
                config: configured_instance
                    .map(|instance| instance.config.clone())
                    .unwrap_or(local_instance_config(
                        definition,
                        workspace_roots,
                        artifact_root,
                    )?),
                credentials: configured_instance
                    .map(|instance| instance.credential_refs.clone())
                    .unwrap_or_default(),
                placement: AgentRuntimePlacement::LocalProcess {
                    host_id: backend_id.to_string(),
                },
                desired_state: ServiceInstanceDesiredState::Active,
                expected_revision,
            })
            .await?;
        let offer = host
            .activate(ActivateAgentServiceInstance {
                instance_id,
                expected_revision: instance.revision,
                transport_profile: transport_profile.clone(),
                transport_profile_digest: transport_profile_digest.clone(),
                host_policy_profile: definition.service_profile_upper_bound.clone(),
                host_policy_digest: profile_digest(&definition.service_profile_upper_bound)?,
                conformance: ConformanceEvidence {
                    suite_revision: manifest.suite_revision,
                    driver_build_digest: manifest.driver_build_digest,
                    verified_profile_digest: manifest.verified_profile_digest,
                    verified_at: Utc::now(),
                },
            })
            .await?;
        diag!(
            Info,
            Subsystem::Relay,
            backend_id = %backend_id,
            host_incarnation_id = %host_incarnation_id,
            service_instance_id = %offer.service_instance_id,
            offer_generation = offer.generation.0,
            result = "advertised",
            "Local Agent Runtime offer activated"
        );
    }
    let transport_id = agentdash_integration_api::AgentRuntimePlacementId::new(LOCAL_TRANSPORT_ID)?;
    let resolver = Arc::new(HostRuntimeDriverEndpointResolver::new(
        host,
        backend_id,
        host_incarnation_id.clone(),
        transport_id,
    ));
    let handler = Arc::new(RuntimeWireCommandHandler::new_with_host_port_router(
        resolver,
        RuntimeRelayTransportDescriptor {
            supported_protocol_revisions: vec![
                agentdash_agent_runtime_wire::RUNTIME_WIRE_PROTOCOL_REVISION,
            ],
            profile: transport_profile,
            profile_digest: transport_profile_digest,
            max_in_flight_frames: 64,
        },
        host_port_router,
    ));
    Ok(LocalAgentRuntimeHost { handler })
}

fn trusted_manifests(
    contributions: &[AgentRuntimeDriverContribution],
    manifests: Vec<agentdash_integration_api::AgentRuntimeTrustManifest>,
) -> anyhow::Result<Vec<TrustedDriverManifest>> {
    let manifests = manifests
        .into_iter()
        .map(|manifest| (manifest.provenance.definition_id.clone(), manifest))
        .collect::<BTreeMap<_, _>>();
    contributions
        .iter()
        .map(|contribution| {
            let definition = &contribution.definition;
            let manifest = manifests
                .get(&definition.provenance.definition_id)
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "Agent service definition {} has no Integration trust manifest",
                        definition.provenance.definition_id
                    )
                })?;
            anyhow::ensure!(
                manifest.provenance == definition.provenance
                    && manifest.driver_build_digest == definition.provenance.build_digest.as_str()
                    && definition
                        .supported_protocol_revisions
                        .contains(&manifest.protocol_revision)
                    && manifest.verified_profile == definition.service_profile_upper_bound,
                "Agent Runtime Integration trust manifest does not match its definition"
            );
            Ok(TrustedDriverManifest {
                provenance: manifest.provenance.clone(),
                suite_revision: manifest.suite_revision.clone(),
                driver_build_digest: manifest.driver_build_digest.clone(),
                protocol_revision: manifest.protocol_revision,
                verified_profile_digest: profile_digest(&manifest.verified_profile)?,
            })
        })
        .collect()
}

fn runtime_wire_transport_profile(
    contributions: &[AgentRuntimeDriverContribution],
) -> anyhow::Result<RuntimeProfile> {
    let mut profile = contributions
        .first()
        .map(|item| item.definition.service_profile_upper_bound.clone())
        .ok_or_else(|| anyhow::anyhow!("Local Runtime has no transport profile source"))?;
    for contribution in &contributions[1..] {
        let other = &contribution.definition.service_profile_upper_bound;
        profile.reference_class = profile.reference_class.max(other.reference_class);
        profile.input.modalities.extend(&other.input.modalities);
        profile
            .instruction
            .channels
            .extend(&other.instruction.channels);
        profile.instruction.configuration_boundary = profile
            .instruction
            .configuration_boundary
            .max(other.instruction.configuration_boundary);
        profile.tools.channels.extend(&other.tools.channels);
        profile.tools.configuration_boundary = profile
            .tools
            .configuration_boundary
            .max(other.tools.configuration_boundary);
        profile.tools.cancellation |= other.tools.cancellation;
        profile
            .workspace
            .capabilities
            .extend(&other.workspace.capabilities);
        profile.workspace.mechanism = profile.workspace.mechanism.min(other.workspace.mechanism);
        profile.interactions.kinds.extend(&other.interactions.kinds);
        profile.interactions.durable_correlation |= other.interactions.durable_correlation;
        profile.lifecycle.extend(&other.lifecycle);
        profile.hooks.configuration_boundary = profile
            .hooks
            .configuration_boundary
            .max(other.hooks.configuration_boundary);
        for other_point in &other.hooks.points {
            if let Some(point) = profile
                .hooks
                .points
                .iter_mut()
                .find(|point| point.point == other_point.point)
            {
                point.actions.extend(&other_point.actions);
                point.strength = point.strength.max(other_point.strength);
                point.mechanism = point.mechanism.min(other_point.mechanism);
                point.failure_policies.extend(&other_point.failure_policies);
                point.acknowledged |= other_point.acknowledged;
            } else {
                profile.hooks.points.push(other_point.clone());
            }
        }
        profile.hooks.points.sort_by_key(|point| point.point);
        profile
            .context
            .capabilities
            .extend(&other.context.capabilities);
        profile.context.fidelity = profile.context.fidelity.max(other.context.fidelity);
        profile.context.activation_idempotent |= other.context.activation_idempotent;
        profile.telemetry_config.extend(&other.telemetry_config);
    }
    Ok(profile)
}

fn local_instance_config(
    definition: &agentdash_integration_api::AgentServiceDefinition,
    workspace_roots: &[std::path::PathBuf],
    artifact_root: &Path,
) -> anyhow::Result<serde_json::Value> {
    let mut config = serde_json::Map::new();
    let properties = definition
        .config_schema
        .get("properties")
        .and_then(serde_json::Value::as_object);
    let cwd = workspace_roots
        .first()
        .cloned()
        .unwrap_or(std::env::current_dir()?);
    if properties.is_some_and(|value| value.contains_key("cwd")) {
        config.insert("cwd".into(), serde_json::json!(cwd));
    }
    if properties.is_some_and(|value| value.contains_key("artifactRoot")) {
        config.insert("artifactRoot".into(), serde_json::json!(artifact_root));
    }
    if properties.is_some_and(|value| value.contains_key("runtimeWorkspaceRoots")) {
        config.insert(
            "runtimeWorkspaceRoots".into(),
            serde_json::json!(workspace_roots),
        );
    }
    Ok(serde_json::Value::Object(config))
}

fn stable_coordinate(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect()
}

pub(crate) struct LocalCredentialBroker;

#[async_trait::async_trait]
impl AgentRuntimeCredentialBroker for LocalCredentialBroker {
    async fn resolve(
        &self,
        slot: &AgentRuntimeCredentialSlot,
        _reference: &AgentRuntimeCredentialRef,
        _purpose: &str,
    ) -> Result<CredentialLease, CredentialResolveError> {
        Err(CredentialResolveError::Unavailable {
            slot: slot.clone(),
            reason: "Local service instance has no configured credential reference".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn ephemeral_host_activates_installed_offer_without_database() {
        let data_root = tempfile::tempdir().expect("temporary Local Runtime data root");
        let artifact_root = data_root.path().join("artifacts");
        std::fs::create_dir_all(&artifact_root).expect("artifact root");
        let host = bootstrap_local_agent_runtime_host(
            "desktop-test",
            &[data_root.path().to_path_buf()],
            &artifact_root,
            &[],
            &[],
            &[],
            Arc::new(LocalCredentialBroker),
            true,
        )
        .await
        .expect("Local Host bootstrap succeeds");

        let offers = host
            .handler
            .advertised_offers()
            .await
            .expect("offers are advertised");
        assert_eq!(offers.len(), 1);
        assert_eq!(offers[0].definition_id.as_str(), "builtin.codex-app-server");
        assert_eq!(offers[0].transport_id.as_str(), LOCAL_TRANSPORT_ID);
    }
}
