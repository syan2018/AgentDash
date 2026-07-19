use std::{collections::BTreeMap, sync::Arc};

use agentdash_agent_runtime_contract::{ManagedAgentRuntimeGateway, ManagedRuntimeGatewayError};
use agentdash_agent_runtime_host::{
    CompleteAgentCallbackBroker, CompleteAgentHookHandler, CompleteAgentHost,
    CompleteAgentHostError, CompleteAgentPlacement, CompleteAgentRegistrationVerifier,
    CompleteAgentRemoteBindingFact, CompleteAgentServiceRegistry, CompleteAgentServiceVerification,
    CompleteAgentToolHandler, CompleteAgentVerificationError, CompleteAgentVerificationRecord,
    CompleteAgentVerificationRequest, CompleteAgentVerifiedBuildEvidence,
    CompleteAgentVerifiedServiceRegistration, complete_agent_managed_runtime_gateway,
};
use agentdash_agent_service_api::{
    AgentHostCallbacks, AgentServiceDescriptor, AgentServiceInstanceId, CompleteAgentService,
};
use agentdash_integration_api::{
    CompleteAgentContributionError, CompleteAgentPlacementRequirement,
    CompleteAgentRegistrationContribution,
};
use async_trait::async_trait;
use sqlx::PgPool;
use thiserror::Error;
use tokio::sync::RwLock;

use crate::{
    PostgresCompleteAgentCallbackRepository, PostgresCompleteAgentHostRepository,
    PostgresManagedRuntimeStateRepository,
};

#[derive(Default)]
struct ProcessCompleteAgentServiceRegistry {
    services: RwLock<BTreeMap<AgentServiceInstanceId, Arc<dyn CompleteAgentService>>>,
}

#[async_trait]
impl CompleteAgentServiceRegistry for ProcessCompleteAgentServiceRegistry {
    async fn attach(
        &self,
        instance_id: AgentServiceInstanceId,
        service: Arc<dyn CompleteAgentService>,
    ) {
        self.services.write().await.insert(instance_id, service);
    }

    async fn resolve(
        &self,
        instance_id: &AgentServiceInstanceId,
    ) -> Option<Arc<dyn CompleteAgentService>> {
        self.services.read().await.get(instance_id).cloned()
    }
}

