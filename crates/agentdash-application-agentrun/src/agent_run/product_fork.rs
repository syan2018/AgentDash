use std::sync::Arc;

use agentdash_agent_runtime_contract::{
    ManagedRuntimeCommandAvailability, ManagedRuntimeCommandKind, ManagedRuntimeSnapshot,
    RuntimeThreadId,
};
use agentdash_domain::agent_run_target::AgentRunTarget;
use chrono::Utc;
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

use super::product_protocol::{
    AgentRunForkFacade, AgentRunForkFacadeError, AgentRunForkParent, AgentRunForkRequestId,
    AgentRunForkSaga, AgentRunForkSagaPhase, AgentRunProductProtocolPorts,
    MaterializeProductAgentRunFork, PreallocatedAgentRunChild,
};
use super::{
    AgentRunProductProjectionError, AgentRunProductProjectionQueryPort,
    AgentRunProductRuntimeSnapshotObservation,
};

const MAX_INLINE_ADVANCES: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunProductForkMessageRef {
    pub turn_id: String,
    pub entry_index: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentRunProductForkRequest {
    pub target: AgentRunTarget,
    pub client_command_id: String,
    pub requested_by_user_id: String,
    pub title: Option<String>,
    pub fork_point_ref: Option<AgentRunProductForkMessageRef>,
    pub metadata_json: Option<Value>,
}

#[derive(Debug, Clone)]
pub struct AgentRunProductForkResult {
    pub saga: AgentRunForkSaga,
    pub replayed: bool,
}

#[derive(Debug, Error)]
pub enum AgentRunProductForkError {
    #[error("fork client_command_id and requested user must be non-empty")]
    InvalidRequest,
    #[error("AgentRun has no committed Runtime binding")]
    TargetNotBound,
    #[error("Runtime Fork command is not available")]
    ForkUnavailable,
    #[error("fork requires a completed Runtime turn")]
    CompletedTurnMissing,
    #[error("fork point does not resolve to a completed source turn")]
    ForkPointNotFound,
    #[error("existing fork request conflicts with the immutable Product intent")]
    RequestConflict,
    #[error("fork remains durably recoverable at {phase:?} for request {request_id}")]
    RecoveryPending {
        request_id: Uuid,
        phase: AgentRunForkSagaPhase,
    },
    #[error("fork failed before a child was created: {0}")]
    Failed(String),
    #[error("fork outcome is Lost and requires operator reconciliation: {0}")]
    Lost(String),
    #[error("fork projection failed: {0}")]
    Projection(String),
    #[error("fork persistence failed: {0}")]
    Persistence(String),
    #[error("fork protocol failed: {0}")]
    Protocol(String),
}

pub struct AgentRunProductForkService {
    projection: Arc<dyn AgentRunProductProjectionQueryPort>,
    protocols: Arc<AgentRunProductProtocolPorts>,
}

impl AgentRunProductForkService {
    pub fn new(
        projection: Arc<dyn AgentRunProductProjectionQueryPort>,
        protocols: Arc<AgentRunProductProtocolPorts>,
    ) -> Self {
        Self {
            projection,
            protocols,
        }
    }

    pub async fn fork(
        &self,
        request: AgentRunProductForkRequest,
    ) -> Result<AgentRunProductForkResult, AgentRunProductForkError> {
        let client_command_id = request.client_command_id.trim();
        let requested_by_user_id = request.requested_by_user_id.trim();
        if client_command_id.is_empty() || requested_by_user_id.is_empty() {
            return Err(AgentRunProductForkError::InvalidRequest);
        }
        let request_id = stable_request_id(&request.target, client_command_id);
        let facade = AgentRunForkFacade::new(
            self.protocols.fork_sagas.as_ref(),
            self.protocols.fork_runtime.as_ref(),
            self.protocols.fork_product_graph.as_ref(),
        );

        let existing = self
            .protocols
            .fork_sagas
            .load(&request_id)
            .await
            .map_err(|error| AgentRunProductForkError::Persistence(error.to_string()))?;
        let (mut saga, replayed) = if let Some(existing) = existing {
            validate_replay(&existing, &request)?;
            (existing, true)
        } else {
            let (binding, snapshot) = self.current_runtime(&request.target).await?;
            ensure_fork_available(&snapshot)?;
            let (through_turn_id, source_turn_id, source_entry_index) =
                resolve_cutoff(&snapshot, request.fork_point_ref.as_ref())?;
            let child = preallocated_child(&request.target, &request_id)?;
            facade
                .materialize_product_fork(MaterializeProductAgentRunFork {
                    request_id: request_id.clone(),
                    parent: AgentRunForkParent {
                        run_id: request.target.run_id,
                        agent_id: request.target.agent_id,
                        runtime_thread_id: binding.runtime_thread_id,
                        through_turn_id,
                    },
                    child,
                    product_intent: super::product_protocol::AgentRunForkProductIntent {
                        requested_by_user_id: requested_by_user_id.to_owned(),
                        requested_at: Utc::now(),
                        title: normalized_title(request.title),
                        source_turn_id,
                        source_entry_index,
                        metadata_json: request.metadata_json,
                    },
                })
                .await
                .map_err(map_facade_error)?
        };

        for _ in 0..MAX_INLINE_ADVANCES {
            if let Some(result) = terminal_result(saga.clone(), replayed)? {
                return Ok(result);
            }
            let before_phase = saga.phase();
            let before_step = saga.next_step();
            match facade.advance(&request_id).await {
                Ok(next) => saga = next,
                Err(error) => {
                    saga = self
                        .protocols
                        .fork_sagas
                        .load(&request_id)
                        .await
                        .map_err(|error| AgentRunProductForkError::Persistence(error.to_string()))?
                        .ok_or_else(|| {
                            AgentRunProductForkError::Persistence(
                                "durable fork request disappeared during recovery".to_owned(),
                            )
                        })?;
                    if let Some(result) = terminal_result(saga.clone(), replayed)? {
                        return Ok(result);
                    }
                    return Err(match error {
                        AgentRunForkFacadeError::ExistingRequestDrift => {
                            AgentRunProductForkError::RequestConflict
                        }
                        AgentRunForkFacadeError::Repository(error) => {
                            AgentRunProductForkError::Persistence(error.to_string())
                        }
                        AgentRunForkFacadeError::Worker(_) => {
                            AgentRunProductForkError::RecoveryPending {
                                request_id: request_id.0,
                                phase: saga.phase(),
                            }
                        }
                    });
                }
            }
            if saga.phase() == before_phase && saga.next_step() == before_step {
                break;
            }
        }
        terminal_result(saga.clone(), replayed)?.ok_or(AgentRunProductForkError::RecoveryPending {
            request_id: request_id.0,
            phase: saga.phase(),
        })
    }

    async fn current_runtime(
        &self,
        target: &AgentRunTarget,
    ) -> Result<
        (super::AgentRunProductRuntimeBinding, ManagedRuntimeSnapshot),
        AgentRunProductForkError,
    > {
        match self
            .projection
            .runtime_snapshot_observation(target)
            .await
            .map_err(map_projection_error)?
        {
            AgentRunProductRuntimeSnapshotObservation::Current {
                product_binding,
                snapshot,
            } => Ok((product_binding, snapshot)),
            AgentRunProductRuntimeSnapshotObservation::Absent { .. } => {
                Err(AgentRunProductForkError::TargetNotBound)
            }
        }
    }
}

fn ensure_fork_available(
    snapshot: &ManagedRuntimeSnapshot,
) -> Result<(), AgentRunProductForkError> {
    if matches!(
        snapshot
            .command_availability
            .get(&ManagedRuntimeCommandKind::Fork),
        Some(ManagedRuntimeCommandAvailability::Available { .. })
    ) {
        Ok(())
    } else {
        Err(AgentRunProductForkError::ForkUnavailable)
    }
}

fn resolve_cutoff(
    snapshot: &ManagedRuntimeSnapshot,
    requested: Option<&AgentRunProductForkMessageRef>,
) -> Result<
    (
        agentdash_agent_runtime_contract::RuntimeTurnId,
        String,
        Option<u32>,
    ),
    AgentRunProductForkError,
> {
    let history =
        agentdash_agent_protocol::CanonicalConversationView::new(&snapshot.conversation_history);
    let turn = history.completed_turn(requested.map(|point| point.turn_id.as_str()));
    let turn = turn.ok_or_else(|| {
        if requested.is_some() {
            AgentRunProductForkError::ForkPointNotFound
        } else {
            AgentRunProductForkError::CompletedTurnMissing
        }
    })?;
    Ok((
        agentdash_agent_runtime_contract::RuntimeTurnId::new(turn.id.clone())
            .map_err(|_| AgentRunProductForkError::ForkPointNotFound)?,
        turn.id.clone(),
        requested.map(|point| point.entry_index),
    ))
}

fn validate_replay(
    saga: &AgentRunForkSaga,
    request: &AgentRunProductForkRequest,
) -> Result<(), AgentRunProductForkError> {
    let intent = saga
        .product_intent()
        .ok_or(AgentRunProductForkError::RequestConflict)?;
    let requested_ref = request.fork_point_ref.as_ref();
    if saga.parent().run_id != request.target.run_id
        || saga.parent().agent_id != request.target.agent_id
        || intent.requested_by_user_id != request.requested_by_user_id.trim()
        || intent.title != normalized_title(request.title.clone())
        || intent.metadata_json != request.metadata_json
        || requested_ref.is_some_and(|requested| {
            intent.source_turn_id != requested.turn_id
                || intent.source_entry_index != Some(requested.entry_index)
        })
        || requested_ref.is_none() && intent.source_entry_index.is_some()
    {
        return Err(AgentRunProductForkError::RequestConflict);
    }
    Ok(())
}

fn terminal_result(
    saga: AgentRunForkSaga,
    replayed: bool,
) -> Result<Option<AgentRunProductForkResult>, AgentRunProductForkError> {
    if let Some(failure) = saga.failure() {
        return Err(AgentRunProductForkError::Failed(failure.reason.clone()));
    }
    if let Some(lost) = saga.lost() {
        return Err(AgentRunProductForkError::Lost(lost.reason.clone()));
    }
    if saga.phase() == AgentRunForkSagaPhase::Succeeded {
        return Ok(Some(AgentRunProductForkResult { saga, replayed }));
    }
    Ok(None)
}

fn preallocated_child(
    target: &AgentRunTarget,
    request_id: &AgentRunForkRequestId,
) -> Result<PreallocatedAgentRunChild, AgentRunProductForkError> {
    let runtime_thread_id =
        RuntimeThreadId::new(stable_uuid(target, request_id, "runtime-thread").to_string())
            .map_err(|error| AgentRunProductForkError::Protocol(error.to_string()))?;
    Ok(PreallocatedAgentRunChild {
        agent_run_id: stable_uuid(target, request_id, "agent-run"),
        run_id: stable_uuid(target, request_id, "run"),
        agent_id: stable_uuid(target, request_id, "agent"),
        frame_id: stable_uuid(target, request_id, "frame"),
        runtime_thread_id,
    })
}

fn stable_request_id(target: &AgentRunTarget, client_command_id: &str) -> AgentRunForkRequestId {
    AgentRunForkRequestId(stable_uuid_from_seed(&format!(
        "agentdash.product-fork/v1:{}:{}:{client_command_id}:request",
        target.run_id, target.agent_id
    )))
}

fn stable_uuid(target: &AgentRunTarget, request_id: &AgentRunForkRequestId, role: &str) -> Uuid {
    stable_uuid_from_seed(&format!(
        "agentdash.product-fork/v1:{}:{}:{}:{role}",
        target.run_id, target.agent_id, request_id.0
    ))
}

fn stable_uuid_from_seed(seed: &str) -> Uuid {
    let digest = Sha256::digest(seed.as_bytes());
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

fn normalized_title(title: Option<String>) -> Option<String> {
    title.and_then(|title| {
        let title = title.trim();
        (!title.is_empty()).then(|| title.to_owned())
    })
}

fn map_projection_error(error: AgentRunProductProjectionError) -> AgentRunProductForkError {
    match error {
        AgentRunProductProjectionError::TargetNotBound => AgentRunProductForkError::TargetNotBound,
        AgentRunProductProjectionError::TargetMismatch => {
            AgentRunProductForkError::Projection("Product target mismatch".to_owned())
        }
        AgentRunProductProjectionError::Binding(message)
        | AgentRunProductProjectionError::Runtime(message)
        | AgentRunProductProjectionError::Terminal(message) => {
            AgentRunProductForkError::Projection(message)
        }
        AgentRunProductProjectionError::Agent(error) => {
            AgentRunProductForkError::Projection(error.to_string())
        }
    }
}

fn map_facade_error(error: AgentRunForkFacadeError) -> AgentRunProductForkError {
    match error {
        AgentRunForkFacadeError::ExistingRequestDrift => AgentRunProductForkError::RequestConflict,
        AgentRunForkFacadeError::Repository(error) => {
            AgentRunProductForkError::Persistence(error.to_string())
        }
        AgentRunForkFacadeError::Worker(error) => {
            AgentRunProductForkError::Protocol(error.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use agentdash_agent_protocol::{
        BackboneEnvelope, BackboneEvent, CanonicalConversationPresentation,
        CanonicalConversationRecord, PresentationDurability, SourceInfo,
        codex_app_server_protocol as codex,
    };
    use agentdash_agent_runtime_contract::{
        ManagedRuntimeAvailabilityEvidence, ManagedRuntimeLifecycleStatus,
        ManagedRuntimeProjectionAuthority, ManagedRuntimeProjectionFidelity,
        RuntimeProjectionRevision,
    };

    use super::*;

    fn snapshot(conversation_history: Vec<CanonicalConversationRecord>) -> ManagedRuntimeSnapshot {
        let revision = RuntimeProjectionRevision(7);
        ManagedRuntimeSnapshot {
            thread_id: RuntimeThreadId::new("runtime-parent").expect("thread"),
            revision,
            captured_at_ms: 10,
            lifecycle: ManagedRuntimeLifecycleStatus::Active,
            interactions: Vec::new(),
            thread_name: None,
            thread_name_source: None,
            operations: Vec::new(),
            source_binding: None,
            authority: ManagedRuntimeProjectionAuthority::SourceAuthoritative,
            fidelity: ManagedRuntimeProjectionFidelity::Exact,
            command_availability: BTreeMap::from([(
                ManagedRuntimeCommandKind::Fork,
                ManagedRuntimeCommandAvailability::Available {
                    evidence: ManagedRuntimeAvailabilityEvidence {
                        blocking_operation_id: None,
                        bound_surface_revision: None,
                        applied_surface_revision: None,
                    },
                },
            )]),
            conversation_history,
        }
    }

    fn turn(
        source: &str,
        status: codex::TurnStatus,
        sequence: usize,
    ) -> CanonicalConversationRecord {
        let turn: codex::Turn = serde_json::from_value(serde_json::json!({
            "id": source,
            "items": [],
            "status": status,
        }))
        .expect("turn fixture");
        let event = if status == codex::TurnStatus::InProgress {
            BackboneEvent::TurnStarted(
                codex::TurnStartedNotification {
                    thread_id: "runtime-parent".to_owned(),
                    turn,
                }
                .into(),
            )
        } else {
            BackboneEvent::TurnCompleted(
                codex::TurnCompletedNotification {
                    thread_id: "runtime-parent".to_owned(),
                    turn,
                }
                .into(),
            )
        };
        CanonicalConversationRecord::new(
            format!("test:{sequence}"),
            CanonicalConversationPresentation::new(
                PresentationDurability::Durable,
                BackboneEnvelope::new(
                    event,
                    "runtime-parent",
                    SourceInfo {
                        connector_id: "test".to_owned(),
                        connector_type: "test".to_owned(),
                        executor_id: None,
                    },
                ),
            ),
        )
    }

    #[test]
    fn explicit_source_reference_resolves_the_runtime_owned_completed_turn() {
        let snapshot = snapshot(vec![
            turn("source-turn-1", codex::TurnStatus::Completed, 1),
            turn("source-turn-2", codex::TurnStatus::Completed, 2),
        ]);
        let (runtime, source, entry) = resolve_cutoff(
            &snapshot,
            Some(&AgentRunProductForkMessageRef {
                turn_id: "source-turn-1".to_owned(),
                entry_index: 9,
            }),
        )
        .expect("cutoff");
        assert_eq!(runtime.as_str(), "source-turn-1");
        assert_eq!(source, "source-turn-1");
        assert_eq!(entry, Some(9));
    }

    #[test]
    fn head_fork_uses_the_latest_completed_turn_and_skips_active_work() {
        let snapshot = snapshot(vec![
            turn("source-turn-1", codex::TurnStatus::Completed, 1),
            turn("source-turn-2", codex::TurnStatus::InProgress, 2),
        ]);
        let (runtime, source, entry) = resolve_cutoff(&snapshot, None).expect("cutoff");
        assert_eq!(runtime.as_str(), "source-turn-1");
        assert_eq!(source, "source-turn-1");
        assert_eq!(entry, None);
    }

    #[test]
    fn stable_product_ids_are_scoped_by_parent_and_client_command() {
        let target = AgentRunTarget {
            run_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
        };
        let first = stable_request_id(&target, "fork-1");
        let replay = stable_request_id(&target, "fork-1");
        let other = stable_request_id(&target, "fork-2");
        assert_eq!(first, replay);
        assert_ne!(first, other);
        assert_eq!(
            preallocated_child(&target, &first).expect("child"),
            preallocated_child(&target, &replay).expect("replay child")
        );
    }
}
