use std::sync::Arc;

use agentdash_agent_runtime_contract::{
    ManagedRuntimeCommandAvailability, ManagedRuntimeCommandKind, ManagedRuntimeContentBlock,
    ManagedRuntimeOperationReceipt,
};
use agentdash_agent_service_api::AgentInputContent;
use agentdash_domain::agent_run_mailbox::{
    AgentRunMailboxCreateOutcome, AgentRunMailboxRepository, ConsumptionBarrier, MailboxDelivery,
    MailboxDrainMode, MailboxMessageOrigin, MailboxMessageStatus, MailboxSourceIdentity,
    NewAgentRunMailboxMessage,
};
use agentdash_domain::agent_run_target::AgentRunTarget;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

use super::{
    AgentRunProductCommand, AgentRunProductCommandFacade, AgentRunProductCommandRequest,
    AgentRunProductProjectionQueryPort, AgentRunProductRuntimeSnapshotObservation,
};

#[derive(Debug, Clone)]
pub struct DeliverAgentRunProductInput {
    pub target: AgentRunTarget,
    pub content: Vec<AgentInputContent>,
    pub source: MailboxSourceIdentity,
    pub origin: MailboxMessageOrigin,
    pub client_command_id: String,
}

#[derive(Debug, Clone)]
pub struct AgentRunProductInputDelivery {
    pub mailbox_message_id: Uuid,
    pub operation_receipt: Option<ManagedRuntimeOperationReceipt>,
    pub queued: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreparedAgentRunProductInputDelivery {
    pub mailbox_message_id: Uuid,
    pub command_request: AgentRunProductCommandRequest,
    pub steered: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AgentRunProductInputPreparation {
    Pending { mailbox_message_id: Uuid },
    Prepared(PreparedAgentRunProductInputDelivery),
}

#[derive(Debug, Error)]
pub enum AgentRunProductInputDeliveryError {
    #[error("Product input is empty")]
    EmptyInput,
    #[error("Product input client command id is invalid")]
    InvalidClientCommandId,
    #[error("Product mailbox persistence failed: {0}")]
    Mailbox(String),
    #[error("Product Runtime projection failed: {0}")]
    Projection(String),
    #[error("Product Runtime projection is stale")]
    StaleProjection,
    #[error("Product Runtime command failed: {0}")]
    Command(String),
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
        match self.prepare_delivery(command).await? {
            AgentRunProductInputPreparation::Pending { mailbox_message_id } => {
                Ok(AgentRunProductInputDelivery {
                    mailbox_message_id,
                    operation_receipt: None,
                    queued: true,
                })
            }
            AgentRunProductInputPreparation::Prepared(prepared) => {
                self.dispatch_prepared(prepared).await
            }
        }
    }

    /// 记录已经由同一 Product durable protocol 提交的输入，不再次调用 Runtime。
    async fn record_dispatched(
        &self,
        command: DeliverAgentRunProductInput,
    ) -> Result<Uuid, AgentRunProductInputDeliveryError>;
}

pub struct AgentRunProductInputDeliveryService {
    mailbox: Arc<dyn AgentRunMailboxRepository>,
    projection: Arc<dyn AgentRunProductProjectionQueryPort>,
    commands: Arc<AgentRunProductCommandFacade>,
}

impl AgentRunProductInputDeliveryService {
    pub fn new(
        mailbox: Arc<dyn AgentRunMailboxRepository>,
        projection: Arc<dyn AgentRunProductProjectionQueryPort>,
        commands: Arc<AgentRunProductCommandFacade>,
    ) -> Self {
        Self {
            mailbox,
            projection,
            commands,
        }
    }
}

#[async_trait]
impl AgentRunProductInputDeliveryPort for AgentRunProductInputDeliveryService {
    async fn prepare_delivery(
        &self,
        command: DeliverAgentRunProductInput,
    ) -> Result<AgentRunProductInputPreparation, AgentRunProductInputDeliveryError> {
        let client_command_id = command.client_command_id.trim().to_owned();
        let managed_content = managed_content(&command.content)?;
        let mailbox_message = self.persist_message(&command).await?;
        let observation = self
            .projection
            .runtime_snapshot_observation(&command.target)
            .await
            .map_err(|error| AgentRunProductInputDeliveryError::Projection(error.to_string()))?;
        let snapshot = match observation {
            AgentRunProductRuntimeSnapshotObservation::Absent { .. } => {
                return Ok(AgentRunProductInputPreparation::Pending {
                    mailbox_message_id: mailbox_message.id,
                });
            }
            AgentRunProductRuntimeSnapshotObservation::Stale(_) => {
                return Err(AgentRunProductInputDeliveryError::StaleProjection);
            }
            AgentRunProductRuntimeSnapshotObservation::Current { snapshot, .. } => snapshot,
        };
        let command_kind = if snapshot.active_turn_id.is_some() {
            ManagedRuntimeCommandKind::Steer
        } else {
            ManagedRuntimeCommandKind::SubmitInput
        };
        if !matches!(
            snapshot.command_availability.get(&command_kind),
            Some(ManagedRuntimeCommandAvailability::Available { .. })
        ) {
            return Ok(AgentRunProductInputPreparation::Pending {
                mailbox_message_id: mailbox_message.id,
            });
        }
        Ok(AgentRunProductInputPreparation::Prepared(
            PreparedAgentRunProductInputDelivery {
                mailbox_message_id: mailbox_message.id,
                command_request: AgentRunProductCommandRequest {
                    target: command.target,
                    client_command_id,
                    expected_revision: snapshot.revision,
                    command: AgentRunProductCommand::SubmitInput {
                        content: managed_content,
                    },
                },
                steered: snapshot.active_turn_id.is_some(),
            },
        ))
    }

