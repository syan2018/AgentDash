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
    #[error("Managed Runtime source binding is missing after {0}")]
    MissingSourceBinding(&'static str),
    #[error("Managed Runtime applied surface does not match Product provisioning")]
    RuntimeBindingMismatch,
}

/// Product-owned fresh AgentRun launch saga.
///
/// The ordering is intentional and crash-safe:
/// provision Host target -> Create -> commit pre-activation Product binding ->
/// Activate -> pin activated binding -> optional first input.
pub struct AgentRunProductLaunchService {
    provisioning: Arc<dyn AgentRunProductRuntimeProvisioningPort>,
    runtime: Arc<dyn ManagedAgentRuntimeGateway>,
    bindings: Arc<dyn AgentRunProductRuntimeBindingStore>,
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
    ) -> Self {
        Self {
            provisioning,
            runtime,
            bindings,
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
    /// binding。该步骤必须先于 Activate。
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
        let binding = AgentRunProductRuntimeBinding {
            target: request.target.clone(),
            runtime_thread_id: request.runtime_thread_id.clone(),
            launch_frame: request.frame.clone(),
            execution_profile: request.execution_profile.clone(),
            execution_profile_digest: request.execution_profile.profile_digest.clone(),
        };
        self.bindings
            .commit_product_binding(&binding)
            .await
            .map_err(AgentRunProductLaunchError::Binding)?;
        Ok(binding)
    }

    /// 在 Runtime Activate evidence 已提交后，幂等 pin Product binding。
    pub async fn converge_activated_runtime(
        &self,
        request: &AgentRunProductRuntimeProvisioningRequest,
    ) -> Result<AgentRunProductRuntimeBinding, AgentRunProductLaunchError> {
        let binding = self
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
            || activated_source.applied_surface_revision.0 != binding.launch_frame.revision
        {
            return Err(AgentRunProductLaunchError::RuntimeBindingMismatch);
        }
        Ok(binding)
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
        let binding = AgentRunProductRuntimeBinding {
            target: request.provisioning.target.clone(),
            runtime_thread_id: request.provisioning.runtime_thread_id.clone(),
            launch_frame: request.provisioning.frame.clone(),
            execution_profile: request.provisioning.execution_profile.clone(),
            execution_profile_digest: request
                .provisioning
                .execution_profile
                .profile_digest
                .clone(),
        };
        self.bindings
            .commit_product_binding(&binding)
            .await
            .map_err(AgentRunProductLaunchError::Binding)?;

        let activate_receipt = self
            .runtime
            .execute(envelope(
                &request.provisioning,
                "activate",
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
            || activated_source.applied_surface_revision.0 != binding.launch_frame.revision
        {
            return Err(AgentRunProductLaunchError::RuntimeBindingMismatch);
        }

        let input_receipt = if request.initial_input.is_empty() {
            None
        } else {
            Some(
                self.runtime
                    .execute(envelope(
                        &request.provisioning,
                        "initial-input",
                        ManagedRuntimeCommand::SubmitInput {
                            content: request.initial_input,
                        },
                    )?)
                    .await?,
            )
        };

        Ok(AgentRunProductLaunchOutcome {
            binding,
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

fn envelope(
    provisioning: &AgentRunProductRuntimeProvisioningRequest,
    phase: &'static str,
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
        AgentRunProductRuntimeProvisioningError, AgentRunProductRuntimeProvisioningEvidence,
        ProductAgentFrameRef, ProductAgentSurfaceFacts, ProductExecutionProfileRef,
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
                ManagedRuntimeCommand::Activate => RuntimeProjectionRevision(4),
                ManagedRuntimeCommand::SubmitInput { .. } => RuntimeProjectionRevision(7),
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
        ) -> Result<super::super::AgentRunCommittedProductRuntimeBinding, String> {
            *self.binding.lock().await = Some(binding.clone());
            binding.committed_receipt()
        }

        async fn replace_product_binding(
            &self,
            _expected_previous_binding_digest: &str,
            binding: &AgentRunProductRuntimeBinding,
        ) -> Result<super::super::AgentRunCommittedProductRuntimeBinding, String> {
            *self.binding.lock().await = Some(binding.clone());
            binding.committed_receipt()
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

    fn activated_snapshot(
        request: &AgentRunProductLaunchRequest,
        source_binding: ManagedRuntimeSourceBindingEvidence,
    ) -> ManagedRuntimeSnapshot {
        ManagedRuntimeSnapshot {
            thread_id: request.provisioning.runtime_thread_id.clone(),
            revision: RuntimeProjectionRevision(6),
            latest_change_sequence: RuntimeChangeSequence(6),
            captured_at_ms: 6,
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
            committed_at_revision: RuntimeProjectionRevision(3),
            applied_surface_revision: SurfaceRevision(1),
            activated_at_revision: Some(RuntimeProjectionRevision(6)),
        };
        let product_binding = AgentRunProductRuntimeBinding {
            target: request.provisioning.target.clone(),
            runtime_thread_id: request.provisioning.runtime_thread_id.clone(),
            launch_frame: request.provisioning.frame.clone(),
            execution_profile: request.provisioning.execution_profile.clone(),
            execution_profile_digest: request
                .provisioning
                .execution_profile
                .profile_digest
                .clone(),
        };
        let runtime = Arc::new(ReplayRuntime {
            snapshot: activated_snapshot(&request, activated_source),
            observed: Mutex::new(Vec::new()),
        });
        let bindings = Arc::new(ReplayBindingStore {
            binding: Mutex::new(Some(product_binding.clone())),
        });
        let service = AgentRunProductLaunchService::new(
            Arc::new(EchoProvisioning),
            runtime.clone(),
            bindings.clone(),
        );

        let outcome = service
            .launch(request.clone())
            .await
            .expect("replay converges");

        assert_eq!(outcome.binding, product_binding);
        assert_eq!(
            bindings.binding.lock().await.as_ref(),
            Some(&product_binding)
        );
        let observed = runtime.observed.lock().await;
        assert_eq!(observed.len(), 3);
        assert!(matches!(
            observed[0].command,
            ManagedRuntimeCommand::Create { .. }
        ));
        assert!(matches!(
            observed[1].command,
            ManagedRuntimeCommand::Activate
        ));
        assert!(matches!(
            observed[2].command,
            ManagedRuntimeCommand::SubmitInput { .. }
        ));
        drop(observed);

        let replayed = service
            .launch(request)
            .await
            .expect("activated binding replay converges");
        assert_eq!(replayed.binding, outcome.binding);
        assert_eq!(runtime.observed.lock().await.len(), 6);
    }
}
