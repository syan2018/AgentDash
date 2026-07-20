use std::sync::Arc;

use agentdash_agent_runtime_contract::{
    ManagedAgentRuntimeGateway, ManagedRuntimeCommand, ManagedRuntimeCommandEnvelope,
    ManagedRuntimeGatewayError, ManagedRuntimeOperationReceipt, ManagedRuntimeOperationStatus,
    ManagedRuntimeReadRequest, RuntimeProjectionRevision,
};
use agentdash_domain::agent_run_target::AgentRunTarget;
use async_trait::async_trait;

use super::{
    AgentRunAppliedResourceSurfaceMaterializationPort,
    AgentRunAppliedResourceSurfaceMaterializeRequest, AgentRunAppliedResourceSurfaceQueryError,
    AgentRunAppliedResourceSurfaceQueryPort, AgentRunProductRuntimeBinding,
    AgentRunProductRuntimeBindingStore, AgentRunProductRuntimeRecoveryId,
    AgentRunProductRuntimeRecoveryPhase, AgentRunProductRuntimeRecoveryRepositoryError,
    AgentRunProductRuntimeRecoverySaga, AgentRunProductRuntimeRecoverySagaRepository,
};

#[async_trait]
pub trait AgentRunProductRuntimeRecoveryPreparationPort: Send + Sync {
    /// Re-materializes the exact Product execution profile and makes its live attachment
    /// available to the Host recovery planner for this Runtime thread.
    async fn prepare_recovery_attachment(
        &self,
        binding: &AgentRunProductRuntimeBinding,
    ) -> Result<(), String>;
}

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

#[async_trait]
pub trait AgentRunProductRuntimeRecoveryAdvancementPort: Send + Sync {
    async fn list_recoverable(
        &self,
        limit: usize,
    ) -> Result<Vec<AgentRunProductRuntimeRecoveryId>, AgentRunProductRuntimeRecoveryRepositoryError>;

    async fn advance(
        &self,
        recovery_id: &AgentRunProductRuntimeRecoveryId,
    ) -> Result<AgentRunProductRuntimeRecoverySaga, AgentRunProductRuntimeRecoveryError>;
}

/// Product-owned, restart-safe recovery saga for a Host generation change.
///
/// The repository persists the first Rebind revision fence and both Runtime operation identities
/// before dispatch. Every later Product/Runtime effect is idempotent and is followed by one
/// optimistic saga transition, so a worker can replay the exact operation after process loss.
pub struct AgentRunProductRuntimeRecoveryService {
    sagas: Arc<dyn AgentRunProductRuntimeRecoverySagaRepository>,
    runtime: Arc<dyn ManagedAgentRuntimeGateway>,
    bindings: Arc<dyn AgentRunProductRuntimeBindingStore>,
    resources: Arc<dyn AgentRunAppliedResourceSurfaceMaterializationPort>,
    resource_query: Arc<dyn AgentRunAppliedResourceSurfaceQueryPort>,
    preparation: Arc<dyn AgentRunProductRuntimeRecoveryPreparationPort>,
}

impl AgentRunProductRuntimeRecoveryService {
    pub fn new(
        sagas: Arc<dyn AgentRunProductRuntimeRecoverySagaRepository>,
        runtime: Arc<dyn ManagedAgentRuntimeGateway>,
        bindings: Arc<dyn AgentRunProductRuntimeBindingStore>,
        resources: Arc<dyn AgentRunAppliedResourceSurfaceMaterializationPort>,
        resource_query: Arc<dyn AgentRunAppliedResourceSurfaceQueryPort>,
        preparation: Arc<dyn AgentRunProductRuntimeRecoveryPreparationPort>,
    ) -> Self {
        Self {
            sagas,
            runtime,
            bindings,
            resources,
            resource_query,
            preparation,
        }
    }

