use std::sync::Arc;

use agentdash_agent_runtime_contract::{
    AgentAppliedEffectOutcome, AgentCommand, AgentCommandEnvelope, AgentCommandId,
    AgentCommandMeta, AgentCommandReceipt, AgentControlAvailability, AgentControlKind,
    AgentEffectIdentity, AgentEffectInspectionState, AgentIdempotencyKey, AgentInput,
    AgentInputContent, AgentInteractionId, AgentInteractionResponse, AgentReadQuery,
    AgentReceiptState, AgentRuntimeOperationReceipt, AgentRuntimeOperationStatus,
    AgentServiceError, AgentSnapshot, AgentTerminalOutcome, AgentTurnId, ResumeAgentCommand,
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
        content: Vec<AgentInputContent>,
    },
    Steer {
        content: Vec<AgentInputContent>,
        expected_turn_id: AgentTurnId,
    },
    Interrupt,
    RequestCompaction,
    ResolveInteraction {
        interaction_id: AgentInteractionId,
        response: AgentInteractionResponse,
    },
    Close,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentRunProductCommandRequest {
    pub target: AgentRunTarget,
    pub client_command_id: String,
    pub command: AgentRunProductCommand,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunProductCommandEvidence {
    pub runtime_thread_id: agentdash_agent_runtime_contract::RuntimeThreadId,
    pub source: agentdash_agent_runtime_contract::AgentSourceCoordinate,
    pub binding_generation: agentdash_agent_runtime_contract::AgentBindingGeneration,
    pub snapshot_revision: agentdash_agent_runtime_contract::AgentSnapshotRevision,
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

/// Executes one explicit concrete-Agent command chosen by the AgentRun Product.
///
/// AgentRun Mailbox owns durable input intake, queueing and delivery recovery. This facade owns
/// only binding resolution plus stable concrete effect execution/inspection; the concrete Agent
/// remains authoritative for command admission and execution history.
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
    ) -> Result<AgentRuntimeOperationReceipt, AgentRunProductCommandError> {
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
            expected_snapshot_revision: Some(snapshot.observation.revision),
        };
        let operation_id = meta.effect_id.clone();
        let expected_effect_id = meta.effect_id.clone();
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
                    AgentRuntimeOperationStatus::Accepted,
                    true,
                ))
            }
            AgentEffectInspectionState::Applied { outcome } => {
                let receipt = applied_product_command_receipt(&request.command, outcome)?;
                if receipt.effect_id != expected_effect_id {
                    return Err(AgentRunProductCommandError::InvalidCommand(
                        "applied effect receipt has a different identity".to_owned(),
                    ));
                }
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
                        service
                            .execute(AgentCommandEnvelope {
                                meta,
                                source: binding.agent.source.clone(),
                                command: map_command(command, &snapshot)?,
                            })
                            .await?
                    }
                };
                if receipt.effect_id != expected_effect_id {
                    return Err(AgentRunProductCommandError::InvalidCommand(
                        "Agent receipt has a different effect identity".to_owned(),
                    ));
                }
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

    pub async fn delivery_evidence(
        &self,
        target: &AgentRunTarget,
    ) -> Result<AgentRunProductCommandEvidence, AgentRunProductCommandError> {
        let binding = self
            .bindings
            .load_product_binding(target)
            .await
            .map_err(AgentRunProductCommandError::Binding)?
            .ok_or(AgentRunProductCommandError::TargetNotBound)?;
        if &binding.target != target {
            return Err(AgentRunProductCommandError::TargetMismatch);
        }
        let resolved = self
            .agents
            .resolve(&binding)
            .await
            .map_err(AgentRunProductCommandError::Unavailable)?;
        let snapshot = resolved
            .service
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
        Ok(AgentRunProductCommandEvidence {
            runtime_thread_id: binding.runtime_thread_id,
            source: binding.agent.source,
            binding_generation: resolved.binding_generation,
            snapshot_revision: snapshot.observation.revision,
        })
    }

    /// Recovery reuses the same concrete effect identity and inspects the concrete Agent authority.
    pub async fn replay_claimed(
        &self,
        target: &AgentRunTarget,
        client_command_id: &str,
        command: &AgentRunProductCommand,
    ) -> Result<Option<AgentRuntimeOperationReceipt>, AgentRunProductCommandError> {
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
            | AgentRunProductCommand::Steer { .. }
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
    snapshot: &AgentSnapshot,
) -> Result<AgentCommand, AgentRunProductCommandError> {
    Ok(match command {
        AgentRunProductCommand::SubmitInput { content } => {
            ensure_control_available(snapshot, AgentControlKind::SubmitInput)?;
            AgentCommand::SubmitInput {
                input: AgentInput { content },
            }
        }
        AgentRunProductCommand::Steer {
            content,
            expected_turn_id,
        } => {
            ensure_control_available(snapshot, AgentControlKind::Steer)?;
            let active_turn_id = snapshot
                .observation
                .execution
                .active_turn
                .as_ref()
                .map(|turn| turn.turn_id.clone())
                .ok_or(AgentRunProductCommandError::ActiveTurnMissing)?;
            if active_turn_id != expected_turn_id {
                return Err(AgentRunProductCommandError::InvalidCommand(
                    "expected turn does not match the active turn".to_owned(),
                ));
            }
            AgentCommand::Steer {
                expected_turn_id,
                input: AgentInput { content },
            }
        }
        AgentRunProductCommand::Interrupt => {
            ensure_control_available(snapshot, AgentControlKind::Interrupt)?;
            AgentCommand::Interrupt {
                expected_turn_id: snapshot
                    .observation
                    .execution
                    .active_turn
                    .as_ref()
                    .map(|turn| turn.turn_id.clone())
                    .ok_or(AgentRunProductCommandError::ActiveTurnMissing)?,
            }
        }
        AgentRunProductCommand::RequestCompaction => {
            ensure_control_available(snapshot, AgentControlKind::RequestCompaction)?;
            AgentCommand::RequestCompaction
        }
        AgentRunProductCommand::ResolveInteraction {
            interaction_id,
            response,
        } => {
            ensure_control_available(snapshot, AgentControlKind::ResolveInteraction)?;
            AgentCommand::ResolveInteraction {
                interaction_id,
                response,
            }
        }
        AgentRunProductCommand::Close => {
            ensure_control_available(snapshot, AgentControlKind::Close)?;
            AgentCommand::Close
        }
        AgentRunProductCommand::Resume => {
            return Err(AgentRunProductCommandError::InvalidCommand(
                "Resume uses the Complete Agent lifecycle command".to_owned(),
            ));
        }
    })
}