    async fn dispatch_prepared(
        &self,
        prepared: PreparedAgentRunProductInputDelivery,
    ) -> Result<AgentRunProductInputDelivery, AgentRunProductInputDeliveryError> {
        let receipt = self
            .commands
            .execute(prepared.command_request)
            .await
            .map_err(|error| AgentRunProductInputDeliveryError::Command(error.to_string()))?;
        self.mailbox
            .mark_message_status(
                prepared.mailbox_message_id,
                None,
                if prepared.steered {
                    MailboxMessageStatus::Steered
                } else {
                    MailboxMessageStatus::Dispatched
                },
                None,
            )
            .await
            .map_err(|error| AgentRunProductInputDeliveryError::Mailbox(error.to_string()))?;
        Ok(AgentRunProductInputDelivery {
            mailbox_message_id: prepared.mailbox_message_id,
            operation_receipt: Some(receipt),
            queued: false,
        })
    }

    async fn deliver(
        &self,
        command: DeliverAgentRunProductInput,
    ) -> Result<AgentRunProductInputDelivery, AgentRunProductInputDeliveryError> {
        let client_command_id = command.client_command_id.trim();
        if client_command_id.is_empty() || client_command_id.len() > 256 {
            return Err(AgentRunProductInputDeliveryError::InvalidClientCommandId);
        }
        let managed_content = managed_content(&command.content)?;
        let preview = input_preview(&command.content)?;
        let has_images = command
            .content
            .iter()
            .any(|block| matches!(block, AgentInputContent::Image { .. }));
        let payload = serde_json::json!({
            "schema": "agentdash.product-input/v1",
            "source": {
                "namespace": &command.source.namespace,
                "kind": &command.source.kind,
                "source_ref": &command.source.source_ref,
                "correlation_ref": &command.source.correlation_ref,
                "actor": &command.source.actor,
                "route": &command.source.route,
                "display_label_key": &command.source.display_label_key,
                "metadata": &command.source.metadata,
            },
            "content": &command.content,
        });
        let request_digest = format!(
            "sha256:{:x}",
            Sha256::digest(
                serde_json::to_vec(&(
                    "agentdash.product-input-delivery/v1",
                    &command.target,
                    client_command_id,
                    &payload,
                ))
                .expect("Product input delivery is serializable")
            )
        );
        let mailbox_message = match self
            .mailbox
            .create_message_idempotent(NewAgentRunMailboxMessage {
                id: Some(stable_message_id(&command.target, client_command_id)),
                run_id: command.target.run_id,
                agent_id: command.target.agent_id,
                origin: command.origin,
                source: command.source,
                delivery: MailboxDelivery::LaunchOrContinueTurn,
                barrier: ConsumptionBarrier::ImmediateIfIdle,
                drain_mode: MailboxDrainMode::One,
                priority: 0,
                source_dedup_key: Some(client_command_id.to_string()),
                delivery_request_digest: request_digest,
                payload_json: Some(payload),
                launch_planning_input: None,
                preview,
                has_images,
                retain_payload: true,
            })
            .await
            .map_err(|error| AgentRunProductInputDeliveryError::Mailbox(error.to_string()))?
        {
            AgentRunMailboxCreateOutcome::Created(message)
            | AgentRunMailboxCreateOutcome::Existing(message) => message,
        };

        let observation = self
            .projection
            .runtime_snapshot_observation(&command.target)
            .await
            .map_err(|error| AgentRunProductInputDeliveryError::Projection(error.to_string()))?;
        let snapshot = match observation {
            AgentRunProductRuntimeSnapshotObservation::Absent { .. } => {
                return Ok(AgentRunProductInputDelivery {
                    mailbox_message_id: mailbox_message.id,
                    operation_receipt: None,
                    queued: true,
                });
            }
            AgentRunProductRuntimeSnapshotObservation::Stale(_) => {
                return Err(AgentRunProductInputDeliveryError::StaleProjection);
            }
            AgentRunProductRuntimeSnapshotObservation::Current { snapshot, .. } => snapshot,
        };
        let command_kind = if snapshot.active_turn_id.is_some() {
            ManagedRuntimeCommandKind::Steer
        } else {
            ManagedRuntimeCommandKind::SubmitInput
        };
        if !matches!(
            snapshot.command_availability.get(&command_kind),
            Some(ManagedRuntimeCommandAvailability::Available { .. })
        ) {
            return Ok(AgentRunProductInputDelivery {
                mailbox_message_id: mailbox_message.id,
                operation_receipt: None,
                queued: true,
            });
        }
        let steered = snapshot.active_turn_id.is_some();
        let receipt = self
            .commands
            .execute(AgentRunProductCommandRequest {
                target: command.target,
                client_command_id: client_command_id.to_string(),
                expected_revision: snapshot.revision,
                command: AgentRunProductCommand::SubmitInput {
                    content: managed_content,
                },
            })
            .await
            .map_err(|error| AgentRunProductInputDeliveryError::Command(error.to_string()))?;
        self.mailbox
            .mark_message_status(
                mailbox_message.id,
                None,
                if steered {
                    MailboxMessageStatus::Steered
                } else {
                    MailboxMessageStatus::Dispatched
                },
                None,
            )
            .await
            .map_err(|error| AgentRunProductInputDeliveryError::Mailbox(error.to_string()))?;
        Ok(AgentRunProductInputDelivery {
            mailbox_message_id: mailbox_message.id,
            operation_receipt: Some(receipt),
            queued: false,
        })
    }

