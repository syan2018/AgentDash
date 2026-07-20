use std::sync::Arc;

use agentdash_agent_runtime_contract::{
    ManagedAgentRuntimeGateway, ManagedRuntimeCommand, ManagedRuntimeCommandEnvelope,
    ManagedRuntimeGatewayError, ManagedRuntimeOperationReceipt, ManagedRuntimeOperationStatus,
    ManagedRuntimeReadRequest,
};
use agentdash_domain::agent_run_target::AgentRunTarget;
use async_trait::async_trait;

use super::{
    AgentRunProductRuntimeBinding, AgentRunProductRuntimeBindingStore,
    AgentRunProductRuntimeRecoveryId, AgentRunProductRuntimeRecoveryPhase,
    AgentRunProductRuntimeRecoveryRepositoryError, AgentRunProductRuntimeRecoverySaga,
    AgentRunProductRuntimeRecoverySagaRepository,
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
}

#[derive(Debug, Clone)]
pub struct AgentRunProductRuntimeRecoveryOutcome {
    pub binding: AgentRunProductRuntimeBinding,
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
/// The repository persists both Runtime operation identities before dispatch. Every later
/// Product/Runtime effect is idempotent and is followed by one optimistic saga transition, so a
/// worker can replay the exact operation after process loss.
pub struct AgentRunProductRuntimeRecoveryService {
    sagas: Arc<dyn AgentRunProductRuntimeRecoverySagaRepository>,
    runtime: Arc<dyn ManagedAgentRuntimeGateway>,
    bindings: Arc<dyn AgentRunProductRuntimeBindingStore>,
    preparation: Arc<dyn AgentRunProductRuntimeRecoveryPreparationPort>,
}

impl AgentRunProductRuntimeRecoveryService {
    pub fn new(
        sagas: Arc<dyn AgentRunProductRuntimeRecoverySagaRepository>,
        runtime: Arc<dyn ManagedAgentRuntimeGateway>,
        bindings: Arc<dyn AgentRunProductRuntimeBindingStore>,
        preparation: Arc<dyn AgentRunProductRuntimeRecoveryPreparationPort>,
    ) -> Self {
        Self {
            sagas,
            runtime,
            bindings,
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
            || before_source.applied_surface_revision.0 != current_binding.launch_frame.revision
        {
            return Err(AgentRunProductRuntimeRecoveryError::RuntimeBindingMismatch);
        }

        let requested = AgentRunProductRuntimeRecoverySaga::requested(
            request.target.clone(),
            client_command_id,
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
                self.advance_activate(saga).await?
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
            || rebound_source.applied_surface_revision.0 != previous.launch_frame.revision
        {
            return Err(AgentRunProductRuntimeRecoveryError::RuntimeBindingMismatch);
        }
        saga.record_rebind_applied(receipt)
            .map_err(AgentRunProductRuntimeRecoveryError::Binding)
    }

    async fn advance_activate(
        &self,
        saga: AgentRunProductRuntimeRecoverySaga,
    ) -> Result<AgentRunProductRuntimeRecoverySaga, AgentRunProductRuntimeRecoveryError> {
        let receipt = self
            .runtime
            .execute(recovery_envelope(
                &saga,
                false,
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
        if activated.thread_id != *saga.runtime_thread_id()
            || activated_source.applied_surface_revision.0
                != saga.previous_binding().launch_frame.revision
            || activated_source.activated_at_revision.is_none()
        {
            return Err(AgentRunProductRuntimeRecoveryError::RuntimeBindingMismatch);
        }
        saga.record_succeeded(receipt)
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
        binding: saga.previous_binding().clone(),
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
        RuntimeProjectionRevision, RuntimeSourceRef, RuntimeThreadId, SurfaceRevision,
    };
    use tokio::sync::Mutex;
    use uuid::Uuid;

    use super::*;
    use crate::agent_run::{AgentRunProductRuntimeBindingRepository, ProductAgentFrameRef};

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
        ) -> Result<super::super::AgentRunCommittedProductRuntimeBinding, String> {
            *self.binding.lock().await = binding.clone();
            binding.committed_receipt()
        }

        async fn replace_product_binding(
            &self,
            expected_previous_binding_digest: &str,
            binding: &AgentRunProductRuntimeBinding,
        ) -> Result<super::super::AgentRunCommittedProductRuntimeBinding, String> {
            let mut current = self.binding.lock().await;
            let current_digest = current.calculated_digest()?;
            let next_digest = binding.calculated_digest()?;
            if current_digest != expected_previous_binding_digest && current_digest != next_digest {
                return Err("recovery CAS digest mismatch".to_owned());
            }
            *current = binding.clone();
            binding.committed_receipt()
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
            snapshot: Mutex::new(runtime_snapshot(thread_id, source_binding(7, Some(7)))),
            receipts: Mutex::new(HashMap::new()),
            envelopes: Mutex::new(Vec::new()),
        });
        let bindings = Arc::new(RecoveryBindings {
            binding: Mutex::new(old_binding.clone()),
        });
        let service = || {
            AgentRunProductRuntimeRecoveryService::new(
                sagas.clone(),
                runtime.clone(),
                bindings.clone(),
                Arc::new(RecoveryPreparation),
            )
        };
        let request = AgentRunProductRuntimeRecoveryRequest {
            target: target.clone(),
            client_command_id: "remote-placement-epoch".to_owned(),
        };

        sagas.fail_next_save();
        let first_error = service().recover(request.clone()).await.unwrap_err();
        assert!(first_error.to_string().contains("simulated process loss"));
        assert_eq!(
            sagas.saga.lock().await.as_ref().unwrap().phase(),
            AgentRunProductRuntimeRecoveryPhase::Requested
        );

        let outcome = service().recover(request).await.unwrap();

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
        assert!(matches!(
            envelopes[2].command,
            ManagedRuntimeCommand::Activate
        ));
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
        _source_binding: ManagedRuntimeSourceBindingEvidence,
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
}
