use agentdash_agent_service_api::{
    AgentBindingGeneration, AgentPayloadDigest, AgentProfileDigest, AgentServiceInstanceId,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompleteAgentVerifiedBuildEvidence {
    pub claimed_build_digest: AgentPayloadDigest,
    pub evidence_digest: AgentPayloadDigest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompleteAgentVerificationMethod {
    PinnedBuiltin,
    RemoteTransportAttestation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompleteAgentVerificationRequest {
    pub service_instance_id: AgentServiceInstanceId,
    pub publisher_integration: String,
    pub service_version: String,
    pub claimed_build_digest: AgentPayloadDigest,
    pub profile_digest: AgentProfileDigest,
    pub claimed_conformance_suite_revision: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompleteAgentVerificationRecord {
    pub service_instance_id: AgentServiceInstanceId,
    pub expected_publisher_integration: String,
    pub expected_service_version: String,
    pub expected_build_digest: AgentPayloadDigest,
    pub expected_profile_digest: AgentProfileDigest,
    pub expected_conformance_suite_revision: String,
    pub method: CompleteAgentVerificationMethod,
    pub verifier_identity: String,
    pub verifier_revision: String,
    pub evidence_digest: AgentPayloadDigest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompleteAgentServiceVerification {
    pub service_instance_id: AgentServiceInstanceId,
    pub publisher_integration: String,
    pub service_version: String,
    pub verifier_identity: String,
    pub verifier_revision: String,
    pub method: CompleteAgentVerificationMethod,
    pub verified_profile_digest: AgentProfileDigest,
    pub claimed_conformance_suite_revision: String,
    pub verified_build: CompleteAgentVerifiedBuildEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CompleteAgentVerificationError {
    #[error("no trusted Complete Agent verification record for {service_instance_id}")]
    MissingRecord {
        service_instance_id: AgentServiceInstanceId,
    },
    #[error("Complete Agent verification claim drifted at {coordinate}")]
    ClaimDrift { coordinate: &'static str },
    #[error("Complete Agent verification record is invalid: {reason}")]
    InvalidRecord { reason: String },
}

#[async_trait]
pub trait CompleteAgentRegistrationVerifier: Send + Sync {
    async fn verify(
        &self,
        request: CompleteAgentVerificationRequest,
    ) -> Result<CompleteAgentServiceVerification, CompleteAgentVerificationError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompleteAgentRemoteBindingFact {
    pub local_service_instance_id: AgentServiceInstanceId,
    pub remote_service_instance_id: AgentServiceInstanceId,
    pub remote_binding_generation: AgentBindingGeneration,
    pub host_incarnation_id: String,
    pub transport_id: String,
}
