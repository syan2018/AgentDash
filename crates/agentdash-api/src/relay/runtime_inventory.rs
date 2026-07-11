use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use agentdash_agent_runtime_contract::{RuntimeServiceInstanceId, intersect_profile_layers};
use agentdash_agent_runtime_host::{
    ActivateAgentServiceInstance, AgentServiceDefinitionRegistry, ConformanceEvidence,
    IntegrationDriverHost, PutAgentServiceInstance, ServiceInstanceDesiredState, profile_digest,
};
use agentdash_integration_api::AgentRuntimePlacement;
use agentdash_relay::RuntimeOfferAdvertisement;
use tokio::sync::Mutex;

type ActiveRuntimeInstances = BTreeMap<String, BTreeMap<String, (RuntimeServiceInstanceId, u64)>>;

pub struct CloudRemoteRuntimeInventory {
    host: Arc<IntegrationDriverHost>,
    trusted_definitions: Arc<AgentServiceDefinitionRegistry>,
    mutation_lock: Mutex<()>,
    active_by_backend: Mutex<ActiveRuntimeInstances>,
    online_backends: Mutex<BTreeSet<String>>,
}

impl CloudRemoteRuntimeInventory {
    pub fn new(
        host: Arc<IntegrationDriverHost>,
        trusted_definitions: Arc<AgentServiceDefinitionRegistry>,
    ) -> Self {
        Self {
            host,
            trusted_definitions,
            mutation_lock: Mutex::new(()),
            active_by_backend: Mutex::new(BTreeMap::new()),
            online_backends: Mutex::new(BTreeSet::new()),
        }
    }

    pub fn validate_inventory(
        &self,
        advertisements: &[RuntimeOfferAdvertisement],
    ) -> anyhow::Result<()> {
        for advertisement in advertisements {
            self.validate(advertisement)?;
        }
        Ok(())
    }

    pub async fn mark_online(&self, backend_id: &str) {
        self.online_backends
            .lock()
            .await
            .insert(backend_id.to_string());
    }

    pub async fn sync(
        &self,
        backend_id: &str,
        advertisements: &[RuntimeOfferAdvertisement],
    ) -> anyhow::Result<()> {
        let _mutation_guard = self.mutation_lock.lock().await;
        self.validate_inventory(advertisements)?;
        let previous_snapshot = self
            .active_by_backend
            .lock()
            .await
            .get(backend_id)
            .cloned()
            .unwrap_or_default();
        let mut next = BTreeMap::new();
        let mut newly_activated = Vec::new();
        for advertisement in advertisements {
            let source_key = format!(
                "{}:{}:{}",
                advertisement.service_instance_id,
                advertisement.driver_generation.0,
                advertisement.profile_digest
            );
            if let Some(active) = previous_snapshot.get(&source_key) {
                next.insert(source_key, active.clone());
                continue;
            }
            let instance_id = RuntimeServiceInstanceId::new(format!(
                "remote-{}-{}-g{}",
                stable_coordinate(backend_id),
                stable_coordinate(advertisement.service_instance_id.as_str()),
                advertisement.driver_generation.0,
            ))?;
            let expected_revision = self
                .host
                .service_instance(&instance_id)
                .await?
                .map(|instance| instance.revision);
            let instance = match self
                .host
                .put_instance(PutAgentServiceInstance {
                    id: instance_id.clone(),
                    definition_id: advertisement.definition_id.clone(),
                    config: serde_json::json!({
                        "sourceServiceInstanceId": advertisement.service_instance_id,
                        "sourceDriverGeneration": advertisement.driver_generation.0,
                    }),
                    credentials: BTreeMap::new(),
                    placement: AgentRuntimePlacement::Remote {
                        host_id: backend_id.to_string(),
                        transport_id: advertisement.transport_id.clone(),
                    },
                    desired_state: ServiceInstanceDesiredState::Active,
                    expected_revision,
                })
                .await
            {
                Ok(instance) => instance,
                Err(error) => {
                    self.rollback_activations(&newly_activated).await;
                    return Err(error.into());
                }
            };
            let offer = match self
                .host
                .activate(ActivateAgentServiceInstance {
                    instance_id: instance_id.clone(),
                    expected_revision: instance.revision,
                    transport_profile: advertisement.effective_profile.profile.clone(),
                    transport_profile_digest: advertisement.profile_digest.clone(),
                    host_policy_profile: advertisement.effective_profile.profile.clone(),
                    host_policy_digest: advertisement.profile_digest.clone(),
                    conformance: ConformanceEvidence {
                        suite_revision: advertisement.conformance_suite_revision.clone(),
                        driver_build_digest: advertisement.conformance_driver_build_digest.clone(),
                        verified_profile_digest: advertisement
                            .conformance_verified_profile_digest
                            .clone(),
                        verified_at: advertisement.conformance_verified_at,
                    },
                })
                .await
            {
                Ok(offer) => offer,
                Err(error) => {
                    self.rollback_activations(&newly_activated).await;
                    return Err(error.into());
                }
            };
            newly_activated.push((instance_id.clone(), offer.instance_revision));
            next.insert(source_key, (instance_id, offer.instance_revision));
        }
        if !self.online_backends.lock().await.contains(backend_id) {
            for (_, (instance_id, revision)) in next {
                self.host.deactivate(&instance_id, revision).await?;
            }
            return Ok(());
        }
        let previous = self
            .active_by_backend
            .lock()
            .await
            .insert(backend_id.to_string(), next.clone())
            .unwrap_or_default();
        let retained = next.keys().cloned().collect::<BTreeSet<_>>();
        for (source_instance_id, (instance_id, revision)) in previous {
            if !retained.contains(&source_instance_id) {
                self.host.deactivate(&instance_id, revision).await?;
            }
        }
        Ok(())
    }

