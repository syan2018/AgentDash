use agentdash_agent_runtime_contract::{
    BindingEpoch, DriverThreadId, DriverTurnId, PresentationThreadId, ProfileDigest,
    ProfileProvenance, RuntimeBindingId, RuntimeDriverGeneration, RuntimeProfile, RuntimeRevision,
    RuntimeSurfaceDescriptor, RuntimeTerminalHookEffectBinding, RuntimeThreadId,
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
    pub presentation_thread_id: PresentationThreadId,
    pub identity: Option<AuthIdentity>,
    pub backend_selection: Option<BackendSelectionInput>,
    pub fork: Option<AgentRunRuntimeForkSource>,
    pub terminal_hook_effect_binding: Option<RuntimeTerminalHookEffectBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentRunRuntimeForkSource {
    pub source_target: AgentRunRuntimeTarget,
    pub through_source_turn_id: Option<DriverTurnId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentRunRuntimeBinding {
    pub target: AgentRunRuntimeTarget,
    pub presentation_thread_id: PresentationThreadId,
    pub thread_id: RuntimeThreadId,
    pub binding_id: RuntimeBindingId,
    pub binding_epoch: BindingEpoch,
    pub driver_generation: RuntimeDriverGeneration,
    pub source_thread_id: DriverThreadId,
    pub profile_digest: ProfileDigest,
    pub profile_provenance: ProfileProvenance,
    pub bound_profile: RuntimeProfile,
    pub surface: RuntimeSurfaceDescriptor,
    pub settings_revision: agentdash_agent_runtime_contract::ThreadSettingsRevision,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRunRuntimeRecoveryState {
    Prepared,
    HostBound,
    Committed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentRunRuntimeRecoveryIntent {
    pub id: String,
    pub target: AgentRunRuntimeTarget,
    pub thread_id: RuntimeThreadId,
    pub expected_old_binding_id: RuntimeBindingId,
    pub expected_old_generation: RuntimeDriverGeneration,
    pub expected_runtime_revision: RuntimeRevision,
    pub binding_epoch: BindingEpoch,
    pub proposed_binding_id: RuntimeBindingId,
    pub selected_offer_id: String,
    pub source_thread_id: DriverThreadId,
    pub state: AgentRunRuntimeRecoveryState,
    pub failure_reason: Option<String>,
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
pub trait AgentRunRuntimePresentationPlanStore: Send + Sync {
    async fn load_exact_presentation_plan(
        &self,
        binding_id: &RuntimeBindingId,
        surface_revision: agentdash_agent_runtime_contract::SurfaceRevision,
        surface_digest: &agentdash_agent_runtime_contract::SurfaceDigest,
    ) -> Result<agentdash_agent_runtime::RuntimeSurfacePresentationPlan, AgentRunRuntimeBindingError>;
}

#[derive(Debug, Clone, Default)]
pub struct AgentRunTurnStartContextFacts {
    pub pending_actions: Vec<agentdash_spi::HookPendingAction>,
    pub notices: Vec<agentdash_spi::HookTurnStartNotice>,
}

#[async_trait]
pub trait AgentRunTurnStartContextSource: Send + Sync {
    async fn take_turn_start_context(
        &self,
        binding_id: &RuntimeBindingId,
    ) -> Result<AgentRunTurnStartContextFacts, AgentRunRuntimeBindingError>;
    async fn acknowledge_turn_start_context(
        &self,
        binding_id: &RuntimeBindingId,
        notice_ids: &[String],
    ) -> Result<(), AgentRunRuntimeBindingError>;
}

#[derive(Default)]
pub struct EmptyAgentRunTurnStartContextSource;

#[async_trait]
impl AgentRunTurnStartContextSource for EmptyAgentRunTurnStartContextSource {
    async fn take_turn_start_context(
        &self,
        _binding_id: &RuntimeBindingId,
    ) -> Result<AgentRunTurnStartContextFacts, AgentRunRuntimeBindingError> {
        Ok(AgentRunTurnStartContextFacts::default())
    }
    async fn acknowledge_turn_start_context(
        &self,
        _binding_id: &RuntimeBindingId,
        _notice_ids: &[String],
    ) -> Result<(), AgentRunRuntimeBindingError> {
        Ok(())
    }
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

    async fn lineage(
        &self,
        target: &AgentRunRuntimeTarget,
    ) -> Result<Vec<AgentRunRuntimeBinding>, AgentRunRuntimeBindingError> {
        self.load(target)
            .await
            .map(|binding| binding.into_iter().collect())
    }

    async fn append_lineage(
        &self,
        expected: &AgentRunRuntimeBinding,
        binding: AgentRunRuntimeBinding,
        recovery_intent_id: &str,
    ) -> Result<AgentRunRuntimeBinding, AgentRunRuntimeBindingError> {
        let _ = (expected, binding, recovery_intent_id);
        Err(AgentRunRuntimeBindingError::Unavailable {
            reason: "binding lineage is not implemented".to_string(),
            retryable: false,
        })
    }

    async fn load_active_recovery(
        &self,
        target: &AgentRunRuntimeTarget,
    ) -> Result<Option<AgentRunRuntimeRecoveryIntent>, AgentRunRuntimeBindingError> {
        let _ = target;
        Ok(None)
    }

    async fn load_latest_recovery(
        &self,
        target: &AgentRunRuntimeTarget,
    ) -> Result<Option<AgentRunRuntimeRecoveryIntent>, AgentRunRuntimeBindingError> {
        self.load_active_recovery(target).await
    }

    async fn prepare_recovery(
        &self,
        intent: AgentRunRuntimeRecoveryIntent,
    ) -> Result<AgentRunRuntimeRecoveryIntent, AgentRunRuntimeBindingError> {
        let _ = intent;
        Err(AgentRunRuntimeBindingError::Unavailable {
            reason: "binding recovery is not implemented".to_string(),
            retryable: false,
        })
    }

    async fn advance_recovery(
        &self,
        intent_id: &str,
        expected: AgentRunRuntimeRecoveryState,
        next: AgentRunRuntimeRecoveryState,
        failure_reason: Option<String>,
    ) -> Result<AgentRunRuntimeRecoveryIntent, AgentRunRuntimeBindingError> {
        let _ = (intent_id, expected, next, failure_reason);
        Err(AgentRunRuntimeBindingError::Unavailable {
            reason: "binding recovery is not implemented".to_string(),
            retryable: false,
        })
    }
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

    async fn recover(
        &self,
        binding: &AgentRunRuntimeBinding,
        runtime_revision: RuntimeRevision,
    ) -> Result<AgentRunRuntimeBinding, AgentRunRuntimeBindingError> {
        let _ = (binding, runtime_revision);
        Err(AgentRunRuntimeBindingError::Unavailable {
            reason: "AgentRun runtime recovery is not available".to_string(),
            retryable: true,
        })
    }
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

    async fn recover(
        &self,
        binding: &AgentRunRuntimeBinding,
        runtime_revision: RuntimeRevision,
    ) -> Result<AgentRunRuntimeBinding, AgentRunRuntimeBindingError> {
        let provisioner =
            self.inner
                .get()
                .ok_or_else(|| AgentRunRuntimeBindingError::Unavailable {
                    reason: "AgentRun runtime composition 尚未绑定 provisioner".to_string(),
                    retryable: true,
                })?;
        provisioner.recover(binding, runtime_revision).await
    }
}
