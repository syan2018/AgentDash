use std::sync::Arc;

use agentdash_agent_runtime_contract::{
    ManagedAgentRuntimeGateway, ManagedRuntimeCommand, ManagedRuntimeCommandEnvelope,
    ManagedRuntimeGatewayError, ManagedRuntimeOperationReceipt, ManagedRuntimeReadRequest,
    RuntimeIdempotencyKey, RuntimeOperationId, RuntimeProjectionRevision,
};
use agentdash_domain::agent_run_target::AgentRunTarget;
use async_trait::async_trait;
use sha2::{Digest, Sha256};

use super::{
    AgentRunAppliedResourceSurfaceMaterializationPort,
    AgentRunAppliedResourceSurfaceMaterializeRequest, AgentRunAppliedResourceSurfaceQueryPort,
    AgentRunProductRuntimeBinding, AgentRunProductRuntimeBindingStore,
};

#[derive(Debug, Clone)]
pub struct AgentRunProductRuntimeRecoveryRequest {
    pub target: AgentRunTarget,
    pub client_command_id: String,
    pub expected_revision: RuntimeProjectionRevision,
}

#[derive(Debug, Clone)]
pub struct AgentRunProductRuntimeRecoveryOutcome {
    pub binding: AgentRunProductRuntimeBinding,
    pub resource_snapshot_revision: u64,
    pub rebind_receipt: ManagedRuntimeOperationReceipt,
    pub activate_receipt: ManagedRuntimeOperationReceipt,
}

