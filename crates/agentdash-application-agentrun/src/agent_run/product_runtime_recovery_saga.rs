use agentdash_agent_runtime_contract::{
    ManagedRuntimeOperationReceipt, RuntimeIdempotencyKey, RuntimeOperationId,
    RuntimeProjectionRevision, RuntimeThreadId,
};
use agentdash_domain::agent_run_target::AgentRunTarget;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use super::AgentRunProductRuntimeBinding;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AgentRunProductRuntimeRecoveryId(String);

impl AgentRunProductRuntimeRecoveryId {
    pub fn for_request(target: &AgentRunTarget, client_command_id: &str) -> Result<Self, String> {
        let client_command_id = client_command_id.trim();
        if client_command_id.is_empty() || client_command_id.len() > 256 {
            return Err("Product Runtime recovery command identity is invalid".to_string());
        }
        let identity = serde_json::to_vec(&(
            "agentdash.product-runtime-recovery/v2",
            target.run_id,
            target.agent_id,
            client_command_id,
        ))
        .map_err(|error| error.to_string())?;
        Ok(Self(format!("sha256:{:x}", Sha256::digest(identity))))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn from_persisted(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err("Persisted Product Runtime recovery identity is empty".to_string());
        }
        Ok(Self(value))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRunProductRuntimeRecoveryPhase {
    Requested,
    RebindApplied,
    ProductBindingPrepared,
    ResourceMaterialized,
    RuntimeActivated,
    Succeeded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentRunProductRuntimeRecoveryOperationIdentity {
    pub operation_id: RuntimeOperationId,
    pub idempotency_key: RuntimeIdempotencyKey,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentRunProductRuntimeRecoverySaga {
    recovery_id: AgentRunProductRuntimeRecoveryId,
    target: AgentRunTarget,
    client_command_id: String,
    runtime_thread_id: RuntimeThreadId,
    rebind_expected_revision: RuntimeProjectionRevision,
    previous_binding: AgentRunProductRuntimeBinding,
    previous_binding_digest: String,
    rebind_identity: AgentRunProductRuntimeRecoveryOperationIdentity,
    activate_identity: AgentRunProductRuntimeRecoveryOperationIdentity,
    phase: AgentRunProductRuntimeRecoveryPhase,
    version: u64,
    rebind_receipt: Option<ManagedRuntimeOperationReceipt>,
    prepared_binding: Option<AgentRunProductRuntimeBinding>,
    prepared_binding_digest: Option<String>,
    resource_snapshot_revision: Option<u64>,
    activate_receipt: Option<ManagedRuntimeOperationReceipt>,
    activated_binding: Option<AgentRunProductRuntimeBinding>,
}

impl AgentRunProductRuntimeRecoverySaga {
    pub fn requested(
        target: AgentRunTarget,
        client_command_id: impl Into<String>,
        rebind_expected_revision: RuntimeProjectionRevision,
        previous_binding: AgentRunProductRuntimeBinding,
    ) -> Result<Self, String> {
        let client_command_id = client_command_id.into();
        let recovery_id =
            AgentRunProductRuntimeRecoveryId::for_request(&target, &client_command_id)?;
        if previous_binding.target != target {
            return Err("Product Runtime recovery binding target does not match".to_string());
        }
        let previous_binding_digest = previous_binding.calculated_digest()?;
        let rebind_identity = operation_identity(&recovery_id, "rebind")?;
        let activate_identity = operation_identity(&recovery_id, "activate")?;
        Ok(Self {
            recovery_id,
            target,
            client_command_id,
            runtime_thread_id: previous_binding.runtime_thread_id.clone(),
            rebind_expected_revision,
            previous_binding,
            previous_binding_digest,
            rebind_identity,
            activate_identity,
            phase: AgentRunProductRuntimeRecoveryPhase::Requested,
            version: 0,
            rebind_receipt: None,
            prepared_binding: None,
            prepared_binding_digest: None,
            resource_snapshot_revision: None,
            activate_receipt: None,
            activated_binding: None,
        })
    }

    pub fn recovery_id(&self) -> &AgentRunProductRuntimeRecoveryId {
        &self.recovery_id
    }

    pub fn target(&self) -> &AgentRunTarget {
        &self.target
    }

    pub fn client_command_id(&self) -> &str {
        &self.client_command_id
    }

    pub fn runtime_thread_id(&self) -> &RuntimeThreadId {
        &self.runtime_thread_id
    }

    pub fn rebind_expected_revision(&self) -> RuntimeProjectionRevision {
        self.rebind_expected_revision
    }

    pub fn previous_binding(&self) -> &AgentRunProductRuntimeBinding {
        &self.previous_binding
    }

    pub fn previous_binding_digest(&self) -> &str {
        &self.previous_binding_digest
    }

    pub fn rebind_identity(&self) -> &AgentRunProductRuntimeRecoveryOperationIdentity {
        &self.rebind_identity
    }

    pub fn activate_identity(&self) -> &AgentRunProductRuntimeRecoveryOperationIdentity {
        &self.activate_identity
    }

    pub fn phase(&self) -> AgentRunProductRuntimeRecoveryPhase {
        self.phase
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn rebind_receipt(&self) -> Option<&ManagedRuntimeOperationReceipt> {
        self.rebind_receipt.as_ref()
    }

    pub fn prepared_binding(&self) -> Option<&AgentRunProductRuntimeBinding> {
        self.prepared_binding.as_ref()
    }

    pub fn prepared_binding_digest(&self) -> Option<&str> {
        self.prepared_binding_digest.as_deref()
    }

    pub fn resource_snapshot_revision(&self) -> Option<u64> {
        self.resource_snapshot_revision
    }

    pub fn activate_receipt(&self) -> Option<&ManagedRuntimeOperationReceipt> {
        self.activate_receipt.as_ref()
    }

    pub fn activated_binding(&self) -> Option<&AgentRunProductRuntimeBinding> {
        self.activated_binding.as_ref()
    }

    pub fn record_rebind_applied(
        mut self,
        receipt: ManagedRuntimeOperationReceipt,
        prepared_binding: AgentRunProductRuntimeBinding,
    ) -> Result<Self, String> {
        self.require_phase(AgentRunProductRuntimeRecoveryPhase::Requested)?;
        self.validate_receipt(&receipt, &self.rebind_identity)?;
        if prepared_binding.target != self.target
            || prepared_binding.runtime_thread_id != self.runtime_thread_id
            || prepared_binding.source_binding.source_ref
                != self.previous_binding.source_binding.source_ref
            || prepared_binding
                .source_binding
                .activated_at_revision
                .is_some()
        {
            return Err("Rebind evidence does not match the durable recovery intent".to_string());
        }
        let prepared_binding_digest = prepared_binding.calculated_digest()?;
        self.rebind_receipt = Some(receipt);
        self.prepared_binding = Some(prepared_binding);
        self.prepared_binding_digest = Some(prepared_binding_digest);
        self.phase = AgentRunProductRuntimeRecoveryPhase::RebindApplied;
        Ok(self)
    }

    pub fn record_product_binding_prepared(mut self) -> Result<Self, String> {
        self.require_phase(AgentRunProductRuntimeRecoveryPhase::RebindApplied)?;
        self.phase = AgentRunProductRuntimeRecoveryPhase::ProductBindingPrepared;
        Ok(self)
    }

    pub fn record_resource_materialized(mut self, snapshot_revision: u64) -> Result<Self, String> {
        self.require_phase(AgentRunProductRuntimeRecoveryPhase::ProductBindingPrepared)?;
        if snapshot_revision == 0 {
            return Err("Product recovery resource snapshot revision must be positive".to_string());
        }
        self.resource_snapshot_revision = Some(snapshot_revision);
        self.phase = AgentRunProductRuntimeRecoveryPhase::ResourceMaterialized;
        Ok(self)
    }

    pub fn record_runtime_activated(
        mut self,
        receipt: ManagedRuntimeOperationReceipt,
        activated_binding: AgentRunProductRuntimeBinding,
    ) -> Result<Self, String> {
        self.require_phase(AgentRunProductRuntimeRecoveryPhase::ResourceMaterialized)?;
        self.validate_receipt(&receipt, &self.activate_identity)?;
        let activated_at_revision = activated_binding
            .source_binding
            .activated_at_revision
            .ok_or_else(|| {
                "Activation evidence does not match the durable recovery intent".to_string()
            })?;
        let mut pre_activation_binding = activated_binding.clone();
        pre_activation_binding.source_binding.activated_at_revision = None;
        if activated_binding.target != self.target
            || activated_binding.runtime_thread_id != self.runtime_thread_id
            || activated_at_revision != receipt.accepted_revision
            || pre_activation_binding.calculated_digest()?
                != self
                    .prepared_binding_digest
                    .as_deref()
                    .ok_or_else(|| "Product recovery prepared binding is missing".to_string())?
        {
            return Err(
                "Activation evidence does not match the durable recovery intent".to_string(),
            );
        }
        self.activate_receipt = Some(receipt);
        self.activated_binding = Some(activated_binding);
        self.phase = AgentRunProductRuntimeRecoveryPhase::RuntimeActivated;
        Ok(self)
    }

    pub fn record_succeeded(mut self) -> Result<Self, String> {
        self.require_phase(AgentRunProductRuntimeRecoveryPhase::RuntimeActivated)?;
        self.phase = AgentRunProductRuntimeRecoveryPhase::Succeeded;
        Ok(self)
    }

    pub fn advance_persisted_version(mut self, expected_version: u64) -> Result<Self, String> {
        if self.version != expected_version {
            return Err(format!(
                "Product Runtime recovery version conflict: expected {expected_version}, actual {}",
                self.version
            ));
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or_else(|| "Product Runtime recovery version overflow".to_string())?;
        Ok(self)
    }

    fn require_phase(&self, expected: AgentRunProductRuntimeRecoveryPhase) -> Result<(), String> {
        if self.phase == expected {
            Ok(())
        } else {
            Err(format!(
                "Product Runtime recovery phase conflict: expected {expected:?}, actual {:?}",
                self.phase
            ))
        }
    }

    fn validate_receipt(
        &self,
        receipt: &ManagedRuntimeOperationReceipt,
        identity: &AgentRunProductRuntimeRecoveryOperationIdentity,
    ) -> Result<(), String> {
        if receipt.operation_id != identity.operation_id
            || receipt.thread_id != self.runtime_thread_id
        {
            return Err("Runtime recovery receipt identity does not match".to_string());
        }
        Ok(())
    }
}

fn operation_identity(
    recovery_id: &AgentRunProductRuntimeRecoveryId,
    phase: &'static str,
) -> Result<AgentRunProductRuntimeRecoveryOperationIdentity, String> {
    let identity = format!(
        "{:x}",
        Sha256::digest(
            serde_json::to_vec(&(
                "agentdash.product-runtime-recovery-operation/v2",
                recovery_id.as_str(),
                phase,
            ))
            .map_err(|error| error.to_string())?,
        )
    );
    Ok(AgentRunProductRuntimeRecoveryOperationIdentity {
        operation_id: RuntimeOperationId::new(format!("product-recovery:{phase}:{identity}"))
            .map_err(|error| error.to_string())?,
        idempotency_key: RuntimeIdempotencyKey::new(format!(
            "product-recovery-idempotency:{phase}:{identity}"
        ))
        .map_err(|error| error.to_string())?,
    })
}

#[derive(Debug, Error)]
pub enum AgentRunProductRuntimeRecoveryRepositoryError {
    #[error("Product Runtime recovery saga already exists")]
    AlreadyExists,
    #[error("Product Runtime recovery saga was not found")]
    NotFound,
    #[error("Product Runtime recovery saga write conflicted")]
    Conflict,
    #[error("Product Runtime recovery saga repository is unavailable: {0}")]
    Unavailable(String),
}

#[async_trait]
pub trait AgentRunProductRuntimeRecoverySagaRepository: Send + Sync {
    async fn create(
        &self,
        saga: AgentRunProductRuntimeRecoverySaga,
    ) -> Result<AgentRunProductRuntimeRecoverySaga, AgentRunProductRuntimeRecoveryRepositoryError>;

    async fn load(
        &self,
        recovery_id: &AgentRunProductRuntimeRecoveryId,
    ) -> Result<
        Option<AgentRunProductRuntimeRecoverySaga>,
        AgentRunProductRuntimeRecoveryRepositoryError,
    >;

    async fn list_recoverable(
        &self,
        limit: usize,
    ) -> Result<Vec<AgentRunProductRuntimeRecoveryId>, AgentRunProductRuntimeRecoveryRepositoryError>;

    async fn save(
        &self,
        expected_version: u64,
        saga: AgentRunProductRuntimeRecoverySaga,
    ) -> Result<AgentRunProductRuntimeRecoverySaga, AgentRunProductRuntimeRecoveryRepositoryError>;
}

#[cfg(test)]
mod tests {
    use agentdash_agent_runtime_contract::{
        ManagedRuntimeOperationStatus, ManagedRuntimeSourceBindingEvidence, RuntimeSourceRef,
        SurfaceRevision,
    };
    use uuid::Uuid;

    use super::*;
    use crate::agent_run::ProductAgentFrameRef;

    fn fixture_execution_profile() -> crate::agent_run::ProductExecutionProfileRef {
        let mut profile = crate::agent_run::ProductExecutionProfileRef {
            profile_key: "codex".to_owned(),
            profile_revision: 1,
            profile_digest: String::new(),
            configuration: serde_json::json!({"executor": "codex"}),
            credential_scope: None,
        };
        profile.refresh_digest();
        profile
    }

    fn binding(target: AgentRunTarget, activated_at: Option<u64>) -> AgentRunProductRuntimeBinding {
        let agent_id = target.agent_id;
        AgentRunProductRuntimeBinding {
            target,
            runtime_thread_id: RuntimeThreadId::new("runtime-thread").unwrap(),
            launch_frame: ProductAgentFrameRef {
                frame_id: Uuid::new_v4(),
                agent_id,
                revision: 3,
            },
            execution_profile_digest: fixture_execution_profile().profile_digest,
            execution_profile: fixture_execution_profile(),
            source_binding: ManagedRuntimeSourceBindingEvidence {
                source_ref: RuntimeSourceRef::new("source").unwrap(),
                committed_at_revision: RuntimeProjectionRevision(8),
                applied_surface_revision: SurfaceRevision(5),
                activated_at_revision: activated_at.map(RuntimeProjectionRevision),
            },
        }
    }

    fn receipt(
        identity: &AgentRunProductRuntimeRecoveryOperationIdentity,
        accepted_revision: u64,
    ) -> ManagedRuntimeOperationReceipt {
        ManagedRuntimeOperationReceipt {
            operation_id: identity.operation_id.clone(),
            thread_id: RuntimeThreadId::new("runtime-thread").unwrap(),
            accepted_revision: RuntimeProjectionRevision(accepted_revision),
            status: ManagedRuntimeOperationStatus::Succeeded,
            evidence: None,
            duplicate: false,
        }
    }

    #[test]
    fn durable_recovery_freezes_revision_and_operation_identities_across_roundtrip() {
        let target = AgentRunTarget {
            run_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
        };
        let saga = AgentRunProductRuntimeRecoverySaga::requested(
            target.clone(),
            "remote-placement-epoch",
            RuntimeProjectionRevision(11),
            binding(target, Some(8)),
        )
        .unwrap()
        .advance_persisted_version(0)
        .unwrap();

        let restored: AgentRunProductRuntimeRecoverySaga =
            serde_json::from_value(serde_json::to_value(&saga).unwrap()).unwrap();

        assert_eq!(restored.recovery_id(), saga.recovery_id());
        assert_eq!(
            restored.rebind_expected_revision(),
            RuntimeProjectionRevision(11)
        );
        assert_eq!(restored.rebind_identity(), saga.rebind_identity());
        assert_eq!(restored.activate_identity(), saga.activate_identity());
    }

    #[test]
    fn durable_recovery_records_the_post_rebind_revision_for_activation() {
        let target = AgentRunTarget {
            run_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
        };
        let saga = AgentRunProductRuntimeRecoverySaga::requested(
            target.clone(),
            "remote-placement-epoch",
            RuntimeProjectionRevision(11),
            binding(target.clone(), Some(8)),
        )
        .unwrap();
        let rebind_receipt = receipt(saga.rebind_identity(), 12);
        let prepared_binding = binding(target.clone(), None);
        let saga = saga
            .record_rebind_applied(rebind_receipt, prepared_binding.clone())
            .unwrap()
            .record_product_binding_prepared()
            .unwrap()
            .record_resource_materialized(4)
            .unwrap();
        let activate_receipt = receipt(saga.activate_identity(), 13);
        let mut activated_binding = prepared_binding;
        activated_binding.source_binding.activated_at_revision =
            Some(RuntimeProjectionRevision(13));
        let saga = saga
            .record_runtime_activated(activate_receipt, activated_binding)
            .unwrap()
            .record_succeeded()
            .unwrap();

        assert_eq!(
            saga.rebind_receipt().unwrap().accepted_revision,
            RuntimeProjectionRevision(12)
        );
        assert_eq!(
            saga.activate_receipt().unwrap().accepted_revision,
            RuntimeProjectionRevision(13)
        );
        assert_eq!(saga.phase(), AgentRunProductRuntimeRecoveryPhase::Succeeded);
    }
}
