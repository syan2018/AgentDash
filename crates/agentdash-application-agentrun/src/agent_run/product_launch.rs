use std::sync::Arc;

use agentdash_agent_runtime_contract::{
    ManagedAgentRuntimeGateway, ManagedRuntimeCommand, ManagedRuntimeCommandEnvelope,
    ManagedRuntimeContentBlock, ManagedRuntimeGatewayError, ManagedRuntimeInitialContextPackage,
    ManagedRuntimeOperationReceipt, ManagedRuntimeReadRequest, RuntimeIdempotencyKey,
    RuntimeOperationId,
};
use async_trait::async_trait;
use sha2::{Digest, Sha256};

use super::{
    AgentRunAppliedResourceSurfaceMaterializationPort,
    AgentRunAppliedResourceSurfaceMaterializeRequest, AgentRunAppliedResourceSurfaceQueryPort,
    AgentRunProductRuntimeBinding, AgentRunProductRuntimeBindingStore,
    AgentRunProductRuntimeProvisioningPort, AgentRunProductRuntimeProvisioningRequest,
};

#[derive(Debug, Clone)]
pub struct AgentRunProductLaunchRequest {
    pub provisioning: AgentRunProductRuntimeProvisioningRequest,
    pub initial_context: Option<ManagedRuntimeInitialContextPackage>,
    pub initial_input: Vec<ManagedRuntimeContentBlock>,
}

#[derive(Debug, Clone)]
pub struct AgentRunProductLaunchOutcome {
    pub binding: AgentRunProductRuntimeBinding,
    pub resource_snapshot_revision: u64,
    pub create_receipt: ManagedRuntimeOperationReceipt,
    pub activate_receipt: ManagedRuntimeOperationReceipt,
    pub input_receipt: Option<ManagedRuntimeOperationReceipt>,
}