    async fn record_dispatched(
        &self,
        command: DeliverAgentRunProductInput,
    ) -> Result<Uuid, AgentRunProductInputDeliveryError> {
        let mailbox_message = self.persist_message(&command).await?;
        self.mailbox
            .mark_message_status(
                mailbox_message.id,
                None,
                MailboxMessageStatus::Dispatched,
                None,
            )
            .await
            .map_err(|error| AgentRunProductInputDeliveryError::Mailbox(error.to_string()))?;
        Ok(mailbox_message.id)
    }
}

impl AgentRunProductInputDeliveryService {
    async fn persist_message(
        &self,
        command: &DeliverAgentRunProductInput,
    ) -> Result<
        agentdash_domain::agent_run_mailbox::AgentRunMailboxMessage,
        AgentRunProductInputDeliveryError,
    > {
        let client_command_id = command.client_command_id.trim();
        if client_command_id.is_empty() || client_command_id.len() > 256 {
            return Err(AgentRunProductInputDeliveryError::InvalidClientCommandId);
        }
        let preview = input_preview(&command.content)?;
        let has_images = command
            .content
            .iter()
            .any(|block| matches!(block, AgentInputContent::Image { .. }));
        let payload = serde_json::json!({
            "schema": "agentdash.product-input/v1",
            "source": {
                "namespace": &command.source.namespace,
                "kind": &command.source.kind,
                "source_ref": &command.source.source_ref,
                "correlation_ref": &command.source.correlation_ref,
                "actor": &command.source.actor,
                "route": &command.source.route,
                "display_label_key": &command.source.display_label_key,
                "metadata": &command.source.metadata,
            },
            "content": &command.content,
        });
        let request_digest = format!(
            "sha256:{:x}",
            Sha256::digest(
                serde_json::to_vec(&(
                    "agentdash.product-input-delivery/v1",
                    &command.target,
                    client_command_id,
                    &payload,
                ))
                .expect("Product input delivery is serializable")
            )
        );
        match self
            .mailbox
            .create_message_idempotent(NewAgentRunMailboxMessage {
                id: Some(stable_message_id(&command.target, client_command_id)),
                run_id: command.target.run_id,
                agent_id: command.target.agent_id,
                origin: command.origin,
                source: command.source.clone(),
                delivery: MailboxDelivery::LaunchOrContinueTurn,
                barrier: ConsumptionBarrier::ImmediateIfIdle,
                drain_mode: MailboxDrainMode::One,
                priority: 0,
                source_dedup_key: Some(client_command_id.to_string()),
                delivery_request_digest: request_digest,
                payload_json: Some(payload),
                launch_planning_input: None,
                preview,
                has_images,
                retain_payload: true,
            })
            .await
            .map_err(|error| AgentRunProductInputDeliveryError::Mailbox(error.to_string()))?
        {
            AgentRunMailboxCreateOutcome::Created(message)
            | AgentRunMailboxCreateOutcome::Existing(message) => Ok(message),
        }
    }
}

fn stable_message_id(target: &AgentRunTarget, client_command_id: &str) -> Uuid {
    let digest = Sha256::digest(
        format!(
            "agentdash:product-input:v1:{}:{}:{client_command_id}",
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
    if content.is_empty() {
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

fn input_preview(
    content: &[AgentInputContent],
) -> Result<String, AgentRunProductInputDeliveryError> {
    let preview = content
        .iter()
        .map(|block| match block {
            AgentInputContent::Text { text } => text.as_str(),
            AgentInputContent::Image { source, .. } => source.as_str(),
            AgentInputContent::Resource { uri, .. } => uri.as_str(),
            AgentInputContent::Structured { schema, .. } => schema.as_str(),
        })
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    if preview.is_empty() {
        return Err(AgentRunProductInputDeliveryError::EmptyInput);
    }
    Ok(preview)
}
