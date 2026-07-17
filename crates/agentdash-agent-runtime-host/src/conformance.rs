use std::collections::BTreeMap;

use agentdash_agent_runtime_contract::{
    AgentRuntimeDriver, ProfileDigest, RuntimeDescriptor, RuntimeServiceInstanceId,
};
use agentdash_integration_api::{
    AgentServiceDefinition, AgentServiceDefinitionId, AgentServiceProvenance,
};
use async_trait::async_trait;
use thiserror::Error;

use crate::{ConformanceEvidence, ConformanceVerificationError, DriverConformanceVerifier};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustedDriverManifest {
    pub provenance: AgentServiceProvenance,
    pub suite_revision: String,
    pub driver_build_digest: String,
    pub protocol_revision: u32,
    pub verified_profile_digest: ProfileDigest,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TrustedManifestRegistryError {
    #[error("trusted driver manifest has an empty suite revision for {definition_id}")]
    EmptySuiteRevision {
        definition_id: AgentServiceDefinitionId,
    },
    #[error("trusted driver manifest has an empty driver build digest for {definition_id}")]
    EmptyDriverBuildDigest {
        definition_id: AgentServiceDefinitionId,
    },
    #[error("trusted driver manifest is duplicated for {definition_id}")]
    DuplicateDefinition {
        definition_id: AgentServiceDefinitionId,
    },
}

#[derive(Debug, Clone)]
pub struct TrustedDriverManifestRegistry {
    manifests: BTreeMap<AgentServiceDefinitionId, TrustedDriverManifest>,
}

impl TrustedDriverManifestRegistry {
    pub fn collect(
        manifests: impl IntoIterator<Item = TrustedDriverManifest>,
    ) -> Result<Self, TrustedManifestRegistryError> {
        let mut trusted = BTreeMap::new();
        for manifest in manifests {
            let definition_id = manifest.provenance.definition_id.clone();
            if manifest.suite_revision.trim().is_empty() {
                return Err(TrustedManifestRegistryError::EmptySuiteRevision { definition_id });
            }
            if manifest.driver_build_digest.trim().is_empty() {
                return Err(TrustedManifestRegistryError::EmptyDriverBuildDigest { definition_id });
            }
            if trusted.insert(definition_id.clone(), manifest).is_some() {
                return Err(TrustedManifestRegistryError::DuplicateDefinition { definition_id });
            }
        }
        Ok(Self { manifests: trusted })
    }

    fn manifest(&self, definition_id: &AgentServiceDefinitionId) -> Option<&TrustedDriverManifest> {
        self.manifests.get(definition_id)
    }
}

#[derive(Debug, Clone)]
pub struct TrustedDriverConformanceVerifier {
    registry: TrustedDriverManifestRegistry,
}

impl TrustedDriverConformanceVerifier {
    pub fn new(registry: TrustedDriverManifestRegistry) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl DriverConformanceVerifier for TrustedDriverConformanceVerifier {
    async fn verify(
        &self,
        _driver: &dyn AgentRuntimeDriver,
        definition: &AgentServiceDefinition,
        expected_service_instance_id: &RuntimeServiceInstanceId,
        descriptor: &RuntimeDescriptor,
        evidence: &ConformanceEvidence,
    ) -> Result<(), ConformanceVerificationError> {
        let manifest = self
            .registry
            .manifest(&definition.provenance.definition_id)
            .ok_or_else(|| rejected("service definition has no trusted driver manifest"))?;

        if manifest.provenance != definition.provenance {
            return Err(rejected(
                "service definition provenance does not match the trusted manifest",
            ));
        }
        if descriptor.service_instance_id != *expected_service_instance_id {
            return Err(rejected(
                "driver descriptor service instance does not match the activation",
            ));
        }
        if descriptor.protocol_revision != manifest.protocol_revision {
            return Err(rejected(
                "driver descriptor protocol revision does not match the trusted manifest",
            ));
        }
        if descriptor.profile_digest != manifest.verified_profile_digest {
            return Err(rejected(
                "driver descriptor profile digest does not match the trusted manifest",
            ));
        }
        if evidence.suite_revision != manifest.suite_revision {
            return Err(rejected(
                "conformance suite revision does not match the trusted manifest",
            ));
        }
        if evidence.driver_build_digest != manifest.driver_build_digest {
            return Err(rejected(
                "driver build digest does not match the trusted manifest",
            ));
        }
        if evidence.verified_profile_digest != manifest.verified_profile_digest {
            return Err(rejected(
                "conformance profile digest does not match the trusted manifest",
            ));
        }
        Ok(())
    }
}

fn rejected(reason: impl Into<String>) -> ConformanceVerificationError {
    ConformanceVerificationError {
        reason: reason.into(),
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, sync::Arc};

    use agentdash_agent_runtime_contract::{
        ConfigurationBoundary, ContextFidelity, ContextProfile, DeliveryMechanism,
        DriverBindRequest, DriverBinding, DriverCommandEnvelope, DriverDescribeRequest,
        DriverDispatchReceipt, DriverError, DriverEventSink, DriverInspection,
        DriverInspectionQuery, HookProfile, InputProfile, InstructionProfile, InteractionProfile,
        ReferenceRuntimeClass, RuntimeProfile, TelemetryCapability, ToolProfile, WorkspaceProfile,
    };
    use agentdash_integration_api::{
        AgentRuntimeFactoryKey, AgentServiceBuildDigest, AgentServiceSchemaDigest,
    };
    use chrono::Utc;
    use serde_json::json;

    use super::*;
    use crate::profile_digest;

    struct FixtureDriver {
        descriptor: RuntimeDescriptor,
    }

    #[async_trait]
    impl AgentRuntimeDriver for FixtureDriver {
        async fn describe(
            &self,
            _request: DriverDescribeRequest,
        ) -> Result<RuntimeDescriptor, DriverError> {
            Ok(self.descriptor.clone())
        }

        async fn bind(&self, _request: DriverBindRequest) -> Result<DriverBinding, DriverError> {
            Err(unsupported())
        }

        async fn dispatch(
            &self,
            _command: DriverCommandEnvelope,
            _sink: Arc<dyn DriverEventSink>,
        ) -> Result<DriverDispatchReceipt, DriverError> {
            Err(unsupported())
        }

        async fn inspect(
            &self,
            _query: DriverInspectionQuery,
        ) -> Result<DriverInspection, DriverError> {
            Err(unsupported())
        }
    }

    fn unsupported() -> DriverError {
        DriverError::Unsupported {
            reason: "unused by conformance verification".to_string(),
        }
    }

    struct Fixture {
        verifier: TrustedDriverConformanceVerifier,
        driver: FixtureDriver,
        definition: AgentServiceDefinition,
        service_instance_id: RuntimeServiceInstanceId,
        evidence: ConformanceEvidence,
    }

    fn fixture() -> Fixture {
        let profile = RuntimeProfile {
            reference_class: ReferenceRuntimeClass::ManagedThread,
            input: InputProfile {
                modalities: BTreeSet::new(),
            },
            instruction: InstructionProfile {
                channels: BTreeSet::new(),
                configuration_boundary: ConfigurationBoundary::StaticService,
            },
            tools: ToolProfile {
                channels: BTreeSet::new(),
                configuration_boundary: ConfigurationBoundary::StaticService,
                cancellation: false,
            },
            workspace: WorkspaceProfile {
                capabilities: BTreeSet::new(),
                mechanism: DeliveryMechanism::Observed,
            },
            interactions: InteractionProfile {
                kinds: BTreeSet::new(),
                durable_correlation: false,
            },
            lifecycle: BTreeSet::new(),
            hooks: HookProfile {
                points: Vec::new(),
                configuration_boundary: ConfigurationBoundary::StaticService,
            },
            context: ContextProfile {
                capabilities: BTreeSet::new(),
                fidelity: ContextFidelity::Opaque,
                activation_idempotent: false,
            },
            telemetry_config: BTreeSet::<TelemetryCapability>::new(),
        };
        let profile_digest = profile_digest(&profile).expect("profile digest");
        let provenance = AgentServiceProvenance {
            definition_id: AgentServiceDefinitionId::new("trusted.agent").expect("definition id"),
            publisher_integration: "trusted.integration".to_string(),
            service_version: "1.0.0".to_string(),
            build_digest: AgentServiceBuildDigest::new("sha256:definition")
                .expect("definition build"),
        };
        let definition = AgentServiceDefinition {
            provenance: provenance.clone(),
            factory_key: AgentRuntimeFactoryKey::new("trusted.factory").expect("factory key"),
            supported_protocol_revisions: vec![7],
            config_schema: json!({"type": "object"}),
            config_schema_digest: AgentServiceSchemaDigest::new("sha256:schema")
                .expect("schema digest"),
            credential_slots: Vec::new(),
            service_profile_upper_bound: profile.clone(),
        };
        let service_instance_id =
            RuntimeServiceInstanceId::new("service-instance").expect("service instance id");
        let descriptor = RuntimeDescriptor {
            protocol_revision: 7,
            service_instance_id: service_instance_id.clone(),
            profile,
            profile_digest: profile_digest.clone(),
        };
        let evidence = ConformanceEvidence {
            suite_revision: "runtime-driver-v3".to_string(),
            driver_build_digest: "sha256:driver".to_string(),
            verified_profile_digest: profile_digest.clone(),
            verified_at: Utc::now(),
        };
        let registry = TrustedDriverManifestRegistry::collect([TrustedDriverManifest {
            provenance,
            suite_revision: evidence.suite_revision.clone(),
            driver_build_digest: evidence.driver_build_digest.clone(),
            protocol_revision: descriptor.protocol_revision,
            verified_profile_digest: profile_digest,
        }])
        .expect("trusted registry");
        Fixture {
            verifier: TrustedDriverConformanceVerifier::new(registry),
            driver: FixtureDriver { descriptor },
            definition,
            service_instance_id,
            evidence,
        }
    }

    async fn verify(fixture: &Fixture) -> Result<(), ConformanceVerificationError> {
        fixture
            .verifier
            .verify(
                &fixture.driver,
                &fixture.definition,
                &fixture.service_instance_id,
                &fixture.driver.descriptor,
                &fixture.evidence,
            )
            .await
    }

    #[tokio::test]
    async fn exact_trusted_attestation_is_accepted() {
        verify(&fixture()).await.expect("trusted evidence");
    }

    #[tokio::test]
    async fn driver_build_mismatch_is_rejected() {
        let mut fixture = fixture();
        fixture.evidence.driver_build_digest = "sha256:other-driver".to_string();
        let error = verify(&fixture).await.expect_err("build mismatch");
        assert!(error.reason.contains("driver build digest"));
    }

    #[tokio::test]
    async fn suite_revision_mismatch_is_rejected() {
        let mut fixture = fixture();
        fixture.evidence.suite_revision = "runtime-driver-v2".to_string();
        let error = verify(&fixture).await.expect_err("suite mismatch");
        assert!(error.reason.contains("suite revision"));
    }

    #[tokio::test]
    async fn verified_profile_mismatch_is_rejected() {
        let mut fixture = fixture();
        fixture.evidence.verified_profile_digest =
            ProfileDigest::new("sha256:other-profile").expect("profile digest");
        let error = verify(&fixture).await.expect_err("profile mismatch");
        assert!(error.reason.contains("profile digest"));
    }

    #[tokio::test]
    async fn service_identity_mismatch_is_rejected() {
        let mut fixture = fixture();
        fixture.driver.descriptor.service_instance_id =
            RuntimeServiceInstanceId::new("other-instance").expect("service instance id");
        let error = verify(&fixture).await.expect_err("service mismatch");
        assert!(error.reason.contains("service instance"));
    }

    #[tokio::test]
    async fn definition_provenance_mismatch_is_rejected() {
        let mut fixture = fixture();
        fixture.definition.provenance.build_digest =
            AgentServiceBuildDigest::new("sha256:other-definition").expect("definition build");
        let error = verify(&fixture).await.expect_err("definition mismatch");
        assert!(error.reason.contains("definition provenance"));
    }
}