    pub async fn withdraw(&self, backend_id: &str) -> anyhow::Result<()> {
        let _mutation_guard = self.mutation_lock.lock().await;
        self.online_backends.lock().await.remove(backend_id);
        let active = self
            .active_by_backend
            .lock()
            .await
            .remove(backend_id)
            .unwrap_or_default();
        for (_, (instance_id, revision)) in active {
            self.host.deactivate(&instance_id, revision).await?;
        }
        Ok(())
    }

    fn validate(&self, advertisement: &RuntimeOfferAdvertisement) -> anyhow::Result<()> {
        validate_advertisement(self.trusted_definitions.as_ref(), advertisement)
    }

    async fn rollback_activations(&self, activations: &[(RuntimeServiceInstanceId, u64)]) {
        for (instance_id, revision) in activations.iter().rev() {
            let _ = self.host.deactivate(instance_id, *revision).await;
        }
    }
}

fn validate_advertisement(
    trusted_definitions: &AgentServiceDefinitionRegistry,
    advertisement: &RuntimeOfferAdvertisement,
) -> anyhow::Result<()> {
    let definition = trusted_definitions.definition(&advertisement.definition_id)?;
    anyhow::ensure!(
        definition.provenance.publisher_integration == advertisement.publisher_integration
            && definition.provenance.service_version == advertisement.service_version
            && definition.provenance.build_digest.as_str() == advertisement.build_digest,
        "remote Runtime offer provenance is not installed and trusted"
    );
    anyhow::ensure!(
        definition
            .supported_protocol_revisions
            .contains(&advertisement.protocol_revision),
        "remote Runtime offer protocol is outside the installed definition"
    );
    anyhow::ensure!(
        profile_digest(&advertisement.effective_profile.profile)? == advertisement.profile_digest,
        "remote Runtime offer profile digest is invalid"
    );
    let service_profile_digest = profile_digest(&definition.service_profile_upper_bound)?;
    anyhow::ensure!(
        advertisement.conformance_verified_profile_digest == service_profile_digest
            && advertisement.effective_profile.provenance.service_digest == service_profile_digest,
        "remote Runtime offer profile provenance is not covered by the installed definition"
    );
    let constrained = intersect_profile_layers(
        &definition.service_profile_upper_bound,
        &advertisement.effective_profile.profile,
        &advertisement.effective_profile.profile,
        advertisement.effective_profile.provenance.clone(),
    );
    anyhow::ensure!(
        constrained.profile == advertisement.effective_profile.profile,
        "remote Runtime offer exceeds the installed definition profile"
    );
    anyhow::ensure!(
        advertisement.conformance_driver_build_digest == advertisement.build_digest,
        "remote Runtime offer conformance does not cover the advertised build"
    );
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_agent_runtime_contract::{
        ProfileProvenance, RuntimeDriverGeneration, RuntimeServiceInstanceId,
        intersect_profile_layers,
    };
    use agentdash_integration_api::AgentRuntimePlacementId;
    use chrono::Utc;

    fn installed_codex() -> (
        AgentServiceDefinitionRegistry,
        agentdash_integration_api::AgentServiceDefinition,
        agentdash_integration_api::AgentRuntimeTrustManifest,
    ) {
        let integration = agentdash_first_party_integrations::builtin_integrations()
            .into_iter()
            .find(|integration| integration.name() == "builtin.codex_runtime")
            .expect("Codex Integration installed");
        let contribution = integration
            .agent_runtime_drivers()
            .pop()
            .expect("Codex contribution");
        let manifest = integration
            .agent_runtime_trust_manifests()
            .pop()
            .expect("Codex trust manifest");
        let definition = contribution.definition.clone();
        (
            AgentServiceDefinitionRegistry::collect(vec![contribution]).expect("trusted registry"),
            definition,
            manifest,
        )
    }

    fn advertisement() -> (AgentServiceDefinitionRegistry, RuntimeOfferAdvertisement) {
        let (registry, definition, manifest) = installed_codex();
        let digest =
            profile_digest(&definition.service_profile_upper_bound).expect("profile digest");
        let effective_profile = intersect_profile_layers(
            &definition.service_profile_upper_bound,
            &definition.service_profile_upper_bound,
            &definition.service_profile_upper_bound,
            ProfileProvenance {
                service_digest: digest.clone(),
                transport_digest: digest.clone(),
                host_policy_digest: digest.clone(),
            },
        );
        (
            registry,
            RuntimeOfferAdvertisement {
                definition_id: definition.provenance.definition_id,
                publisher_integration: definition.provenance.publisher_integration,
                service_version: definition.provenance.service_version,
                build_digest: definition.provenance.build_digest.to_string(),
                service_instance_id: RuntimeServiceInstanceId::new("local-codex")
                    .expect("instance id"),
                instance_revision: 1,
                driver_generation: RuntimeDriverGeneration(3),
                protocol_revision: manifest.protocol_revision,
                effective_profile,
                profile_digest: digest.clone(),
                conformance_suite_revision: manifest.suite_revision,
                conformance_driver_build_digest: manifest.driver_build_digest,
                conformance_verified_profile_digest: digest,
                conformance_verified_at: Utc::now(),
                transport_id: AgentRuntimePlacementId::new("desktop-runtime-wire")
                    .expect("transport id"),
            },
        )
    }

    #[test]
    fn installed_definition_accepts_exact_local_offer_and_rejects_forged_build() {
        let (registry, offer) = advertisement();
        validate_advertisement(&registry, &offer).expect("exact offer is trusted");

        let mut forged = offer;
        forged.build_digest = "forged-build".to_string();
        assert!(validate_advertisement(&registry, &forged).is_err());
    }
}
