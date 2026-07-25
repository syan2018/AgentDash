use std::sync::Arc;

use agentdash_agent_service_api::{
    AgentBindingGeneration, AgentPayloadDigest, AgentServiceDescriptor, AgentServiceError,
    AgentServiceInstanceId, CompleteAgentService,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Non-authoritative build and conformance claims declared by one Integration.
///
/// W8 passes these claims to a Host-owned verifier. Only the verifier may produce verified profile
/// or build evidence; an Integration cannot attest its own trust, credentials, health, placement
/// transport, or runtime offer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CompleteAgentRegistrationClaim {
    pub publisher_integration: String,
    pub service_version: String,
    pub claimed_service_build_digest: AgentPayloadDigest,
    pub claimed_conformance_suite_revision: String,
}

/// Platform-neutral placement requested by an Integration contribution.
///
/// The composition root adds Host-owned incarnation and transport evidence before constructing
/// the final Host placement. Relay remains transport and never becomes the service identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CompleteAgentPlacementRequirement {
    InProcess,
    LocalProcess {
        host_id: String,
    },
    Remote {
        host_id: String,
        transport_id: String,
    },
}

/// Immutable identity and generation rewrite fact for one remote Complete Agent binding.
///
/// The local service identity is the Host-facing logical key. The remote identity/generation is
/// the Runtime Wire attachment target. Per-thread Host binding generations remain separate and
/// are mapped by the exact callback route installed during surface application.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CompleteAgentRemoteBindingMapping {
    pub local_service_instance_id: AgentServiceInstanceId,
    pub remote_service_instance_id: AgentServiceInstanceId,
    pub remote_binding_generation: AgentBindingGeneration,
}

impl CompleteAgentRemoteBindingMapping {
    fn validate(
        &self,
        contribution_instance_id: &AgentServiceInstanceId,
    ) -> Result<(), CompleteAgentContributionError> {
        if &self.local_service_instance_id != contribution_instance_id {
            return Err(CompleteAgentContributionError::RemoteBindingMismatch {
                coordinate: "local_service_instance_id".to_owned(),
                expected: contribution_instance_id.to_string(),
                actual: self.local_service_instance_id.to_string(),
            });
        }
        if self.remote_binding_generation.0 == 0 {
            return Err(CompleteAgentContributionError::InvalidRegistration {
                reason: "remote binding generations must be non-zero".to_owned(),
            });
        }
        Ok(())
    }
}

impl CompleteAgentPlacementRequirement {
    pub fn validate(&self) -> Result<(), CompleteAgentContributionError> {
        let valid = match self {
            Self::InProcess => true,
            Self::LocalProcess { host_id } => !host_id.trim().is_empty(),
            Self::Remote {
                host_id,
                transport_id,
            } => !host_id.trim().is_empty() && !transport_id.trim().is_empty(),
        };
        if valid {
            Ok(())
        } else {
            Err(CompleteAgentContributionError::InvalidRegistration {
                reason: "Complete Agent placement coordinates must not be empty".to_owned(),
            })
        }
    }
}

/// Typed factory failure before a Complete Agent service can enter Host registration.
///
/// Credential and health failures are explicit. There is no implicit healthy/credentialed state,
/// trust flag, fallback service, or default-success factory path.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CompleteAgentServiceFactoryError {
    #[error("Complete Agent configuration is invalid: {reason}")]
    InvalidConfiguration { reason: String },
    #[error("Complete Agent credential is unavailable: {reason}")]
    CredentialUnavailable { reason: String },
    #[error("Complete Agent service is unhealthy: {reason}")]
    Unhealthy { reason: String, retryable: bool },
    #[error("Complete Agent service is unavailable: {reason}")]
    Unavailable { reason: String, retryable: bool },
}

#[async_trait]
pub trait CompleteAgentServiceFactory: Send + Sync {
    async fn materialize(
        &self,
    ) -> Result<Arc<dyn CompleteAgentService>, CompleteAgentServiceFactoryError>;
}

