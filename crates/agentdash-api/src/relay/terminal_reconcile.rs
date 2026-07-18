use std::sync::Arc;

use agentdash_agent_runtime_contract::RuntimePayloadDigest;
use agentdash_application_agentrun::agent_run::{
    AgentRunTerminalAvailability, AgentRunTerminalLifecycleState, AgentRunTerminalOutputSequence,
    AgentRunTerminalProjection, AgentRunTerminalProjectionRepository,
    AgentRunTerminalProjectionStoreError, AgentRunTerminalReconcileRequest,
    AgentRunTerminalReconcileResult, AgentRunTerminalSourceInventory,
    AgentRunTerminalSourceReconcilePort, AgentRunTerminalSourceResolution,
    AgentRunTerminalSourceSequence, AgentRunTerminalSourceSnapshot,
};
use agentdash_relay::{
    RelayMessage, TerminalInventoryCursor, TerminalInventoryRequest, TerminalInventoryResponse,
    TerminalSourceSnapshot,
};
use async_trait::async_trait;
use sha2::{Digest, Sha256};

use super::registry::BackendRegistry;

pub struct RelayAgentRunTerminalSourceReconcile {
    backends: Arc<BackendRegistry>,
    projections: Arc<dyn AgentRunTerminalProjectionRepository>,
}

impl RelayAgentRunTerminalSourceReconcile {
    pub fn new(
        backends: Arc<BackendRegistry>,
        projections: Arc<dyn AgentRunTerminalProjectionRepository>,
    ) -> Self {
        Self {
            backends,
            projections,
        }
    }
}

#[async_trait]
impl AgentRunTerminalSourceReconcilePort for RelayAgentRunTerminalSourceReconcile {
    async fn reconcile(
        &self,
        request: AgentRunTerminalReconcileRequest,
    ) -> Result<AgentRunTerminalReconcileResult, AgentRunTerminalProjectionStoreError> {
        let central = self
            .projections
            .load_snapshot(&request.target)
            .await?
            .terminals
            .into_iter()
            .find(|terminal| terminal.terminal_id == request.terminal_id)
            .ok_or(AgentRunTerminalProjectionStoreError::Conflict)?;
        if central.owner.terminal_owner_epoch_id != request.terminal_owner_epoch_id {
            return Err(AgentRunTerminalProjectionStoreError::Conflict);
        }
        let response = self
            .backends
            .send_command(
                &central.owner.backend_id,
                RelayMessage::CommandTerminalInventory {
                    id: RelayMessage::new_id("terminal-reconcile"),
                    payload: TerminalInventoryRequest {
                        cursors: vec![TerminalInventoryCursor {
                            terminal_id: request.terminal_id.as_str().to_string(),
                            terminal_owner_epoch_id: transparent_string(
                                &request.terminal_owner_epoch_id,
                            )?,
                            after_source_sequence: request.after_source_sequence.0,
                        }],
                    },
                },
            )
            .await
            .map_err(|error| {
                AgentRunTerminalProjectionStoreError::Persistence(error.to_string())
            })?;
        let RelayMessage::ResponseTerminalInventory {
            payload: Some(inventory),
            error: None,
            ..
        } = response
        else {
            return Err(AgentRunTerminalProjectionStoreError::Persistence(
                "Local terminal inventory returned an invalid response".to_string(),
            ));
        };
        reconcile_inventory(request, central, inventory)
    }
}