    async fn accept_or_load(
        &self,
        request: &AgentRunProductRuntimeRecoveryRequest,
    ) -> Result<AgentRunProductRuntimeRecoverySaga, AgentRunProductRuntimeRecoveryError> {
        let client_command_id = request.client_command_id.trim();
        if client_command_id.is_empty() || client_command_id.len() > 256 {
            return Err(AgentRunProductRuntimeRecoveryError::InvalidRequest);
        }
        let recovery_id =
            AgentRunProductRuntimeRecoveryId::for_request(&request.target, client_command_id)
                .map_err(|_| AgentRunProductRuntimeRecoveryError::InvalidRequest)?;
        if let Some(existing) = self
            .sagas
            .load(&recovery_id)
            .await
            .map_err(repository_error)?
        {
            return validate_existing_request(existing, request);
        }

        let current_binding = self
            .bindings
            .load_product_binding(&request.target)
            .await
            .map_err(AgentRunProductRuntimeRecoveryError::Binding)?
            .ok_or(AgentRunProductRuntimeRecoveryError::BindingMissing)?;
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

        let requested = AgentRunProductRuntimeRecoverySaga::requested(
            request.target.clone(),
            client_command_id,
            request.expected_revision,
            current_binding,
        )
        .map_err(AgentRunProductRuntimeRecoveryError::Binding)?;
        match self.sagas.create(requested).await {
            Ok(saga) => Ok(saga),
            Err(AgentRunProductRuntimeRecoveryRepositoryError::AlreadyExists) => {
                let existing = self
                    .sagas
                    .load(&recovery_id)
                    .await
                    .map_err(repository_error)?
                    .ok_or(AgentRunProductRuntimeRecoveryError::BindingMissing)?;
                validate_existing_request(existing, request)
            }
            Err(error) => Err(repository_error(error)),
        }
    }

    async fn advance_once(
        &self,
        recovery_id: &AgentRunProductRuntimeRecoveryId,
    ) -> Result<AgentRunProductRuntimeRecoverySaga, AgentRunProductRuntimeRecoveryError> {
        let saga = self
            .sagas
            .load(recovery_id)
            .await
            .map_err(repository_error)?
            .ok_or(AgentRunProductRuntimeRecoveryError::BindingMissing)?;
        let expected_version = saga.version();
        let advanced = match saga.phase() {
            AgentRunProductRuntimeRecoveryPhase::Requested => self.advance_rebind(saga).await?,
            AgentRunProductRuntimeRecoveryPhase::RebindApplied => {
                let prepared = saga
                    .prepared_binding()
                    .ok_or(AgentRunProductRuntimeRecoveryError::RuntimeBindingMismatch)?;
                self.bindings
                    .prepare_product_binding_recovery(saga.previous_binding_digest(), prepared)
                    .await
                    .map_err(AgentRunProductRuntimeRecoveryError::Binding)?;
                saga.record_product_binding_prepared()
                    .map_err(AgentRunProductRuntimeRecoveryError::Binding)?
            }
            AgentRunProductRuntimeRecoveryPhase::ProductBindingPrepared => {
                self.advance_resource(saga).await?
            }
            AgentRunProductRuntimeRecoveryPhase::ResourceMaterialized => {
                self.advance_activate(saga).await?
            }
            AgentRunProductRuntimeRecoveryPhase::RuntimeActivated => {
                let binding = saga
                    .activated_binding()
                    .ok_or(AgentRunProductRuntimeRecoveryError::RuntimeBindingMismatch)?;
                self.bindings
                    .activate_product_binding(
                        binding,
                        saga.prepared_binding_digest()
                            .ok_or(AgentRunProductRuntimeRecoveryError::RuntimeBindingMismatch)?,
                        saga.resource_snapshot_revision()
                            .ok_or(AgentRunProductRuntimeRecoveryError::RuntimeBindingMismatch)?,
                    )
                    .await
                    .map_err(AgentRunProductRuntimeRecoveryError::Binding)?;
                saga.record_succeeded()
                    .map_err(AgentRunProductRuntimeRecoveryError::Binding)?
            }
            AgentRunProductRuntimeRecoveryPhase::Succeeded => return Ok(saga),
        };
        self.sagas
            .save(expected_version, advanced)
            .await
            .map_err(repository_error)
    }