#[derive(Debug, thiserror::Error)]
pub enum AgentRunProductRuntimeRecoveryError {
    #[error("Product Runtime recovery request is invalid")]
    InvalidRequest,
    #[error("Product Runtime binding is missing")]
    BindingMissing,
    #[error("Product Runtime binding failed: {0}")]
    Binding(String),
    #[error("Product Runtime resource materialization failed: {0}")]
    ResourceSurface(String),
    #[error("Managed Runtime recovery evidence does not match the Product binding")]
    RuntimeBindingMismatch,
    #[error(transparent)]
    Runtime(#[from] ManagedRuntimeGatewayError),
}

#[async_trait]
pub trait AgentRunProductRuntimeRecoveryPort: Send + Sync {
    async fn recover(
        &self,
        request: AgentRunProductRuntimeRecoveryRequest,
    ) -> Result<AgentRunProductRuntimeRecoveryOutcome, AgentRunProductRuntimeRecoveryError>;
}

/// Product-owned recovery saga for a Host generation change.
///
/// Runtime owns the actual Rebind. Product then advances its binding CAS fence, rematerializes
/// the exact immutable resource surface, activates the rebound Runtime target, and pins the new
/// Host generation through the binding store.
pub struct AgentRunProductRuntimeRecoveryService {
    runtime: Arc<dyn ManagedAgentRuntimeGateway>,
    bindings: Arc<dyn AgentRunProductRuntimeBindingStore>,
    resources: Arc<dyn AgentRunAppliedResourceSurfaceMaterializationPort>,
    resource_query: Arc<dyn AgentRunAppliedResourceSurfaceQueryPort>,
}

impl AgentRunProductRuntimeRecoveryService {
    pub fn new(
        runtime: Arc<dyn ManagedAgentRuntimeGateway>,
        bindings: Arc<dyn AgentRunProductRuntimeBindingStore>,
        resources: Arc<dyn AgentRunAppliedResourceSurfaceMaterializationPort>,
        resource_query: Arc<dyn AgentRunAppliedResourceSurfaceQueryPort>,
    ) -> Self {
        Self {
            runtime,
            bindings,
            resources,
            resource_query,
        }
    }
}

#[async_trait]
impl AgentRunProductRuntimeRecoveryPort for AgentRunProductRuntimeRecoveryService {
    async fn recover(
        &self,
        request: AgentRunProductRuntimeRecoveryRequest,
    ) -> Result<AgentRunProductRuntimeRecoveryOutcome, AgentRunProductRuntimeRecoveryError> {
        let client_command_id = request.client_command_id.trim();
        if client_command_id.is_empty() || client_command_id.len() > 256 {
            return Err(AgentRunProductRuntimeRecoveryError::InvalidRequest);
        }
        let current_binding = self
            .bindings
            .load_product_binding(&request.target)
            .await
            .map_err(AgentRunProductRuntimeRecoveryError::Binding)?
            .ok_or(AgentRunProductRuntimeRecoveryError::BindingMissing)?;
        let previous_digest = current_binding
            .calculated_digest()
            .map_err(AgentRunProductRuntimeRecoveryError::Binding)?;
        let before = self
            .runtime
            .read(ManagedRuntimeReadRequest {
                thread_id: current_binding.runtime_thread_id.clone(),
            })
            .await?;
        let before_source = before
            .source_binding
            .as_ref()
            .ok_or(AgentRunProductRuntimeRecoveryError::RuntimeBindingMismatch)?;
        if before.thread_id != current_binding.runtime_thread_id
            || before_source.source_ref != current_binding.source_binding.source_ref
            || before_source.applied_surface_revision
                != current_binding.source_binding.applied_surface_revision
            || before_source.committed_at_revision
                < current_binding.source_binding.committed_at_revision
        {
            return Err(AgentRunProductRuntimeRecoveryError::RuntimeBindingMismatch);
        }

        let rebind_receipt = self
            .runtime
            .execute(recovery_envelope(
                &current_binding,
                client_command_id,
                "rebind",
                Some(request.expected_revision),
                ManagedRuntimeCommand::Rebind,
            )?)
            .await?;
        let rebound = self
            .runtime
            .read(ManagedRuntimeReadRequest {
                thread_id: current_binding.runtime_thread_id.clone(),
            })
            .await?;
        let rebound_source = rebound
            .source_binding
            .clone()
            .ok_or(AgentRunProductRuntimeRecoveryError::RuntimeBindingMismatch)?;
        if rebound.thread_id != current_binding.runtime_thread_id
            || rebound_source.source_ref != current_binding.source_binding.source_ref
            || rebound_source.applied_surface_revision
                != current_binding.source_binding.applied_surface_revision
            || rebound_source.committed_at_revision
                < current_binding.source_binding.committed_at_revision
        {
            return Err(AgentRunProductRuntimeRecoveryError::RuntimeBindingMismatch);
        }
        let runtime_already_activated = rebound_source.activated_at_revision.is_some();
        let pre_activation_binding = AgentRunProductRuntimeBinding {
            source_binding: rebound_source,
            ..current_binding.clone()
        };
        let rebound_digest = pre_activation_binding
            .calculated_digest()
            .map_err(AgentRunProductRuntimeRecoveryError::Binding)?;
        let recovery_already_pinned = previous_digest == rebound_digest
            && current_binding.source_binding.activated_at_revision.is_some()
            && runtime_already_activated;
        if !recovery_already_pinned {
            self.bindings
                .prepare_product_binding_recovery(&previous_digest, &pre_activation_binding)
                .await
                .map_err(AgentRunProductRuntimeRecoveryError::Binding)?;
        }
        let mut recovered_resource = self
            .resource_query
            .applied_resource_surface(&pre_activation_binding.target, None)
            .await
            .map_err(|error| {
                AgentRunProductRuntimeRecoveryError::ResourceSurface(error.to_string())
            })?;
        if recovered_resource.surface.product_binding_digest != rebound_digest {
            let expected_current_snapshot_revision = recovered_resource.snapshot_revision;
            self.resources
                .materialize(AgentRunAppliedResourceSurfaceMaterializeRequest {
                    target: pre_activation_binding.target.clone(),
                    expected_current_snapshot_revision: Some(expected_current_snapshot_revision),
                    product_binding_digest: rebound_digest.clone(),
                })
                .await
                .map_err(|error| {
                    AgentRunProductRuntimeRecoveryError::ResourceSurface(error.to_string())
                })?;
            recovered_resource = self
                .resource_query
                .applied_resource_surface(&pre_activation_binding.target, None)
                .await
                .map_err(|error| {
                    AgentRunProductRuntimeRecoveryError::ResourceSurface(error.to_string())
                })?;
        }
        let activate_receipt = self
            .runtime
            .execute(recovery_envelope(
                &pre_activation_binding,
                client_command_id,
                "activate",
                Some(rebind_receipt.accepted_revision),
                ManagedRuntimeCommand::Activate,
            )?)
            .await?;
        let activated = self
            .runtime
            .read(ManagedRuntimeReadRequest {
                thread_id: pre_activation_binding.runtime_thread_id.clone(),
            })
            .await?;
        let activated_source = activated
            .source_binding
            .clone()
            .ok_or(AgentRunProductRuntimeRecoveryError::RuntimeBindingMismatch)?;
        if activated_source.source_ref != pre_activation_binding.source_binding.source_ref
            || activated_source.committed_at_revision
                != pre_activation_binding.source_binding.committed_at_revision
            || activated_source.applied_surface_revision
                != pre_activation_binding.source_binding.applied_surface_revision
            || activated_source.activated_at_revision.is_none()
        {
            return Err(AgentRunProductRuntimeRecoveryError::RuntimeBindingMismatch);
        }
        let activated_binding = AgentRunProductRuntimeBinding {
            source_binding: activated_source,
            ..pre_activation_binding
        };
        self.bindings
            .activate_product_binding(
                &activated_binding,
                &rebound_digest,
                recovered_resource.snapshot_revision,
            )
            .await
            .map_err(AgentRunProductRuntimeRecoveryError::Binding)?;
        Ok(AgentRunProductRuntimeRecoveryOutcome {
            binding: activated_binding,
            resource_snapshot_revision: recovered_resource.snapshot_revision,
            rebind_receipt,
            activate_receipt,
        })
    }
}

fn recovery_envelope(
    binding: &AgentRunProductRuntimeBinding,
    client_command_id: &str,
    phase: &'static str,
    expected_revision: Option<RuntimeProjectionRevision>,
    command: ManagedRuntimeCommand,
) -> Result<ManagedRuntimeCommandEnvelope, AgentRunProductRuntimeRecoveryError> {
    let identity = format!(
        "{:x}",
        Sha256::digest(
            serde_json::to_vec(&(
                "agentdash.product-runtime-recovery/v1",
                binding.target.run_id,
                binding.target.agent_id,
                client_command_id,
                phase,
            ))
            .expect("Product recovery identity is serializable"),
        )
    );
    Ok(ManagedRuntimeCommandEnvelope {
        operation_id: RuntimeOperationId::new(format!("product-recovery:{phase}:{identity}"))
            .map_err(|_| AgentRunProductRuntimeRecoveryError::InvalidRequest)?,
        idempotency_key: RuntimeIdempotencyKey::new(format!(
            "product-recovery-idempotency:{phase}:{identity}"
        ))
        .map_err(|_| AgentRunProductRuntimeRecoveryError::InvalidRequest)?,
        thread_id: binding.runtime_thread_id.clone(),
        expected_revision,
        command,
    })
}