fn control_available(snapshot: &AgentSnapshot, command: AgentControlKind) -> bool {
    matches!(
        snapshot.observation.command_availability.get(&command),
        Some(AgentControlAvailability::Available { .. })
    )
}

fn ensure_control_available(
    snapshot: &AgentSnapshot,
    command: AgentControlKind,
) -> Result<(), AgentRunProductCommandError> {
    if control_available(snapshot, command) {
        return Ok(());
    }
    Err(AgentRunProductCommandError::InvalidCommand(format!(
        "owner reported {command:?} unavailable: {:?}",
        snapshot.observation.command_availability.get(&command)
    )))
}

fn receipt_status(receipt: &AgentCommandReceipt) -> AgentRuntimeOperationStatus {
    match &receipt.state {
        AgentReceiptState::Accepted => AgentRuntimeOperationStatus::Accepted,
        AgentReceiptState::Rejected { .. } => AgentRuntimeOperationStatus::Failed,
        AgentReceiptState::AlreadyApplied { terminal } => terminal
            .map(terminal_status)
            .unwrap_or(AgentRuntimeOperationStatus::Succeeded),
        AgentReceiptState::Terminal { outcome } => terminal_status(*outcome),
        AgentReceiptState::Unknown => AgentRuntimeOperationStatus::Lost,
    }
}

fn terminal_status(outcome: AgentTerminalOutcome) -> AgentRuntimeOperationStatus {
    match outcome {
        AgentTerminalOutcome::Succeeded | AgentTerminalOutcome::Closed => {
            AgentRuntimeOperationStatus::Succeeded
        }
        AgentTerminalOutcome::Failed => AgentRuntimeOperationStatus::Failed,
        AgentTerminalOutcome::Interrupted => AgentRuntimeOperationStatus::Interrupted,
        AgentTerminalOutcome::Lost => AgentRuntimeOperationStatus::Lost,
    }
}