    async fn advance_rebind(
        &self,
        saga: AgentRunProductRuntimeRecoverySaga,
    ) -> Result<AgentRunProductRuntimeRecoverySaga, AgentRunProductRuntimeRecoveryError> {
        self.preparation
            .prepare_recovery_attachment(saga.previous_binding())
            .await
            .map_err(AgentRunProductRuntimeRecoveryError::Binding)?;
        let receipt = self
            .runtime
            .execute(recovery_envelope(
                &saga,
                true,
                Some(saga.rebind_expected_revision()),
                ManagedRuntimeCommand::Rebind,
            ))
            .await?;
        require_succeeded_receipt(&receipt)?;
        let rebound = self
            .runtime
            .read(ManagedRuntimeReadRequest {
                thread_id: saga.runtime_thread_id().clone(),
            })
            .await?;
        let rebound_source = rebound
            .source_binding
            .clone()
            .ok_or(AgentRunProductRuntimeRecoveryError::RuntimeBindingMismatch)?;
        let previous = saga.previous_binding();
        if rebound.thread_id != *saga.runtime_thread_id()
            || rebound_source.source_ref != previous.source_binding.source_ref
            || rebound_source.applied_surface_revision
                != previous.source_binding.applied_surface_revision
            || rebound_source.committed_at_revision < previous.source_binding.committed_at_revision
        {
            return Err(AgentRunProductRuntimeRecoveryError::RuntimeBindingMismatch);
        }
        let mut prepared_source = rebound_source;
        prepared_source.activated_at_revision = None;
        let prepared = AgentRunProductRuntimeBinding {
            source_binding: prepared_source,
            ..previous.clone()
        };
        saga.record_rebind_applied(receipt, prepared)
            .map_err(AgentRunProductRuntimeRecoveryError::Binding)
    }

    async fn advance_resource(
        &self,
        saga: AgentRunProductRuntimeRecoverySaga,
    ) -> Result<AgentRunProductRuntimeRecoverySaga, AgentRunProductRuntimeRecoveryError> {
        let prepared = saga
            .prepared_binding()
            .ok_or(AgentRunProductRuntimeRecoveryError::RuntimeBindingMismatch)?;
        let digest = saga
            .prepared_binding_digest()
            .ok_or(AgentRunProductRuntimeRecoveryError::RuntimeBindingMismatch)?;
        let current = match self
            .resource_query
            .applied_resource_surface(&prepared.target, None)
            .await
        {
            Ok(snapshot) => Some(snapshot),
            Err(AgentRunAppliedResourceSurfaceQueryError::SurfaceNotApplied) => None,
            Err(error) => {
                return Err(AgentRunProductRuntimeRecoveryError::ResourceSurface(
                    error.to_string(),
                ));
            }
        };
        let snapshot_revision = if current
            .as_ref()
            .is_some_and(|snapshot| snapshot.surface.product_binding_digest == digest)
        {
            current
                .as_ref()
                .expect("matching current Product resource snapshot")
                .snapshot_revision
        } else {
            let expected_current_snapshot_revision =
                current.as_ref().map(|snapshot| snapshot.snapshot_revision);
            self.resources
                .materialize(AgentRunAppliedResourceSurfaceMaterializeRequest {
                    target: prepared.target.clone(),
                    expected_current_snapshot_revision,
                    product_binding_digest: digest.to_string(),
                })
                .await
                .map_err(|error| {
                    AgentRunProductRuntimeRecoveryError::ResourceSurface(error.to_string())
                })?;
            let materialized = self
                .resource_query
                .applied_resource_surface(&prepared.target, None)
                .await
                .map_err(|error| {
                    AgentRunProductRuntimeRecoveryError::ResourceSurface(error.to_string())
                })?;
            if materialized.surface.product_binding_digest != digest {
                return Err(AgentRunProductRuntimeRecoveryError::RuntimeBindingMismatch);
            }
            materialized.snapshot_revision
        };
        saga.record_resource_materialized(snapshot_revision)
            .map_err(AgentRunProductRuntimeRecoveryError::Binding)
    }

    async fn advance_activate(
        &self,
        saga: AgentRunProductRuntimeRecoverySaga,
    ) -> Result<AgentRunProductRuntimeRecoverySaga, AgentRunProductRuntimeRecoveryError> {
        let rebind_receipt = saga
            .rebind_receipt()
            .ok_or(AgentRunProductRuntimeRecoveryError::RuntimeBindingMismatch)?;
        let receipt = self
            .runtime
            .execute(recovery_envelope(
                &saga,
                false,
                Some(rebind_receipt.accepted_revision),
                ManagedRuntimeCommand::Activate,
            ))
            .await?;
        require_succeeded_receipt(&receipt)?;
        let activated = self
            .runtime
            .read(ManagedRuntimeReadRequest {
                thread_id: saga.runtime_thread_id().clone(),
            })
            .await?;
        let activated_source = activated
            .source_binding
            .clone()
            .ok_or(AgentRunProductRuntimeRecoveryError::RuntimeBindingMismatch)?;
        let prepared = saga
            .prepared_binding()
            .ok_or(AgentRunProductRuntimeRecoveryError::RuntimeBindingMismatch)?;
        if activated.thread_id != *saga.runtime_thread_id()
            || activated_source.source_ref != prepared.source_binding.source_ref
            || activated_source.committed_at_revision
                != prepared.source_binding.committed_at_revision
            || activated_source.applied_surface_revision
                != prepared.source_binding.applied_surface_revision
            || activated_source.activated_at_revision.is_none()
        {
            return Err(AgentRunProductRuntimeRecoveryError::RuntimeBindingMismatch);
        }
        let activated_binding = AgentRunProductRuntimeBinding {
            source_binding: activated_source,
            ..prepared.clone()
        };
        saga.record_runtime_activated(receipt, activated_binding)
            .map_err(AgentRunProductRuntimeRecoveryError::Binding)
    }
}

