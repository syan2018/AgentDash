use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use agentdash_agent_runtime_contract::RuntimePayloadDigest;
use agentdash_application_agentrun::agent_run::{
    AgentRunTerminalAvailability, AgentRunTerminalChange, AgentRunTerminalChangeId,
    AgentRunTerminalChangeOrigin, AgentRunTerminalChangeSequence, AgentRunTerminalLifecycleState,
    AgentRunTerminalOutboxEntry, AgentRunTerminalOutputSequence, AgentRunTerminalOutputStream,
    AgentRunTerminalProductChangeKind, AgentRunTerminalProjection,
    AgentRunTerminalProjectionCommit, AgentRunTerminalProjectionDelta,
    AgentRunTerminalProjectionRepository, AgentRunTerminalProjectionRevision,
    AgentRunTerminalProjectionStoreError, AgentRunTerminalProjectionUnitOfWork,
    AgentRunTerminalReconcileRequest, AgentRunTerminalSourceProjectionLookup,
    AgentRunTerminalSourceReconcilePort, AgentRunTerminalSourceResolution,
    AgentRunTerminalSourceSequence,
};
use agentdash_relay::{
    PtyTerminalProcessState, PtyTerminalStateChangedPayload, TerminalOutputDelta,
    TerminalOutputPayload, TerminalOutputStream,
};
use sha2::{Digest, Sha256};

pub struct RelayAgentRunTerminalProjectionProducer {
    projections: Arc<dyn AgentRunTerminalProjectionRepository>,
    lookup: Arc<dyn AgentRunTerminalSourceProjectionLookup>,
    unit_of_work: Arc<dyn AgentRunTerminalProjectionUnitOfWork>,
    reconcile: Arc<dyn AgentRunTerminalSourceReconcilePort>,
}

impl RelayAgentRunTerminalProjectionProducer {
    pub fn new<T>(store: Arc<T>, reconcile: Arc<dyn AgentRunTerminalSourceReconcilePort>) -> Self
    where
        T: AgentRunTerminalProjectionRepository
            + AgentRunTerminalSourceProjectionLookup
            + AgentRunTerminalProjectionUnitOfWork
            + 'static,
    {
        Self {
            projections: store.clone(),
            lookup: store.clone(),
            unit_of_work: store,
            reconcile,
        }
    }

    pub async fn mark_backend_offline(
        &self,
        backend_id: &str,
    ) -> Result<usize, AgentRunTerminalProjectionStoreError> {
        let projections = self
            .lookup
            .list_backend_source_projections(backend_id)
            .await?;
        let mut committed = 0;
        for projection in projections {
            if projection.availability == AgentRunTerminalAvailability::Offline {
                continue;
            }
            self.commit_product_delta(
                &projection,
                format!(
                    "backend-offline:{backend_id}:{}:{}:{}",
                    projection.terminal_id.as_str(),
                    projection.latest_source_sequence.0,
                    now_ms(),
                ),
                AgentRunTerminalProductChangeKind::BackendAvailability,
                AgentRunTerminalProjectionDelta::AvailabilityChanged {
                    terminal_id: projection.terminal_id.clone(),
                    owner: projection.owner.clone(),
                    availability: AgentRunTerminalAvailability::Offline,
                    changed_at_ms: now_ms(),
                },
                None,
            )
            .await?;
            committed += 1;
        }
        Ok(committed)
    }

    pub async fn apply_output(
        &self,
        backend_id: &str,
        event_id: &str,
        payload: &TerminalOutputPayload,
    ) -> Result<(), AgentRunTerminalProjectionStoreError> {
        let projection = self
            .load_exact_projection(
                backend_id,
                &payload.terminal_id,
                &payload.source.terminal_owner_epoch_id,
            )
            .await?;
        self.ensure_online(&projection).await?;
        let projection = self
            .load_exact_projection(
                backend_id,
                &payload.terminal_id,
                &payload.source.terminal_owner_epoch_id,
            )
            .await?;
        let output_sequence = projection.output.next_sequence;
        let delta = match &payload.delta {
            TerminalOutputDelta::Appended { stream, data } => {
                AgentRunTerminalProjectionDelta::OutputAppended {
                    terminal_id: projection.terminal_id.clone(),
                    owner: projection.owner.clone(),
                    output_sequence,
                    stream: match stream {
                        TerminalOutputStream::Stdout => AgentRunTerminalOutputStream::Stdout,
                        TerminalOutputStream::Stderr => AgentRunTerminalOutputStream::Stderr,
                        TerminalOutputStream::Pty => AgentRunTerminalOutputStream::Pty,
                    },
                    data: data.clone(),
                }
            }
            TerminalOutputDelta::Omitted {
                omitted_bytes,
                retained_output,
            } => AgentRunTerminalProjectionDelta::OutputOmitted {
                terminal_id: projection.terminal_id.clone(),
                owner: projection.owner.clone(),
                output_sequence,
                omitted_bytes: u64::try_from(*omitted_bytes).unwrap_or(u64::MAX),
                retained_output: retained_output.clone(),
            },
        };
        self.commit_source_delta(
            &projection,
            event_id,
            AgentRunTerminalSourceSequence(payload.source.source_sequence),
            delta,
            Some(output_sequence),
            None,
        )
        .await
    }

