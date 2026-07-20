use std::sync::Arc;

use agentdash_agent_runtime_contract::{
    ManagedRuntimeContentBlock, ManagedRuntimeInitialContextPackage,
    ManagedRuntimeOperationReceipt, ManagedRuntimeOperationStatus, RuntimeOperationId,
    RuntimeProjectionRevision,
};
use agentdash_agent_service_api::{AgentCommandReceipt, AgentReceiptState, AgentTerminalOutcome};
use async_trait::async_trait;
use sha2::{Digest, Sha256};

use super::{
    AgentRunProductCommand, AgentRunProductCommandFacade, AgentRunProductCommandRequest,
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
    pub input_receipt: Option<ManagedRuntimeOperationReceipt>,
}

#[derive(Debug, thiserror::Error)]
pub enum AgentRunProductLaunchError {
    #[error(transparent)]
    Provisioning(#[from] super::AgentRunProductRuntimeProvisioningError),
    #[error("Product launch request is invalid: {0}")]
    Invalid(String),
    #[error("Product Agent command failed: {0}")]
    Command(String),
    #[error("Product Agent association persistence failed: {0}")]
    Binding(String),
    #[error("Product Agent launch evidence does not match the requested target")]
    AssociationMismatch,
}

/// Product-owned fresh Agent launch.
///
/// Product selects and prepares the current live attachment, hands one stable Create effect to the
/// concrete Agent, persists the returned source association, then optionally hands off the first
/// input synchronously. Runtime/Host projections and an artificial Activate phase are not part of
/// the durable workflow.
pub struct AgentRunProductLaunchService {
    provisioning: Arc<dyn AgentRunProductRuntimeProvisioningPort>,
    bindings: Arc<dyn AgentRunProductRuntimeBindingStore>,
    commands: Arc<AgentRunProductCommandFacade>,
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
        bindings: Arc<dyn AgentRunProductRuntimeBindingStore>,
        commands: Arc<AgentRunProductCommandFacade>,
    ) -> Self {
        Self {
            provisioning,
            bindings,
            commands,
        }
    }

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
            return Err(AgentRunProductLaunchError::AssociationMismatch);
        }
        Ok(())
    }

    /// Commits source association evidence produced by an external direct Agent lifecycle step.
    pub async fn converge_created_runtime(
        &self,
        request: &AgentRunProductRuntimeProvisioningRequest,
    ) -> Result<AgentRunProductRuntimeBinding, AgentRunProductLaunchError> {
        let binding = AgentRunProductRuntimeBinding {
            target: request.target.clone(),
            runtime_thread_id: request.runtime_thread_id.clone(),
            agent: self
                .provisioning
                .created_agent_association(&request.runtime_thread_id)
                .await?,
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

    /// Activation is not a Product or Runtime fact. Legacy workflow phases converge by proving the
    /// stable Product association already exists.
    pub async fn converge_activated_runtime(
        &self,
        request: &AgentRunProductRuntimeProvisioningRequest,
    ) -> Result<AgentRunProductRuntimeBinding, AgentRunProductLaunchError> {
        let binding = self
            .bindings
            .load_product_binding(&request.target)
            .await
            .map_err(AgentRunProductLaunchError::Binding)?
            .ok_or(AgentRunProductLaunchError::AssociationMismatch)?;
        if binding.runtime_thread_id != request.runtime_thread_id
            || binding.launch_frame != request.frame
        {
            return Err(AgentRunProductLaunchError::AssociationMismatch);
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
                "initial context package digest is invalid".to_owned(),
            ));
        }
        self.prepare_runtime_target(&request.provisioning).await?;
        let created = self
            .provisioning
            .create_agent_source(&request.provisioning, request.initial_context)
            .await?;
        if created.receipt.source != created.association.source {
            return Err(AgentRunProductLaunchError::AssociationMismatch);
        }
        let binding = AgentRunProductRuntimeBinding {
            target: request.provisioning.target.clone(),
            runtime_thread_id: request.provisioning.runtime_thread_id.clone(),
            agent: created.association,
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

        let input_receipt = if request.initial_input.is_empty() {
            None
        } else {
            Some(
                self.commands
                    .execute(AgentRunProductCommandRequest {
                        target: binding.target.clone(),
                        client_command_id: initial_input_identity(&request.provisioning),
                        command: AgentRunProductCommand::SubmitInput {
                            content: request.initial_input,
                        },
                    })
                    .await
                    .map_err(|error| AgentRunProductLaunchError::Command(error.to_string()))?,
            )
        };

        Ok(AgentRunProductLaunchOutcome {
            create_receipt: managed_create_receipt(&binding.runtime_thread_id, &created.receipt)?,
            binding,
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

fn initial_input_identity(request: &AgentRunProductRuntimeProvisioningRequest) -> String {
    format!(
        "launch-input:v2:{:x}",
        Sha256::digest(
            serde_json::to_vec(&(
                &request.target,
                &request.runtime_thread_id,
                &request.idempotency_key,
            ))
            .expect("Product launch identity is serializable")
        )
    )
}

fn managed_create_receipt(
    thread_id: &agentdash_agent_runtime_contract::RuntimeThreadId,
    receipt: &AgentCommandReceipt,
) -> Result<ManagedRuntimeOperationReceipt, AgentRunProductLaunchError> {
    let status = match receipt.state {
        AgentReceiptState::Accepted => ManagedRuntimeOperationStatus::Accepted,
        AgentReceiptState::Rejected { .. } => ManagedRuntimeOperationStatus::Failed,
        AgentReceiptState::AlreadyApplied { terminal } => terminal
            .map(terminal_status)
            .unwrap_or(ManagedRuntimeOperationStatus::Succeeded),
        AgentReceiptState::Terminal { outcome } => terminal_status(outcome),
        AgentReceiptState::Unknown => ManagedRuntimeOperationStatus::Lost,
    };
    let operation_id = RuntimeOperationId::new(format!(
        "product-create:v2:{:x}",
        Sha256::digest(receipt.effect_id.as_str().as_bytes())
    ))
    .map_err(|error| AgentRunProductLaunchError::Invalid(error.to_string()))?;
    Ok(ManagedRuntimeOperationReceipt {
        operation_id,
        thread_id: thread_id.clone(),
        accepted_revision: RuntimeProjectionRevision(
            receipt.snapshot_revision.map_or(0, |revision| revision.0),
        ),
        status,
        evidence: None,
        duplicate: matches!(receipt.state, AgentReceiptState::AlreadyApplied { .. }),
    })
}

fn terminal_status(outcome: AgentTerminalOutcome) -> ManagedRuntimeOperationStatus {
    match outcome {
        AgentTerminalOutcome::Succeeded | AgentTerminalOutcome::Closed => {
            ManagedRuntimeOperationStatus::Succeeded
        }
        AgentTerminalOutcome::Failed => ManagedRuntimeOperationStatus::Failed,
        AgentTerminalOutcome::Interrupted => ManagedRuntimeOperationStatus::Interrupted,
        AgentTerminalOutcome::Lost => ManagedRuntimeOperationStatus::Lost,
    }
}