/// Dependency-light declared contribution collected from [`crate::AgentDashIntegration`].
///
/// `declared_descriptor` and `registration_claim` are Integration claims, not verified Host facts.
/// Host definition, health, credential, verifier and offer facts are deliberately absent: W8
/// verifies this input, materializes the service, and then calls the final Host registration
/// boundary.
#[derive(Clone)]
pub struct CompleteAgentRegistrationContribution {
    facts: CompleteAgentRegistrationFacts,
    factory: Arc<dyn CompleteAgentServiceFactory>,
}

/// Immutable declared facts preserved from Integration collection through Host verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompleteAgentRegistrationFacts {
    declared_descriptor: AgentServiceDescriptor,
    instance_id: AgentServiceInstanceId,
    placement: CompleteAgentPlacementRequirement,
    remote_binding: Option<CompleteAgentRemoteBindingMapping>,
    registration_claim: CompleteAgentRegistrationClaim,
}

impl CompleteAgentRegistrationFacts {
    pub fn declared_descriptor(&self) -> &AgentServiceDescriptor {
        &self.declared_descriptor
    }

    pub fn instance_id(&self) -> &AgentServiceInstanceId {
        &self.instance_id
    }

    pub fn placement(&self) -> &CompleteAgentPlacementRequirement {
        &self.placement
    }

    pub fn remote_binding(&self) -> Option<&CompleteAgentRemoteBindingMapping> {
        self.remote_binding.as_ref()
    }

    pub fn registration_claim(&self) -> &CompleteAgentRegistrationClaim {
        &self.registration_claim
    }
}

impl CompleteAgentRegistrationContribution {
    pub fn new(
        declared_descriptor: AgentServiceDescriptor,
        instance_id: AgentServiceInstanceId,
        placement: CompleteAgentPlacementRequirement,
        remote_binding: Option<CompleteAgentRemoteBindingMapping>,
        registration_claim: CompleteAgentRegistrationClaim,
        factory: Arc<dyn CompleteAgentServiceFactory>,
    ) -> Result<Self, CompleteAgentContributionError> {
        placement.validate()?;
        match (&placement, &remote_binding) {
            (CompleteAgentPlacementRequirement::Remote { .. }, Some(remote_binding)) => {
                remote_binding.validate(&instance_id)?
            }
            (CompleteAgentPlacementRequirement::Remote { .. }, None) => {
                return Err(CompleteAgentContributionError::InvalidRegistration {
                    reason: "remote Complete Agent placement requires an explicit identity and generation mapping"
                        .to_owned(),
                });
            }
            (_, Some(_)) => {
                return Err(CompleteAgentContributionError::InvalidRegistration {
                    reason:
                        "remote binding mapping is only valid for remote Complete Agent placement"
                            .to_owned(),
                });
            }
            (_, None) => {}
        }
        if declared_descriptor.title.trim().is_empty()
            || declared_descriptor.protocol_revision == 0
            || registration_claim.publisher_integration.trim().is_empty()
            || registration_claim.service_version.trim().is_empty()
            || registration_claim
                .claimed_conformance_suite_revision
                .trim()
                .is_empty()
        {
            return Err(CompleteAgentContributionError::InvalidRegistration {
                reason: "Complete Agent descriptor and registration claim fields must not be empty"
                    .to_owned(),
            });
        }
        Ok(Self {
            facts: CompleteAgentRegistrationFacts {
                declared_descriptor,
                instance_id,
                placement,
                remote_binding,
                registration_claim,
            },
            factory,
        })
    }

    pub fn facts(&self) -> &CompleteAgentRegistrationFacts {
        &self.facts
    }

    pub async fn materialize(
        &self,
    ) -> Result<MaterializedCompleteAgentCandidate, CompleteAgentContributionError> {
        let service = self.factory.materialize().await?;
        let actual = service.describe().await?;
        if actual != self.facts.declared_descriptor {
            return Err(CompleteAgentContributionError::DescriptorMismatch {
                expected: self.facts.declared_descriptor.definition_id.to_string(),
                actual: actual.definition_id.to_string(),
            });
        }
        Ok(MaterializedCompleteAgentCandidate {
            facts: self.facts.clone(),
            service,
        })
    }
}