#[async_trait]
impl AgentRunProductRuntimeRecoveryAdvancementPort for AgentRunProductRuntimeRecoveryService {
    async fn list_recoverable(
        &self,
        limit: usize,
    ) -> Result<Vec<AgentRunProductRuntimeRecoveryId>, AgentRunProductRuntimeRecoveryRepositoryError>
    {
        self.sagas.list_recoverable(limit).await
    }

    async fn advance(
        &self,
        recovery_id: &AgentRunProductRuntimeRecoveryId,
    ) -> Result<AgentRunProductRuntimeRecoverySaga, AgentRunProductRuntimeRecoveryError> {
        self.advance_once(recovery_id).await
    }
}

#[async_trait]
impl AgentRunProductRuntimeRecoveryPort for AgentRunProductRuntimeRecoveryService {
    async fn recover(
        &self,
        request: AgentRunProductRuntimeRecoveryRequest,
    ) -> Result<AgentRunProductRuntimeRecoveryOutcome, AgentRunProductRuntimeRecoveryError> {
        let accepted = self.accept_or_load(&request).await?;
        let recovery_id = accepted.recovery_id().clone();
        for _ in 0..16 {
            let saga = match self.advance_once(&recovery_id).await {
                Err(AgentRunProductRuntimeRecoveryError::Binding(reason))
                    if reason.contains("write conflicted") =>
                {
                    continue;
                }
                result => result?,
            };
            if saga.phase() == AgentRunProductRuntimeRecoveryPhase::Succeeded {
                return saga_outcome(&saga);
            }
        }
        Err(AgentRunProductRuntimeRecoveryError::Binding(
            "Product Runtime recovery did not converge within the foreground window".to_string(),
        ))
    }
}

fn recovery_envelope(
    saga: &AgentRunProductRuntimeRecoverySaga,
    rebind: bool,
    expected_revision: Option<RuntimeProjectionRevision>,
    command: ManagedRuntimeCommand,
) -> ManagedRuntimeCommandEnvelope {
    let identity = if rebind {
        saga.rebind_identity()
    } else {
        saga.activate_identity()
    };
    ManagedRuntimeCommandEnvelope {
        operation_id: identity.operation_id.clone(),
        idempotency_key: identity.idempotency_key.clone(),
        thread_id: saga.runtime_thread_id().clone(),
        expected_revision,
        command,
    }
}

fn validate_existing_request(
    saga: AgentRunProductRuntimeRecoverySaga,
    request: &AgentRunProductRuntimeRecoveryRequest,
) -> Result<AgentRunProductRuntimeRecoverySaga, AgentRunProductRuntimeRecoveryError> {
    if saga.target() != &request.target
        || saga.client_command_id() != request.client_command_id.trim()
    {
        return Err(AgentRunProductRuntimeRecoveryError::InvalidRequest);
    }
    Ok(saga)
}

fn require_succeeded_receipt(
    receipt: &ManagedRuntimeOperationReceipt,
) -> Result<(), AgentRunProductRuntimeRecoveryError> {
    if receipt.status == ManagedRuntimeOperationStatus::Succeeded {
        Ok(())
    } else {
        Err(AgentRunProductRuntimeRecoveryError::Runtime(
            ManagedRuntimeGatewayError::Unavailable {
                reason: format!(
                    "Product Runtime recovery operation is not terminal: {:?}",
                    receipt.status
                ),
            },
        ))
    }
}

