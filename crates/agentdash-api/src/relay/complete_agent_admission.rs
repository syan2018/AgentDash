use std::{collections::BTreeMap, sync::Arc};

use agentdash_agent_runtime_contract::RuntimeThreadId;
use agentdash_agent_runtime_host::{
    CompleteAgentBindingTarget, CompleteAgentLiveCatalog, CompleteAgentVerificationMethod,
    CompleteAgentVerificationRecord,
};
use agentdash_agent_runtime_wire::{
    RuntimeWireAgentBindingTarget, RuntimeWireAuthenticatedTransport,
    RuntimeWirePlacementProvenance, RuntimeWireServiceOfferAdvertisement,
};
use agentdash_agent_service_api::{AgentPayloadDigest, AgentProfileDigest, AgentServiceInstanceId};
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
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::sync::{Mutex, RwLock};

use super::runtime_wire::{CloudRuntimeWireError, CloudRuntimeWirePlacementRegistry};

#[derive(Debug, Clone, Deserialize, Serialize)]
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
    #[error("Remote Complete Agent recovery failed: {reason}")]
    Recovery { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeWireCompleteAgentRecoveryRequest {
    pub runtime_thread_id: RuntimeThreadId,
    pub recovery_id: String,
}

#[async_trait]
pub trait RuntimeWireCompleteAgentRecoveryObserver: Send + Sync {
    async fn recover(&self, request: RuntimeWireCompleteAgentRecoveryRequest)
    -> Result<(), String>;
}

#[derive(Clone)]
struct ActiveRemotePlacement {
    local_instance_id: AgentServiceInstanceId,
    target: CompleteAgentBindingTarget,
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

#[derive(Clone)]
struct PendingRemoteRecovery {
    desired: DesiredRemotePlacement,
    previous: Option<ActiveRemotePlacement>,
    runtime_threads: Option<Vec<RuntimeThreadId>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RemotePlacementPreparation {
    Ignore,
    Admit,
    RetryRecovery,
}

#[derive(Default)]
struct RuntimeWireCompleteAgentAdmissionState {
    desired: BTreeMap<(String, String), DesiredRemotePlacement>,
    admitting: BTreeMap<(String, String), DesiredRemotePlacement>,
    active: BTreeMap<(String, String), ActiveRemotePlacement>,
    pending_recovery: BTreeMap<(String, String), PendingRemoteRecovery>,
}

impl RuntimeWireCompleteAgentAdmissionState {
    fn prepare(
        &mut self,
        key: &(String, String),
        desired: &DesiredRemotePlacement,
        local_instance_id: &AgentServiceInstanceId,
    ) -> Result<RemotePlacementPreparation, RuntimeWireCompleteAgentAdmissionError> {
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
        {
            return Ok(
                if self
                    .pending_recovery
                    .get(key)
                    .is_some_and(|pending| pending.desired == *desired)
                {
                    RemotePlacementPreparation::RetryRecovery
                } else {
                    RemotePlacementPreparation::Ignore
                },
            );
        }
        if self.admitting.get(key) == Some(desired) {
            return Ok(RemotePlacementPreparation::Ignore);
        }
        self.admitting.insert(key.clone(), desired.clone());
        Ok(RemotePlacementPreparation::Admit)
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
        self.pending_recovery.remove(key);
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
    recovery_only: bool,
}

pub struct RuntimeWireCompleteAgentAdmission {
    placements: Arc<CloudRuntimeWirePlacementRegistry>,
    complete_agent: Arc<CompleteAgentComposition>,
    verifier: Arc<PinnedCompleteAgentVerificationCatalog>,
    selections: Arc<CompleteAgentServiceSelectionCatalog>,
    trust: Arc<PinnedRuntimeWireDeploymentCatalog>,
    state: Mutex<RuntimeWireCompleteAgentAdmissionState>,
    recovery_observer: RwLock<Option<Arc<dyn RuntimeWireCompleteAgentRecoveryObserver>>>,
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
            recovery_observer: RwLock::new(None),
        })
    }

    pub async fn install_recovery_observer(
        &self,
        observer: Arc<dyn RuntimeWireCompleteAgentRecoveryObserver>,
    ) {
        *self.recovery_observer.write().await = Some(observer);
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
        let preparation = self
            .state
            .lock()
            .await
            .prepare(&key, &desired, &local_instance_id)?;
        if preparation == RemotePlacementPreparation::Ignore {
            return Ok(None);
        }
        Ok(Some(RuntimeWireCompleteAgentAdmissionTicket {
            key,
            desired,
            transport,
            advertisement,
            trusted,
            local_instance_id,
            recovery_only: preparation == RemotePlacementPreparation::RetryRecovery,
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
        if ticket.recovery_only {
            self.recover_pending_placement(&ticket.key, &ticket.desired)
                .await?;
            return Ok(ticket.local_instance_id);
        }
        let RuntimeWireCompleteAgentAdmissionTicket {
            key,
            desired,
            transport,
            advertisement,
            trusted,
            local_instance_id,
            recovery_only: _,
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
        let mapping = CompleteAgentRemoteBindingMapping {
            local_service_instance_id: local_instance_id.clone(),
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
        let selection = match self
            .complete_agent
            .register_contribution(contribution)
            .await
        {
            Ok(selection) => selection,
            Err(error) => {
                placement
                    .close("remote Complete Agent registration failed")
                    .await;
                return Err(RuntimeWireCompleteAgentAdmissionError::Registration {
                    reason: error.to_string(),
                });
            }
        };
        let target = selection.target;

        let mut state = self.state.lock().await;
        if state.desired.get(&key) != Some(&desired) {
            drop(state);
            self.complete_agent
                .live_catalog
                .retire(
                    &target.live_attachment_id,
                    "remote Complete Agent admission was superseded".to_owned(),
                )
                .await;
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
                    .map(|previous| (previous.profiles.as_slice(), &previous.target)),
                &trusted.product_profiles,
                &target,
            )
            .await
        {
            drop(state);
            self.complete_agent
                .live_catalog
                .retire(
                    &target.live_attachment_id,
                    "remote Complete Agent selection activation failed".to_owned(),
                )
                .await;
            placement
                .close("remote Complete Agent selection activation failed")
                .await;
            return Err(RuntimeWireCompleteAgentAdmissionError::Registration {
                reason: error.to_string(),
            });
        }
        let active = ActiveRemotePlacement {
            local_instance_id: local_instance_id.clone(),
            target: target.clone(),
            service_profile_digest: advertisement.descriptor.profile_digest.clone(),
            profiles: trusted.product_profiles,
            placement,
        };
        state.active.insert(key.clone(), active);
        state.pending_recovery.insert(
            key.clone(),
            PendingRemoteRecovery {
                desired: desired.clone(),
                previous,
                runtime_threads: None,
            },
        );
        drop(state);
        self.recover_pending_placement(&key, &desired).await?;
        Ok(local_instance_id)
    }

    async fn recover_pending_placement(
        &self,
        key: &(String, String),
        desired: &DesiredRemotePlacement,
    ) -> Result<(), RuntimeWireCompleteAgentAdmissionError> {
        let (active, pending) = {
            let state = self.state.lock().await;
            if state.desired.get(key) != Some(desired) {
                return Err(RuntimeWireCompleteAgentAdmissionError::StaleAdmission);
            }
            let active = state
                .active
                .get(key)
                .cloned()
                .ok_or(RuntimeWireCompleteAgentAdmissionError::StaleAdmission)?;
            let pending = state
                .pending_recovery
                .get(key)
                .filter(|pending| pending.desired == *desired)
                .cloned()
                .ok_or(RuntimeWireCompleteAgentAdmissionError::StaleAdmission)?;
            (active, pending)
        };
        if let Some(previous) = pending.previous {
            self.complete_agent
                .host
                .mark_target_bindings_lost(&previous.target)
                .await
                .map_err(|error| RuntimeWireCompleteAgentAdmissionError::Recovery {
                    reason: error.to_string(),
                })?;
            self.complete_agent
                .live_catalog
                .retire(
                    &previous.target.live_attachment_id,
                    "remote Complete Agent placement was replaced by a new connection epoch"
                        .to_owned(),
                )
                .await;
            previous
                .placement
                .close("remote Complete Agent placement was superseded")
                .await;
        }
        let runtime_threads = if let Some(runtime_threads) = pending.runtime_threads {
            runtime_threads
        } else {
            let runtime_threads = self
                .complete_agent
                .host
                .lost_runtime_threads_for_profile(&active.service_profile_digest)
                .await
                .map_err(|error| RuntimeWireCompleteAgentAdmissionError::Recovery {
                    reason: error.to_string(),
                })?;
            let mut state = self.state.lock().await;
            if state.desired.get(key) != Some(desired) {
                return Err(RuntimeWireCompleteAgentAdmissionError::StaleAdmission);
            }
            let pending = state
                .pending_recovery
                .get_mut(key)
                .filter(|pending| pending.desired == *desired)
                .ok_or(RuntimeWireCompleteAgentAdmissionError::StaleAdmission)?;
            pending.previous = None;
            pending.runtime_threads = Some(runtime_threads.clone());
            runtime_threads
        };
        if !runtime_threads.is_empty() {
            let observer = self.recovery_observer.read().await.clone().ok_or_else(|| {
                RuntimeWireCompleteAgentAdmissionError::Recovery {
                    reason: "Product Runtime recovery observer is not installed".to_owned(),
                }
            })?;
            for runtime_thread_id in runtime_threads {
                observer
                    .recover(RuntimeWireCompleteAgentRecoveryRequest {
                        recovery_id: remote_recovery_id(
                            key,
                            desired,
                            &active.target,
                            &runtime_thread_id,
                        ),
                        runtime_thread_id,
                    })
                    .await
                    .map_err(|reason| RuntimeWireCompleteAgentAdmissionError::Recovery {
                        reason,
                    })?;
            }
        }
        let mut state = self.state.lock().await;
        if state.desired.get(key) == Some(desired)
            && state
                .pending_recovery
                .get(key)
                .is_some_and(|pending| pending.desired == *desired)
        {
            state.pending_recovery.remove(key);
        }
        Ok(())
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
                    Some((active.profiles.as_slice(), &active.target)),
                    &[],
                    &active.target,
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
                .mark_target_bindings_lost(&active.target)
                .await
                .map_err(
                    |error| RuntimeWireCompleteAgentAdmissionError::Registration {
                        reason: error.to_string(),
                    },
                )?;
            self.complete_agent
                .live_catalog
                .retire(
                    &active.target.live_attachment_id,
                    "remote Complete Agent endpoint was withdrawn".to_owned(),
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

fn remote_recovery_id(
    key: &(String, String),
    desired: &DesiredRemotePlacement,
    target: &CompleteAgentBindingTarget,
    runtime_thread_id: &RuntimeThreadId,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"agentdash.remote-complete-agent-recovery/v1\0");
    hasher.update(key.0.as_bytes());
    hasher.update(b"\0");
    hasher.update(key.1.as_bytes());
    hasher.update(b"\0");
    hasher.update(desired.transport_id.as_bytes());
    hasher.update(b"\0");
    hasher.update(desired.revision.to_be_bytes());
    hasher.update(b"\0");
    hasher.update(desired.digest.as_str().as_bytes());
    hasher.update(b"\0");
    hasher.update(target.live_attachment_id.as_str().as_bytes());
    hasher.update(b"\0");
    hasher.update(target.host_incarnation_id().as_bytes());
    hasher.update(b"\0");
    hasher.update(runtime_thread_id.as_str().as_bytes());
    format!("remote-complete-agent-recovery:{:x}", hasher.finalize())
}

fn trust_error(error: impl std::fmt::Display) -> RuntimeWireCompleteAgentAdmissionError {
    RuntimeWireCompleteAgentAdmissionError::TrustConfiguration {
        reason: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use agentdash_agent_runtime_host::CompleteAgentPlacement;
    use agentdash_agent_runtime_wire::RuntimeWireAdvertisementRevision;
    use agentdash_agent_service_api::{
        AgentBindingGeneration, AgentServiceDefinitionId, CompleteAgentLiveAttachmentId,
    };

    use super::*;

    struct FixturePlacement;

    fn fixture_binding_target(instance_id: AgentServiceInstanceId) -> CompleteAgentBindingTarget {
        let profile_digest = AgentProfileDigest::new("sha256:profile").expect("profile");
        CompleteAgentBindingTarget {
            logical_instance_id: instance_id,
            live_attachment_id: CompleteAgentLiveAttachmentId::new("attachment")
                .expect("attachment"),
            definition_id: AgentServiceDefinitionId::new("definition").expect("definition"),
            verified_build_digest: AgentPayloadDigest::new("sha256:build").expect("build"),
            verified_profile_digest: profile_digest.clone(),
            offer_profile_digest: profile_digest,
            placement: CompleteAgentPlacement::InProcess {
                host_incarnation_id: "fixture-host".to_owned(),
            },
            remote_binding: None,
        }
    }

    #[async_trait]
    impl RuntimeWirePlacement for FixturePlacement {
        async fn send(
            &self,
            _frame: agentdash_agent_runtime_wire::RuntimeWireEnvelope,
        ) -> Result<(), agentdash_integration_remote_runtime::RemoteRuntimeTransportError> {
            unreachable!("state-only fixture never sends frames")
        }

        async fn receive(
            &self,
        ) -> Result<
            agentdash_integration_remote_runtime::RuntimeWirePlacementEvent,
            agentdash_integration_remote_runtime::RemoteRuntimeTransportError,
        > {
            unreachable!("state-only fixture never receives frames")
        }
    }

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

        assert_eq!(
            state.prepare(&key, &epoch_n, &instance).unwrap(),
            RemotePlacementPreparation::Admit
        );
        assert_eq!(
            state.prepare(&key, &epoch_n, &instance).unwrap(),
            RemotePlacementPreparation::Ignore
        );
        assert_eq!(
            state.prepare(&key, &epoch_n1, &instance).unwrap(),
            RemotePlacementPreparation::Admit
        );
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
        assert_eq!(
            state.prepare(&key, &desired, &instance).unwrap(),
            RemotePlacementPreparation::Admit
        );

        assert!(state.withdraw(&key, 7).unwrap().is_none());
        assert!(!state.desired.contains_key(&key));
        assert!(!state.admitting.contains_key(&key));
    }

    #[test]
    fn admitted_placement_replays_only_pending_product_recovery() {
        let key = ("backend-a".to_owned(), "codex".to_owned());
        let instance = AgentServiceInstanceId::new("remote.fixture").unwrap();
        let desired = DesiredRemotePlacement {
            transport_id: "transport-a".to_owned(),
            revision: 7,
            digest: AgentPayloadDigest::new("sha256:epoch").unwrap(),
        };
        let profile_digest = AgentProfileDigest::new("sha256:profile").unwrap();
        let active = ActiveRemotePlacement {
            local_instance_id: instance.clone(),
            target: fixture_binding_target(instance.clone()),
            service_profile_digest: profile_digest,
            profiles: Vec::new(),
            placement: Arc::new(FixturePlacement),
        };
        let mut state = RuntimeWireCompleteAgentAdmissionState::default();
        state.desired.insert(key.clone(), desired.clone());
        state.active.insert(key.clone(), active);
        state.pending_recovery.insert(
            key.clone(),
            PendingRemoteRecovery {
                desired: desired.clone(),
                previous: None,
                runtime_threads: Some(vec![
                    RuntimeThreadId::new("runtime-thread-a").expect("RuntimeThread"),
                ]),
            },
        );

        assert_eq!(
            state.prepare(&key, &desired, &instance).unwrap(),
            RemotePlacementPreparation::RetryRecovery
        );
        assert!(!state.admitting.contains_key(&key));
        assert_eq!(
            state
                .pending_recovery
                .get(&key)
                .and_then(|pending| pending.runtime_threads.as_ref())
                .map(Vec::as_slice),
            Some([RuntimeThreadId::new("runtime-thread-a").expect("RuntimeThread")].as_slice())
        );
    }

    #[test]
    fn recovery_identity_is_stable_per_placement_epoch_and_runtime_thread() {
        let key = ("backend-a".to_owned(), "codex".to_owned());
        let desired = DesiredRemotePlacement {
            transport_id: "transport-a".to_owned(),
            revision: 7,
            digest: AgentPayloadDigest::new("sha256:epoch").unwrap(),
        };
        let instance = AgentServiceInstanceId::new("remote.fixture").unwrap();
        let target = fixture_binding_target(instance);
        let thread = RuntimeThreadId::new("runtime-thread-a").unwrap();

        assert_eq!(
            remote_recovery_id(&key, &desired, &target, &thread),
            remote_recovery_id(&key, &desired, &target, &thread)
        );
        assert_ne!(
            remote_recovery_id(&key, &desired, &target, &thread),
            remote_recovery_id(
                &key,
                &DesiredRemotePlacement {
                    revision: 8,
                    ..desired
                },
                &target,
                &thread
            )
        );
    }
}