/// Fully materialized, non-authoritative Integration candidate.
///
/// No registration claim or binding mapping is discarded. W8 verifies the descriptor/build/
/// conformance claims, persists Host-owned verifier evidence and the remote generation mapping,
/// maps `placement` to its Host-owned placement, then registers `service`.
pub struct MaterializedCompleteAgentCandidate {
    facts: CompleteAgentRegistrationFacts,
    service: Arc<dyn CompleteAgentService>,
}

impl MaterializedCompleteAgentCandidate {
    pub fn facts(&self) -> &CompleteAgentRegistrationFacts {
        &self.facts
    }

    pub fn service(&self) -> Arc<dyn CompleteAgentService> {
        self.service.clone()
    }

    pub fn into_parts(
        self,
    ) -> (
        CompleteAgentRegistrationFacts,
        Arc<dyn CompleteAgentService>,
    ) {
        (self.facts, self.service)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CompleteAgentContributionError {
    #[error("Complete Agent registration is invalid: {reason}")]
    InvalidRegistration { reason: String },
    #[error("Complete Agent descriptor mismatch: expected {expected}, actual {actual}")]
    DescriptorMismatch { expected: String, actual: String },
    #[error(
        "Complete Agent remote binding {coordinate} mismatch: expected {expected}, actual {actual}"
    )]
    RemoteBindingMismatch {
        coordinate: String,
        expected: String,
        actual: String,
    },
    #[error(transparent)]
    Factory(#[from] CompleteAgentServiceFactoryError),
    #[error(transparent)]
    Service(#[from] AgentServiceError),
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{BTreeMap, BTreeSet},
        future::Future,
        task::{Context, Poll, Wake, Waker},
    };

    use agentdash_agent_service_api::{
        AgentCapabilityProfile, AgentChangePage, AgentChangesQuery, AgentCommandCapability,
        AgentCommandEnvelope, AgentCommandReceipt, AgentCompactionMode, AgentConfigurationBoundary,
        AgentEffectIdentity, AgentEffectInspection, AgentForkCapability, AgentLifecycleCapability,
        AgentProfileDigest, AgentReadQuery, AgentServiceDefinitionId, AgentSnapshot,
        AgentSourceChangeLevel, AgentSurfaceProfile, AppliedAgentSurfaceReceipt,
        ApplyBoundAgentSurface, CreateAgentCommand, ForkAgentCommand, ForkAgentReceipt,
        InitialContextAppliedEvidence, InitialContextProfile, ResumeAgentCommand,
        RevokeBoundAgentSurface, SemanticFidelity,
    };

    use super::*;

    struct UnavailableFactory;

    #[async_trait]
    impl CompleteAgentServiceFactory for UnavailableFactory {
        async fn materialize(
            &self,
        ) -> Result<Arc<dyn CompleteAgentService>, CompleteAgentServiceFactoryError> {
            Err(CompleteAgentServiceFactoryError::CredentialUnavailable {
                reason: "credential slot was not resolved".to_owned(),
            })
        }
    }

    fn descriptor() -> AgentServiceDescriptor {
        AgentServiceDescriptor {
            definition_id: AgentServiceDefinitionId::new("fixture").expect("definition"),
            title: "Fixture".to_owned(),
            protocol_revision: 1,
            profile: AgentCapabilityProfile {
                lifecycle: BTreeSet::from([AgentLifecycleCapability::Create]),
                commands: BTreeSet::from([AgentCommandCapability::SubmitInput]),
                fork: AgentForkCapability {
                    cutoffs: BTreeMap::new(),
                    lineage_fidelity: SemanticFidelity::Unsupported,
                    native_durability: SemanticFidelity::Unsupported,
                },
                compaction: BTreeMap::<AgentCompactionMode, SemanticFidelity>::new(),
                source_changes: AgentSourceChangeLevel::SnapshotOnly,
                initial_context: InitialContextProfile {
                    contribution_fidelity: BTreeMap::new(),
                    applied_evidence: InitialContextAppliedEvidence::Unsupported,
                    renderer_versions: BTreeSet::new(),
                },
                surface: AgentSurfaceProfile { facets: Vec::new() },
                inspect_effects: SemanticFidelity::Exact,
            },
            profile_digest: AgentProfileDigest::new("fixture-profile").expect("profile"),
            configuration_boundary: AgentConfigurationBoundary::StaticService,
        }
    }

    fn claim() -> CompleteAgentRegistrationClaim {
        CompleteAgentRegistrationClaim {
            publisher_integration: "fixture.integration".to_owned(),
            service_version: "1".to_owned(),
            claimed_service_build_digest: AgentPayloadDigest::new("sha256:fixture")
                .expect("digest"),
            claimed_conformance_suite_revision: "complete-agent-v1".to_owned(),
        }
    }

    struct DescriptorOnlyService {
        descriptor: AgentServiceDescriptor,
    }

    #[async_trait]
    impl CompleteAgentService for DescriptorOnlyService {
        async fn describe(&self) -> Result<AgentServiceDescriptor, AgentServiceError> {
            Ok(self.descriptor.clone())
        }

        async fn create(
            &self,
            _command: CreateAgentCommand,
        ) -> Result<AgentCommandReceipt, AgentServiceError> {
            unreachable!()
        }

        async fn resume(
            &self,
            _command: ResumeAgentCommand,
        ) -> Result<AgentCommandReceipt, AgentServiceError> {
            unreachable!()
        }

        async fn fork(
            &self,
            _command: ForkAgentCommand,
        ) -> Result<ForkAgentReceipt, AgentServiceError> {
            unreachable!()
        }

        async fn execute(
            &self,
            _command: AgentCommandEnvelope,
        ) -> Result<AgentCommandReceipt, AgentServiceError> {
            unreachable!()
        }

        async fn read(&self, _query: AgentReadQuery) -> Result<AgentSnapshot, AgentServiceError> {
            unreachable!()
        }

        async fn changes(
            &self,
            _query: AgentChangesQuery,
        ) -> Result<AgentChangePage, AgentServiceError> {
            unreachable!()
        }

        async fn inspect(
            &self,
            _identity: AgentEffectIdentity,
        ) -> Result<AgentEffectInspection, AgentServiceError> {
            unreachable!()
        }

        async fn apply_surface(
            &self,
            _command: ApplyBoundAgentSurface,
        ) -> Result<AppliedAgentSurfaceReceipt, AgentServiceError> {
            unreachable!()
        }

        async fn revoke_surface(
            &self,
            _command: RevokeBoundAgentSurface,
        ) -> Result<AgentCommandReceipt, AgentServiceError> {
            unreachable!()
        }
    }

    struct DescriptorOnlyFactory {
        descriptor: AgentServiceDescriptor,
    }

    #[async_trait]
    impl CompleteAgentServiceFactory for DescriptorOnlyFactory {
        async fn materialize(
            &self,
        ) -> Result<Arc<dyn CompleteAgentService>, CompleteAgentServiceFactoryError> {
            Ok(Arc::new(DescriptorOnlyService {
                descriptor: self.descriptor.clone(),
            }))
        }
    }

    struct NoopWake;

    impl Wake for NoopWake {
        fn wake(self: Arc<Self>) {}
    }

    fn block_on<F: Future>(future: F) -> F::Output {
        let waker = Waker::from(Arc::new(NoopWake));
        let mut context = Context::from_waker(&waker);
        let mut future = Box::pin(future);
        loop {
            match Future::poll(future.as_mut(), &mut context) {
                Poll::Ready(output) => return output,
                Poll::Pending => std::thread::yield_now(),
            }
        }
    }

    #[test]
    fn contribution_never_defaults_missing_credentials_or_health_to_success() {
        let expected = descriptor();
        let contribution = CompleteAgentRegistrationContribution::new(
            expected.clone(),
            AgentServiceInstanceId::new("fixture-instance").expect("instance"),
            CompleteAgentPlacementRequirement::InProcess,
            None,
            claim(),
            Arc::new(UnavailableFactory),
        )
        .expect("registration");

        assert!(matches!(
            block_on(contribution.materialize()),
            Err(CompleteAgentContributionError::Factory(
                CompleteAgentServiceFactoryError::CredentialUnavailable { .. }
            ))
        ));
    }

    #[test]
    fn placement_and_remote_binding_mapping_are_validated_before_factory_side_effects() {
        let expected = descriptor();
        assert!(matches!(
            CompleteAgentRegistrationContribution::new(
                expected,
                AgentServiceInstanceId::new("fixture-instance").expect("instance"),
                CompleteAgentPlacementRequirement::Remote {
                    host_id: String::new(),
                    transport_id: String::new(),
                },
                None,
                claim(),
                Arc::new(UnavailableFactory),
            ),
            Err(CompleteAgentContributionError::InvalidRegistration { .. })
        ));
    }

    #[test]
    fn remote_registration_requires_an_explicit_matching_local_identity() {
        let declared_descriptor = descriptor();
        let local_instance = AgentServiceInstanceId::new("local-instance").expect("instance");
        let placement = CompleteAgentPlacementRequirement::Remote {
            host_id: "remote-host".to_owned(),
            transport_id: "runtime-wire".to_owned(),
        };

        assert!(matches!(
            CompleteAgentRegistrationContribution::new(
                declared_descriptor.clone(),
                local_instance.clone(),
                placement.clone(),
                None,
                claim(),
                Arc::new(UnavailableFactory),
            ),
            Err(CompleteAgentContributionError::InvalidRegistration { .. })
        ));
        assert!(matches!(
            CompleteAgentRegistrationContribution::new(
                declared_descriptor,
                local_instance,
                placement,
                Some(CompleteAgentRemoteBindingMapping {
                    local_service_instance_id: AgentServiceInstanceId::new("another-local")
                        .expect("instance"),
                    remote_service_instance_id: AgentServiceInstanceId::new("remote-instance")
                        .expect("instance"),
                    remote_binding_generation: AgentBindingGeneration(9),
                }),
                claim(),
                Arc::new(UnavailableFactory),
            ),
            Err(CompleteAgentContributionError::RemoteBindingMismatch {
                coordinate,
                ..
            }) if coordinate == "local_service_instance_id"
        ));
    }

    #[test]
    fn materialized_candidate_preserves_descriptor_claim_and_mapping_facts() {
        let declared_descriptor = descriptor();
        let registration_claim = claim();
        let local_instance =
            AgentServiceInstanceId::new("fixture-local-instance").expect("instance");
        let remote_binding = CompleteAgentRemoteBindingMapping {
            local_service_instance_id: local_instance.clone(),
            remote_service_instance_id: AgentServiceInstanceId::new("fixture-remote-instance")
                .expect("instance"),
            remote_binding_generation: AgentBindingGeneration(9),
        };
        let contribution = CompleteAgentRegistrationContribution::new(
            declared_descriptor.clone(),
            local_instance.clone(),
            CompleteAgentPlacementRequirement::Remote {
                host_id: "remote-host".to_owned(),
                transport_id: "runtime-wire".to_owned(),
            },
            Some(remote_binding.clone()),
            registration_claim.clone(),
            Arc::new(DescriptorOnlyFactory {
                descriptor: declared_descriptor.clone(),
            }),
        )
        .expect("registration");

        let candidate = block_on(contribution.materialize()).expect("materialize");

        assert_eq!(
            candidate.facts().declared_descriptor(),
            &declared_descriptor
        );
        assert_eq!(candidate.facts().instance_id(), &local_instance);
        assert_eq!(candidate.facts().remote_binding(), Some(&remote_binding));
        assert_eq!(candidate.facts().registration_claim(), &registration_claim);
    }

    #[test]
    fn integration_claim_has_no_host_verified_profile_authority() {
        let contract = include_str!("agent_runtime.rs");

        assert!(!contract.contains(concat!("verified_profile_", "digest")));
    }

    #[test]
    fn agent_registration_module_depends_on_service_api_not_runtime_concrete() {
        let manifest = include_str!("../Cargo.toml");
        let integration_contract = include_str!("integration.rs");

        assert!(manifest.contains("agentdash-agent-service-api"));
        assert!(!manifest.contains(concat!("agentdash-agent-", "runtime-contract")));
        for legacy in [
            concat!("AgentRuntime", "DriverContribution"),
            concat!("AgentRuntime", "TrustManifest"),
            concat!("agent_runtime_", "drivers"),
            concat!("agent_runtime_", "trust_manifests"),
        ] {
            assert!(
                !integration_contract.contains(legacy),
                "legacy Agent contribution symbol survived: {legacy}"
            );
        }
    }
}