fn saga_outcome(
    saga: &AgentRunProductRuntimeRecoverySaga,
) -> Result<AgentRunProductRuntimeRecoveryOutcome, AgentRunProductRuntimeRecoveryError> {
    Ok(AgentRunProductRuntimeRecoveryOutcome {
        binding: saga
            .activated_binding()
            .cloned()
            .ok_or(AgentRunProductRuntimeRecoveryError::RuntimeBindingMismatch)?,
        resource_snapshot_revision: saga
            .resource_snapshot_revision()
            .ok_or(AgentRunProductRuntimeRecoveryError::RuntimeBindingMismatch)?,
        rebind_receipt: saga
            .rebind_receipt()
            .cloned()
            .ok_or(AgentRunProductRuntimeRecoveryError::RuntimeBindingMismatch)?,
        activate_receipt: saga
            .activate_receipt()
            .cloned()
            .ok_or(AgentRunProductRuntimeRecoveryError::RuntimeBindingMismatch)?,
    })
}

fn repository_error(
    error: AgentRunProductRuntimeRecoveryRepositoryError,
) -> AgentRunProductRuntimeRecoveryError {
    AgentRunProductRuntimeRecoveryError::Binding(error.to_string())
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{BTreeMap, HashMap},
        sync::atomic::{AtomicBool, Ordering},
    };

    use agentdash_agent_runtime_contract::{
        ManagedRuntimeChangePage, ManagedRuntimeChangesRequest, ManagedRuntimeLifecycleStatus,
        ManagedRuntimeOperationStatus, ManagedRuntimeProjectionAuthority,
        ManagedRuntimeProjectionFidelity, ManagedRuntimeSnapshot,
        ManagedRuntimeSourceBindingEvidence, RuntimeChangeSequence, RuntimeOperationId,
        RuntimeSourceRef, RuntimeThreadId, SurfaceRevision,
    };
    use tokio::sync::Mutex;
    use uuid::Uuid;

    use super::*;
    use crate::agent_run::{
        AgentRunAppliedResourceSurface, AgentRunAppliedResourceSurfaceCommitOutcome,
        AgentRunAppliedResourceSurfaceProvenance, AgentRunAppliedResourceSurfaceSnapshot,
        AgentRunAppliedResourceSurfaceWriteError, AgentRunProductRuntimeBindingRepository,
        AppliedTaskGrant, AppliedVfsGrant, AppliedVfsMount, ProductAgentFrameRef,
    };

    #[derive(Default)]
    struct RecoverySagaMemory {
        saga: Mutex<Option<AgentRunProductRuntimeRecoverySaga>>,
        fail_next_save: AtomicBool,
    }

    impl RecoverySagaMemory {
        fn fail_next_save(&self) {
            self.fail_next_save.store(true, Ordering::SeqCst);
        }
    }

    #[async_trait]
    impl AgentRunProductRuntimeRecoverySagaRepository for RecoverySagaMemory {
        async fn create(
            &self,
            saga: AgentRunProductRuntimeRecoverySaga,
        ) -> Result<AgentRunProductRuntimeRecoverySaga, AgentRunProductRuntimeRecoveryRepositoryError>
        {
            let mut stored = self.saga.lock().await;
            if stored.is_some() {
                return Err(AgentRunProductRuntimeRecoveryRepositoryError::AlreadyExists);
            }
            let saga = saga.advance_persisted_version(0).map_err(|error| {
                AgentRunProductRuntimeRecoveryRepositoryError::Unavailable(error)
            })?;
            *stored = Some(saga.clone());
            Ok(saga)
        }

        async fn load(
            &self,
            recovery_id: &AgentRunProductRuntimeRecoveryId,
        ) -> Result<
            Option<AgentRunProductRuntimeRecoverySaga>,
            AgentRunProductRuntimeRecoveryRepositoryError,
        > {
            Ok(self
                .saga
                .lock()
                .await
                .as_ref()
                .filter(|saga| saga.recovery_id() == recovery_id)
                .cloned())
        }

        async fn list_recoverable(
            &self,
            limit: usize,
        ) -> Result<
            Vec<AgentRunProductRuntimeRecoveryId>,
            AgentRunProductRuntimeRecoveryRepositoryError,
        > {
            if limit == 0 {
                return Ok(Vec::new());
            }
            Ok(self
                .saga
                .lock()
                .await
                .as_ref()
                .filter(|saga| saga.phase() != AgentRunProductRuntimeRecoveryPhase::Succeeded)
                .map(|saga| vec![saga.recovery_id().clone()])
                .unwrap_or_default())
        }

        async fn save(
            &self,
            expected_version: u64,
            saga: AgentRunProductRuntimeRecoverySaga,
        ) -> Result<AgentRunProductRuntimeRecoverySaga, AgentRunProductRuntimeRecoveryRepositoryError>
        {
            if self.fail_next_save.swap(false, Ordering::SeqCst) {
                return Err(AgentRunProductRuntimeRecoveryRepositoryError::Unavailable(
                    "simulated process loss before saga save".to_owned(),
                ));
            }
            let mut stored = self.saga.lock().await;
            let current = stored
                .as_ref()
                .ok_or(AgentRunProductRuntimeRecoveryRepositoryError::NotFound)?;
            if current.version() != expected_version {
                return Err(AgentRunProductRuntimeRecoveryRepositoryError::Conflict);
            }
            let saga = saga
                .advance_persisted_version(expected_version)
                .map_err(AgentRunProductRuntimeRecoveryRepositoryError::Unavailable)?;
            *stored = Some(saga.clone());
            Ok(saga)
        }
    }

    struct RecoveryRuntime {
        snapshot: Mutex<ManagedRuntimeSnapshot>,
        receipts: Mutex<HashMap<RuntimeOperationId, ManagedRuntimeOperationReceipt>>,
        envelopes: Mutex<Vec<ManagedRuntimeCommandEnvelope>>,
    }

    #[async_trait]
    impl ManagedAgentRuntimeGateway for RecoveryRuntime {
        async fn execute(
            &self,
            envelope: ManagedRuntimeCommandEnvelope,
        ) -> Result<ManagedRuntimeOperationReceipt, ManagedRuntimeGatewayError> {
            self.envelopes.lock().await.push(envelope.clone());
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
            if envelope.expected_revision != Some(snapshot.revision) {
                return Err(ManagedRuntimeGatewayError::Invalid {
                    reason: "test Runtime revision fence mismatch".to_owned(),
                });
            }
            let next_revision = RuntimeProjectionRevision(snapshot.revision.0 + 1);
            let source = snapshot.source_binding.as_mut().ok_or_else(|| {
                ManagedRuntimeGatewayError::Invalid {
                    reason: "test Runtime source binding is missing".to_owned(),
                }
            })?;
            match &envelope.command {
                ManagedRuntimeCommand::Rebind => {
                    source.committed_at_revision = next_revision;
                    source.activated_at_revision = None;
                }
                ManagedRuntimeCommand::Activate => {
                    source.activated_at_revision = Some(next_revision);
                }
                _ => {
                    return Err(ManagedRuntimeGatewayError::Invalid {
                        reason: "unexpected recovery command".to_owned(),
                    });
                }
            }
            snapshot.revision = next_revision;
            let receipt = ManagedRuntimeOperationReceipt {
                operation_id: envelope.operation_id,
                thread_id: envelope.thread_id,
                accepted_revision: next_revision,
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
            let mut current = self.binding.lock().await;
            if current.calculated_digest()? != expected_binding_digest {
                return Err("activation CAS digest mismatch".to_owned());
            }
            *current = binding.clone();
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
            if request.expected_current_snapshot_revision != Some(snapshot.snapshot_revision) {
                return Err(AgentRunAppliedResourceSurfaceWriteError::Conflict {
                    message: "test resource revision fence mismatch".to_owned(),
                });
            }
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

    struct RecoveryPreparation;

    #[async_trait]
    impl AgentRunProductRuntimeRecoveryPreparationPort for RecoveryPreparation {
        async fn prepare_recovery_attachment(
            &self,
            binding: &AgentRunProductRuntimeBinding,
        ) -> Result<(), String> {
            if binding.execution_profile.validate() {
                Ok(())
            } else {
                Err("invalid recovery execution profile".to_owned())
            }
        }
    }

    #[tokio::test]
    async fn process_restart_replays_the_frozen_rebind_and_continues_to_activation() {
        let target = AgentRunTarget {
            run_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
        };
        let thread_id = RuntimeThreadId::new("recovery-thread").unwrap();
        let old_binding = product_binding(&target, &thread_id, source_binding(7, Some(7)));
        let sagas = Arc::new(RecoverySagaMemory::default());
        let runtime = Arc::new(RecoveryRuntime {
            snapshot: Mutex::new(runtime_snapshot(
                thread_id,
                old_binding.source_binding.clone(),
            )),
            receipts: Mutex::new(HashMap::new()),
            envelopes: Mutex::new(Vec::new()),
        });
        let bindings = Arc::new(RecoveryBindings {
            binding: Mutex::new(old_binding.clone()),
        });
        let resources = Arc::new(RecoveryResources {
            snapshot: Mutex::new(resource_snapshot(
                target.clone(),
                old_binding.calculated_digest().unwrap(),
            )),
        });
        let service = || {
            AgentRunProductRuntimeRecoveryService::new(
                sagas.clone(),
                runtime.clone(),
                bindings.clone(),
                resources.clone(),
                resources.clone(),
                Arc::new(RecoveryPreparation),
            )
        };
        let request = AgentRunProductRuntimeRecoveryRequest {
            target: target.clone(),
            client_command_id: "remote-placement-epoch".to_owned(),
            expected_revision: RuntimeProjectionRevision(7),
        };

        sagas.fail_next_save();
        let first_error = service().recover(request.clone()).await.unwrap_err();
        assert!(first_error.to_string().contains("simulated process loss"));
        assert_eq!(
            sagas.saga.lock().await.as_ref().unwrap().phase(),
            AgentRunProductRuntimeRecoveryPhase::Requested
        );

        let outcome = service()
            .recover(AgentRunProductRuntimeRecoveryRequest {
                expected_revision: RuntimeProjectionRevision(999),
                ..request
            })
            .await
            .unwrap();

        assert_eq!(
            outcome.rebind_receipt.accepted_revision,
            RuntimeProjectionRevision(8)
        );
        assert_eq!(
            outcome.activate_receipt.accepted_revision,
            RuntimeProjectionRevision(9)
        );
        assert_eq!(
            sagas.saga.lock().await.as_ref().unwrap().phase(),
            AgentRunProductRuntimeRecoveryPhase::Succeeded
        );
        let envelopes = runtime.envelopes.lock().await;
        assert_eq!(envelopes.len(), 3);
        assert!(matches!(
            envelopes[0].command,
            ManagedRuntimeCommand::Rebind
        ));
        assert!(matches!(
            envelopes[1].command,
            ManagedRuntimeCommand::Rebind
        ));
        assert_eq!(envelopes[0].operation_id, envelopes[1].operation_id);
        assert_eq!(envelopes[0].idempotency_key, envelopes[1].idempotency_key);
        assert_eq!(
            envelopes[1].expected_revision,
            Some(RuntimeProjectionRevision(7))
        );
        assert!(matches!(
            envelopes[2].command,
            ManagedRuntimeCommand::Activate
        ));
        assert_eq!(
            envelopes[2].expected_revision,
            Some(RuntimeProjectionRevision(8))
        );
        assert_eq!(runtime.receipts.lock().await.len(), 2);
    }

    fn source_binding(
        committed_revision: u64,
        activated_revision: Option<u64>,
    ) -> ManagedRuntimeSourceBindingEvidence {
        ManagedRuntimeSourceBindingEvidence {
            source_ref: RuntimeSourceRef::new("source:recovery").unwrap(),
            committed_at_revision: RuntimeProjectionRevision(committed_revision),
            applied_surface_revision: SurfaceRevision(3),
            activated_at_revision: activated_revision.map(RuntimeProjectionRevision),
        }
    }

    fn product_binding(
        target: &AgentRunTarget,
        runtime_thread_id: &RuntimeThreadId,
        source_binding: ManagedRuntimeSourceBindingEvidence,
    ) -> AgentRunProductRuntimeBinding {
        let execution_profile = recovery_execution_profile();
        AgentRunProductRuntimeBinding {
            target: target.clone(),
            runtime_thread_id: runtime_thread_id.clone(),
            launch_frame: ProductAgentFrameRef {
                frame_id: Uuid::new_v4(),
                agent_id: target.agent_id,
                revision: 3,
            },
            execution_profile_digest: execution_profile.profile_digest.clone(),
            execution_profile,
            source_binding,
        }
    }

    fn recovery_execution_profile() -> crate::agent_run::ProductExecutionProfileRef {
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

    fn runtime_snapshot(
        thread_id: RuntimeThreadId,
        source_binding: ManagedRuntimeSourceBindingEvidence,
    ) -> ManagedRuntimeSnapshot {
        ManagedRuntimeSnapshot {
            thread_id,
            revision: source_binding.committed_at_revision,
            latest_change_sequence: RuntimeChangeSequence(1),
            captured_at_ms: 1,
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
}