    pub async fn apply_state(
        &self,
        backend_id: &str,
        event_id: &str,
        payload: &PtyTerminalStateChangedPayload,
    ) -> Result<(), AgentRunTerminalProjectionStoreError> {
        let projection = self
            .load_exact_projection(
                backend_id,
                &payload.terminal_id,
                &payload.source.terminal_owner_epoch_id,
            )
            .await?;
        if payload.state == PtyTerminalProcessState::Lost {
            return self.reconcile_lost(projection, event_id).await;
        }
        self.ensure_online(&projection).await?;
        let projection = self
            .load_exact_projection(
                backend_id,
                &payload.terminal_id,
                &payload.source.terminal_owner_epoch_id,
            )
            .await?;
        let state = match payload.state {
            PtyTerminalProcessState::Starting => AgentRunTerminalLifecycleState::Starting,
            PtyTerminalProcessState::Running => AgentRunTerminalLifecycleState::Running,
            PtyTerminalProcessState::Exited => AgentRunTerminalLifecycleState::Exited,
            PtyTerminalProcessState::Killed => AgentRunTerminalLifecycleState::Killed,
            PtyTerminalProcessState::Lost => unreachable!("Lost is reconciled from inventory"),
        };
        self.commit_source_delta(
            &projection,
            event_id,
            AgentRunTerminalSourceSequence(payload.source.source_sequence),
            AgentRunTerminalProjectionDelta::StateChanged {
                terminal_id: projection.terminal_id.clone(),
                owner: projection.owner.clone(),
                state,
                exit_code: payload.exit_code,
                changed_at_ms: now_ms(),
            },
            None,
            Some(projection.state),
        )
        .await
    }

    async fn reconcile_lost(
        &self,
        projection: AgentRunTerminalProjection,
        event_id: &str,
    ) -> Result<(), AgentRunTerminalProjectionStoreError> {
        let result = self
            .reconcile
            .reconcile(AgentRunTerminalReconcileRequest {
                target: projection.owner.target.clone(),
                terminal_id: projection.terminal_id.clone(),
                terminal_owner_epoch_id: projection.owner.terminal_owner_epoch_id.clone(),
                after_source_sequence: projection.latest_source_sequence,
            })
            .await?;
        if !matches!(
            result.resolution,
            AgentRunTerminalSourceResolution::Unknown
                | AgentRunTerminalSourceResolution::OwnerFenceUnprovable
        ) {
            return Err(AgentRunTerminalProjectionStoreError::Conflict);
        }
        self.commit_product_delta(
            &projection,
            format!("reconcile-lost:{event_id}"),
            AgentRunTerminalProductChangeKind::ReconcileLost,
            AgentRunTerminalProjectionDelta::StateChanged {
                terminal_id: projection.terminal_id.clone(),
                owner: projection.owner.clone(),
                state: AgentRunTerminalLifecycleState::Lost,
                exit_code: projection.exit_code,
                changed_at_ms: now_ms(),
            },
            Some(projection.state),
        )
        .await
    }

    async fn ensure_online(
        &self,
        projection: &AgentRunTerminalProjection,
    ) -> Result<(), AgentRunTerminalProjectionStoreError> {
        if projection.availability == AgentRunTerminalAvailability::Online {
            return Ok(());
        }
        self.commit_product_delta(
            projection,
            format!(
                "backend-online:{}:{}:{}",
                projection.owner.backend_id,
                projection.terminal_id.as_str(),
                projection.latest_source_sequence.0
            ),
            AgentRunTerminalProductChangeKind::BackendAvailability,
            AgentRunTerminalProjectionDelta::AvailabilityChanged {
                terminal_id: projection.terminal_id.clone(),
                owner: projection.owner.clone(),
                availability: AgentRunTerminalAvailability::Online,
                changed_at_ms: now_ms(),
            },
            None,
        )
        .await
    }