#[derive(Debug, thiserror::Error)]
pub enum AgentRunProductLaunchError {
    #[error(transparent)]
    Provisioning(#[from] super::AgentRunProductRuntimeProvisioningError),
    #[error("Product launch request is invalid: {0}")]
    Invalid(String),
    #[error(transparent)]
    Runtime(#[from] ManagedRuntimeGatewayError),
    #[error("Product Runtime binding persistence failed: {0}")]
    Binding(String),
    #[error("Product applied resource surface failed: {0}")]
    ResourceSurface(String),
    #[error("Managed Runtime source binding is missing after {0}")]
    MissingSourceBinding(&'static str),
    #[error("Managed Runtime source binding does not match Product provisioning")]
    RuntimeBindingMismatch,
}

/// Product-owned fresh AgentRun launch saga.
///
/// The ordering is intentional and crash-safe:
/// provision Host target -> Create -> commit pre-activation Product binding ->
/// materialize immutable Product resources -> Activate -> pin activated binding
/// -> optional first input.
pub struct AgentRunProductLaunchService {
    provisioning: Arc<dyn AgentRunProductRuntimeProvisioningPort>,
    runtime: Arc<dyn ManagedAgentRuntimeGateway>,
    bindings: Arc<dyn AgentRunProductRuntimeBindingStore>,
    resources: Arc<dyn AgentRunAppliedResourceSurfaceMaterializationPort>,
    resource_query: Arc<dyn AgentRunAppliedResourceSurfaceQueryPort>,
}

#[async_trait]
pub trait AgentRunProductLaunchPort: Send + Sync {
    async fn launch(
        &self,
        request: AgentRunProductLaunchRequest,
    ) -> Result<AgentRunProductLaunchOutcome, AgentRunProductLaunchError>;
}

impl AgentRunProductLaunchService {
    pub fn new(
        provisioning: Arc<dyn AgentRunProductRuntimeProvisioningPort>,
        runtime: Arc<dyn ManagedAgentRuntimeGateway>,
        bindings: Arc<dyn AgentRunProductRuntimeBindingStore>,
        resources: Arc<dyn AgentRunAppliedResourceSurfaceMaterializationPort>,
        resource_query: Arc<dyn AgentRunAppliedResourceSurfaceQueryPort>,
    ) -> Self {
        Self {
            provisioning,
            runtime,
            bindings,
            resources,
            resource_query,
        }
    }

    /// 幂等完成 fresh Runtime Create 之前的 Host target / surface provisioning。
    pub async fn prepare_runtime_target(
        &self,
        request: &AgentRunProductRuntimeProvisioningRequest,
    ) -> Result<(), AgentRunProductLaunchError> {
        request.validate()?;
        let provisioned = self
            .provisioning
            .provision_runtime_target(request.clone())
            .await?;
        if provisioned.target != request.target
            || provisioned.runtime_thread_id != request.runtime_thread_id
            || provisioned.frame != request.frame
            || provisioned.profile_digest != request.execution_profile.profile_digest
            || provisioned.surface_facts_digest != request.surface_facts.surface_digest
        {
            return Err(AgentRunProductLaunchError::RuntimeBindingMismatch);
        }
        Ok(())
    }

    /// 在 Create 已有 authoritative source evidence 后，幂等提交 Product pre-activation
    /// binding 并 materialize resource surface。该步骤必须先于 Activate。
    pub async fn converge_created_runtime(
        &self,
        request: &AgentRunProductRuntimeProvisioningRequest,
    ) -> Result<AgentRunProductRuntimeBinding, AgentRunProductLaunchError> {
        let created = self
            .runtime
            .read(ManagedRuntimeReadRequest {
                thread_id: request.runtime_thread_id.clone(),
            })
            .await?;
        let observed_source_binding = created
            .source_binding
            .clone()
            .ok_or(AgentRunProductLaunchError::MissingSourceBinding("Create"))?;
        if created.thread_id != request.runtime_thread_id
            || observed_source_binding.applied_surface_revision.0
                != request.surface_facts.surface_revision
        {
            return Err(AgentRunProductLaunchError::RuntimeBindingMismatch);
        }
        let mut pre_activation_source_binding = observed_source_binding;
        pre_activation_source_binding.activated_at_revision = None;
        let binding = AgentRunProductRuntimeBinding {
            target: request.target.clone(),
            runtime_thread_id: request.runtime_thread_id.clone(),
            launch_frame: request.frame.clone(),
            execution_profile_digest: request.execution_profile.profile_digest.clone(),
            source_binding: pre_activation_source_binding,
        };
        let binding_digest = binding
            .calculated_digest()
            .map_err(AgentRunProductLaunchError::Binding)?;
        match self
            .bindings
            .load_product_binding(&request.target)
            .await
            .map_err(AgentRunProductLaunchError::Binding)?
        {
            Some(existing)
                if existing
                    .calculated_digest()
                    .map_err(AgentRunProductLaunchError::Binding)?
                    == binding_digest => {}
            Some(_) => return Err(AgentRunProductLaunchError::RuntimeBindingMismatch),
            None => self
                .bindings
                .commit_product_binding(&binding)
                .await
                .map_err(AgentRunProductLaunchError::Binding)?,
        }
        self.resources
            .materialize(AgentRunAppliedResourceSurfaceMaterializeRequest {
                target: request.target.clone(),
                expected_current_snapshot_revision: None,
                product_binding_digest: binding_digest,
            })
            .await
            .map_err(|error| AgentRunProductLaunchError::ResourceSurface(error.to_string()))?;
        Ok(binding)
    }

    /// 在 Runtime Activate evidence 已提交后，幂等 pin Product binding。
    pub async fn converge_activated_runtime(
        &self,
        request: &AgentRunProductRuntimeProvisioningRequest,
    ) -> Result<AgentRunProductRuntimeBinding, AgentRunProductLaunchError> {
        let pre_activation = self
            .bindings
            .load_product_binding(&request.target)
            .await
            .map_err(AgentRunProductLaunchError::Binding)?
            .ok_or(AgentRunProductLaunchError::RuntimeBindingMismatch)?;
        let activated = self
            .runtime
            .read(ManagedRuntimeReadRequest {
                thread_id: request.runtime_thread_id.clone(),
            })
            .await?;
        let activated_source = activated
            .source_binding
            .clone()
            .ok_or(AgentRunProductLaunchError::MissingSourceBinding("Activate"))?;
        if activated.thread_id != request.runtime_thread_id
            || activated_source.activated_at_revision.is_none()
            || activated_source.source_ref != pre_activation.source_binding.source_ref
            || activated_source.committed_at_revision
                != pre_activation.source_binding.committed_at_revision
        {
            return Err(AgentRunProductLaunchError::RuntimeBindingMismatch);
        }
        let activated_binding = AgentRunProductRuntimeBinding {
            source_binding: activated_source,
            ..pre_activation
        };
        let activated_binding_digest = activated_binding
            .calculated_digest()
            .map_err(AgentRunProductLaunchError::Binding)?;
        let resource_snapshot = self
            .resource_query
            .applied_resource_surface(&request.target, None)
            .await
            .map_err(|error| AgentRunProductLaunchError::ResourceSurface(error.to_string()))?;
        self.bindings
            .activate_product_binding(
                &activated_binding,
                &activated_binding_digest,
                resource_snapshot.snapshot_revision,
            )
            .await
            .map_err(AgentRunProductLaunchError::Binding)?;
        Ok(activated_binding)
    }

    pub async fn launch(
        &self,
        request: AgentRunProductLaunchRequest,
    ) -> Result<AgentRunProductLaunchOutcome, AgentRunProductLaunchError> {
        request.provisioning.validate()?;
        if let Some(initial_context) = request.initial_context.as_ref()
            && !initial_context.validate()
        {
            return Err(AgentRunProductLaunchError::Invalid(
                "initial context package digest is invalid".to_string(),
            ));
        }
        let provisioned = self
            .provisioning
            .provision_runtime_target(request.provisioning.clone())
            .await?;
        if provisioned.target != request.provisioning.target
            || provisioned.runtime_thread_id != request.provisioning.runtime_thread_id
            || provisioned.frame != request.provisioning.frame
            || provisioned.profile_digest != request.provisioning.execution_profile.profile_digest
            || provisioned.surface_facts_digest != request.provisioning.surface_facts.surface_digest
        {
            return Err(AgentRunProductLaunchError::RuntimeBindingMismatch);
        }

        let create_receipt = self
            .runtime
            .execute(envelope(
                &request.provisioning,
                "create",
                None,
                ManagedRuntimeCommand::Create {
                    initial_context: request.initial_context,
                },
            )?)
            .await?;
        let created = self
            .runtime
            .read(ManagedRuntimeReadRequest {
                thread_id: request.provisioning.runtime_thread_id.clone(),
            })
            .await?;
        let observed_source_binding = created
            .source_binding
            .clone()
            .ok_or(AgentRunProductLaunchError::MissingSourceBinding("Create"))?;
        if created.thread_id != request.provisioning.runtime_thread_id
            || observed_source_binding.applied_surface_revision.0
                != request.provisioning.surface_facts.surface_revision
        {
            return Err(AgentRunProductLaunchError::RuntimeBindingMismatch);
        }
        let mut pre_activation_source_binding = observed_source_binding;
        pre_activation_source_binding.activated_at_revision = None;
        let pre_activation_binding = AgentRunProductRuntimeBinding {
            target: request.provisioning.target.clone(),
            runtime_thread_id: request.provisioning.runtime_thread_id.clone(),
            launch_frame: request.provisioning.frame.clone(),
            execution_profile_digest: request
                .provisioning
                .execution_profile
                .profile_digest
                .clone(),
            source_binding: pre_activation_source_binding,
        };
        match self
            .bindings
            .load_product_binding(&request.provisioning.target)
            .await
            .map_err(AgentRunProductLaunchError::Binding)?
        {
            Some(existing) if same_launch_binding(&existing, &pre_activation_binding) => {}
            Some(_) => return Err(AgentRunProductLaunchError::RuntimeBindingMismatch),
            None => {
                self.bindings
                    .commit_product_binding(&pre_activation_binding)
                    .await
                    .map_err(AgentRunProductLaunchError::Binding)?;
            }
        }

        self.resources
            .materialize(AgentRunAppliedResourceSurfaceMaterializeRequest {
                target: request.provisioning.target.clone(),
                expected_current_snapshot_revision: None,
                product_binding_digest: pre_activation_binding
                    .calculated_digest()
                    .map_err(AgentRunProductLaunchError::Binding)?,
            })
            .await
            .map_err(|error| AgentRunProductLaunchError::ResourceSurface(error.to_string()))?;
        let resource_snapshot = self
            .resource_query
            .applied_resource_surface(&request.provisioning.target, None)
            .await
            .map_err(|error| AgentRunProductLaunchError::ResourceSurface(error.to_string()))?;

        let activate_receipt = self
            .runtime
            .execute(envelope(
                &request.provisioning,
                "activate",
                Some(create_receipt.accepted_revision),
                ManagedRuntimeCommand::Activate,
            )?)
            .await?;
        let activated = self
            .runtime
            .read(ManagedRuntimeReadRequest {
                thread_id: request.provisioning.runtime_thread_id.clone(),
            })
            .await?;
        let activated_source = activated
            .source_binding
            .clone()
            .ok_or(AgentRunProductLaunchError::MissingSourceBinding("Activate"))?;
        if activated.thread_id != request.provisioning.runtime_thread_id
            || activated_source.activated_at_revision.is_none()
            || activated_source.source_ref != pre_activation_binding.source_binding.source_ref
            || activated_source.committed_at_revision
                != pre_activation_binding.source_binding.committed_at_revision
            || activated_source.applied_surface_revision
                != pre_activation_binding
                    .source_binding
                    .applied_surface_revision
        {
            return Err(AgentRunProductLaunchError::RuntimeBindingMismatch);
        }
        let activated_binding = AgentRunProductRuntimeBinding {
            source_binding: activated_source,
            ..pre_activation_binding
        };
        let activated_binding_digest = activated_binding
            .calculated_digest()
            .map_err(AgentRunProductLaunchError::Binding)?;
        self.bindings
            .activate_product_binding(
                &activated_binding,
                &activated_binding_digest,
                resource_snapshot.snapshot_revision,
            )
            .await
            .map_err(AgentRunProductLaunchError::Binding)?;

        let input_receipt = if request.initial_input.is_empty() {
            None
        } else {
            Some(
                self.runtime
                    .execute(envelope(
                        &request.provisioning,
                        "initial-input",
                        Some(activate_receipt.accepted_revision),
                        ManagedRuntimeCommand::SubmitInput {
                            content: request.initial_input,
                        },
                    )?)
                    .await?,
            )
        };

        Ok(AgentRunProductLaunchOutcome {
            binding: activated_binding,
            resource_snapshot_revision: resource_snapshot.snapshot_revision,
            create_receipt,
            activate_receipt,
            input_receipt,
        })
    }
}

#[async_trait]
impl AgentRunProductLaunchPort for AgentRunProductLaunchService {
    async fn launch(
        &self,
        request: AgentRunProductLaunchRequest,
    ) -> Result<AgentRunProductLaunchOutcome, AgentRunProductLaunchError> {
        AgentRunProductLaunchService::launch(self, request).await
    }
}

fn same_launch_binding(
    existing: &AgentRunProductRuntimeBinding,
    expected: &AgentRunProductRuntimeBinding,
) -> bool {
    existing.target == expected.target
        && existing.runtime_thread_id == expected.runtime_thread_id
        && existing.launch_frame == expected.launch_frame
        && existing.execution_profile_digest == expected.execution_profile_digest
        && existing.source_binding.source_ref == expected.source_binding.source_ref
        && existing.source_binding.committed_at_revision
            == expected.source_binding.committed_at_revision
        && existing.source_binding.applied_surface_revision
            == expected.source_binding.applied_surface_revision
}

fn envelope(
    provisioning: &AgentRunProductRuntimeProvisioningRequest,
    phase: &'static str,
    expected_revision: Option<agentdash_agent_runtime_contract::RuntimeProjectionRevision>,
    command: ManagedRuntimeCommand,
) -> Result<ManagedRuntimeCommandEnvelope, AgentRunProductLaunchError> {
    let identity = format!(
        "{:x}",
        Sha256::digest(
            serde_json::to_vec(&(
                "agentdash.product-launch-operation/v1",
                provisioning.target.run_id,
                provisioning.target.agent_id,
                &provisioning.idempotency_key,
                phase,
            ))
            .expect("Product launch identity is serializable"),
        )
    );
    Ok(ManagedRuntimeCommandEnvelope {
        operation_id: RuntimeOperationId::new(format!("product-launch:{phase}:{identity}"))
            .map_err(|error| AgentRunProductLaunchError::Invalid(error.to_string()))?,
        idempotency_key: RuntimeIdempotencyKey::new(format!(
            "product-launch-idempotency:{phase}:{identity}"
        ))
        .map_err(|error| AgentRunProductLaunchError::Invalid(error.to_string()))?,
        thread_id: provisioning.runtime_thread_id.clone(),
        expected_revision,
        command,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use agentdash_agent_runtime_contract::{
        ManagedRuntimeChangePage, ManagedRuntimeChangesRequest, ManagedRuntimeLifecycleStatus,
        ManagedRuntimeOperationStatus, ManagedRuntimeProjectionAuthority,
        ManagedRuntimeProjectionFidelity, ManagedRuntimeSnapshot,
        ManagedRuntimeSourceBindingEvidence, RuntimeChangeSequence, RuntimeProjectionRevision,
        RuntimeSourceRef, RuntimeThreadId, SurfaceRevision,
    };
    use async_trait::async_trait;
    use tokio::sync::Mutex;
    use uuid::Uuid;

    use super::*;
    use crate::agent_run::{
        AgentRunAppliedResourceSurface, AgentRunAppliedResourceSurfaceCommitOutcome,
        AgentRunAppliedResourceSurfaceProvenance, AgentRunAppliedResourceSurfaceQueryError,
        AgentRunAppliedResourceSurfaceSnapshot, AgentRunAppliedResourceSurfaceWriteError,
        AgentRunProductRuntimeProvisioningError, AgentRunProductRuntimeProvisioningEvidence,
        AppliedTaskGrant, AppliedVfsGrant, AppliedVfsMount, ProductAgentFrameRef,
        ProductAgentSurfaceFacts, ProductExecutionProfileRef,
    };

    struct ReplayRuntime {
        snapshot: ManagedRuntimeSnapshot,
        observed: Mutex<Vec<ManagedRuntimeCommandEnvelope>>,
    }

    #[async_trait]
    impl ManagedAgentRuntimeGateway for ReplayRuntime {
        async fn execute(
            &self,
            envelope: ManagedRuntimeCommandEnvelope,
        ) -> Result<ManagedRuntimeOperationReceipt, ManagedRuntimeGatewayError> {
            let accepted_revision = match &envelope.command {
                ManagedRuntimeCommand::Create { .. } => RuntimeProjectionRevision(1),
                ManagedRuntimeCommand::Activate => RuntimeProjectionRevision(2),
                ManagedRuntimeCommand::SubmitInput { .. } => RuntimeProjectionRevision(3),
                command => {
                    return Err(ManagedRuntimeGatewayError::Invalid {
                        reason: format!("unexpected replay command {:?}", command.kind()),
                    });
                }
            };
            self.observed.lock().await.push(envelope.clone());
            Ok(ManagedRuntimeOperationReceipt {
                operation_id: envelope.operation_id,
                thread_id: envelope.thread_id,
                accepted_revision,
                status: ManagedRuntimeOperationStatus::Succeeded,
                evidence: None,
                duplicate: true,
            })
        }

        async fn read(
            &self,
            _request: ManagedRuntimeReadRequest,
        ) -> Result<ManagedRuntimeSnapshot, ManagedRuntimeGatewayError> {
            Ok(self.snapshot.clone())
        }

        async fn changes(
            &self,
            _request: ManagedRuntimeChangesRequest,
        ) -> Result<ManagedRuntimeChangePage, ManagedRuntimeGatewayError> {
            Err(ManagedRuntimeGatewayError::Invalid {
                reason: "launch test does not consume changes".to_owned(),
            })
        }
    }

    struct EchoProvisioning;

    #[async_trait]
    impl AgentRunProductRuntimeProvisioningPort for EchoProvisioning {
        async fn provision_runtime_target(
            &self,
            request: AgentRunProductRuntimeProvisioningRequest,
        ) -> Result<
            AgentRunProductRuntimeProvisioningEvidence,
            AgentRunProductRuntimeProvisioningError,
        > {
            Ok(AgentRunProductRuntimeProvisioningEvidence {
                target: request.target,
                runtime_thread_id: request.runtime_thread_id,
                idempotency_key: request.idempotency_key,
                frame: request.frame,
                profile_digest: request.execution_profile.profile_digest,
                surface_facts_digest: request.surface_facts.surface_digest,
            })
        }
    }

    struct ReplayBindingStore {
        binding: Mutex<Option<AgentRunProductRuntimeBinding>>,
    }

    #[async_trait]
    impl super::super::AgentRunProductRuntimeBindingRepository for ReplayBindingStore {
        async fn load_product_binding(
            &self,
            _target: &agentdash_domain::agent_run_target::AgentRunTarget,
        ) -> Result<Option<AgentRunProductRuntimeBinding>, String> {
            Ok(self.binding.lock().await.clone())
        }
    }

    #[async_trait]
    impl AgentRunProductRuntimeBindingStore for ReplayBindingStore {
        async fn commit_product_binding(
            &self,
            binding: &AgentRunProductRuntimeBinding,
        ) -> Result<(), String> {
            *self.binding.lock().await = Some(binding.clone());
            Ok(())
        }

        async fn activate_product_binding(
            &self,
            binding: &AgentRunProductRuntimeBinding,
            expected_binding_digest: &str,
            _expected_snapshot_revision: u64,
        ) -> Result<(), String> {
            if binding.calculated_digest()? != expected_binding_digest {
                return Err("binding digest mismatch".to_owned());
            }
            *self.binding.lock().await = Some(binding.clone());
            Ok(())
        }

        async fn prepare_product_binding_recovery(
            &self,
            _expected_previous_binding_digest: &str,
            binding: &AgentRunProductRuntimeBinding,
        ) -> Result<(), String> {
            *self.binding.lock().await = Some(binding.clone());
            Ok(())
        }
    }

    struct ReplayResources {
        snapshot: AgentRunAppliedResourceSurfaceSnapshot,
        requests: Mutex<Vec<AgentRunAppliedResourceSurfaceMaterializeRequest>>,
    }

    #[async_trait]
    impl AgentRunAppliedResourceSurfaceMaterializationPort for ReplayResources {
        async fn materialize(
            &self,
            request: AgentRunAppliedResourceSurfaceMaterializeRequest,
        ) -> Result<
            AgentRunAppliedResourceSurfaceCommitOutcome,
            AgentRunAppliedResourceSurfaceWriteError,
        > {
            self.requests.lock().await.push(request);
            Ok(AgentRunAppliedResourceSurfaceCommitOutcome::AlreadyCurrent)
        }
    }

    #[async_trait]
    impl AgentRunAppliedResourceSurfaceQueryPort for ReplayResources {
        async fn applied_resource_surface(
            &self,
            _target: &agentdash_domain::agent_run_target::AgentRunTarget,
            _expected_snapshot_revision: Option<u64>,
        ) -> Result<AgentRunAppliedResourceSurfaceSnapshot, AgentRunAppliedResourceSurfaceQueryError>
        {
            Ok(self.snapshot.clone())
        }
    }

    fn request() -> AgentRunProductLaunchRequest {
        let target = agentdash_domain::agent_run_target::AgentRunTarget {
            run_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
        };
        let mut execution_profile = ProductExecutionProfileRef {
            profile_key: "PI_AGENT".to_owned(),
            profile_revision: 1,
            profile_digest: String::new(),
            configuration: serde_json::json!({"executor":"PI_AGENT"}),
            credential_scope: None,
        };
        execution_profile.refresh_digest();
        let mut surface_facts = ProductAgentSurfaceFacts {
            surface_revision: 1,
            surface_digest: String::new(),
            capability: None,
            context: None,
            context_source: None,
            vfs: None,
            mcp: None,
            hook_plan: None,
        };
        surface_facts.surface_digest = surface_facts.calculated_digest();
        AgentRunProductLaunchRequest {
            provisioning: AgentRunProductRuntimeProvisioningRequest {
                target: target.clone(),
                runtime_thread_id: RuntimeThreadId::new("replay-thread").expect("thread"),
                idempotency_key: "replay-launch".to_owned(),
                frame: ProductAgentFrameRef {
                    frame_id: Uuid::new_v4(),
                    agent_id: target.agent_id,
                    revision: 1,
                },
                execution_profile,
                surface_facts,
            },
            initial_context: None,
            initial_input: vec![ManagedRuntimeContentBlock::Text {
                text: "first input".to_owned(),
            }],
        }
    }

    fn resource_snapshot(
        request: &AgentRunProductLaunchRequest,
        product_binding_digest: String,
    ) -> AgentRunAppliedResourceSurfaceSnapshot {
        let provenance = AgentRunAppliedResourceSurfaceProvenance {
            source_kind: "agent_frame".to_owned(),
            source_id: request.provisioning.frame.frame_id.to_string(),
            source_revision: 1,
            projection_revision: 1,
            captured_at_ms: 1,
        };
        AgentRunAppliedResourceSurfaceSnapshot {
            snapshot_revision: 1,
            surface: AgentRunAppliedResourceSurface {
                target: request.provisioning.target.clone(),
                project_id: Uuid::new_v4(),
                workspace_id: None,
                vfs_mounts: Vec::<AppliedVfsMount>::new(),
                default_mount_id: None,
                vfs_grants: Vec::<AppliedVfsGrant>::new(),
                agent_surface_revision: 1,
                agent_surface_digest: request.provisioning.surface_facts.surface_digest.clone(),
                vfs_digest: "sha256:vfs".to_owned(),
                task_grants: Vec::<AppliedTaskGrant>::new(),
                task_surface_revision: 1,
                task_surface_digest: "sha256:task".to_owned(),
                task_provenance: provenance.clone(),
                product_binding_digest,
                provenance,
            },
        }
    }

    fn activated_snapshot(
        request: &AgentRunProductLaunchRequest,
        source_binding: ManagedRuntimeSourceBindingEvidence,
    ) -> ManagedRuntimeSnapshot {
        ManagedRuntimeSnapshot {
            thread_id: request.provisioning.runtime_thread_id.clone(),
            revision: RuntimeProjectionRevision(3),
            latest_change_sequence: RuntimeChangeSequence(3),
            captured_at_ms: 3,
            lifecycle: ManagedRuntimeLifecycleStatus::Active,
            active_turn_id: None,
            turns: Vec::new(),
            items: Vec::new(),
            interactions: Vec::new(),
            thread_name: None,
            thread_name_source: None,
            operations: Vec::new(),
            source_binding: Some(source_binding),
            authority: ManagedRuntimeProjectionAuthority::SourceAuthoritative,
            fidelity: ManagedRuntimeProjectionFidelity::Exact,
            command_availability: BTreeMap::new(),
            conversation_history: Vec::new(),
        }
    }

    #[tokio::test]
    async fn replay_after_runtime_activation_converges_product_binding_and_uses_stable_envelopes() {
        let request = request();
        let activated_source = ManagedRuntimeSourceBindingEvidence {
            source_ref: RuntimeSourceRef::new("source:replay").expect("source"),
            committed_at_revision: RuntimeProjectionRevision(1),
            applied_surface_revision: SurfaceRevision(1),
            activated_at_revision: Some(RuntimeProjectionRevision(2)),
        };
        let mut pre_activation_source = activated_source.clone();
        pre_activation_source.activated_at_revision = None;
        let pre_activation_binding = AgentRunProductRuntimeBinding {
            target: request.provisioning.target.clone(),
            runtime_thread_id: request.provisioning.runtime_thread_id.clone(),
            launch_frame: request.provisioning.frame.clone(),
            execution_profile_digest: request
                .provisioning
                .execution_profile
                .profile_digest
                .clone(),
            source_binding: pre_activation_source,
        };
        let binding_digest = pre_activation_binding
            .calculated_digest()
            .expect("binding digest");
        let runtime = Arc::new(ReplayRuntime {
            snapshot: activated_snapshot(&request, activated_source),
            observed: Mutex::new(Vec::new()),
        });
        let bindings = Arc::new(ReplayBindingStore {
            binding: Mutex::new(Some(pre_activation_binding)),
        });
        let resources = Arc::new(ReplayResources {
            snapshot: resource_snapshot(&request, binding_digest),
            requests: Mutex::new(Vec::new()),
        });
        let service = AgentRunProductLaunchService::new(
            Arc::new(EchoProvisioning),
            runtime.clone(),
            bindings.clone(),
            resources.clone(),
            resources.clone(),
        );

        let outcome = service
            .launch(request.clone())
            .await
            .expect("replay converges");

        assert_eq!(
            outcome.binding.source_binding.activated_at_revision,
            Some(RuntimeProjectionRevision(2))
        );
        assert_eq!(
            bindings
                .binding
                .lock()
                .await
                .as_ref()
                .and_then(|binding| binding.source_binding.activated_at_revision),
            Some(RuntimeProjectionRevision(2))
        );
        let observed = runtime.observed.lock().await;
        assert_eq!(observed.len(), 3);
        assert!(matches!(
            observed[0].command,
            ManagedRuntimeCommand::Create { .. }
        ));
        assert_eq!(observed[0].expected_revision, None);
        assert!(matches!(
            observed[1].command,
            ManagedRuntimeCommand::Activate
        ));
        assert_eq!(
            observed[1].expected_revision,
            Some(RuntimeProjectionRevision(1))
        );
        assert!(matches!(
            observed[2].command,
            ManagedRuntimeCommand::SubmitInput { .. }
        ));
        assert_eq!(
            observed[2].expected_revision,
            Some(RuntimeProjectionRevision(2))
        );
        drop(observed);

        let replayed = service
            .launch(request)
            .await
            .expect("activated binding replay converges");
        assert_eq!(replayed.binding, outcome.binding);
        assert_eq!(runtime.observed.lock().await.len(), 6);
        let resource_requests = resources.requests.lock().await;
        assert_eq!(resource_requests.len(), 2);
        assert_eq!(
            resource_requests[0].product_binding_digest,
            resource_requests[1].product_binding_digest,
            "resource surface remains pinned to the pre-activation binding identity"
        );
    }
}
