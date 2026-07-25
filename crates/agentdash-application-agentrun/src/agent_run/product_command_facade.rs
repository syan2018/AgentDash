use std::sync::Arc;

use agentdash_agent_runtime_contract::{
    ManagedRuntimeContentBlock, ManagedRuntimeInteractionResponse, ManagedRuntimeOperationReceipt,
    ManagedRuntimeOperationStatus, RuntimeInteractionId, RuntimeOperationId,
};
use agentdash_agent_service_api::{
    AgentAppliedEffectOutcome, AgentCommand, AgentCommandEnvelope, AgentCommandId,
    AgentCommandMeta, AgentCommandReceipt, AgentEffectIdentity, AgentEffectInspectionState,
    AgentIdempotencyKey, AgentInput, AgentInputContent, AgentInteractionId,
    AgentInteractionResponse, AgentPayloadDigest, AgentReadQuery, AgentReceiptState,
    AgentServiceError, AgentTerminalOutcome, ResumeAgentCommand,
};
use agentdash_domain::agent_run_target::AgentRunTarget;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use super::{AgentRunCompleteAgentResolverPort, AgentRunProductRuntimeBindingRepository};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AgentRunProductCommand {
    Resume,
    SubmitInput {
        content: Vec<ManagedRuntimeContentBlock>,
    },
    Interrupt,
    RequestCompaction,
    Rebind,
    ResolveInteraction {
        interaction_id: RuntimeInteractionId,
        response: ManagedRuntimeInteractionResponse,
    },
    Close,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentRunProductCommandRequest {
    pub target: AgentRunTarget,
    pub client_command_id: String,
    pub command: AgentRunProductCommand,
}

#[derive(Debug, Error)]
pub enum AgentRunProductCommandError {
    #[error("AgentRun Product binding is missing")]
    TargetNotBound,
    #[error("AgentRun Product binding repository failed: {0}")]
    Binding(String),
    #[error("AgentRun Product binding does not match the requested target")]
    TargetMismatch,
    #[error("client command id is invalid")]
    InvalidClientCommandId,
    #[error("Agent command is invalid: {0}")]
    InvalidCommand(String),
    #[error("Agent has no active turn for this command")]
    ActiveTurnMissing,
    #[error("Complete Agent is unavailable: {0}")]
    Unavailable(String),
    #[error("Agent effect inspection is inconclusive; retry with the same client command id")]
    InspectionPending,
    #[error(transparent)]
    Agent(#[from] AgentServiceError),
}

/// Synchronous Product-to-Agent handoff.
///
/// Product owns only the target association and the caller's stable client identity. The concrete
/// Agent owns command admission, effect idempotency and the resulting execution history. No
/// Product claim, mailbox delivery or Runtime operation state is persisted.
pub struct AgentRunProductCommandFacade {
    bindings: Arc<dyn AgentRunProductRuntimeBindingRepository>,
    agents: Arc<dyn AgentRunCompleteAgentResolverPort>,
}

impl AgentRunProductCommandFacade {
    pub fn new(
        bindings: Arc<dyn AgentRunProductRuntimeBindingRepository>,
        agents: Arc<dyn AgentRunCompleteAgentResolverPort>,
    ) -> Self {
        Self { bindings, agents }
    }

    pub async fn execute(
        &self,
        request: AgentRunProductCommandRequest,
    ) -> Result<ManagedRuntimeOperationReceipt, AgentRunProductCommandError> {
        let client_command_id = validate_client_command_id(&request.client_command_id)?;
        let binding = self
            .bindings
            .load_product_binding(&request.target)
            .await
            .map_err(AgentRunProductCommandError::Binding)?
            .ok_or(AgentRunProductCommandError::TargetNotBound)?;
        if binding.target != request.target {
            return Err(AgentRunProductCommandError::TargetMismatch);
        }

        let resolved = self
            .agents
            .resolve(&binding)
            .await
            .map_err(AgentRunProductCommandError::Unavailable)?;
        let service = resolved.service;
        let generation = resolved.binding_generation;
        if matches!(request.command, AgentRunProductCommand::Rebind) {
            return Ok(operation_receipt(
                stable_product_command_operation_id(&request.target, client_command_id)?,
                binding.runtime_thread_id,
                ManagedRuntimeOperationStatus::Succeeded,
                false,
            ));
        }
        let snapshot = service
            .read(AgentReadQuery {
                source: binding.agent.source.clone(),
                at_revision: None,
            })
            .await?;
        if snapshot.source != binding.agent.source {
            return Err(AgentRunProductCommandError::InvalidCommand(
                "Agent read returned a different source".to_owned(),
            ));
        }

        let identity = product_command_identity(&request.target, client_command_id);
        let operation_id = stable_product_command_operation_id(&request.target, client_command_id)?;
        let meta = AgentCommandMeta {
            command_id: AgentCommandId::new(format!("product-command:v2:{identity}"))
                .map_err(|error| AgentRunProductCommandError::InvalidCommand(error.to_string()))?,
            effect_id: AgentEffectIdentity::new(format!("product-effect:v2:{identity}"))
                .map_err(|error| AgentRunProductCommandError::InvalidCommand(error.to_string()))?,
            idempotency_key: AgentIdempotencyKey::new(format!(
                "product-command-idempotency:v2:{identity}"
            ))
            .map_err(|error| AgentRunProductCommandError::InvalidCommand(error.to_string()))?,
            binding_generation: generation,
            expected_snapshot_revision: None,
        };
        let inspection = service.inspect(meta.effect_id.clone()).await?;
        if !inspection.validate() || inspection.effect_id != meta.effect_id {
            return Err(AgentRunProductCommandError::InvalidCommand(
                "Agent returned invalid effect inspection evidence".to_owned(),
            ));
        }

        match inspection.state {
            AgentEffectInspectionState::Unknown => {
                Err(AgentRunProductCommandError::InspectionPending)
            }
            AgentEffectInspectionState::Accepted { source } => {
                if source != binding.agent.source {
                    return Err(AgentRunProductCommandError::InvalidCommand(
                        "accepted effect belongs to another source".to_owned(),
                    ));
                }
                Ok(operation_receipt(
                    operation_id,
                    binding.runtime_thread_id,
                    ManagedRuntimeOperationStatus::Accepted,
                    true,
                ))
            }
            AgentEffectInspectionState::Applied { outcome } => {
                let receipt = applied_product_command_receipt(&request.command, outcome)?;
                if receipt.source != binding.agent.source {
                    return Err(AgentRunProductCommandError::InvalidCommand(
                        "applied effect belongs to another source".to_owned(),
                    ));
                }
                Ok(operation_receipt(
                    operation_id,
                    binding.runtime_thread_id,
                    receipt_status(&receipt),
                    true,
                ))
            }
            AgentEffectInspectionState::NotApplied => {
                let receipt = match request.command {
                    AgentRunProductCommand::Resume => {
                        service
                            .resume(ResumeAgentCommand {
                                meta,
                                source: binding.agent.source.clone(),
                            })
                            .await?
                    }
                    command => {
                        let active_turn_id = snapshot
                            .active_turn_id()
                            .map(|turn_id| {
                                agentdash_agent_service_api::AgentTurnId::new(turn_id.to_owned())
                                    .map_err(|error| {
                                        AgentRunProductCommandError::InvalidCommand(
                                            error.to_string(),
                                        )
                                    })
                            })
                            .transpose()?;
                        service
                            .execute(AgentCommandEnvelope {
                                meta,
                                source: binding.agent.source.clone(),
                                command: map_command(command, active_turn_id.as_ref())?,
                            })
                            .await?
                    }
                };
                if receipt.source != binding.agent.source {
                    return Err(AgentRunProductCommandError::InvalidCommand(
                        "Agent receipt belongs to another source".to_owned(),
                    ));
                }
                let duplicate = matches!(receipt.state, AgentReceiptState::AlreadyApplied { .. });
                Ok(operation_receipt(
                    operation_id,
                    binding.runtime_thread_id,
                    receipt_status(&receipt),
                    duplicate,
                ))
            }
        }
    }

    /// Recovery is the same synchronous handoff with the same stable identity. The concrete Agent
    /// performs the only durable replay/inspection.
    pub async fn replay_claimed(
        &self,
        target: &AgentRunTarget,
        client_command_id: &str,
        command: &AgentRunProductCommand,
    ) -> Result<Option<ManagedRuntimeOperationReceipt>, AgentRunProductCommandError> {
        self.execute(AgentRunProductCommandRequest {
            target: target.clone(),
            client_command_id: client_command_id.to_owned(),
            command: command.clone(),
        })
        .await
        .map(Some)
    }
}

fn applied_product_command_receipt(
    command: &AgentRunProductCommand,
    outcome: AgentAppliedEffectOutcome,
) -> Result<AgentCommandReceipt, AgentRunProductCommandError> {
    let receipt = match (command, outcome) {
        (AgentRunProductCommand::Resume, AgentAppliedEffectOutcome::Resume { receipt })
        | (
            AgentRunProductCommand::SubmitInput { .. }
            | AgentRunProductCommand::Interrupt
            | AgentRunProductCommand::RequestCompaction
            | AgentRunProductCommand::ResolveInteraction { .. }
            | AgentRunProductCommand::Close,
            AgentAppliedEffectOutcome::Command { receipt },
        ) => receipt,
        _ => {
            return Err(AgentRunProductCommandError::InvalidCommand(
                "applied effect kind does not match the Product command".to_owned(),
            ));
        }
    };
    Ok(AgentCommandReceipt {
        command_id: receipt.command_id,
        effect_id: receipt.effect_id,
        source: receipt.source,
        state: AgentReceiptState::AlreadyApplied {
            terminal: receipt.terminal,
        },
        snapshot_revision: receipt.snapshot_revision,
        initial_context: receipt.initial_context,
    })
}

fn validate_client_command_id(value: &str) -> Result<&str, AgentRunProductCommandError> {
    let value = value.trim();
    if value.is_empty() || value.len() > 256 {
        return Err(AgentRunProductCommandError::InvalidClientCommandId);
    }
    Ok(value)
}

fn map_command(
    command: AgentRunProductCommand,
    active_turn_id: Option<&agentdash_agent_service_api::AgentTurnId>,
) -> Result<AgentCommand, AgentRunProductCommandError> {
    Ok(match command {
        AgentRunProductCommand::SubmitInput { content } => {
            let input = AgentInput {
                content: map_input(content)?,
            };
            active_turn_id.map_or(
                AgentCommand::SubmitInput {
                    input: input.clone(),
                },
                |expected_turn_id| AgentCommand::Steer {
                    expected_turn_id: expected_turn_id.clone(),
                    input,
                },
            )
        }
        AgentRunProductCommand::Interrupt => AgentCommand::Interrupt {
            expected_turn_id: active_turn_id
                .cloned()
                .ok_or(AgentRunProductCommandError::ActiveTurnMissing)?,
        },
        AgentRunProductCommand::RequestCompaction => AgentCommand::RequestCompaction,
        AgentRunProductCommand::ResolveInteraction {
            interaction_id,
            response,
        } => AgentCommand::ResolveInteraction {
            interaction_id: source_interaction_id(interaction_id)?,
            response: map_interaction_response(response)?,
        },
        AgentRunProductCommand::Close => AgentCommand::Close,
        AgentRunProductCommand::Resume => {
            return Err(AgentRunProductCommandError::InvalidCommand(
                "Resume uses the Complete Agent lifecycle command".to_owned(),
            ));
        }
        AgentRunProductCommand::Rebind => {
            return Err(AgentRunProductCommandError::InvalidCommand(
                "surface rebind uses the Host live surface workflow".to_owned(),
            ));
        }
    })
}

fn source_interaction_id(
    interaction_id: RuntimeInteractionId,
) -> Result<AgentInteractionId, AgentRunProductCommandError> {
    let source = interaction_id
        .as_str()
        .strip_prefix("agent-interaction:")
        .ok_or_else(|| {
            AgentRunProductCommandError::InvalidCommand(
                "interaction id is not an Agent-native presentation identity".to_owned(),
            )
        })?;
    AgentInteractionId::new(source)
        .map_err(|error| AgentRunProductCommandError::InvalidCommand(error.to_string()))
}

fn map_interaction_response(
    response: ManagedRuntimeInteractionResponse,
) -> Result<AgentInteractionResponse, AgentRunProductCommandError> {
    Ok(match response {
        ManagedRuntimeInteractionResponse::Approved => AgentInteractionResponse::Approved,
        ManagedRuntimeInteractionResponse::Denied { reason } => {
            AgentInteractionResponse::Denied { reason }
        }
        ManagedRuntimeInteractionResponse::UserInput { content } => {
            AgentInteractionResponse::UserInput {
                input: AgentInput {
                    content: map_input(content)?,
                },
            }
        }
        ManagedRuntimeInteractionResponse::Structured { value, .. } => {
            AgentInteractionResponse::McpElicitation { response: value }
        }
    })
}

fn map_input(
    content: Vec<ManagedRuntimeContentBlock>,
) -> Result<Vec<AgentInputContent>, AgentRunProductCommandError> {
    content
        .into_iter()
        .map(|block| {
            Ok(match block {
                ManagedRuntimeContentBlock::Text { text } => AgentInputContent::Text { text },
                ManagedRuntimeContentBlock::Image {
                    media_type,
                    source,
                    digest,
                } => AgentInputContent::Image {
                    media_type,
                    source,
                    digest: AgentPayloadDigest::new(digest.into_inner()).map_err(|error| {
                        AgentRunProductCommandError::InvalidCommand(error.to_string())
                    })?,
                },
                ManagedRuntimeContentBlock::Resource {
                    uri,
                    media_type,
                    digest,
                } => AgentInputContent::Resource {
                    uri,
                    media_type,
                    digest: digest
                        .map(|digest| AgentPayloadDigest::new(digest.into_inner()))
                        .transpose()
                        .map_err(|error| {
                            AgentRunProductCommandError::InvalidCommand(error.to_string())
                        })?,
                },
                ManagedRuntimeContentBlock::Structured { schema, value } => {
                    AgentInputContent::Structured { schema, value }
                }
            })
        })
        .collect()
}

fn receipt_status(receipt: &AgentCommandReceipt) -> ManagedRuntimeOperationStatus {
    match &receipt.state {
        AgentReceiptState::Accepted => ManagedRuntimeOperationStatus::Accepted,
        AgentReceiptState::Rejected { .. } => ManagedRuntimeOperationStatus::Failed,
        AgentReceiptState::AlreadyApplied { terminal } => terminal
            .map(terminal_status)
            .unwrap_or(ManagedRuntimeOperationStatus::Succeeded),
        AgentReceiptState::Terminal { outcome } => terminal_status(*outcome),
        AgentReceiptState::Unknown => ManagedRuntimeOperationStatus::Lost,
    }
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

fn operation_receipt(
    operation_id: RuntimeOperationId,
    thread_id: agentdash_agent_runtime_contract::RuntimeThreadId,
    status: ManagedRuntimeOperationStatus,
    duplicate: bool,
) -> ManagedRuntimeOperationReceipt {
    ManagedRuntimeOperationReceipt {
        operation_id,
        thread_id,
        status,
        evidence: None,
        duplicate,
    }
}

pub fn stable_product_command_operation_id(
    target: &AgentRunTarget,
    client_command_id: &str,
) -> Result<RuntimeOperationId, AgentRunProductCommandError> {
    let client_command_id = validate_client_command_id(client_command_id)?;
    RuntimeOperationId::new(format!(
        "product-command:v2:{}",
        product_command_identity(target, client_command_id)
    ))
    .map_err(|_| AgentRunProductCommandError::InvalidClientCommandId)
}

fn product_command_identity(target: &AgentRunTarget, client_command_id: &str) -> String {
    format!(
        "{:x}",
        Sha256::digest(
            serde_json::to_vec(&(
                "agentdash.product-command-identity/v2",
                target.run_id,
                target.agent_id,
                client_command_id,
            ))
            .expect("Product command identity is serializable"),
        )
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn operation_identity_depends_only_on_product_target_and_client_identity() {
        let target = AgentRunTarget {
            run_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
        };
        let first = stable_product_command_operation_id(&target, "client-1").unwrap();
        let replay = stable_product_command_operation_id(&target, "client-1").unwrap();
        let other = stable_product_command_operation_id(&target, "client-2").unwrap();

        assert_eq!(first, replay);
        assert_ne!(first, other);
    }

    #[test]
    fn presentation_interaction_identity_maps_back_to_native_coordinate() {
        let source = source_interaction_id(
            RuntimeInteractionId::new("agent-interaction:approval-1").unwrap(),
        )
        .unwrap();
        assert_eq!(source.as_str(), "approval-1");
    }

    #[test]
    fn applied_agent_outcome_is_recovered_without_redispatch() {
        let command_id = AgentCommandId::new("command-1").unwrap();
        let effect_id = AgentEffectIdentity::new("effect-1").unwrap();
        let source = agentdash_agent_service_api::AgentSourceCoordinate::new("source-1").unwrap();
        let receipt = applied_product_command_receipt(
            &AgentRunProductCommand::Close,
            AgentAppliedEffectOutcome::Command {
                receipt: agentdash_agent_service_api::AppliedAgentCommandReceipt {
                    command_id: command_id.clone(),
                    effect_id: effect_id.clone(),
                    source: source.clone(),
                    terminal: Some(AgentTerminalOutcome::Closed),
                    snapshot_revision: Some(agentdash_agent_service_api::AgentSnapshotRevision(42)),
                    initial_context: None,
                },
            },
        )
        .unwrap();

        assert_eq!(receipt.command_id, command_id);
        assert_eq!(receipt.effect_id, effect_id);
        assert_eq!(receipt.source, source);
        assert_eq!(
            receipt.state,
            AgentReceiptState::AlreadyApplied {
                terminal: Some(AgentTerminalOutcome::Closed)
            }
        );
        assert_eq!(
            receipt.snapshot_revision,
            Some(agentdash_agent_service_api::AgentSnapshotRevision(42))
        );
    }

    #[test]
    fn applied_agent_outcome_kind_must_match_product_command() {
        let result = applied_product_command_receipt(
            &AgentRunProductCommand::Resume,
            AgentAppliedEffectOutcome::Command {
                receipt: agentdash_agent_service_api::AppliedAgentCommandReceipt {
                    command_id: AgentCommandId::new("command-1").unwrap(),
                    effect_id: AgentEffectIdentity::new("effect-1").unwrap(),
                    source: agentdash_agent_service_api::AgentSourceCoordinate::new("source-1")
                        .unwrap(),
                    terminal: None,
                    snapshot_revision: None,
                    initial_context: None,
                },
            },
        );

        assert!(matches!(
            result,
            Err(AgentRunProductCommandError::InvalidCommand(_))
        ));
    }
}
