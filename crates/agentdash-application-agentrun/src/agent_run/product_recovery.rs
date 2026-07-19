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
        let mut pre_activation_source = rebound_source.clone();
        pre_activation_source.activated_at_revision = None;
        let pre_activation_binding = AgentRunProductRuntimeBinding {
            source_binding: pre_activation_source,
            ..current_binding.clone()
        };
        let rebound_digest = pre_activation_binding
            .calculated_digest()
            .map_err(AgentRunProductRuntimeRecoveryError::Binding)?;
        let runtime_activated_binding = AgentRunProductRuntimeBinding {
            source_binding: rebound_source,
            ..current_binding.clone()
        };
        let runtime_activated_digest = runtime_activated_binding
            .calculated_digest()
            .map_err(AgentRunProductRuntimeRecoveryError::Binding)?;
        let recovery_already_pinned = previous_digest == runtime_activated_digest
            && current_binding
                .source_binding
                .activated_at_revision
                .is_some()
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
                != pre_activation_binding
                    .source_binding
                    .applied_surface_revision
            || activated_source.activated_at_revision.is_none()
        {
            return Err(AgentRunProductRuntimeRecoveryError::RuntimeBindingMismatch);
        }
        let activated_binding = AgentRunProductRuntimeBinding {
            source_binding: activated_source,
            ..pre_activation_binding
        };
        if !recovery_already_pinned {
            self.bindings
                .activate_product_binding(
                    &activated_binding,
                    &rebound_digest,
                    recovered_resource.snapshot_revision,
                )
                .await
                .map_err(AgentRunProductRuntimeRecoveryError::Binding)?;
        }
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

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashMap};

    use agentdash_agent_runtime_contract::{
        ManagedRuntimeChangePage, ManagedRuntimeChangesRequest, ManagedRuntimeLifecycleStatus,
        ManagedRuntimeOperationStatus, ManagedRuntimeProjectionAuthority,
        ManagedRuntimeProjectionFidelity, ManagedRuntimeSnapshot,
        ManagedRuntimeSourceBindingEvidence, RuntimeChangeSequence, RuntimeSourceRef,
        RuntimeThreadId, SurfaceRevision,
    };
    use tokio::sync::Mutex;
    use uuid::Uuid;

    use super::*;
    use crate::agent_run::{
        AgentRunAppliedResourceSurface, AgentRunAppliedResourceSurfaceCommitOutcome,
        AgentRunAppliedResourceSurfaceProvenance, AgentRunAppliedResourceSurfaceQueryError,
        AgentRunAppliedResourceSurfaceSnapshot, AgentRunAppliedResourceSurfaceWriteError,
        AgentRunProductRuntimeBindingRepository, AppliedTaskGrant, AppliedVfsGrant,
        AppliedVfsMount, ProductAgentFrameRef,
    };

    #[derive(Clone, Copy)]
    enum CrashPoint {
        AfterRebind,
        AfterBindingCas,
        AfterResourceMaterialization,
        AfterActivate,
    }

    struct RecoveryRuntime {
        snapshot: Mutex<ManagedRuntimeSnapshot>,
        receipts: Mutex<HashMap<RuntimeOperationId, ManagedRuntimeOperationReceipt>>,
    }

    #[async_trait]
    impl ManagedAgentRuntimeGateway for RecoveryRuntime {
        async fn execute(
            &self,
            envelope: ManagedRuntimeCommandEnvelope,
        ) -> Result<ManagedRuntimeOperationReceipt, ManagedRuntimeGatewayError> {
            if let Some(receipt) = self
                .receipts
                .lock()
                .await
                .get(&envelope.operation_id)
                .cloned()
            {
                return Ok(ManagedRuntimeOperationReceipt {
                    duplicate: true,
                    ..receipt
                });
            }
            let mut snapshot = self.snapshot.lock().await;
            let source = snapshot.source_binding.as_mut().ok_or_else(|| {
                ManagedRuntimeGatewayError::Invalid {
                    reason: "test Runtime source binding is missing".to_owned(),
                }
            })?;
            let accepted_revision = match envelope.command {
                ManagedRuntimeCommand::Rebind => {
                    source.committed_at_revision = RuntimeProjectionRevision(2);
                    source.activated_at_revision = None;
                    RuntimeProjectionRevision(2)
                }
                ManagedRuntimeCommand::Activate => {
                    source.activated_at_revision = Some(RuntimeProjectionRevision(3));
                    RuntimeProjectionRevision(3)
                }
                _ => {
                    return Err(ManagedRuntimeGatewayError::Invalid {
                        reason: "unexpected recovery command".to_owned(),
                    });
                }
            };
            snapshot.revision = accepted_revision;
            let receipt = ManagedRuntimeOperationReceipt {
                operation_id: envelope.operation_id,
                thread_id: envelope.thread_id,
                accepted_revision,
                status: ManagedRuntimeOperationStatus::Succeeded,
                evidence: None,
                duplicate: false,
            };
            self.receipts
                .lock()
                .await
                .insert(receipt.operation_id.clone(), receipt.clone());
            Ok(receipt)
        }

        async fn read(
            &self,
            _request: ManagedRuntimeReadRequest,
        ) -> Result<ManagedRuntimeSnapshot, ManagedRuntimeGatewayError> {
            Ok(self.snapshot.lock().await.clone())
        }

        async fn changes(
            &self,
            _request: ManagedRuntimeChangesRequest,
        ) -> Result<ManagedRuntimeChangePage, ManagedRuntimeGatewayError> {
            Err(ManagedRuntimeGatewayError::Invalid {
                reason: "recovery test does not consume changes".to_owned(),
            })
        }
    }

    struct RecoveryBindings {
        binding: Mutex<AgentRunProductRuntimeBinding>,
    }

    #[async_trait]
    impl AgentRunProductRuntimeBindingRepository for RecoveryBindings {
        async fn load_product_binding(
            &self,
            _target: &AgentRunTarget,
        ) -> Result<Option<AgentRunProductRuntimeBinding>, String> {
            Ok(Some(self.binding.lock().await.clone()))
        }
    }

    #[async_trait]
    impl AgentRunProductRuntimeBindingStore for RecoveryBindings {
        async fn commit_product_binding(
            &self,
            binding: &AgentRunProductRuntimeBinding,
        ) -> Result<(), String> {
            *self.binding.lock().await = binding.clone();
            Ok(())
        }

        async fn activate_product_binding(
            &self,
            binding: &AgentRunProductRuntimeBinding,
            expected_binding_digest: &str,
            _expected_snapshot_revision: u64,
        ) -> Result<(), String> {
            let current_digest = self.binding.lock().await.calculated_digest()?;
            if current_digest != expected_binding_digest {
                return Err("activation CAS digest mismatch".to_owned());
            }
            *self.binding.lock().await = binding.clone();
            Ok(())
        }

        async fn prepare_product_binding_recovery(
            &self,
            expected_previous_binding_digest: &str,
            binding: &AgentRunProductRuntimeBinding,
        ) -> Result<(), String> {
            let mut current = self.binding.lock().await;
            let current_digest = current.calculated_digest()?;
            let next_digest = binding.calculated_digest()?;
            if current_digest != expected_previous_binding_digest && current_digest != next_digest {
                return Err("recovery CAS digest mismatch".to_owned());
            }
            *current = binding.clone();
            Ok(())
        }
    }

    struct RecoveryResources {
        snapshot: Mutex<AgentRunAppliedResourceSurfaceSnapshot>,
    }

    #[async_trait]
    impl AgentRunAppliedResourceSurfaceMaterializationPort for RecoveryResources {
        async fn materialize(
            &self,
            request: AgentRunAppliedResourceSurfaceMaterializeRequest,
        ) -> Result<
            AgentRunAppliedResourceSurfaceCommitOutcome,
            AgentRunAppliedResourceSurfaceWriteError,
        > {
            let mut snapshot = self.snapshot.lock().await;
            snapshot.snapshot_revision += 1;
            snapshot.surface.product_binding_digest = request.product_binding_digest;
            Ok(AgentRunAppliedResourceSurfaceCommitOutcome::Committed)
        }
    }

    #[async_trait]
    impl AgentRunAppliedResourceSurfaceQueryPort for RecoveryResources {
        async fn applied_resource_surface(
            &self,
            _target: &AgentRunTarget,
            _expected_snapshot_revision: Option<u64>,
        ) -> Result<AgentRunAppliedResourceSurfaceSnapshot, AgentRunAppliedResourceSurfaceQueryError>
        {
            Ok(self.snapshot.lock().await.clone())
        }
    }

    struct RecoveryFixture {
        service: AgentRunProductRuntimeRecoveryService,
        runtime: Arc<RecoveryRuntime>,
        bindings: Arc<RecoveryBindings>,
        resources: Arc<RecoveryResources>,
        request: AgentRunProductRuntimeRecoveryRequest,
    }

    impl RecoveryFixture {
        fn at(crash_point: CrashPoint) -> Self {
            let target = AgentRunTarget {
                run_id: Uuid::new_v4(),
                agent_id: Uuid::new_v4(),
            };
            let runtime_thread_id =
                RuntimeThreadId::new("recovery-thread").expect("RuntimeThread id");
            let old_source = source_binding(1, true);
            let rebound_source =
                source_binding(2, matches!(crash_point, CrashPoint::AfterActivate));
            let old_binding = product_binding(&target, &runtime_thread_id, old_source);
            let rebound_binding =
                product_binding(&target, &runtime_thread_id, rebound_source.clone());
            let product_binding = match crash_point {
                CrashPoint::AfterRebind => old_binding.clone(),
                CrashPoint::AfterBindingCas
                | CrashPoint::AfterResourceMaterialization
                | CrashPoint::AfterActivate => {
                    let mut binding = rebound_binding.clone();
                    binding.source_binding.activated_at_revision = None;
                    binding
                }
            };
            let rebound_digest = {
                let mut binding = rebound_binding.clone();
                binding.source_binding.activated_at_revision = None;
                binding.calculated_digest().expect("rebound digest")
            };
            let resource_digest = if matches!(
                crash_point,
                CrashPoint::AfterResourceMaterialization | CrashPoint::AfterActivate
            ) {
                rebound_digest
            } else {
                old_binding.calculated_digest().expect("old digest")
            };
            let client_command_id = "stable-recovery-command";
            let mut receipts = HashMap::new();
            let rebind_envelope = recovery_envelope(
                &product_binding,
                client_command_id,
                "rebind",
                Some(RuntimeProjectionRevision(1)),
                ManagedRuntimeCommand::Rebind,
            )
            .expect("rebind envelope");
            receipts.insert(
                rebind_envelope.operation_id.clone(),
                successful_receipt(rebind_envelope, RuntimeProjectionRevision(2)),
            );
            if matches!(crash_point, CrashPoint::AfterActivate) {
                let activate_envelope = recovery_envelope(
                    &product_binding,
                    client_command_id,
                    "activate",
                    Some(RuntimeProjectionRevision(2)),
                    ManagedRuntimeCommand::Activate,
                )
                .expect("activate envelope");
                receipts.insert(
                    activate_envelope.operation_id.clone(),
                    successful_receipt(activate_envelope, RuntimeProjectionRevision(3)),
                );
            }
            let runtime = Arc::new(RecoveryRuntime {
                snapshot: Mutex::new(runtime_snapshot(runtime_thread_id.clone(), rebound_source)),
                receipts: Mutex::new(receipts),
            });
            let bindings = Arc::new(RecoveryBindings {
                binding: Mutex::new(product_binding),
            });
            let resources = Arc::new(RecoveryResources {
                snapshot: Mutex::new(resource_snapshot(target.clone(), resource_digest)),
            });
            let service = AgentRunProductRuntimeRecoveryService::new(
                runtime.clone(),
                bindings.clone(),
                resources.clone(),
                resources.clone(),
            );
            Self {
                service,
                runtime,
                bindings,
                resources,
                request: AgentRunProductRuntimeRecoveryRequest {
                    target,
                    client_command_id: client_command_id.to_owned(),
                    expected_revision: RuntimeProjectionRevision(1),
                },
            }
        }

        async fn assert_converges(self) {
            let first = self
                .service
                .recover(self.request.clone())
                .await
                .expect("recovery converges");
            let second = self
                .service
                .recover(self.request)
                .await
                .expect("recovery replay converges");
            assert_eq!(
                first.rebind_receipt.operation_id,
                second.rebind_receipt.operation_id
            );
            assert_eq!(
                first.activate_receipt.operation_id,
                second.activate_receipt.operation_id
            );
            assert!(second.rebind_receipt.duplicate);
            assert!(second.activate_receipt.duplicate);
            let binding = self.bindings.binding.lock().await.clone();
            assert!(binding.source_binding.activated_at_revision.is_some());
            let mut pre_activation = binding.clone();
            pre_activation.source_binding.activated_at_revision = None;
            assert_eq!(
                self.resources
                    .snapshot
                    .lock()
                    .await
                    .surface
                    .product_binding_digest,
                pre_activation
                    .calculated_digest()
                    .expect("pre-activation digest")
            );
            assert_eq!(self.runtime.receipts.lock().await.len(), 2);
        }
    }

    fn source_binding(
        committed_revision: u64,
        activated: bool,
    ) -> ManagedRuntimeSourceBindingEvidence {
        ManagedRuntimeSourceBindingEvidence {
            source_ref: RuntimeSourceRef::new("source:recovery").expect("source"),
            committed_at_revision: RuntimeProjectionRevision(committed_revision),
            applied_surface_revision: SurfaceRevision(1),
            activated_at_revision: activated.then_some(RuntimeProjectionRevision(3)),
        }
    }

    fn successful_receipt(
        envelope: ManagedRuntimeCommandEnvelope,
        accepted_revision: RuntimeProjectionRevision,
    ) -> ManagedRuntimeOperationReceipt {
        ManagedRuntimeOperationReceipt {
            operation_id: envelope.operation_id,
            thread_id: envelope.thread_id,
            accepted_revision,
            status: ManagedRuntimeOperationStatus::Succeeded,
            evidence: None,
            duplicate: false,
        }
    }

    fn product_binding(
        target: &AgentRunTarget,
        runtime_thread_id: &RuntimeThreadId,
        source_binding: ManagedRuntimeSourceBindingEvidence,
    ) -> AgentRunProductRuntimeBinding {
        AgentRunProductRuntimeBinding {
            target: target.clone(),
            runtime_thread_id: runtime_thread_id.clone(),
            launch_frame: ProductAgentFrameRef {
                frame_id: Uuid::new_v4(),
                agent_id: target.agent_id,
                revision: 1,
            },
            execution_profile_digest: "sha256:profile".to_owned(),
            source_binding,
        }
    }

    fn runtime_snapshot(
        thread_id: RuntimeThreadId,
        source_binding: ManagedRuntimeSourceBindingEvidence,
    ) -> ManagedRuntimeSnapshot {
        ManagedRuntimeSnapshot {
            thread_id,
            revision: source_binding.committed_at_revision,
            latest_change_sequence: RuntimeChangeSequence(1),
            captured_at_ms: 1,
            lifecycle: if source_binding.activated_at_revision.is_some() {
                ManagedRuntimeLifecycleStatus::Active
            } else {
                ManagedRuntimeLifecycleStatus::Provisioning
            },
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

    fn resource_snapshot(
        target: AgentRunTarget,
        product_binding_digest: String,
    ) -> AgentRunAppliedResourceSurfaceSnapshot {
        let provenance = AgentRunAppliedResourceSurfaceProvenance {
            source_kind: "agent_frame".to_owned(),
            source_id: Uuid::new_v4().to_string(),
            source_revision: 1,
            projection_revision: 1,
            captured_at_ms: 1,
        };
        AgentRunAppliedResourceSurfaceSnapshot {
            snapshot_revision: 1,
            surface: AgentRunAppliedResourceSurface {
                target,
                project_id: Uuid::new_v4(),
                workspace_id: None,
                vfs_mounts: Vec::<AppliedVfsMount>::new(),
                default_mount_id: None,
                vfs_grants: Vec::<AppliedVfsGrant>::new(),
                agent_surface_revision: 1,
                agent_surface_digest: "sha256:surface".to_owned(),
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

    #[tokio::test]
    async fn recovery_replays_after_rebind() {
        RecoveryFixture::at(CrashPoint::AfterRebind)
            .assert_converges()
            .await;
    }

    #[tokio::test]
    async fn recovery_replays_after_product_binding_cas() {
        RecoveryFixture::at(CrashPoint::AfterBindingCas)
            .assert_converges()
            .await;
    }

    #[tokio::test]
    async fn recovery_replays_after_resource_materialization() {
        RecoveryFixture::at(CrashPoint::AfterResourceMaterialization)
            .assert_converges()
            .await;
    }

    #[tokio::test]
    async fn recovery_replays_after_runtime_activate() {
        RecoveryFixture::at(CrashPoint::AfterActivate)
            .assert_converges()
            .await;
    }
}