    async fn load_exact_projection(
        &self,
        backend_id: &str,
        terminal_id: &str,
        owner_epoch_id: &str,
    ) -> Result<AgentRunTerminalProjection, AgentRunTerminalProjectionStoreError> {
        let terminal_id =
            agentdash_application_agentrun::agent_run::AgentRunTerminalId::new(terminal_id)
                .map_err(protocol_store_error)?;
        let owner_epoch_id =
            agentdash_application_agentrun::agent_run::AgentRunTerminalOwnerEpochId::new(
                owner_epoch_id,
            )
            .map_err(protocol_store_error)?;
        self.lookup
            .load_source_projection(&terminal_id, &owner_epoch_id, backend_id)
            .await?
            .ok_or(AgentRunTerminalProjectionStoreError::Conflict)
    }

    async fn commit_source_delta(
        &self,
        projection: &AgentRunTerminalProjection,
        change_id: &str,
        source_sequence: AgentRunTerminalSourceSequence,
        delta: AgentRunTerminalProjectionDelta,
        expected_output_sequence: Option<AgentRunTerminalOutputSequence>,
        expected_terminal_state: Option<AgentRunTerminalLifecycleState>,
    ) -> Result<(), AgentRunTerminalProjectionStoreError> {
        if source_sequence
            != AgentRunTerminalSourceSequence(projection.latest_source_sequence.0.saturating_add(1))
        {
            return Err(AgentRunTerminalProjectionStoreError::Conflict);
        }
        self.commit(
            projection,
            change_id,
            AgentRunTerminalChangeOrigin::SourceFact {
                terminal_owner_epoch_id: projection.owner.terminal_owner_epoch_id.clone(),
                source_sequence,
            },
            delta,
            Some(projection.latest_source_sequence),
            expected_output_sequence,
            expected_terminal_state,
        )
        .await
    }

    async fn commit_product_delta(
        &self,
        projection: &AgentRunTerminalProjection,
        change_id: String,
        change_kind: AgentRunTerminalProductChangeKind,
        delta: AgentRunTerminalProjectionDelta,
        expected_terminal_state: Option<AgentRunTerminalLifecycleState>,
    ) -> Result<(), AgentRunTerminalProjectionStoreError> {
        self.commit(
            projection,
            &change_id,
            AgentRunTerminalChangeOrigin::ProductFact { change_kind },
            delta,
            None,
            None,
            expected_terminal_state,
        )
        .await
    }

    async fn commit(
        &self,
        projection: &AgentRunTerminalProjection,
        change_id: &str,
        origin: AgentRunTerminalChangeOrigin,
        delta: AgentRunTerminalProjectionDelta,
        expected_source_sequence: Option<AgentRunTerminalSourceSequence>,
        expected_output_sequence: Option<AgentRunTerminalOutputSequence>,
        expected_terminal_state: Option<AgentRunTerminalLifecycleState>,
    ) -> Result<(), AgentRunTerminalProjectionStoreError> {
        let head = self.projections.load_head(&projection.owner.target).await?;
        let next = head.revision.0.saturating_add(1);
        let change_id = AgentRunTerminalChangeId::new(change_id).map_err(protocol_store_error)?;
        let payload_digest = payload_digest(&delta)?;
        self.unit_of_work
            .commit(AgentRunTerminalProjectionCommit {
                expected_revision: head.revision,
                expected_source_sequence,
                expected_output_sequence,
                expected_terminal_state,
                change: AgentRunTerminalChange {
                    change_id: change_id.clone(),
                    target: projection.owner.target.clone(),
                    sequence: AgentRunTerminalChangeSequence(next),
                    revision: AgentRunTerminalProjectionRevision(next),
                    origin,
                    payload_digest,
                    delta,
                },
                outbox: AgentRunTerminalOutboxEntry {
                    change_id,
                    target: projection.owner.target.clone(),
                    sequence: AgentRunTerminalChangeSequence(next),
                },
            })
            .await
    }
}

fn payload_digest(
    delta: &AgentRunTerminalProjectionDelta,
) -> Result<RuntimePayloadDigest, AgentRunTerminalProjectionStoreError> {
    let encoded = serde_json::to_vec(delta)
        .map_err(|error| AgentRunTerminalProjectionStoreError::Persistence(error.to_string()))?;
    RuntimePayloadDigest::new(format!("sha256:{:x}", Sha256::digest(encoded)))
        .map_err(|error| AgentRunTerminalProjectionStoreError::Persistence(error.to_string()))
}

fn protocol_store_error(
    error: agentdash_application_agentrun::agent_run::AgentRunTerminalProtocolError,
) -> AgentRunTerminalProjectionStoreError {
    AgentRunTerminalProjectionStoreError::Persistence(error.to_string())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}
