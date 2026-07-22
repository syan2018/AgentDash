use std::sync::Arc;

use agentdash_agent_runtime_contract::{
    ManagedRuntimeContentBlock, ManagedRuntimeOperationReceipt,
};
use agentdash_agent_service_api::AgentInputContent;
use agentdash_domain::agent_input::{AgentInputOrigin, AgentInputSourceIdentity};
use agentdash_domain::agent_run_target::AgentRunTarget;
use agentdash_domain::workflow::LifecycleAgentRepository;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

use super::{
    AgentRunProductCommand, AgentRunProductCommandFacade, AgentRunProductCommandRequest,
    AgentRunProductProjectionQueryPort,
};

#[derive(Debug, Clone)]
pub struct DeliverAgentRunProductInput {
    pub target: AgentRunTarget,
    pub content: Vec<AgentInputContent>,
    pub source: AgentInputSourceIdentity,
    pub origin: AgentInputOrigin,
    pub client_command_id: String,
}

#[derive(Debug, Clone)]
pub struct AgentRunProductInputDelivery {
    /// Deterministic identity retained by the owning Product workflow as input-handoff evidence.
    pub handoff_id: Uuid,
    pub operation_receipt: ManagedRuntimeOperationReceipt,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreparedAgentRunProductInputDelivery {
    pub handoff_id: Uuid,
    pub command_request: AgentRunProductCommandRequest,
    pub steered: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AgentRunProductInputPreparation {
    Prepared(PreparedAgentRunProductInputDelivery),
}

#[derive(Debug, Error)]
pub enum AgentRunProductInputDeliveryError {
    #[error("Product input is empty")]
    EmptyInput,
    #[error("Product input client command id is invalid")]
    InvalidClientCommandId,
    #[error("Product input handoff failed: {0}")]
    Command(String),
    #[error("AgentRun title initialization failed: {0}")]
    TitleInitialization(String),
}

#[async_trait]
pub trait AgentRunProductInputDeliveryPort: Send + Sync {
    async fn prepare_delivery(
        &self,
        command: DeliverAgentRunProductInput,
    ) -> Result<AgentRunProductInputPreparation, AgentRunProductInputDeliveryError>;

    async fn dispatch_prepared(
        &self,
        prepared: PreparedAgentRunProductInputDelivery,
    ) -> Result<AgentRunProductInputDelivery, AgentRunProductInputDeliveryError>;

    async fn deliver(
        &self,
        command: DeliverAgentRunProductInput,
    ) -> Result<AgentRunProductInputDelivery, AgentRunProductInputDeliveryError> {
        let AgentRunProductInputPreparation::Prepared(prepared) =
            self.prepare_delivery(command).await?;
        self.dispatch_prepared(prepared).await
    }

    /// Records the deterministic Product handoff identity when Create/Fork already carried the
    /// same first input into the concrete Agent. No Product receipt ledger is written.
    async fn record_dispatched(
        &self,
        command: DeliverAgentRunProductInput,
    ) -> Result<Uuid, AgentRunProductInputDeliveryError>;
}

/// Synchronous Product input handoff.
///
/// Product validates and maps the request, then immediately hands it to the concrete Agent through
/// the command facade. Agent unavailability is returned to the caller; Product never accepts an
/// offline queue or creates a background-delivery promise.
pub struct AgentRunProductInputDeliveryService {
    commands: Arc<AgentRunProductCommandFacade>,
    projection: Arc<dyn AgentRunProductProjectionQueryPort>,
    agents: Arc<dyn LifecycleAgentRepository>,
}

impl AgentRunProductInputDeliveryService {
    pub fn new(
        commands: Arc<AgentRunProductCommandFacade>,
        projection: Arc<dyn AgentRunProductProjectionQueryPort>,
        agents: Arc<dyn LifecycleAgentRepository>,
    ) -> Self {
        Self {
            commands,
            projection,
            agents,
        }
    }

    async fn initialize_title(&self, target: &AgentRunTarget) -> Result<(), String> {
        let snapshot = self
            .projection
            .runtime_snapshot(target)
            .await
            .map_err(|error| error.to_string())?;
        if let Some(title) = snapshot.thread_name.as_deref() {
            self.agents
                .initialize_title_from_agent(target, title)
                .await
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }
}

#[async_trait]
impl AgentRunProductInputDeliveryPort for AgentRunProductInputDeliveryService {
    async fn prepare_delivery(
        &self,
        command: DeliverAgentRunProductInput,
    ) -> Result<AgentRunProductInputPreparation, AgentRunProductInputDeliveryError> {
        let client_command_id = validate_client_command_id(&command.client_command_id)?;
        let content = managed_content(&command.content)?;
        let handoff_id = stable_handoff_id(&command.target, client_command_id);
        Ok(AgentRunProductInputPreparation::Prepared(
            PreparedAgentRunProductInputDelivery {
                handoff_id,
                command_request: AgentRunProductCommandRequest {
                    target: command.target,
                    client_command_id: client_command_id.to_owned(),
                    command: AgentRunProductCommand::SubmitInput { content },
                },
                // The concrete Agent decides Submit versus Steer from its authoritative active turn.
                steered: false,
            },
        ))
    }

    async fn dispatch_prepared(
        &self,
        prepared: PreparedAgentRunProductInputDelivery,
    ) -> Result<AgentRunProductInputDelivery, AgentRunProductInputDeliveryError> {
        let target = prepared.command_request.target.clone();
        let receipt = self
            .commands
            .execute(prepared.command_request)
            .await
            .map_err(|error| AgentRunProductInputDeliveryError::Command(error.to_string()))?;
        self.initialize_title(&target)
            .await
            .map_err(AgentRunProductInputDeliveryError::TitleInitialization)?;
        Ok(AgentRunProductInputDelivery {
            handoff_id: prepared.handoff_id,
            operation_receipt: receipt,
        })
    }

    async fn record_dispatched(
        &self,
        command: DeliverAgentRunProductInput,
    ) -> Result<Uuid, AgentRunProductInputDeliveryError> {
        let client_command_id = validate_client_command_id(&command.client_command_id)?;
        managed_content(&command.content)?;
        Ok(stable_handoff_id(&command.target, client_command_id))
    }
}

fn validate_client_command_id(value: &str) -> Result<&str, AgentRunProductInputDeliveryError> {
    let value = value.trim();
    if value.is_empty() || value.len() > 256 {
        return Err(AgentRunProductInputDeliveryError::InvalidClientCommandId);
    }
    Ok(value)
}

fn stable_handoff_id(target: &AgentRunTarget, client_command_id: &str) -> Uuid {
    let digest = Sha256::digest(
        format!(
            "agentdash:product-input-handoff:v2:{}:{}:{client_command_id}",
            target.run_id, target.agent_id
        )
        .as_bytes(),
    );
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    Uuid::from_bytes(bytes)
}

fn managed_content(
    content: &[AgentInputContent],
) -> Result<Vec<ManagedRuntimeContentBlock>, AgentRunProductInputDeliveryError> {
    if content.is_empty() || !content.iter().any(non_empty_input) {
        return Err(AgentRunProductInputDeliveryError::EmptyInput);
    }
    Ok(content
        .iter()
        .map(|block| match block {
            AgentInputContent::Text { text } => {
                ManagedRuntimeContentBlock::Text { text: text.clone() }
            }
            AgentInputContent::Image {
                media_type,
                source,
                digest,
            } => ManagedRuntimeContentBlock::Image {
                media_type: media_type.clone(),
                source: source.clone(),
                digest: agentdash_agent_runtime_contract::RuntimePayloadDigest::new(
                    digest.as_str().to_owned(),
                )
                .expect("Agent input digest is already validated"),
            },
            AgentInputContent::Resource {
                uri,
                media_type,
                digest,
            } => ManagedRuntimeContentBlock::Resource {
                uri: uri.clone(),
                media_type: media_type.clone(),
                digest: digest.as_ref().map(|digest| {
                    agentdash_agent_runtime_contract::RuntimePayloadDigest::new(
                        digest.as_str().to_owned(),
                    )
                    .expect("Agent input digest is already validated")
                }),
            },
            AgentInputContent::Structured { schema, value } => {
                ManagedRuntimeContentBlock::Structured {
                    schema: schema.clone(),
                    value: value.clone(),
                }
            }
        })
        .collect())
}

fn non_empty_input(content: &AgentInputContent) -> bool {
    match content {
        AgentInputContent::Text { text } => !text.trim().is_empty(),
        AgentInputContent::Image { source, .. } => !source.trim().is_empty(),
        AgentInputContent::Resource { uri, .. } => !uri.trim().is_empty(),
        AgentInputContent::Structured { schema, .. } => !schema.trim().is_empty(),
    }
}