#[derive(Debug, Error)]
pub enum CompleteAgentCompositionError {
    #[error(transparent)]
    Runtime(#[from] ManagedRuntimeGatewayError),
    #[error(transparent)]
    Host(#[from] CompleteAgentHostError),
    #[error(transparent)]
    Contribution(#[from] CompleteAgentContributionError),
    #[error(transparent)]
    Verification(#[from] CompleteAgentVerificationError),
    #[error("Complete Agent Host incarnation identity must not be empty")]
    InvalidHostIncarnation,
}

/// Independently configured Host trust catalog.
///
/// Records come from deployment/builtin trusted configuration or a successfully validated remote
/// transport advertisement. Contributions are only lookup requests and never populate this map.
pub struct PinnedCompleteAgentVerificationCatalog {
    records: RwLock<BTreeMap<AgentServiceInstanceId, CompleteAgentVerificationRecord>>,
    templates: Vec<CompleteAgentVerificationTemplate>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompleteAgentVerificationTemplate {
    pub expected_publisher_integration: String,
    pub expected_service_version: String,
    pub expected_build_digest: agentdash_agent_service_api::AgentPayloadDigest,
    pub expected_profile_digest: agentdash_agent_service_api::AgentProfileDigest,
    pub expected_conformance_suite_revision: String,
    pub method: agentdash_agent_runtime_host::CompleteAgentVerificationMethod,
    pub verifier_identity: String,
    pub verifier_revision: String,
    pub evidence_digest: agentdash_agent_service_api::AgentPayloadDigest,
}

impl PinnedCompleteAgentVerificationCatalog {
    pub fn new(
        records: impl IntoIterator<Item = CompleteAgentVerificationRecord>,
    ) -> Result<Self, CompleteAgentVerificationError> {
        let mut catalog = BTreeMap::new();
        for record in records {
            validate_record(&record)?;
            let instance_id = record.service_instance_id.clone();
            if catalog.insert(instance_id.clone(), record).is_some() {
                return Err(CompleteAgentVerificationError::InvalidRecord {
                    reason: format!("duplicate trusted verification record for {instance_id}"),
                });
            }
        }
        Ok(Self {
            records: RwLock::new(catalog),
            templates: Vec::new(),
        })
    }

    pub fn new_with_templates(
        records: impl IntoIterator<Item = CompleteAgentVerificationRecord>,
        templates: impl IntoIterator<Item = CompleteAgentVerificationTemplate>,
    ) -> Result<Self, CompleteAgentVerificationError> {
        let mut catalog = Self::new(records)?;
        for template in templates {
            validate_template(&template)?;
            if catalog.templates.contains(&template) {
                return Err(CompleteAgentVerificationError::InvalidRecord {
                    reason: "duplicate trusted Complete Agent verification template".to_owned(),
                });
            }
            catalog.templates.push(template);
        }
        Ok(catalog)
    }

    /// Adds one exact record produced by an independently verified placement advertisement.
    pub async fn register_record(
        &self,
        record: CompleteAgentVerificationRecord,
    ) -> Result<(), CompleteAgentVerificationError> {
        validate_record(&record)?;
        let mut records = self.records.write().await;
        if let Some(existing) = records.get(&record.service_instance_id) {
            if existing == &record {
                return Ok(());
            }
            return Err(CompleteAgentVerificationError::InvalidRecord {
                reason: format!(
                    "trusted verification record conflicts for {}",
                    record.service_instance_id
                ),
            });
        }
        records.insert(record.service_instance_id.clone(), record);
        Ok(())
    }
}

#[async_trait]
impl CompleteAgentRegistrationVerifier for PinnedCompleteAgentVerificationCatalog {
    async fn verify(
        &self,
        request: CompleteAgentVerificationRequest,
    ) -> Result<CompleteAgentServiceVerification, CompleteAgentVerificationError> {
        let exact = self
            .records
            .read()
            .await
            .get(&request.service_instance_id)
            .cloned();
        let record = if let Some(record) = exact {
            record
        } else {
            let template = self
                .templates
                .iter()
                .find(|template| template_matches(template, &request))
                .ok_or_else(|| CompleteAgentVerificationError::MissingRecord {
                    service_instance_id: request.service_instance_id.clone(),
                })?;
            CompleteAgentVerificationRecord {
                service_instance_id: request.service_instance_id.clone(),
                expected_publisher_integration: template.expected_publisher_integration.clone(),
                expected_service_version: template.expected_service_version.clone(),
                expected_build_digest: template.expected_build_digest.clone(),
                expected_profile_digest: template.expected_profile_digest.clone(),
                expected_conformance_suite_revision: template
                    .expected_conformance_suite_revision
                    .clone(),
                method: template.method,
                verifier_identity: template.verifier_identity.clone(),
                verifier_revision: template.verifier_revision.clone(),
                evidence_digest: template.evidence_digest.clone(),
            }
        };
        for (matches, coordinate) in [
            (
                record.expected_publisher_integration == request.publisher_integration,
                "publisher_integration",
            ),
            (
                record.expected_service_version == request.service_version,
                "service_version",
            ),
            (
                record.expected_build_digest == request.claimed_build_digest,
                "claimed_build_digest",
            ),
            (
                record.expected_profile_digest == request.profile_digest,
                "profile_digest",
            ),
            (
                record.expected_conformance_suite_revision
                    == request.claimed_conformance_suite_revision,
                "claimed_conformance_suite_revision",
            ),
        ] {
            if !matches {
                return Err(CompleteAgentVerificationError::ClaimDrift { coordinate });
            }
        }
        Ok(CompleteAgentServiceVerification {
            service_instance_id: request.service_instance_id,
            publisher_integration: request.publisher_integration,
            service_version: request.service_version,
            verifier_identity: record.verifier_identity.clone(),
            verifier_revision: record.verifier_revision.clone(),
            method: record.method,
            verified_profile_digest: request.profile_digest,
            claimed_conformance_suite_revision: request.claimed_conformance_suite_revision,
            verified_build: CompleteAgentVerifiedBuildEvidence {
                claimed_build_digest: request.claimed_build_digest,
                evidence_digest: record.evidence_digest.clone(),
            },
        })
    }
}

fn validate_record(
    record: &CompleteAgentVerificationRecord,
) -> Result<(), CompleteAgentVerificationError> {
    if record.service_instance_id.as_str().trim().is_empty()
        || record.expected_publisher_integration.trim().is_empty()
        || record.expected_service_version.trim().is_empty()
        || record.expected_build_digest.as_str().trim().is_empty()
        || record.expected_profile_digest.as_str().trim().is_empty()
        || record.expected_conformance_suite_revision.trim().is_empty()
        || record.verifier_identity.trim().is_empty()
        || record.verifier_revision.trim().is_empty()
        || record.evidence_digest.as_str().trim().is_empty()
    {
        return Err(CompleteAgentVerificationError::InvalidRecord {
            reason: "trusted verification record contains empty coordinates".to_owned(),
        });
    }
    Ok(())
}

fn validate_template(
    template: &CompleteAgentVerificationTemplate,
) -> Result<(), CompleteAgentVerificationError> {
    if template.expected_publisher_integration.trim().is_empty()
        || template.expected_service_version.trim().is_empty()
        || template.expected_build_digest.as_str().trim().is_empty()
        || template.expected_profile_digest.as_str().trim().is_empty()
        || template
            .expected_conformance_suite_revision
            .trim()
            .is_empty()
        || template.verifier_identity.trim().is_empty()
        || template.verifier_revision.trim().is_empty()
        || template.evidence_digest.as_str().trim().is_empty()
    {
        return Err(CompleteAgentVerificationError::InvalidRecord {
            reason: "trusted verification template contains empty coordinates".to_owned(),
        });
    }
    Ok(())
}

fn template_matches(
    template: &CompleteAgentVerificationTemplate,
    request: &CompleteAgentVerificationRequest,
) -> bool {
    template.expected_publisher_integration == request.publisher_integration
        && template.expected_service_version == request.service_version
        && template.expected_build_digest == request.claimed_build_digest
        && template.expected_profile_digest == request.profile_digest
        && template.expected_conformance_suite_revision
            == request.claimed_conformance_suite_revision
}

/// Final production kernel for Managed Runtime and Complete Agent coordination.
///
/// The process registry contains live handles only. Every service descriptor, placement,
/// binding, generation, effect, callback and normalized Runtime fact is recovered from the three
/// PostgreSQL repositories.
pub struct CompleteAgentComposition {
    pub runtime_repository: Arc<PostgresManagedRuntimeStateRepository>,
    pub host_repository: Arc<PostgresCompleteAgentHostRepository>,
    pub callback_repository: Arc<PostgresCompleteAgentCallbackRepository>,
    pub host: Arc<CompleteAgentHost>,
    pub callbacks: Arc<CompleteAgentCallbackBroker>,
    pub runtime: Arc<dyn ManagedAgentRuntimeGateway>,
    verifier: Arc<dyn CompleteAgentRegistrationVerifier>,
    host_incarnation_id: String,
}

impl CompleteAgentComposition {
    pub fn build(
        pool: PgPool,
        tool_handler: Arc<dyn CompleteAgentToolHandler>,
        hook_handler: Arc<dyn CompleteAgentHookHandler>,
        verifier: Arc<dyn CompleteAgentRegistrationVerifier>,
        host_incarnation_id: impl Into<String>,
        dispatch_owner: impl Into<String>,
        lease_duration_ms: u64,
    ) -> Result<Self, CompleteAgentCompositionError> {
        let host_incarnation_id = host_incarnation_id.into();
        if host_incarnation_id.trim().is_empty() {
            return Err(CompleteAgentCompositionError::InvalidHostIncarnation);
        }
        let runtime_repository = Arc::new(PostgresManagedRuntimeStateRepository::new(pool.clone()));
        let host_repository = Arc::new(PostgresCompleteAgentHostRepository::new(pool.clone()));
        let callback_repository = Arc::new(PostgresCompleteAgentCallbackRepository::new(pool));
        let registry = Arc::new(ProcessCompleteAgentServiceRegistry::default());
        let host = Arc::new(CompleteAgentHost::new(host_repository.clone(), registry));
        let callbacks = Arc::new(CompleteAgentCallbackBroker::new(
            tool_handler,
            hook_handler,
            host_repository.clone(),
            callback_repository.clone(),
        ));
        let runtime = complete_agent_managed_runtime_gateway(
            runtime_repository.clone(),
            host.clone(),
            dispatch_owner,
            lease_duration_ms,
        )?;
        Ok(Self {
            runtime_repository,
            host_repository,
            callback_repository,
            host,
            callbacks,
            runtime,
            verifier,
            host_incarnation_id,
        })
    }

    pub fn host_callbacks(&self) -> Arc<dyn AgentHostCallbacks> {
        self.callbacks.clone()
    }

    pub async fn register_contribution(
        &self,
        contribution: CompleteAgentRegistrationContribution,
    ) -> Result<AgentServiceDescriptor, CompleteAgentCompositionError> {
        let candidate = contribution.materialize().await?;
        let facts = candidate.facts();
        let claim = facts.registration_claim();
        let verification = self
            .verifier
            .verify(CompleteAgentVerificationRequest {
                service_instance_id: facts.instance_id().clone(),
                publisher_integration: claim.publisher_integration.clone(),
                service_version: claim.service_version.clone(),
                claimed_build_digest: claim.claimed_service_build_digest.clone(),
                profile_digest: facts.declared_descriptor().profile_digest.clone(),
                claimed_conformance_suite_revision: claim
                    .claimed_conformance_suite_revision
                    .clone(),
            })
            .await?;
        let placement = match facts.placement() {
            CompleteAgentPlacementRequirement::InProcess => CompleteAgentPlacement::InProcess {
                host_incarnation_id: self.host_incarnation_id.clone(),
            },
            CompleteAgentPlacementRequirement::LocalProcess { host_id } => {
                CompleteAgentPlacement::LocalProcess {
                    host_id: host_id.clone(),
                    host_incarnation_id: self.host_incarnation_id.clone(),
                }
            }
            CompleteAgentPlacementRequirement::Remote {
                host_id,
                transport_id,
            } => CompleteAgentPlacement::Remote {
                host_id: host_id.clone(),
                transport_id: transport_id.clone(),
                host_incarnation_id: self.host_incarnation_id.clone(),
            },
        };
        let remote_binding = facts
            .remote_binding()
            .map(|mapping| CompleteAgentRemoteBindingFact {
                local_service_instance_id: mapping.local_service_instance_id.clone(),
                local_binding_generation: mapping.local_binding_generation,
                remote_service_instance_id: mapping.remote_service_instance_id.clone(),
                remote_binding_generation: mapping.remote_binding_generation,
                host_incarnation_id: self.host_incarnation_id.clone(),
                transport_id: match facts.placement() {
                    CompleteAgentPlacementRequirement::Remote { transport_id, .. } => {
                        transport_id.clone()
                    }
                    _ => unreachable!("remote mapping requires remote placement"),
                },
            });
        Ok(self
            .host
            .register_verified_service(
                CompleteAgentVerifiedServiceRegistration {
                    instance_id: facts.instance_id().clone(),
                    placement,
                    verification,
                    remote_binding,
                },
                candidate.service(),
            )
            .await?)
    }
}

#[cfg(test)]
mod tests {
    use agentdash_agent_runtime_host::{
        CompleteAgentRegistrationVerifier, CompleteAgentVerificationMethod,
    };
    use agentdash_agent_service_api::{
        AgentPayloadDigest, AgentProfileDigest, AgentServiceInstanceId,
    };

    use super::*;

    fn instance_id() -> AgentServiceInstanceId {
        AgentServiceInstanceId::new("fixture-service").expect("service instance")
    }

    fn record() -> CompleteAgentVerificationRecord {
        CompleteAgentVerificationRecord {
            service_instance_id: instance_id(),
            expected_publisher_integration: "fixture-integration".to_owned(),
            expected_service_version: "1.2.3".to_owned(),
            expected_build_digest: AgentPayloadDigest::new("sha256:build").expect("build digest"),
            expected_profile_digest: AgentProfileDigest::new("sha256:profile")
                .expect("profile digest"),
            expected_conformance_suite_revision: "suite-4".to_owned(),
            method: CompleteAgentVerificationMethod::PinnedBuiltin,
            verifier_identity: "deployment-catalog".to_owned(),
            verifier_revision: "catalog-7".to_owned(),
            evidence_digest: AgentPayloadDigest::new("sha256:evidence").expect("evidence digest"),
        }
    }

    fn request() -> CompleteAgentVerificationRequest {
        CompleteAgentVerificationRequest {
            service_instance_id: instance_id(),
            publisher_integration: "fixture-integration".to_owned(),
            service_version: "1.2.3".to_owned(),
            claimed_build_digest: AgentPayloadDigest::new("sha256:build").expect("build digest"),
            profile_digest: AgentProfileDigest::new("sha256:profile").expect("profile digest"),
            claimed_conformance_suite_revision: "suite-4".to_owned(),
        }
    }

    #[tokio::test]
    async fn contribution_claims_require_an_independent_catalog_record() {
        let catalog =
            PinnedCompleteAgentVerificationCatalog::new([]).expect("empty trusted catalog");

        assert!(matches!(
            catalog.verify(request()).await,
            Err(CompleteAgentVerificationError::MissingRecord {
                service_instance_id
            }) if service_instance_id == instance_id()
        ));
    }

    #[tokio::test]
    async fn catalog_rejects_claim_drift_and_emits_host_owned_evidence() {
        let trusted_record = record();
        let catalog = PinnedCompleteAgentVerificationCatalog::new([trusted_record.clone()])
            .expect("trusted catalog");
        let mut drifted = request();
        drifted.claimed_build_digest =
            AgentPayloadDigest::new("sha256:untrusted-build").expect("build digest");

        assert_eq!(
            catalog.verify(drifted).await,
            Err(CompleteAgentVerificationError::ClaimDrift {
                coordinate: "claimed_build_digest"
            })
        );

        let verified = catalog.verify(request()).await.expect("verified record");
        assert_eq!(
            verified.method,
            CompleteAgentVerificationMethod::PinnedBuiltin
        );
        assert_eq!(verified.verifier_identity, trusted_record.verifier_identity);
        assert_eq!(verified.verifier_revision, trusted_record.verifier_revision);
        assert_eq!(
            verified.verified_build.evidence_digest,
            trusted_record.evidence_digest
        );
        assert_eq!(
            verified.verified_build.claimed_build_digest,
            trusted_record.expected_build_digest
        );
    }

    #[tokio::test]
    async fn trusted_builtin_template_verifies_dynamic_instance_without_trusting_its_claims() {
        let trusted = record();
        let catalog = PinnedCompleteAgentVerificationCatalog::new_with_templates(
            [],
            [CompleteAgentVerificationTemplate {
                expected_publisher_integration: trusted.expected_publisher_integration.clone(),
                expected_service_version: trusted.expected_service_version.clone(),
                expected_build_digest: trusted.expected_build_digest.clone(),
                expected_profile_digest: trusted.expected_profile_digest.clone(),
                expected_conformance_suite_revision: trusted
                    .expected_conformance_suite_revision
                    .clone(),
                method: trusted.method,
                verifier_identity: trusted.verifier_identity.clone(),
                verifier_revision: trusted.verifier_revision.clone(),
                evidence_digest: trusted.evidence_digest.clone(),
            }],
        )
        .expect("trusted template");
        let mut dynamic = request();
        dynamic.service_instance_id =
            AgentServiceInstanceId::new("fixture-service-dynamic").unwrap();

        let verified = catalog.verify(dynamic).await.expect("template match");
        assert_eq!(
            verified.service_instance_id.as_str(),
            "fixture-service-dynamic"
        );
        assert_eq!(verified.verifier_identity, trusted.verifier_identity);

        let mut drifted = request();
        drifted.service_instance_id =
            AgentServiceInstanceId::new("fixture-service-untrusted").unwrap();
        drifted.claimed_build_digest = AgentPayloadDigest::new("sha256:drifted").unwrap();
        assert!(matches!(
            catalog.verify(drifted).await,
            Err(CompleteAgentVerificationError::MissingRecord { .. })
        ));
    }
}
