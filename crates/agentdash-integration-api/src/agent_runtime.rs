use std::sync::Arc;

use agentdash_agent_service_api::{
    AgentPayloadDigest, AgentProfileDigest, AgentServiceDescriptor, AgentServiceError,
    AgentServiceInstanceId, CompleteAgentService,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Build and conformance evidence attached to one trusted Complete Agent definition.
///
/// This is an Integration input to Host offer normalization. It is not a boolean trust decision
/// and does not claim that credentials, health, placement transport, or a runtime offer are
/// currently available.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CompleteAgentOfferProvenance {
    pub publisher_integration: String,
    pub service_version: String,
    pub service_build_digest: AgentPayloadDigest,
    pub conformance_suite_revision: String,
    pub verified_profile_digest: AgentProfileDigest,
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

/// Dependency-light, trusted contribution collected from [`crate::AgentDashIntegration`].
///
/// `expected_descriptor` is service-API owned. Host definition, placement, health and offer facts
/// are deliberately absent: the W8 composition root normalizes this input with Host-owned
/// evidence, materializes the service, and then calls its final registration boundary.
#[derive(Clone)]
pub struct CompleteAgentRegistrationContribution {
    pub expected_descriptor: AgentServiceDescriptor,
    pub instance_id: AgentServiceInstanceId,
    pub placement: CompleteAgentPlacementRequirement,
    pub offer_provenance: CompleteAgentOfferProvenance,
    pub factory: Arc<dyn CompleteAgentServiceFactory>,
}

impl CompleteAgentRegistrationContribution {
    pub fn new(
        expected_descriptor: AgentServiceDescriptor,
        instance_id: AgentServiceInstanceId,
        placement: CompleteAgentPlacementRequirement,
        offer_provenance: CompleteAgentOfferProvenance,
        factory: Arc<dyn CompleteAgentServiceFactory>,
    ) -> Result<Self, CompleteAgentContributionError> {
        placement.validate()?;
        if expected_descriptor.title.trim().is_empty()
            || expected_descriptor.protocol_revision == 0
            || offer_provenance.publisher_integration.trim().is_empty()
            || offer_provenance.service_version.trim().is_empty()
            || offer_provenance
                .conformance_suite_revision
                .trim()
                .is_empty()
        {
            return Err(CompleteAgentContributionError::InvalidRegistration {
                reason: "Complete Agent definition and provenance fields must not be empty"
                    .to_owned(),
            });
        }
        if expected_descriptor.profile_digest != offer_provenance.verified_profile_digest {
            return Err(CompleteAgentContributionError::InvalidRegistration {
                reason: "Complete Agent verified profile digest must match the expected descriptor"
                    .to_owned(),
            });
        }
        Ok(Self {
            expected_descriptor,
            instance_id,
            placement,
            offer_provenance,
            factory,
        })
    }

    pub async fn materialize(
        &self,
    ) -> Result<MaterializedCompleteAgentRegistration, CompleteAgentContributionError> {
        let service = self.factory.materialize().await?;
        let actual = service.describe().await?;
        if actual != self.expected_descriptor {
            return Err(CompleteAgentContributionError::DescriptorMismatch {
                expected: self.expected_descriptor.definition_id.to_string(),
                actual: actual.definition_id.to_string(),
            });
        }
        Ok(MaterializedCompleteAgentRegistration {
            expected_descriptor: self.expected_descriptor.clone(),
            instance_id: self.instance_id.clone(),
            placement: self.placement.clone(),
            offer_provenance: self.offer_provenance.clone(),
            service,
        })
    }
}

/// Fully materialized Integration output.
///
/// W8 maps `placement` to its Host-owned placement by adding the active host incarnation, then
/// passes `instance_id`, the mapped placement, and `service` to the Host registration call.
pub struct MaterializedCompleteAgentRegistration {
    pub expected_descriptor: AgentServiceDescriptor,
    pub instance_id: AgentServiceInstanceId,
    pub placement: CompleteAgentPlacementRequirement,
    pub offer_provenance: CompleteAgentOfferProvenance,
    pub service: Arc<dyn CompleteAgentService>,
}

impl MaterializedCompleteAgentRegistration {
    pub fn into_integration_parts(
        self,
    ) -> (
        AgentServiceInstanceId,
        CompleteAgentPlacementRequirement,
        Arc<dyn CompleteAgentService>,
    ) {
        (self.instance_id, self.placement, self.service)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CompleteAgentContributionError {
    #[error("Complete Agent registration is invalid: {reason}")]
    InvalidRegistration { reason: String },
    #[error("Complete Agent descriptor mismatch: expected {expected}, actual {actual}")]
    DescriptorMismatch { expected: String, actual: String },
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
        AgentCapabilityProfile, AgentCommandCapability, AgentCompactionMode,
        AgentConfigurationBoundary, AgentForkCapability, AgentLifecycleCapability,
        AgentServiceDefinitionId, AgentSourceChangeLevel, AgentSurfaceProfile,
        InitialContextAppliedEvidence, InitialContextProfile, SemanticFidelity,
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

    fn provenance(profile_digest: AgentProfileDigest) -> CompleteAgentOfferProvenance {
        CompleteAgentOfferProvenance {
            publisher_integration: "fixture.integration".to_owned(),
            service_version: "1".to_owned(),
            service_build_digest: AgentPayloadDigest::new("sha256:fixture").expect("digest"),
            conformance_suite_revision: "complete-agent-v1".to_owned(),
            verified_profile_digest: profile_digest,
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
            provenance(expected.profile_digest),
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
    fn placement_and_verified_profile_are_validated_before_factory_side_effects() {
        let expected = descriptor();
        let invalid_profile = AgentProfileDigest::new("another-profile").expect("profile");
        assert!(matches!(
            CompleteAgentRegistrationContribution::new(
                expected,
                AgentServiceInstanceId::new("fixture-instance").expect("instance"),
                CompleteAgentPlacementRequirement::Remote {
                    host_id: String::new(),
                    transport_id: String::new(),
                },
                provenance(invalid_profile),
                Arc::new(UnavailableFactory),
            ),
            Err(CompleteAgentContributionError::InvalidRegistration { .. })
        ));
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