fn operation_receipt(
    operation_id: AgentEffectIdentity,
    thread_id: agentdash_agent_runtime_contract::RuntimeThreadId,
    status: AgentRuntimeOperationStatus,
    duplicate: bool,
) -> AgentRuntimeOperationReceipt {
    AgentRuntimeOperationReceipt {
        operation_id,
        thread_id,
        status,
        duplicate,
    }
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
    use agentdash_agent_runtime_test_support::coherent_runtime::CoherentAgentObservationBuilder;

    use super::*;

    fn snapshot_with_active_turn(
        kind: agentdash_agent_runtime_contract::AgentActiveTurnKind,
        cancellable: bool,
    ) -> AgentSnapshot {
        let execution = agentdash_agent_runtime_contract::AgentExecutionSnapshot {
            active_turn: Some(agentdash_agent_runtime_contract::AgentActiveTurnSnapshot {
                turn_id: agentdash_agent_runtime_contract::AgentTurnId::new("turn-1").unwrap(),
                kind,
                phase: agentdash_agent_runtime_contract::AgentActiveTurnPhase::Running,
                started_at_ms: 1,
                cancellable,
            }),
            queued_compaction: None,
            last_compaction_outcome: None,
        };
        CoherentAgentObservationBuilder::new(1)
            .execution(execution)
            .snapshot("source-1")
    }

    #[test]
    fn interaction_identity_is_shared_with_the_complete_agent_contract() {
        let interaction = AgentInteractionId::new("approval-1").unwrap();
        let command = AgentRunProductCommand::ResolveInteraction {
            interaction_id: interaction.clone(),
            response: AgentInteractionResponse::Approved,
        };
        assert!(matches!(
            command,
            AgentRunProductCommand::ResolveInteraction { interaction_id, .. }
                if interaction_id == interaction
        ));
    }

    #[test]
    fn submit_and_steer_are_distinct_product_commands() {
        let input = AgentRunProductCommand::SubmitInput {
            content: vec![AgentInputContent::Text {
                text: "continue".to_owned(),
            }],
        };
        let conversation = snapshot_with_active_turn(
            agentdash_agent_runtime_contract::AgentActiveTurnKind::Conversation,
            true,
        );
        assert!(map_command(input.clone(), &conversation).is_err());

        let compaction = snapshot_with_active_turn(
            agentdash_agent_runtime_contract::AgentActiveTurnKind::ContextCompaction,
            false,
        );
        assert!(matches!(
            map_command(input, &compaction).unwrap(),
            AgentCommand::SubmitInput { .. }
        ));

        let steer = AgentRunProductCommand::Steer {
            content: vec![AgentInputContent::Text {
                text: "now".to_owned(),
            }],
            expected_turn_id: AgentTurnId::new("turn-1").unwrap(),
        };
        assert!(matches!(
            map_command(steer, &conversation).unwrap(),
            AgentCommand::Steer { .. }
        ));
    }

    #[test]
    fn applied_agent_outcome_is_recovered_without_redispatch() {
        let command_id = AgentCommandId::new("command-1").unwrap();
        let effect_id = AgentEffectIdentity::new("effect-1").unwrap();
        let source =
            agentdash_agent_runtime_contract::AgentSourceCoordinate::new("source-1").unwrap();
        let receipt = applied_product_command_receipt(
            &AgentRunProductCommand::Close,
            AgentAppliedEffectOutcome::Command {
                receipt: agentdash_agent_runtime_contract::AppliedAgentCommandReceipt {
                    command_id: command_id.clone(),
                    effect_id: effect_id.clone(),
                    source: source.clone(),
                    terminal: Some(AgentTerminalOutcome::Closed),
                    snapshot_revision: Some(
                        agentdash_agent_runtime_contract::AgentSnapshotRevision(42),
                    ),
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
            Some(agentdash_agent_runtime_contract::AgentSnapshotRevision(42))
        );
    }

    #[test]
    fn applied_agent_outcome_kind_must_match_product_command() {
        let result = applied_product_command_receipt(
            &AgentRunProductCommand::Resume,
            AgentAppliedEffectOutcome::Command {
                receipt: agentdash_agent_runtime_contract::AppliedAgentCommandReceipt {
                    command_id: AgentCommandId::new("command-1").unwrap(),
                    effect_id: AgentEffectIdentity::new("effect-1").unwrap(),
                    source: agentdash_agent_runtime_contract::AgentSourceCoordinate::new(
                        "source-1",
                    )
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

    #[test]
    fn runtime_receipt_identity_is_the_complete_agent_effect_identity() {
        let effect_id = AgentEffectIdentity::new("product-effect:v2:stable-identity").unwrap();
        let receipt = operation_receipt(
            effect_id.clone(),
            agentdash_agent_runtime_contract::RuntimeThreadId::new("thread-1").unwrap(),
            AgentRuntimeOperationStatus::Accepted,
            false,
        );
        assert_eq!(receipt.operation_id, effect_id);
    }
}