fn reconcile_inventory(
    request: AgentRunTerminalReconcileRequest,
    central: AgentRunTerminalProjection,
    inventory: TerminalInventoryResponse,
) -> Result<AgentRunTerminalReconcileResult, AgentRunTerminalProjectionStoreError> {
    let local = inventory
        .terminals
        .iter()
        .find(|terminal| terminal.terminal_id == request.terminal_id.as_str());
    let Some(local) = local else {
        let owner = central.owner;
        let result = AgentRunTerminalReconcileResult {
            request: request.clone(),
            resolution: AgentRunTerminalSourceResolution::Unknown,
            inventory: AgentRunTerminalSourceInventory {
                owner,
                captured_at_source_sequence: request.after_source_sequence,
                terminals: Vec::new(),
            },
            snapshot: None,
            deltas: Vec::new(),
        };
        result.validate().map_err(|error| {
            AgentRunTerminalProjectionStoreError::Persistence(error.to_string())
        })?;
        return Ok(result);
    };
    let requested_epoch = transparent_string(&request.terminal_owner_epoch_id)?;
    if local.terminal_owner_epoch_id != requested_epoch {
        let mut changed_owner = central.owner;
        changed_owner.terminal_owner_epoch_id =
            agentdash_application_agentrun::agent_run::AgentRunTerminalOwnerEpochId::new(
                local.terminal_owner_epoch_id.clone(),
            )
            .map_err(|error| {
                AgentRunTerminalProjectionStoreError::Persistence(error.to_string())
            })?;
        let changed_projection = local_projection(central, local, changed_owner.clone())?;
        let source_snapshot = source_snapshot(changed_projection)?;
        let result = AgentRunTerminalReconcileResult {
            request,
            resolution: AgentRunTerminalSourceResolution::OwnerFenceUnprovable,
            inventory: AgentRunTerminalSourceInventory {
                owner: changed_owner,
                captured_at_source_sequence: AgentRunTerminalSourceSequence(
                    local.latest_source_sequence,
                ),
                terminals: vec![source_snapshot],
            },
            snapshot: None,
            deltas: Vec::new(),
        };
        result.validate().map_err(|error| {
            AgentRunTerminalProjectionStoreError::Persistence(error.to_string())
        })?;
        return Ok(result);
    }
    if local.latest_source_sequence < request.after_source_sequence.0 {
        return Err(AgentRunTerminalProjectionStoreError::Conflict);
    }
    let owner = central.owner.clone();
    let source_snapshot = source_snapshot(local_projection(central, local, owner.clone())?)?;
    let result = AgentRunTerminalReconcileResult {
        request,
        resolution: AgentRunTerminalSourceResolution::Exact,
        inventory: AgentRunTerminalSourceInventory {
            owner,
            captured_at_source_sequence: AgentRunTerminalSourceSequence(
                local.latest_source_sequence,
            ),
            terminals: vec![source_snapshot.clone()],
        },
        snapshot: Some(source_snapshot),
        deltas: Vec::new(),
    };
    result
        .validate()
        .map_err(|error| AgentRunTerminalProjectionStoreError::Persistence(error.to_string()))?;
    Ok(result)
}

fn local_projection(
    mut projection: AgentRunTerminalProjection,
    local: &TerminalSourceSnapshot,
    owner: agentdash_application_agentrun::agent_run::AgentRunTerminalOwnerFence,
) -> Result<AgentRunTerminalProjection, AgentRunTerminalProjectionStoreError> {
    projection.owner = owner;
    projection.max_output_bytes = u64::try_from(local.max_output_bytes).unwrap_or(u64::MAX);
    projection.state = match local.state {
        agentdash_relay::PtyTerminalProcessState::Starting => {
            AgentRunTerminalLifecycleState::Starting
        }
        agentdash_relay::PtyTerminalProcessState::Running => {
            AgentRunTerminalLifecycleState::Running
        }
        agentdash_relay::PtyTerminalProcessState::Exited => AgentRunTerminalLifecycleState::Exited,
        agentdash_relay::PtyTerminalProcessState::Lost => AgentRunTerminalLifecycleState::Lost,
        agentdash_relay::PtyTerminalProcessState::Killed => AgentRunTerminalLifecycleState::Killed,
    };
    projection.availability = AgentRunTerminalAvailability::Online;
    projection.latest_source_sequence =
        AgentRunTerminalSourceSequence(local.latest_source_sequence);
    projection.exit_code = local.exit_code;
    projection.output.next_sequence = AgentRunTerminalOutputSequence(local.next_output_sequence);
    projection.output.retained_output = local
        .chunks
        .iter()
        .map(|chunk| chunk.data.as_str())
        .collect();
    projection.output.truncated = local.truncation.truncated;
    projection.output.omitted_bytes =
        u64::try_from(local.truncation.omitted_bytes).unwrap_or(u64::MAX);
    Ok(projection)
}

fn source_snapshot(
    terminal: AgentRunTerminalProjection,
) -> Result<AgentRunTerminalSourceSnapshot, AgentRunTerminalProjectionStoreError> {
    let encoded = serde_json::to_vec(&terminal)
        .map_err(|error| AgentRunTerminalProjectionStoreError::Persistence(error.to_string()))?;
    let payload_digest = RuntimePayloadDigest::new(format!("sha256:{:x}", Sha256::digest(encoded)))
        .map_err(|error| AgentRunTerminalProjectionStoreError::Persistence(error.to_string()))?;
    Ok(AgentRunTerminalSourceSnapshot {
        terminal,
        payload_digest,
    })
}

fn transparent_string<T: serde::Serialize>(
    value: &T,
) -> Result<String, AgentRunTerminalProjectionStoreError> {
    serde_json::to_value(value)
        .map_err(|error| AgentRunTerminalProjectionStoreError::Persistence(error.to_string()))?
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| {
            AgentRunTerminalProjectionStoreError::Persistence(
                "terminal owner epoch must serialize as a string".to_string(),
            )
        })
}
