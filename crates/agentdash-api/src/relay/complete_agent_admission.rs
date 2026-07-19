use std::{collections::BTreeMap, sync::Arc};

use agentdash_agent_runtime_host::{
    CompleteAgentVerificationMethod, CompleteAgentVerificationRecord,
};
use agentdash_agent_runtime_wire::{
    RuntimeWireAgentBindingTarget, RuntimeWireAuthenticatedTransport,
    RuntimeWirePlacementProvenance, RuntimeWireServiceOfferAdvertisement,
};
use agentdash_agent_service_api::{
    AgentBindingGeneration, AgentPayloadDigest, AgentProfileDigest, AgentServiceInstanceId,
};
use agentdash_application_agentrun::agent_run::ProductExecutionProfileRef;
use agentdash_infrastructure::{
    CompleteAgentComposition, CompleteAgentServiceSelectionCatalog,
    PinnedCompleteAgentVerificationCatalog,
};
use agentdash_integration_api::{
    CompleteAgentRegistrationClaim, CompleteAgentRemoteBindingMapping,
};
use agentdash_integration_remote_runtime::{
    RuntimeWirePlacement, remote_complete_agent_contribution,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::sync::Mutex;

use super::runtime_wire::{CloudRuntimeWireError, CloudRuntimeWirePlacementRegistry};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PinnedRuntimeWireDeployment {
    pub backend_id: String,
    pub endpoint_id: String,
    pub deployment_manifest_id: String,
    pub deployment_manifest_revision: String,
    pub remote_service_instance_id: String,
    pub publisher_integration: String,
    pub service_version: String,
    pub build_digest: String,
    pub service_profile_digest: String,
    pub conformance_suite_revision: String,
    pub verifier_identity: String,
    pub verifier_revision: String,
    pub evidence_digest: String,
    pub product_profiles: Vec<ProductExecutionProfileRef>,
}

#[derive(Default)]
pub struct PinnedRuntimeWireDeploymentCatalog {
    deployments: BTreeMap<(String, String), PinnedRuntimeWireDeployment>,
}

impl PinnedRuntimeWireDeploymentCatalog {
    pub fn from_json(raw: &str) -> Result<Self, RuntimeWireCompleteAgentAdmissionError> {
        let entries: Vec<PinnedRuntimeWireDeployment> =
            serde_json::from_str(raw).map_err(|error| {
                RuntimeWireCompleteAgentAdmissionError::TrustConfiguration {
                    reason: error.to_string(),
                }
            })?;
        let mut deployments = BTreeMap::new();
        for entry in entries {
            validate_deployment(&entry)?;
            let key = (entry.backend_id.clone(), entry.endpoint_id.clone());
            if deployments.insert(key.clone(), entry).is_some() {
                return Err(RuntimeWireCompleteAgentAdmissionError::TrustConfiguration {
                    reason: format!("duplicate Runtime Wire trust entry for {}/{}", key.0, key.1),
                });
            }
        }
        Ok(Self { deployments })
    }

    pub fn empty() -> Self {
        Self::default()
    }

    fn verify(
        &self,
        transport: &RuntimeWireAuthenticatedTransport,
        advertisement: &RuntimeWireServiceOfferAdvertisement,
    ) -> Result<&PinnedRuntimeWireDeployment, RuntimeWireCompleteAgentAdmissionError> {
        let deployment = self
            .deployments
            .get(&(
                transport.backend_id.clone(),
                advertisement.endpoint_id.clone(),
            ))
            .ok_or_else(
                || RuntimeWireCompleteAgentAdmissionError::UntrustedPlacement {
                    backend_id: transport.backend_id.clone(),
                    endpoint_id: advertisement.endpoint_id.clone(),
                },
            )?;
        let expected_instance =
            AgentServiceInstanceId::new(deployment.remote_service_instance_id.clone()).map_err(
                |error| RuntimeWireCompleteAgentAdmissionError::TrustConfiguration {
                    reason: error.to_string(),
                },
            )?;
        let expected_build =
            AgentPayloadDigest::new(deployment.build_digest.clone()).map_err(|error| {
                RuntimeWireCompleteAgentAdmissionError::TrustConfiguration {
                    reason: error.to_string(),
                }
            })?;
        let expected_profile = AgentProfileDigest::new(deployment.service_profile_digest.clone())
            .map_err(|error| {
            RuntimeWireCompleteAgentAdmissionError::TrustConfiguration {
                reason: error.to_string(),
            }
        })?;
        for (matches, coordinate) in [
            (
                deployment.deployment_manifest_id == advertisement.deployment_manifest_id,
                "deployment_manifest_id",
            ),
            (
                deployment.deployment_manifest_revision
                    == advertisement.deployment_manifest_revision,
                "deployment_manifest_revision",
            ),
            (
                expected_instance == advertisement.service_instance_id,
                "service_instance_id",
            ),
            (
                deployment.publisher_integration == advertisement.publisher_integration,
                "publisher_integration",
            ),
            (
                deployment.service_version == advertisement.service_version,
                "service_version",
            ),
            (
                expected_build == advertisement.claimed_build_digest,
                "claimed_build_digest",
            ),
            (
                expected_profile == advertisement.descriptor.profile_digest,
                "service_profile_digest",
            ),
            (
                deployment.conformance_suite_revision
                    == advertisement.claimed_conformance_suite_revision,
                "claimed_conformance_suite_revision",
            ),
        ] {
            if !matches {
                return Err(RuntimeWireCompleteAgentAdmissionError::AttestationDrift {
                    coordinate,
                });
            }
        }
        Ok(deployment)
    }
}

#[derive(Debug, Error)]
pub enum RuntimeWireCompleteAgentAdmissionError {
    #[error("Runtime Wire trust configuration is invalid: {reason}")]
    TrustConfiguration { reason: String },
    #[error("Runtime Wire placement is not pinned: {backend_id}/{endpoint_id}")]
    UntrustedPlacement {
        backend_id: String,
        endpoint_id: String,
    },
    #[error("Runtime Wire placement attestation drifted at {coordinate}")]
    AttestationDrift { coordinate: &'static str },
    #[error("Runtime Wire placement admission was superseded or withdrawn")]
    StaleAdmission,
    #[error(transparent)]
    Placement(#[from] CloudRuntimeWireError),
    #[error("Remote Complete Agent contribution is invalid: {reason}")]
    Contribution { reason: String },
    #[error("Remote Complete Agent registration failed: {reason}")]
    Registration { reason: String },
}

#[derive(Clone)]
struct ActiveRemotePlacement {
    local_instance_id: AgentServiceInstanceId,
    service_profile_digest: AgentProfileDigest,
    profiles: Vec<ProductExecutionProfileRef>,
    placement: Arc<dyn RuntimeWirePlacement>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DesiredRemotePlacement {
    transport_id: String,
    revision: u64,
    digest: AgentPayloadDigest,
}

#[derive(Default)]
struct RuntimeWireCompleteAgentAdmissionState {
    desired: BTreeMap<(String, String), DesiredRemotePlacement>,
    admitting: BTreeMap<(String, String), DesiredRemotePlacement>,
    active: BTreeMap<(String, String), ActiveRemotePlacement>,
}

impl RuntimeWireCompleteAgentAdmissionState {
    fn prepare(
        &mut self,
        key: &(String, String),
        desired: &DesiredRemotePlacement,
        local_instance_id: &AgentServiceInstanceId,
    ) -> Result<bool, RuntimeWireCompleteAgentAdmissionError> {
        if let Some(current) = self.desired.get(key)
            && (desired.revision < current.revision
                || (desired.revision == current.revision && desired.digest != current.digest)
                || (desired.revision == current.revision
                    && desired.transport_id != current.transport_id))
        {
            return Err(RuntimeWireCompleteAgentAdmissionError::StaleAdmission);
        }
        self.desired.insert(key.clone(), desired.clone());
        if self
            .active
            .get(key)
            .is_some_and(|active| active.local_instance_id == *local_instance_id)
            || self.admitting.get(key) == Some(desired)
        {
            return Ok(false);
        }
        self.admitting.insert(key.clone(), desired.clone());
        Ok(true)
    }

    fn finish_admission(&mut self, key: &(String, String), desired: &DesiredRemotePlacement) {
        if self.admitting.get(key) == Some(desired) {
            self.admitting.remove(key);
        }
    }

    fn withdraw(
        &mut self,
        key: &(String, String),
        revision: u64,
    ) -> Result<Option<ActiveRemotePlacement>, RuntimeWireCompleteAgentAdmissionError> {
        if self
            .desired
            .get(key)
            .is_some_and(|desired| revision < desired.revision)
        {
            return Err(RuntimeWireCompleteAgentAdmissionError::StaleAdmission);
        }
        self.desired.remove(key);
        self.admitting.remove(key);
        Ok(self.active.get(key).cloned())
    }
}

pub struct RuntimeWireCompleteAgentAdmissionTicket {
    key: (String, String),
    desired: DesiredRemotePlacement,
    transport: RuntimeWireAuthenticatedTransport,
    advertisement: RuntimeWireServiceOfferAdvertisement,
    trusted: PinnedRuntimeWireDeployment,
    local_instance_id: AgentServiceInstanceId,
}

pub struct RuntimeWireCompleteAgentAdmission {
    placements: Arc<CloudRuntimeWirePlacementRegistry>,
    complete_agent: Arc<CompleteAgentComposition>,
    verifier: Arc<PinnedCompleteAgentVerificationCatalog>,
    selections: Arc<CompleteAgentServiceSelectionCatalog>,
    trust: Arc<PinnedRuntimeWireDeploymentCatalog>,
    state: Mutex<RuntimeWireCompleteAgentAdmissionState>,
}

impl RuntimeWireCompleteAgentAdmission {
    pub fn new(
        placements: Arc<CloudRuntimeWirePlacementRegistry>,
        complete_agent: Arc<CompleteAgentComposition>,
        verifier: Arc<PinnedCompleteAgentVerificationCatalog>,
        selections: Arc<CompleteAgentServiceSelectionCatalog>,
        trust: Arc<PinnedRuntimeWireDeploymentCatalog>,
    ) -> Arc<Self> {
        Arc::new(Self {
            placements,
            complete_agent,
            verifier,
            selections,
            trust,
            state: Mutex::new(RuntimeWireCompleteAgentAdmissionState::default()),
        })
    }

    /// Records the desired placement epoch in relay receive order before asynchronous open begins.
    pub async fn prepare(
        &self,
        transport: RuntimeWireAuthenticatedTransport,
        advertisement: RuntimeWireServiceOfferAdvertisement,
    ) -> Result<
        Option<RuntimeWireCompleteAgentAdmissionTicket>,
        RuntimeWireCompleteAgentAdmissionError,
    > {
        let trusted = self.trust.verify(&transport, &advertisement)?.clone();
        let local_instance_id = local_instance_id(&transport, &advertisement)?;
        let key = (
            transport.backend_id.clone(),
            advertisement.endpoint_id.clone(),
        );
        let desired = DesiredRemotePlacement {
            transport_id: transport.transport_id.clone(),
            revision: advertisement.revision.0,
            digest: advertisement.digest.clone(),
        };
        if !self
            .state
            .lock()
            .await
            .prepare(&key, &desired, &local_instance_id)?
        {
            return Ok(None);
        }
        Ok(Some(RuntimeWireCompleteAgentAdmissionTicket {
            key,
            desired,
            transport,
            advertisement,
            trusted,
            local_instance_id,
        }))
    }

    /// Opens and admits one prepared epoch.
    ///
    /// The caller must run this outside the relay receive loop because placement opening waits for
    /// the peer's open acknowledgement, which is delivered by that same loop.
    pub async fn admit(
        &self,
        ticket: RuntimeWireCompleteAgentAdmissionTicket,
    ) -> Result<AgentServiceInstanceId, RuntimeWireCompleteAgentAdmissionError> {
        let key = ticket.key.clone();
        let desired = ticket.desired.clone();
        let result = self.admit_prepared(ticket).await;
        self.state.lock().await.finish_admission(&key, &desired);
        result
    }

    async fn admit_prepared(
        &self,
        ticket: RuntimeWireCompleteAgentAdmissionTicket,
    ) -> Result<AgentServiceInstanceId, RuntimeWireCompleteAgentAdmissionError> {
        self.ensure_desired(&ticket).await?;
        let RuntimeWireCompleteAgentAdmissionTicket {
            key,
            desired,
            transport,
            advertisement,
            trusted,
            local_instance_id,
        } = ticket;
        let provenance = RuntimeWirePlacementProvenance {
            transport: transport.clone(),
            endpoint_id: advertisement.endpoint_id.clone(),
            host_incarnation_id: advertisement.host_incarnation_id.clone(),
            service_instance_id: advertisement.service_instance_id.clone(),
            binding_generation: advertisement.binding_generation,
            advertisement_revision: advertisement.revision,
            advertisement_digest: advertisement.digest.clone(),
            profile_digest: advertisement.descriptor.profile_digest.clone(),
        };
        let placement = self
            .placements
            .open(
                provenance,
                super::runtime_wire::RUNTIME_WIRE_DEFAULT_MAX_IN_FLIGHT,
                chrono::Utc::now().timestamp_millis(),
            )
            .await?;
        if !self.is_desired(&key, &desired).await {
            placement
                .close("placement admission was superseded before registration")
                .await;
            return Err(RuntimeWireCompleteAgentAdmissionError::StaleAdmission);
        }
        let verification = CompleteAgentVerificationRecord {
            service_instance_id: local_instance_id.clone(),
            expected_publisher_integration: trusted.publisher_integration.clone(),
            expected_service_version: trusted.service_version.clone(),
            expected_build_digest: AgentPayloadDigest::new(trusted.build_digest.clone())
                .map_err(trust_error)?,
            expected_profile_digest: AgentProfileDigest::new(
                trusted.service_profile_digest.clone(),
            )
            .map_err(trust_error)?,
            expected_conformance_suite_revision: trusted.conformance_suite_revision.clone(),
            method: CompleteAgentVerificationMethod::RemoteTransportAttestation,
            verifier_identity: trusted.verifier_identity.clone(),
            verifier_revision: trusted.verifier_revision.clone(),
            evidence_digest: AgentPayloadDigest::new(trusted.evidence_digest.clone())
                .map_err(trust_error)?,
        };
        if let Err(error) = self.verifier.register_record(verification).await {
            placement
                .close("placement verification registration failed")
                .await;
            return Err(RuntimeWireCompleteAgentAdmissionError::Registration {
                reason: error.to_string(),
            });
        }
        let local_generation = AgentBindingGeneration(1);
        let mapping = CompleteAgentRemoteBindingMapping {
            local_service_instance_id: local_instance_id.clone(),
            local_binding_generation: local_generation,
            remote_service_instance_id: advertisement.service_instance_id.clone(),
            remote_binding_generation: advertisement.binding_generation,
        };
        let contribution = match remote_complete_agent_contribution(
            advertisement.descriptor.clone(),
            local_instance_id.clone(),
            transport.backend_id.clone(),
            transport.transport_id.clone(),
            CompleteAgentRegistrationClaim {
                publisher_integration: advertisement.publisher_integration.clone(),
                service_version: advertisement.service_version.clone(),
                claimed_service_build_digest: advertisement.claimed_build_digest.clone(),
                claimed_conformance_suite_revision: advertisement
                    .claimed_conformance_suite_revision
                    .clone(),
            },
            mapping,
            RuntimeWireAgentBindingTarget {
                service_instance_id: advertisement.service_instance_id.clone(),
                binding_generation: advertisement.binding_generation,
            },
            placement.clone(),
            self.complete_agent.host_callbacks(),
        ) {
            Ok(contribution) => contribution,
            Err(error) => {
                placement
                    .close("remote Complete Agent contribution was rejected")
                    .await;
                return Err(RuntimeWireCompleteAgentAdmissionError::Contribution {
                    reason: error.to_string(),
                });
            }
        };
        if let Err(error) = self
            .complete_agent
            .register_contribution(contribution)
            .await
        {
            placement
                .close("remote Complete Agent registration failed")
                .await;
            return Err(RuntimeWireCompleteAgentAdmissionError::Registration {
                reason: error.to_string(),
            });
        }

        let mut state = self.state.lock().await;
        if state.desired.get(&key) != Some(&desired) {
            drop(state);
            placement
                .close("placement admission was superseded before activation")
                .await;
            return Err(RuntimeWireCompleteAgentAdmissionError::StaleAdmission);
        }
        let previous = state.active.get(&key).cloned();
        if let Err(error) = self
            .selections
            .switch_placement(
                previous
                    .as_ref()
                    .map(|previous| (previous.profiles.as_slice(), &previous.local_instance_id)),
                &trusted.product_profiles,
                &local_instance_id,
            )
            .await
        {
            drop(state);
            placement
                .close("remote Complete Agent selection activation failed")
                .await;
            return Err(RuntimeWireCompleteAgentAdmissionError::Registration {
                reason: error.to_string(),
            });
        }
        state.active.insert(
            key,
            ActiveRemotePlacement {
                local_instance_id: local_instance_id.clone(),
                service_profile_digest: advertisement.descriptor.profile_digest.clone(),
                profiles: trusted.product_profiles,
                placement,
            },
        );
        drop(state);
        self.selections
            .activate_recovery_profile(
                advertisement.descriptor.profile_digest,
                local_instance_id.clone(),
            )
            .await;
        if let Some(previous) = previous {
            self.complete_agent
                .host
                .mark_service_bindings_lost(&previous.local_instance_id)
                .await
                .map_err(
                    |error| RuntimeWireCompleteAgentAdmissionError::Registration {
                        reason: error.to_string(),
                    },
                )?;
            previous
                .placement
                .close("remote Complete Agent placement was superseded")
                .await;
        }
        Ok(local_instance_id)
    }

    pub async fn withdraw(
        &self,
        backend_id: &str,
        endpoint_id: &str,
        revision: u64,
    ) -> Result<(), RuntimeWireCompleteAgentAdmissionError> {
        let key = (backend_id.to_owned(), endpoint_id.to_owned());
        let mut state = self.state.lock().await;
        let active = state.withdraw(&key, revision)?;
        if let Some(active) = active {
            self.selections
                .switch_placement(
                    Some((active.profiles.as_slice(), &active.local_instance_id)),
                    &[],
                    &active.local_instance_id,
                )
                .await
                .map_err(
                    |error| RuntimeWireCompleteAgentAdmissionError::Registration {
                        reason: error.to_string(),
                    },
                )?;
            state.active.remove(&key);
            drop(state);
            self.complete_agent
                .host
                .mark_service_bindings_lost(&active.local_instance_id)
                .await
                .map_err(
                    |error| RuntimeWireCompleteAgentAdmissionError::Registration {
                        reason: error.to_string(),
                    },
                )?;
            self.selections
                .deactivate_recovery_profile(
                    &active.service_profile_digest,
                    &active.local_instance_id,
                )
                .await;
            active
                .placement
                .close("remote Complete Agent endpoint was withdrawn")
                .await;
        }
        Ok(())
    }

    pub async fn disconnect_backend(
        &self,
        backend_id: &str,
    ) -> Result<(), RuntimeWireCompleteAgentAdmissionError> {
        let endpoints = self
            .state
            .lock()
            .await
            .desired
            .keys()
            .filter(|(candidate, _)| candidate == backend_id)
            .map(|(_, endpoint)| endpoint.clone())
            .collect::<Vec<_>>();
        for endpoint in endpoints {
            self.withdraw(backend_id, &endpoint, u64::MAX).await?;
        }
        Ok(())
    }

    async fn ensure_desired(
        &self,
        ticket: &RuntimeWireCompleteAgentAdmissionTicket,
    ) -> Result<(), RuntimeWireCompleteAgentAdmissionError> {
        if self.is_desired(&ticket.key, &ticket.desired).await {
            Ok(())
        } else {
            Err(RuntimeWireCompleteAgentAdmissionError::StaleAdmission)
        }
    }

    async fn is_desired(&self, key: &(String, String), desired: &DesiredRemotePlacement) -> bool {
        self.state.lock().await.desired.get(key) == Some(desired)
    }
}

fn validate_deployment(
    deployment: &PinnedRuntimeWireDeployment,
) -> Result<(), RuntimeWireCompleteAgentAdmissionError> {
    for (coordinate, value) in [
        ("backend_id", deployment.backend_id.as_str()),
        ("endpoint_id", deployment.endpoint_id.as_str()),
        (
            "deployment_manifest_id",
            deployment.deployment_manifest_id.as_str(),
        ),
        (
            "deployment_manifest_revision",
            deployment.deployment_manifest_revision.as_str(),
        ),
        (
            "remote_service_instance_id",
            deployment.remote_service_instance_id.as_str(),
        ),
        (
            "publisher_integration",
            deployment.publisher_integration.as_str(),
        ),
        ("service_version", deployment.service_version.as_str()),
        ("build_digest", deployment.build_digest.as_str()),
        (
            "service_profile_digest",
            deployment.service_profile_digest.as_str(),
        ),
        (
            "conformance_suite_revision",
            deployment.conformance_suite_revision.as_str(),
        ),
        ("verifier_identity", deployment.verifier_identity.as_str()),
        ("verifier_revision", deployment.verifier_revision.as_str()),
        ("evidence_digest", deployment.evidence_digest.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(RuntimeWireCompleteAgentAdmissionError::TrustConfiguration {
                reason: format!("{coordinate} cannot be empty"),
            });
        }
    }
    AgentServiceInstanceId::new(deployment.remote_service_instance_id.clone())
        .map_err(trust_error)?;
    AgentPayloadDigest::new(deployment.build_digest.clone()).map_err(trust_error)?;
    AgentProfileDigest::new(deployment.service_profile_digest.clone()).map_err(trust_error)?;
    AgentPayloadDigest::new(deployment.evidence_digest.clone()).map_err(trust_error)?;
    if deployment.product_profiles.is_empty()
        || deployment
            .product_profiles
            .iter()
            .any(|profile| !profile.validate())
    {
        return Err(RuntimeWireCompleteAgentAdmissionError::TrustConfiguration {
            reason: "remote deployment requires at least one valid Product profile".to_owned(),
        });
    }
    Ok(())
}

fn local_instance_id(
    transport: &RuntimeWireAuthenticatedTransport,
    advertisement: &RuntimeWireServiceOfferAdvertisement,
) -> Result<AgentServiceInstanceId, RuntimeWireCompleteAgentAdmissionError> {
    let mut hasher = Sha256::new();
    hasher.update(b"agentdash.remote-complete-agent-instance/v1\0");
    hasher.update(transport.backend_id.as_bytes());
    hasher.update(b"\0");
    hasher.update(advertisement.endpoint_id.as_bytes());
    hasher.update(b"\0");
    hasher.update(advertisement.digest.as_str().as_bytes());
    let digest = format!("{:x}", hasher.finalize());
    AgentServiceInstanceId::new(format!("remote.{}", &digest[..32])).map_err(trust_error)
}

fn trust_error(error: impl std::fmt::Display) -> RuntimeWireCompleteAgentAdmissionError {
    RuntimeWireCompleteAgentAdmissionError::TrustConfiguration {
        reason: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use agentdash_agent_runtime_wire::RuntimeWireAdvertisementRevision;

    use super::*;

    fn fixture() -> (
        PinnedRuntimeWireDeploymentCatalog,
        RuntimeWireAuthenticatedTransport,
        RuntimeWireServiceOfferAdvertisement,
    ) {
        let mut profile = ProductExecutionProfileRef {
            profile_key: "REMOTE_CODEX".to_owned(),
            profile_revision: 1,
            profile_digest: String::new(),
            configuration: serde_json::json!({"executor": "REMOTE_CODEX"}),
            credential_scope: None,
        };
        profile.refresh_digest();
        let descriptor = agentdash_integration_codex::codex_complete_agent_descriptor();
        let deployment = PinnedRuntimeWireDeployment {
            backend_id: "backend-a".to_owned(),
            endpoint_id: "codex".to_owned(),
            deployment_manifest_id: "manifest-a".to_owned(),
            deployment_manifest_revision: "7".to_owned(),
            remote_service_instance_id: "remote-codex".to_owned(),
            publisher_integration: "enterprise.codex".to_owned(),
            service_version: "1".to_owned(),
            build_digest: "sha256:build".to_owned(),
            service_profile_digest: descriptor.profile_digest.as_str().to_owned(),
            conformance_suite_revision: "suite-1".to_owned(),
            verifier_identity: "deployment-catalog".to_owned(),
            verifier_revision: "3".to_owned(),
            evidence_digest: "sha256:evidence".to_owned(),
            product_profiles: vec![profile],
        };
        let catalog = PinnedRuntimeWireDeploymentCatalog::from_json(
            &serde_json::to_string(&vec![deployment]).unwrap(),
        )
        .unwrap();
        let transport = RuntimeWireAuthenticatedTransport {
            backend_id: "backend-a".to_owned(),
            transport_id: "transport-a".to_owned(),
        };
        let mut advertisement = RuntimeWireServiceOfferAdvertisement {
            endpoint_id: "codex".to_owned(),
            revision: RuntimeWireAdvertisementRevision(1),
            digest: AgentPayloadDigest::new("placeholder").unwrap(),
            host_incarnation_id: "local-host-a".to_owned(),
            service_instance_id: AgentServiceInstanceId::new("remote-codex").unwrap(),
            binding_generation: AgentBindingGeneration(4),
            descriptor,
            publisher_integration: "enterprise.codex".to_owned(),
            service_version: "1".to_owned(),
            claimed_build_digest: AgentPayloadDigest::new("sha256:build").unwrap(),
            claimed_conformance_suite_revision: "suite-1".to_owned(),
            deployment_manifest_id: "manifest-a".to_owned(),
            deployment_manifest_revision: "7".to_owned(),
            advertised_at_unix_ms: 1,
            expires_at_unix_ms: i64::MAX,
            signed_deployment_evidence: None,
        };
        advertisement.digest = advertisement.calculated_digest();
        (catalog, transport, advertisement)
    }

    #[test]
    fn pinned_remote_deployment_matches_every_security_sensitive_coordinate() {
        let (catalog, transport, advertisement) = fixture();
        let verified = catalog.verify(&transport, &advertisement).unwrap();
        assert_eq!(verified.product_profiles[0].profile_key, "REMOTE_CODEX");

        let mut drifted = advertisement;
        drifted.claimed_build_digest = AgentPayloadDigest::new("sha256:drifted").unwrap();
        assert!(matches!(
            catalog.verify(&transport, &drifted),
            Err(RuntimeWireCompleteAgentAdmissionError::AttestationDrift {
                coordinate: "claimed_build_digest"
            })
        ));
    }

    #[test]
    fn desired_epoch_fences_replay_and_out_of_order_admission() {
        let key = ("backend-a".to_owned(), "codex".to_owned());
        let instance = AgentServiceInstanceId::new("remote.fixture").unwrap();
        let epoch_n = DesiredRemotePlacement {
            transport_id: "transport-a".to_owned(),
            revision: 1,
            digest: AgentPayloadDigest::new("sha256:epoch-n").unwrap(),
        };
        let epoch_n1 = DesiredRemotePlacement {
            transport_id: "transport-a".to_owned(),
            revision: 2,
            digest: AgentPayloadDigest::new("sha256:epoch-n1").unwrap(),
        };
        let mut state = RuntimeWireCompleteAgentAdmissionState::default();

        assert!(state.prepare(&key, &epoch_n, &instance).unwrap());
        assert!(!state.prepare(&key, &epoch_n, &instance).unwrap());
        assert!(state.prepare(&key, &epoch_n1, &instance).unwrap());
        assert_eq!(state.desired.get(&key), Some(&epoch_n1));
        assert_eq!(state.admitting.get(&key), Some(&epoch_n1));
        assert!(matches!(
            state.prepare(&key, &epoch_n, &instance),
            Err(RuntimeWireCompleteAgentAdmissionError::StaleAdmission)
        ));

        state.finish_admission(&key, &epoch_n);
        assert_eq!(state.admitting.get(&key), Some(&epoch_n1));
    }

    #[test]
    fn withdraw_tombstones_prepared_admission_before_async_open() {
        let key = ("backend-a".to_owned(), "codex".to_owned());
        let instance = AgentServiceInstanceId::new("remote.fixture").unwrap();
        let desired = DesiredRemotePlacement {
            transport_id: "transport-a".to_owned(),
            revision: 7,
            digest: AgentPayloadDigest::new("sha256:epoch").unwrap(),
        };
        let mut state = RuntimeWireCompleteAgentAdmissionState::default();
        assert!(state.prepare(&key, &desired, &instance).unwrap());

        assert!(state.withdraw(&key, 7).unwrap().is_none());
        assert!(!state.desired.contains_key(&key));
        assert!(!state.admitting.contains_key(&key));
    }
}
