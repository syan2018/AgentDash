use agentdash_agent_runtime_contract::{
    BoundRuntimeHookPlan, DriverThreadId, ProfileDigest, ProfileProvenance, RuntimeBindingId,
    RuntimeDriverGeneration, RuntimeProfile, RuntimeThreadId, SurfaceDigest,
};
use agentdash_spi::AuthIdentity;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, OnceLock};
use thiserror::Error;
use uuid::Uuid;

use crate::launch::BackendSelectionInput;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct AgentRunRuntimeTarget {
    pub run_id: Uuid,
    pub agent_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentRunRuntimeProvisionRequest {
    pub target: AgentRunRuntimeTarget,
    pub identity: Option<AuthIdentity>,
    pub backend_selection: Option<BackendSelectionInput>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentRunRuntimeBinding {
    pub target: AgentRunRuntimeTarget,
    pub thread_id: RuntimeThreadId,
    pub binding_id: RuntimeBindingId,
    pub driver_generation: RuntimeDriverGeneration,
    pub source_thread_id: DriverThreadId,
    pub profile_digest: ProfileDigest,
    pub profile_provenance: ProfileProvenance,
    pub bound_profile: RuntimeProfile,
    pub surface_digest: SurfaceDigest,
    pub settings_revision: agentdash_agent_runtime_contract::ThreadSettingsRevision,
    pub tool_set_revision: agentdash_agent_runtime_contract::ToolSetRevision,
    pub hook_plan: BoundRuntimeHookPlan,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AgentRunRuntimeBindingError {
    #[error("AgentRun runtime binding was not found")]
    NotFound,
    #[error("AgentRun runtime binding already exists with different coordinates")]
    Conflict,
    #[error("AgentRun runtime binding is unavailable: {reason}")]
    Unavailable { reason: String, retryable: bool },
    #[error("AgentRun runtime binding persistence failed: {reason}")]
    Persistence { reason: String },
}

#[async_trait]
pub trait AgentRunRuntimeBindingRepository: Send + Sync {
    async fn load(
        &self,
        target: &AgentRunRuntimeTarget,
    ) -> Result<Option<AgentRunRuntimeBinding>, AgentRunRuntimeBindingError>;

    async fn load_by_thread_id(
        &self,
        thread_id: &RuntimeThreadId,
    ) -> Result<Option<AgentRunRuntimeBinding>, AgentRunRuntimeBindingError>;

    async fn list_by_run(
        &self,
        run_id: Uuid,
    ) -> Result<Vec<AgentRunRuntimeBinding>, AgentRunRuntimeBindingError>;

    async fn list_by_agent(
        &self,
        agent_id: Uuid,
    ) -> Result<Vec<AgentRunRuntimeBinding>, AgentRunRuntimeBindingError>;

    async fn insert(
        &self,
        binding: AgentRunRuntimeBinding,
    ) -> Result<AgentRunRuntimeBinding, AgentRunRuntimeBindingError>;
}

/// Product-facing provisioning seam implemented by the production composition root.
///
/// A successful result references a durable active Host binding. AgentRun use cases never see
/// Integration offers, placement transports, factory selection or driver source mapping.
#[async_trait]
pub trait AgentRunRuntimeProvisioner: Send + Sync {
    async fn provision(
        &self,
        request: &AgentRunRuntimeProvisionRequest,
    ) -> Result<AgentRunRuntimeBinding, AgentRunRuntimeBindingError>;
}

#[derive(Clone, Default)]
pub struct SharedAgentRunRuntimeProvisionerHandle {
    inner: Arc<OnceLock<Arc<dyn AgentRunRuntimeProvisioner>>>,
}

impl SharedAgentRunRuntimeProvisionerHandle {
    pub fn set(
        &self,
        provisioner: Arc<dyn AgentRunRuntimeProvisioner>,
    ) -> Result<(), Arc<dyn AgentRunRuntimeProvisioner>> {
        self.inner.set(provisioner)
    }

    pub fn is_bound(&self) -> bool {
        self.inner.get().is_some()
    }
}

#[async_trait]
impl AgentRunRuntimeProvisioner for SharedAgentRunRuntimeProvisionerHandle {
    async fn provision(
        &self,
        request: &AgentRunRuntimeProvisionRequest,
    ) -> Result<AgentRunRuntimeBinding, AgentRunRuntimeBindingError> {
        let provisioner =
            self.inner
                .get()
                .ok_or_else(|| AgentRunRuntimeBindingError::Unavailable {
                    reason: "AgentRun runtime composition 尚未绑定 provisioner".to_string(),
                    retryable: true,
                })?;
        provisioner.provision(request).await
    }
}
