use agentdash_agent_runtime_contract::{
    AgentRuntimeDriver, ProfileDigest, RuntimeBindingId, RuntimeDescriptor,
    RuntimeDriverGeneration, RuntimeServiceInstanceId, RuntimeThreadId,
};
use agentdash_integration_api::{AgentServiceDefinition, AgentServiceOfferId};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use thiserror::Error;

use crate::{
    AgentServiceInstance, AppliedSurface, ConformanceEvidence, DriverLease, RuntimeBinding,
    RuntimeDriverCoordinate, RuntimeOffer, RuntimeSourceCoordinate,
};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("driver conformance verification failed: {reason}")]
pub struct ConformanceVerificationError {
    pub reason: String,
}

#[async_trait]
pub trait DriverConformanceVerifier: Send + Sync {
    async fn verify(
        &self,
        driver: &dyn AgentRuntimeDriver,
        definition: &AgentServiceDefinition,
        expected_service_instance_id: &RuntimeServiceInstanceId,
        descriptor: &RuntimeDescriptor,
        evidence: &ConformanceEvidence,
    ) -> Result<(), ConformanceVerificationError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum HostStoreError {
    #[error("host fact was not found: {entity} {id}")]
    NotFound { entity: &'static str, id: String },
    #[error(
        "host fact revision conflict for {entity} {id}: expected {expected:?}, actual {actual:?}"
    )]
    Conflict {
        entity: &'static str,
        id: String,
        expected: Option<u64>,
        actual: Option<u64>,
    },
    #[error("host fact violates an invariant: {reason}")]
    Invariant { reason: String },
    #[error("host persistence failed: {reason}")]
    Persistence { reason: String },
}

#[async_trait]
pub trait AgentRuntimeHostRepository: Send + Sync {
    async fn load_instance(
        &self,
        id: &RuntimeServiceInstanceId,
    ) -> Result<Option<AgentServiceInstance>, HostStoreError>;

    async fn put_instance(
        &self,
        instance: AgentServiceInstance,
        expected_revision: Option<u64>,
    ) -> Result<AgentServiceInstance, HostStoreError>;

    async fn next_generation(
        &self,
        instance_id: &RuntimeServiceInstanceId,
        expected_revision: u64,
    ) -> Result<RuntimeDriverGeneration, HostStoreError>;

    async fn commit_activation(
        &self,
        instance: AgentServiceInstance,
        offer: RuntimeOffer,
    ) -> Result<(), HostStoreError>;

    async fn load_activation_instance(
        &self,
        instance_id: &RuntimeServiceInstanceId,
        generation: RuntimeDriverGeneration,
    ) -> Result<Option<AgentServiceInstance>, HostStoreError>;

    async fn load_offer(
        &self,
        id: &AgentServiceOfferId,
    ) -> Result<Option<RuntimeOffer>, HostStoreError>;

    async fn list_offers(&self) -> Result<Vec<RuntimeOffer>, HostStoreError>;

    async fn disable_offers(
        &self,
        instance_id: &RuntimeServiceInstanceId,
    ) -> Result<(), HostStoreError>;

    async fn set_observed_state(
        &self,
        instance_id: &RuntimeServiceInstanceId,
        expected_revision: u64,
        observed: crate::ServiceInstanceObservedState,
    ) -> Result<(), HostStoreError>;

    async fn reserve_binding(&self, binding: RuntimeBinding) -> Result<(), HostStoreError>;

    async fn activate_binding(
        &self,
        binding_id: &RuntimeBindingId,
        expected_generation: RuntimeDriverGeneration,
        applied: AppliedSurface,
        driver_binding_id: agentdash_agent_runtime_contract::DriverBindingId,
        source: RuntimeSourceCoordinate,
    ) -> Result<RuntimeBinding, HostStoreError>;

    async fn load_binding(
        &self,
        id: &RuntimeBindingId,
    ) -> Result<Option<RuntimeBinding>, HostStoreError>;

    async fn pending_bindings(&self) -> Result<Vec<RuntimeBinding>, HostStoreError>;

    async fn record_apply(
        &self,
        binding_id: &RuntimeBindingId,
        expected_generation: RuntimeDriverGeneration,
        applied: AppliedSurface,
    ) -> Result<RuntimeBinding, HostStoreError>;

    async fn fail_binding(
        &self,
        binding_id: &RuntimeBindingId,
        expected_generation: RuntimeDriverGeneration,
    ) -> Result<(), HostStoreError>;

    async fn find_binding_by_thread(
        &self,
        thread_id: &RuntimeThreadId,
    ) -> Result<Option<RuntimeBinding>, HostStoreError>;

    async fn find_source(
        &self,
        binding_id: &RuntimeBindingId,
        generation: RuntimeDriverGeneration,
    ) -> Result<Option<RuntimeSourceCoordinate>, HostStoreError>;

    async fn record_driver_coordinate(
        &self,
        binding_id: &RuntimeBindingId,
        generation: RuntimeDriverGeneration,
        coordinate: RuntimeDriverCoordinate,
    ) -> Result<(), HostStoreError>;

    async fn acquire_lease(
        &self,
        binding_id: &RuntimeBindingId,
        generation: RuntimeDriverGeneration,
        owner: &str,
        now: DateTime<Utc>,
        expires_at: DateTime<Utc>,
    ) -> Result<DriverLease, HostStoreError>;

    async fn validate_lease(
        &self,
        binding_id: &RuntimeBindingId,
        generation: RuntimeDriverGeneration,
        owner: &str,
        token: &str,
        now: DateTime<Utc>,
    ) -> Result<DriverLease, HostStoreError>;

    async fn renew_lease(
        &self,
        binding_id: &RuntimeBindingId,
        generation: RuntimeDriverGeneration,
        owner: &str,
        token: &str,
        now: DateTime<Utc>,
        expires_at: DateTime<Utc>,
    ) -> Result<DriverLease, HostStoreError>;

    async fn release_lease(
        &self,
        binding_id: &RuntimeBindingId,
        generation: RuntimeDriverGeneration,
        owner: &str,
        token: &str,
    ) -> Result<(), HostStoreError>;

    async fn mark_binding_lost(
        &self,
        binding_id: &RuntimeBindingId,
        generation: RuntimeDriverGeneration,
    ) -> Result<(), HostStoreError>;

    async fn profile_digest_for_binding(
        &self,
        binding_id: &RuntimeBindingId,
    ) -> Result<Option<ProfileDigest>, HostStoreError>;
}
