use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use agentdash_agent_runtime_contract::{
    ManagedRuntimeChangeDelta, ManagedRuntimeChangeGap, ManagedRuntimeChangePage,
    ManagedRuntimeCommandAvailability, ManagedRuntimeCommandKind, ManagedRuntimeEntityStatus,
    ManagedRuntimeInteraction, ManagedRuntimeInteractionRequest,
    ManagedRuntimeInteractionResolution, ManagedRuntimeInteractionStatus, ManagedRuntimeItem,
    ManagedRuntimeItemBody, ManagedRuntimeItemPresentation, ManagedRuntimeItemTerminalEvidence,
    ManagedRuntimeItemTransition, ManagedRuntimeItemUpdate, ManagedRuntimeLifecycleStatus,
    ManagedRuntimeOperation, ManagedRuntimePlatformChange, ManagedRuntimePresentationContentBlock,
    ManagedRuntimeProjectionAuthority, ManagedRuntimeProjectionFidelity,
    ManagedRuntimeProjectionSection, ManagedRuntimeSnapshot, ManagedRuntimeSourceProjectionDelta,
    ManagedRuntimeThreadNameSource, ManagedRuntimeTurn, RuntimeChangeSequence,
    RuntimeInteractionId, RuntimeItemId, RuntimePayloadDigest, RuntimeProjectionRevision,
    RuntimeThreadId, RuntimeTurnId, SurfaceRevision,
};
use agentdash_agent_service_api::{
    AgentChangePage, AgentChangePayload, AgentChangesQuery, AgentEntityStatus, AgentInteractionId,
    AgentInteractionSnapshot, AgentItemId, AgentItemSnapshot, AgentLifecycleStatus, AgentReadQuery,
    AgentServiceError, AgentSnapshot, AgentSnapshotAuthority, AgentSnapshotRevision,
    AgentSnapshotSource, AgentSourceChangeLevel, AgentSourceCoordinate, AgentSourceCursor,
    AgentSourceRevision, AgentThreadNameSnapshot, AgentTurnId, AgentTurnSnapshot,
    AppliedAgentSurface, AppliedInitialContextEvidence, CanonicalConversationRecord,
    CompleteAgentService,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{
    ManagedRuntimeFacts, ManagedRuntimeOutboxEntry, ManagedRuntimeStateCommit,
    ManagedRuntimeStateRepository, ManagedRuntimeStateSnapshot, ManagedRuntimeStateStoreError,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NormalizedAgentTurn {
    pub id: AgentTurnId,
    pub status: AgentEntityStatus,
    pub item_ids: Vec<AgentItemId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NormalizedAgentItem {
    pub turn_id: AgentTurnId,
    pub item: AgentItemSnapshot,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NormalizedAgentProjection {
    pub source: AgentSourceCoordinate,
    pub platform_revision: u64,
    pub snapshot_revision: AgentSnapshotRevision,
    pub lifecycle: AgentLifecycleStatus,
    pub active_turn_id: Option<AgentTurnId>,
    pub turns: BTreeMap<AgentTurnId, NormalizedAgentTurn>,
    pub items: BTreeMap<AgentItemId, NormalizedAgentItem>,
    pub interactions: BTreeMap<AgentInteractionId, AgentInteractionSnapshot>,
    pub thread_name: Option<String>,
    pub thread_name_source: Option<AgentSnapshotSource>,
    pub source_info: AgentSnapshotSource,
    pub source_cursor: Option<AgentSourceCursor>,
    pub applied_surface: Option<AppliedAgentSurface>,
    pub initial_context: Option<AppliedInitialContextEvidence>,
    pub conversation_history: Vec<CanonicalConversationRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NormalizedAgentPlatformChangePayload {
    SnapshotReplaced {
        snapshot_revision: AgentSnapshotRevision,
        authority: AgentSnapshotAuthority,
        fidelity: agentdash_agent_service_api::SemanticFidelity,
        source_revision: Option<AgentSourceRevision>,
        source_cursor: Option<AgentSourceCursor>,
    },
    SourceChangeApplied {
        source_cursor: AgentSourceCursor,
        source_revision: Option<AgentSourceRevision>,
        payload: Box<AgentChangePayload>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NormalizedAgentPlatformChange {
    pub sequence: u64,
    pub platform_revision: u64,
    pub payload: NormalizedAgentPlatformChangePayload,
    pub changed_sections: BTreeSet<ManagedRuntimeProjectionSection>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NormalizedAgentChangePage {
    pub source: AgentSourceCoordinate,
    pub requested_after_sequence: u64,
    pub earliest_available_sequence: Option<u64>,
    pub latest_available_sequence: Option<u64>,
    pub changes: Vec<NormalizedAgentPlatformChange>,
    pub next_sequence: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PreparedCompleteAgentObservation {
    pub projection: NormalizedAgentProjection,
    pub observations: Vec<PreparedSourceObservation>,
    pub captured_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PreparedSourceObservation {
    pub payload: NormalizedAgentPlatformChangePayload,
    pub changed_sections: BTreeSet<ManagedRuntimeProjectionSection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CompleteAgentStateError {
    #[error(transparent)]
    Store(#[from] ManagedRuntimeStateStoreError),
    #[error(transparent)]
    Projection(#[from] CompleteAgentRuntimeProjectionError),
    #[error(transparent)]
    Service(#[from] AgentServiceError),
    #[error("Agent snapshot/change source does not match the requested source")]
    SourceMismatch,
    #[error("Agent snapshot revision moved backwards")]
    SnapshotRevisionRegression,
    #[error("same Agent snapshot revision returned different normalized facts")]
    SnapshotRevisionConflict,
    #[error("Agent snapshot authority would be downgraded")]
    AuthorityDowngrade,
    #[error("Agent snapshot fidelity would be downgraded")]
    FidelityDowngrade,
    #[error("Agent snapshot contains duplicate or dangling coordinates: {reason}")]
    InvalidSnapshot { reason: String },
    #[error("Agent change page does not continue the persisted source cursor")]
    CursorMismatch,
    #[error("Agent change page contains invalid cursor framing")]
    InvalidChangePage,
    #[error("Agent change cannot be applied to the normalized projection: {reason}")]
    InvalidChange { reason: String },
}

#[derive(Debug, Clone, PartialEq)]
pub enum CompleteAgentReconcileOutcome {
    Unchanged(NormalizedAgentProjection),
    Committed(NormalizedAgentProjection),
    SnapshotReloadRequired,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompleteAgentSourceSyncOutcome {
    pub projection: NormalizedAgentProjection,
    pub reloaded_snapshot: bool,
}

pub struct CompleteAgentStateReconciler<R> {
    repository: Arc<R>,
    identities: CompleteAgentRuntimeIdentityMap,
    initial_command_availability:
        BTreeMap<ManagedRuntimeCommandKind, ManagedRuntimeCommandAvailability>,
}

impl<R> CompleteAgentStateReconciler<R>
where
    R: ManagedRuntimeStateRepository,
{
    pub fn new(
        repository: Arc<R>,
        identities: CompleteAgentRuntimeIdentityMap,
        initial_command_availability: BTreeMap<
            ManagedRuntimeCommandKind,
            ManagedRuntimeCommandAvailability,
        >,
    ) -> Self {
        Self {
            repository,
            identities,
            initial_command_availability,
        }
    }

    pub async fn reconcile_snapshot(
        &self,
        snapshot: AgentSnapshot,
        source_cursor: Option<AgentSourceCursor>,
    ) -> Result<CompleteAgentReconcileOutcome, CompleteAgentStateError> {
        if snapshot.source != *self.identities.source() {
            return Err(CompleteAgentStateError::SourceMismatch);
        }
        let state = self.repository.load(self.identities.thread_id()).await?;
        self.validate_stored_identities(&state.facts)?;
        let next_revision = next_runtime_revision(&state)?;
        let prepared = prepare_snapshot_observation(
            state.facts.source_projection.as_ref(),
            snapshot,
            source_cursor,
            next_revision.0,
        )?;
        let Some(prepared) = prepared else {
            return Ok(CompleteAgentReconcileOutcome::Unchanged(
                state
                    .facts
                    .source_projection
                    .expect("unchanged snapshot requires an existing source projection"),
            ));
        };
        self.commit_observation(state, prepared).await
    }

    pub async fn reconcile_change_page(
        &self,
        query: &AgentChangesQuery,
        page: AgentChangePage,
    ) -> Result<CompleteAgentReconcileOutcome, CompleteAgentStateError> {
        let state = self.repository.load(self.identities.thread_id()).await?;
        self.validate_stored_identities(&state.facts)?;
        let next_revision = next_runtime_revision(&state)?;
        match prepare_change_observation(
            state.facts.source_projection.as_ref(),
            query,
            page,
            next_revision.0,
        )? {
            PreparedChangeObservation::SnapshotReloadRequired => {
                Ok(CompleteAgentReconcileOutcome::SnapshotReloadRequired)
            }
            PreparedChangeObservation::Unchanged(projection) => {
                Ok(CompleteAgentReconcileOutcome::Unchanged(projection))
            }
            PreparedChangeObservation::Commit(prepared) => {
                self.commit_observation(state, prepared).await
            }
        }
    }

    /// Synchronizes one source and records every accepted observation in the Runtime journal.
    ///
    /// Snapshot/observation sources reconcile from authoritative snapshots. Ordered sources only
    /// consume a tail after a durable cursor is known; a missing cursor or gap reloads the
    /// authoritative snapshot, whose contract may intentionally leave the cursor unknown.
    pub async fn synchronize_source(
        &self,
        service: &dyn CompleteAgentService,
        source: AgentSourceCoordinate,
        limit: u32,
    ) -> Result<CompleteAgentSourceSyncOutcome, CompleteAgentStateError> {
        if source != *self.identities.source() {
            return Err(CompleteAgentStateError::SourceMismatch);
        }
        let descriptor = service.describe().await?;
        let current = self.repository.load(self.identities.thread_id()).await?;
        self.validate_stored_identities(&current.facts)?;
        if matches!(
            descriptor.profile.source_changes,
            AgentSourceChangeLevel::SnapshotOnly | AgentSourceChangeLevel::ObservationOnly
        ) || current
            .facts
            .source_projection
            .as_ref()
            .and_then(|projection| projection.source_cursor.as_ref())
            .is_none()
        {
            let snapshot = service
                .read(AgentReadQuery {
                    source: source.clone(),
                    at_revision: None,
                })
                .await?;
            if snapshot.source != source {
                return Err(CompleteAgentStateError::SourceMismatch);
            }
            let outcome = self.reconcile_snapshot(snapshot, None).await?;
            return Ok(CompleteAgentSourceSyncOutcome {
                projection: outcome_projection(outcome)
                    .expect("snapshot reconciliation always returns a projection"),
                reloaded_snapshot: true,
            });
        }

        if limit == 0 {
            return Err(CompleteAgentStateError::InvalidChangePage);
        }
        loop {
            let state = self.repository.load(self.identities.thread_id()).await?;
            let current_projection = state
                .facts
                .source_projection
                .ok_or(CompleteAgentStateError::CursorMismatch)?;
            let query = AgentChangesQuery {
                source: source.clone(),
                after: current_projection.source_cursor,
                limit,
            };
            let page = service.changes(query.clone()).await?;
            if page.source != source {
                return Err(CompleteAgentStateError::SourceMismatch);
            }
            let page_is_full = page.changes.len() == limit as usize;
            match self.reconcile_change_page(&query, page).await? {
                CompleteAgentReconcileOutcome::Unchanged(projection) => {
                    return Ok(CompleteAgentSourceSyncOutcome {
                        projection,
                        reloaded_snapshot: false,
                    });
                }
                CompleteAgentReconcileOutcome::Committed(projection) if !page_is_full => {
                    return Ok(CompleteAgentSourceSyncOutcome {
                        projection,
                        reloaded_snapshot: false,
                    });
                }
                CompleteAgentReconcileOutcome::Committed(_) => {}
                CompleteAgentReconcileOutcome::SnapshotReloadRequired => {
                    let snapshot = service
                        .read(AgentReadQuery {
                            source: source.clone(),
                            at_revision: None,
                        })
                        .await?;
                    if snapshot.source != source {
                        return Err(CompleteAgentStateError::SourceMismatch);
                    }
                    let outcome = self.reconcile_snapshot(snapshot, None).await?;
                    return Ok(CompleteAgentSourceSyncOutcome {
                        projection: outcome_projection(outcome)
                            .expect("snapshot reconciliation always returns a projection"),
                        reloaded_snapshot: true,
                    });
                }
            }
        }
    }

    fn validate_stored_identities(
        &self,
        facts: &ManagedRuntimeFacts,
    ) -> Result<(), CompleteAgentStateError> {
        if let Some(stored) = &facts.source_identities {
            self.identities.validate_extension_of(stored)?;
        }
        Ok(())
    }

    async fn commit_observation(
        &self,
        state: ManagedRuntimeStateSnapshot,
        prepared: PreparedCompleteAgentObservation,
    ) -> Result<CompleteAgentReconcileOutcome, CompleteAgentStateError> {
        let mut facts = state.facts;
        let next_revision = RuntimeProjectionRevision(prepared.projection.platform_revision);
        let mut availability = facts.projection.as_ref().map_or_else(
            || self.initial_command_availability.clone(),
            |projection| projection.command_availability.clone(),
        );
        for value in availability.values_mut() {
            value.evidence_mut().decided_at_revision = next_revision;
        }
        let operations = facts
            .operations
            .values()
            .map(|record| record.operation.clone())
            .collect::<Vec<_>>();
        let current_sequence = facts
            .projection
            .as_ref()
            .map_or(RuntimeChangeSequence(0), |projection| {
                projection.latest_change_sequence
            });
        let mut managed_projection = project_managed_runtime_snapshot(
            &prepared.projection,
            &self.identities,
            CompleteAgentRuntimeProjectionInput {
                thread_id: self.identities.thread_id().clone(),
                projection_revision: next_revision,
                latest_change_sequence: current_sequence,
                captured_at_ms: prepared.captured_at_ms,
                operations,
                command_availability: availability.clone(),
            },
        )?;

        let source_base_sequence = facts
            .source_changes
            .last()
            .map_or(0, |change| change.sequence);
        let mut committed_observations = Vec::with_capacity(prepared.observations.len());
        for (offset, observation) in prepared.observations.iter().cloned().enumerate() {
            let sequence = source_base_sequence
                .checked_add(u64::try_from(offset).map_err(|_| {
                    ManagedRuntimeStateStoreError::Invariant {
                        reason: "source change offset exceeds u64".to_owned(),
                    }
                })?)
                .and_then(|value| value.checked_add(1))
                .ok_or_else(|| ManagedRuntimeStateStoreError::Invariant {
                    reason: "source change sequence is exhausted".to_owned(),
                })?;
            let source_change = NormalizedAgentPlatformChange {
                sequence,
                platform_revision: prepared.projection.platform_revision,
                payload: observation.payload,
                changed_sections: observation.changed_sections,
            };
            facts.source_changes.push(source_change.clone());
            committed_observations.push(source_change);
        }

        let mut deltas = Vec::new();
        let mut observation_projection = facts.source_projection.clone();
        for observation in &committed_observations {
            deltas.push(source_observation_delta(
                self.identities.source(),
                observation,
            )?);
            let after = apply_normalized_observation(
                observation_projection.as_ref(),
                observation,
                &prepared.projection,
            )?;
            deltas.extend(project_source_projection_deltas(
                observation,
                &after,
                &self.identities,
            )?);
            observation_projection = Some(after);
        }
        for command in ManagedRuntimeCommandKind::ALL {
            deltas.push(ManagedRuntimeChangeDelta::CommandAvailabilityChanged {
                command,
                availability: availability
                    .get(&command)
                    .expect("availability completeness is validated by Runtime facts")
                    .clone(),
            });
        }
        let mut next_sequence = current_sequence.0;
        for delta in deltas {
            next_sequence = next_sequence.checked_add(1).ok_or_else(|| {
                ManagedRuntimeStateStoreError::Invariant {
                    reason: "Runtime change sequence is exhausted".to_owned(),
                }
            })?;
            let change = ManagedRuntimePlatformChange {
                thread_id: self.identities.thread_id().clone(),
                sequence: RuntimeChangeSequence(next_sequence),
                revision: next_revision,
                delta,
            };
            facts.changes.push(change.clone());
            facts.outbox.push(ManagedRuntimeOutboxEntry {
                sequence: change.sequence,
                operation_id: None,
                change,
            });
        }
        managed_projection.latest_change_sequence = RuntimeChangeSequence(next_sequence);
        facts.projection = Some(managed_projection);
        facts.source_projection = Some(prepared.projection.clone());
        facts.source_identities = Some(self.identities.clone());

        self.repository
            .commit(ManagedRuntimeStateCommit {
                thread_id: self.identities.thread_id().clone(),
                expected_revision: state.revision,
                facts,
            })
            .await?;
        Ok(CompleteAgentReconcileOutcome::Committed(
            prepared.projection,
        ))
    }
}

enum PreparedChangeObservation {
    Unchanged(NormalizedAgentProjection),
    Commit(PreparedCompleteAgentObservation),
    SnapshotReloadRequired,
}

fn next_runtime_revision(
    state: &ManagedRuntimeStateSnapshot,
) -> Result<RuntimeProjectionRevision, ManagedRuntimeStateStoreError> {
    let current = state
        .facts
        .projection
        .as_ref()
        .map_or(0, |projection| projection.revision.0);
    current
        .checked_add(1)
        .map(RuntimeProjectionRevision)
        .ok_or_else(|| ManagedRuntimeStateStoreError::Invariant {
            reason: "Runtime projection revision is exhausted".to_owned(),
        })
}

fn prepare_snapshot_observation(
    existing: Option<&NormalizedAgentProjection>,
    snapshot: AgentSnapshot,
    source_cursor: Option<AgentSourceCursor>,
    platform_revision: u64,
) -> Result<Option<PreparedCompleteAgentObservation>, CompleteAgentStateError> {
    let captured_at_ms = snapshot.source_info.observed_at_ms;
    let normalized = normalize_snapshot(snapshot, source_cursor, platform_revision)?;
    if let Some(existing) = existing {
        ensure_snapshot_can_replace(existing, &normalized)?;
        if same_projection_facts(existing, &normalized) {
            return Ok(None);
        }
    }
    let mut changed_sections = BTreeSet::from([ManagedRuntimeProjectionSection::Snapshot]);
    if normalized.thread_name_source.is_some()
        && existing.is_none_or(|current| {
            current.thread_name_source.is_none() || current.thread_name != normalized.thread_name
        })
    {
        changed_sections.insert(ManagedRuntimeProjectionSection::ThreadName);
    }
    Ok(Some(PreparedCompleteAgentObservation {
        observations: vec![PreparedSourceObservation {
            payload: NormalizedAgentPlatformChangePayload::SnapshotReplaced {
                snapshot_revision: normalized.snapshot_revision,
                authority: normalized.source_info.authority,
                fidelity: normalized.source_info.fidelity,
                source_revision: normalized.source_info.source_revision.clone(),
                source_cursor: normalized.source_cursor.clone(),
            },
            changed_sections,
        }],
        projection: normalized,
        captured_at_ms,
    }))
}

fn prepare_change_observation(
    existing: Option<&NormalizedAgentProjection>,
    query: &AgentChangesQuery,
    page: AgentChangePage,
    platform_revision: u64,
) -> Result<PreparedChangeObservation, CompleteAgentStateError> {
    if query.source != page.source {
        return Err(CompleteAgentStateError::SourceMismatch);
    }
    if page.gap
        || page.changes.iter().any(|change| {
            matches!(
                source_state_payload(&change.payload),
                AgentChangePayload::SnapshotInvalidated { .. }
            )
        })
    {
        return Ok(PreparedChangeObservation::SnapshotReloadRequired);
    }
    if query.limit == 0 || page.changes.len() > query.limit as usize {
        return Err(CompleteAgentStateError::InvalidChangePage);
    }
    let Some(existing) = existing else {
        return Ok(PreparedChangeObservation::SnapshotReloadRequired);
    };
    if existing.source != query.source {
        return Err(CompleteAgentStateError::SourceMismatch);
    }
    if existing.source_cursor != query.after {
        return Err(CompleteAgentStateError::CursorMismatch);
    }
    validate_page_cursors(query, &page)?;
    if page.changes.is_empty() && existing.source_cursor == page.next {
        return Ok(PreparedChangeObservation::Unchanged(existing.clone()));
    }

    let mut projection = existing.clone();
    let mut captured_at_ms = projection.source_info.observed_at_ms;
    let mut observations = Vec::with_capacity(page.changes.len());
    for change in page.changes {
        let before = projection.clone();
        apply_source_change(&mut projection, &change.payload)?;
        let changed_sections = changed_sections(&before, &projection, &change.payload);
        if let Some(source_revision) = &change.source_revision {
            projection.source_info.source_revision = Some(source_revision.clone());
        }
        captured_at_ms = captured_at_ms.max(change.occurred_at_ms);
        observations.push(PreparedSourceObservation {
            payload: NormalizedAgentPlatformChangePayload::SourceChangeApplied {
                source_cursor: change.cursor,
                source_revision: change.source_revision,
                payload: Box::new(change.payload),
            },
            changed_sections,
        });
    }
    projection.platform_revision = platform_revision;
    projection.source_cursor = page.next;
    projection.source_info.observed_at_ms = captured_at_ms;
    Ok(PreparedChangeObservation::Commit(
        PreparedCompleteAgentObservation {
            projection,
            observations,
            captured_at_ms,
        },
    ))
}

fn outcome_projection(outcome: CompleteAgentReconcileOutcome) -> Option<NormalizedAgentProjection> {
    match outcome {
        CompleteAgentReconcileOutcome::Unchanged(projection)
        | CompleteAgentReconcileOutcome::Committed(projection) => Some(projection),
        CompleteAgentReconcileOutcome::SnapshotReloadRequired => None,
    }
}

fn normalize_snapshot(
    snapshot: AgentSnapshot,
    source_cursor: Option<AgentSourceCursor>,
    platform_revision: u64,
) -> Result<NormalizedAgentProjection, CompleteAgentStateError> {
    validate_conversation_history(&snapshot.conversation_history)?;
    let (thread_name, thread_name_source) = normalize_thread_name(snapshot.thread_name)?;
    let mut turns = BTreeMap::new();
    let mut items = BTreeMap::new();
    for turn in snapshot.turns {
        insert_turn(&mut turns, &mut items, turn)?;
    }
    if snapshot
        .active_turn_id
        .as_ref()
        .is_some_and(|turn_id| !turns.contains_key(turn_id))
    {
        return Err(CompleteAgentStateError::InvalidSnapshot {
            reason: "active turn does not exist in the snapshot".to_owned(),
        });
    }
    let mut interactions = BTreeMap::new();
    for interaction in snapshot.interactions {
        if !turns.contains_key(&interaction.turn_id) {
            return Err(CompleteAgentStateError::InvalidSnapshot {
                reason: "interaction references an unknown turn".to_owned(),
            });
        }
        if !interaction.validate() {
            return Err(CompleteAgentStateError::InvalidSnapshot {
                reason: "interaction status and resolution evidence do not agree".to_owned(),
            });
        }
        if interactions
            .insert(interaction.id.clone(), interaction)
            .is_some()
        {
            return Err(CompleteAgentStateError::InvalidSnapshot {
                reason: "duplicate interaction id".to_owned(),
            });
        }
    }
    Ok(NormalizedAgentProjection {
        source: snapshot.source,
        platform_revision,
        snapshot_revision: snapshot.revision,
        lifecycle: snapshot.lifecycle,
        active_turn_id: snapshot.active_turn_id,
        turns,
        items,
        interactions,
        thread_name,
        thread_name_source,
        source_info: snapshot.source_info,
        source_cursor,
        applied_surface: snapshot.applied_surface,
        initial_context: snapshot.initial_context,
        conversation_history: snapshot.conversation_history,
    })
}

fn validate_conversation_history(
    history: &[CanonicalConversationRecord],
) -> Result<(), CompleteAgentStateError> {
    let mut ids = BTreeSet::new();
    if history.iter().any(|record| {
        record.presentation_id.trim().is_empty() || !ids.insert(&record.presentation_id)
    }) {
        return Err(CompleteAgentStateError::InvalidSnapshot {
            reason: "conversation history contains a blank or duplicate presentation id".to_owned(),
        });
    }
    Ok(())
}

fn normalize_thread_name(
    thread_name: Option<AgentThreadNameSnapshot>,
) -> Result<(Option<String>, Option<AgentSnapshotSource>), CompleteAgentStateError> {
    let Some(thread_name) = thread_name else {
        return Ok((None, None));
    };
    if thread_name.source_info.authority != AgentSnapshotAuthority::AgentAuthoritative
        || thread_name.source_info.fidelity != agentdash_agent_service_api::SemanticFidelity::Exact
    {
        return Err(CompleteAgentStateError::InvalidSnapshot {
            reason: "thread name must be source-authoritative with exact fidelity".to_owned(),
        });
    }
    if thread_name
        .thread_name
        .as_ref()
        .is_some_and(|value| value.trim().is_empty())
    {
        return Err(CompleteAgentStateError::InvalidSnapshot {
            reason: "thread name must be non-blank when present".to_owned(),
        });
    }
    Ok((thread_name.thread_name, Some(thread_name.source_info)))
}

fn insert_turn(
    turns: &mut BTreeMap<AgentTurnId, NormalizedAgentTurn>,
    items: &mut BTreeMap<AgentItemId, NormalizedAgentItem>,
    turn: AgentTurnSnapshot,
) -> Result<(), CompleteAgentStateError> {
    if turns.contains_key(&turn.id) {
        return Err(CompleteAgentStateError::InvalidSnapshot {
            reason: "duplicate turn id".to_owned(),
        });
    }
    let turn_id = turn.id.clone();
    let mut item_ids = Vec::with_capacity(turn.items.len());
    for item in turn.items {
        item.validate()
            .map_err(|error| CompleteAgentStateError::InvalidSnapshot {
                reason: error.to_string(),
            })?;
        if items
            .insert(
                item.id.clone(),
                NormalizedAgentItem {
                    turn_id: turn_id.clone(),
                    item: item.clone(),
                },
            )
            .is_some()
        {
            return Err(CompleteAgentStateError::InvalidSnapshot {
                reason: "duplicate item id".to_owned(),
            });
        }
        item_ids.push(item.id);
    }
    turns.insert(
        turn_id.clone(),
        NormalizedAgentTurn {
            id: turn_id,
            status: turn.status,
            item_ids,
        },
    );
    Ok(())
}

fn ensure_snapshot_can_replace(
    existing: &NormalizedAgentProjection,
    incoming: &NormalizedAgentProjection,
) -> Result<(), CompleteAgentStateError> {
    if existing.source != incoming.source {
        return Err(CompleteAgentStateError::SourceMismatch);
    }
    if incoming.snapshot_revision < existing.snapshot_revision {
        return Err(CompleteAgentStateError::SnapshotRevisionRegression);
    }
    if authority_rank(incoming.source_info.authority)
        < authority_rank(existing.source_info.authority)
    {
        return Err(CompleteAgentStateError::AuthorityDowngrade);
    }
    if !incoming
        .source_info
        .fidelity
        .satisfies(existing.source_info.fidelity)
    {
        return Err(CompleteAgentStateError::FidelityDowngrade);
    }
    if incoming.snapshot_revision == existing.snapshot_revision
        && !same_snapshot_revision_facts(existing, incoming)
    {
        return Err(CompleteAgentStateError::SnapshotRevisionConflict);
    }
    Ok(())
}

fn same_snapshot_revision_facts(
    left: &NormalizedAgentProjection,
    right: &NormalizedAgentProjection,
) -> bool {
    left.source == right.source
        && left.snapshot_revision == right.snapshot_revision
        && left.lifecycle == right.lifecycle
        && left.active_turn_id == right.active_turn_id
        && left.turns == right.turns
        && left.items == right.items
        && left.interactions == right.interactions
        && left.thread_name == right.thread_name
        && left.thread_name_source == right.thread_name_source
        && left.applied_surface == right.applied_surface
        && left.initial_context == right.initial_context
        && left.conversation_history == right.conversation_history
}

fn authority_rank(authority: AgentSnapshotAuthority) -> u8 {
    match authority {
        AgentSnapshotAuthority::Derived => 1,
        AgentSnapshotAuthority::AgentObserved => 2,
        AgentSnapshotAuthority::AgentAuthoritative => 3,
    }
}

fn same_projection_facts(
    left: &NormalizedAgentProjection,
    right: &NormalizedAgentProjection,
) -> bool {
    left.source == right.source
        && left.snapshot_revision == right.snapshot_revision
        && left.lifecycle == right.lifecycle
        && left.active_turn_id == right.active_turn_id
        && left.turns == right.turns
        && left.items == right.items
        && left.interactions == right.interactions
        && left.thread_name == right.thread_name
        && left.thread_name_source == right.thread_name_source
        && left.source_info == right.source_info
        && left.source_cursor == right.source_cursor
        && left.applied_surface == right.applied_surface
        && left.initial_context == right.initial_context
        && left.conversation_history == right.conversation_history
}

fn validate_page_cursors(
    query: &AgentChangesQuery,
    page: &AgentChangePage,
) -> Result<(), CompleteAgentStateError> {
    let mut seen = BTreeSet::new();
    for change in &page.changes {
        if !seen.insert(change.cursor.clone()) || query.after.as_ref() == Some(&change.cursor) {
            return Err(CompleteAgentStateError::InvalidChangePage);
        }
    }
    if let Some(last) = page.changes.last() {
        if page.next.as_ref() != Some(&last.cursor) {
            return Err(CompleteAgentStateError::InvalidChangePage);
        }
    } else if page.next != query.after {
        return Err(CompleteAgentStateError::InvalidChangePage);
    }
    Ok(())
}

fn apply_source_change(
    projection: &mut NormalizedAgentProjection,
    payload: &AgentChangePayload,
) -> Result<(), CompleteAgentStateError> {
    match payload {
        AgentChangePayload::SourceObservation {
            state,
            presentation,
        } => {
            if matches!(state.as_ref(), AgentChangePayload::SourceObservation { .. }) {
                return Err(CompleteAgentStateError::InvalidChange {
                    reason: "nested source observation is invalid".to_owned(),
                });
            }
            apply_source_change(projection, state)?;
            let mut known = projection
                .conversation_history
                .iter()
                .map(|record| record.presentation_id.as_str())
                .collect::<BTreeSet<_>>();
            if presentation.iter().any(|record| {
                record.presentation_id.trim().is_empty()
                    || !known.insert(record.presentation_id.as_str())
            }) {
                return Err(CompleteAgentStateError::InvalidChange {
                    reason: "conversation presentation id is blank or already committed".to_owned(),
                });
            }
            projection.conversation_history.extend(presentation.clone());
        }
        AgentChangePayload::ThreadNameChanged {
            thread_name,
            source_info,
        } => {
            if source_info.authority != AgentSnapshotAuthority::AgentAuthoritative
                || source_info.fidelity != agentdash_agent_service_api::SemanticFidelity::Exact
            {
                return Err(CompleteAgentStateError::InvalidChange {
                    reason: "thread name must be source-authoritative with exact fidelity"
                        .to_owned(),
                });
            }
            if thread_name
                .as_ref()
                .is_some_and(|value| value.trim().is_empty())
            {
                return Err(CompleteAgentStateError::InvalidChange {
                    reason: "thread name must be non-blank when present".to_owned(),
                });
            }
            projection.thread_name = thread_name.clone();
            projection.thread_name_source = Some(source_info.clone());
        }
        AgentChangePayload::LifecycleChanged { status } => projection.lifecycle = *status,
        AgentChangePayload::TurnChanged { turn } => {
            if let Some(existing) = projection.turns.remove(&turn.id) {
                for item_id in existing.item_ids {
                    projection.items.remove(&item_id);
                }
            }
            insert_turn(&mut projection.turns, &mut projection.items, turn.clone())?;
        }
        AgentChangePayload::ActiveTurnChanged { active_turn_id } => {
            if active_turn_id
                .as_ref()
                .is_some_and(|turn_id| !projection.turns.contains_key(turn_id))
            {
                return Err(CompleteAgentStateError::InvalidChange {
                    reason: "active turn change references an unknown turn".to_owned(),
                });
            }
            projection.active_turn_id = active_turn_id.clone();
        }
        AgentChangePayload::ItemChanged { turn_id, item } => {
            item.validate()
                .map_err(|error| CompleteAgentStateError::InvalidChange {
                    reason: error.to_string(),
                })?;
            let turn = projection.turns.get_mut(turn_id).ok_or_else(|| {
                CompleteAgentStateError::InvalidChange {
                    reason: "item change references an unknown turn".to_owned(),
                }
            })?;
            if let Some(existing) = projection.items.get(&item.id)
                && &existing.turn_id != turn_id
            {
                return Err(CompleteAgentStateError::InvalidChange {
                    reason: "item id is already owned by another turn".to_owned(),
                });
            }
            if !turn.item_ids.contains(&item.id) {
                turn.item_ids.push(item.id.clone());
            }
            projection.items.insert(
                item.id.clone(),
                NormalizedAgentItem {
                    turn_id: turn_id.clone(),
                    item: item.clone(),
                },
            );
        }
        AgentChangePayload::ItemTransitioned {
            turn_id,
            item_id,
            transition,
        } => {
            let turn = projection.turns.get_mut(turn_id).ok_or_else(|| {
                CompleteAgentStateError::InvalidChange {
                    reason: "item transition references an unknown turn".to_owned(),
                }
            })?;
            let previous = projection.items.get(item_id).map(|item| &item.item);
            let item =
                AgentItemSnapshot::from_transition(item_id.clone(), previous, transition.clone())
                    .map_err(|error| CompleteAgentStateError::InvalidChange {
                    reason: error.to_string(),
                })?;
            if !turn.item_ids.contains(item_id) {
                turn.item_ids.push(item_id.clone());
            }
            projection.items.insert(
                item_id.clone(),
                NormalizedAgentItem {
                    turn_id: turn_id.clone(),
                    item,
                },
            );
        }
        AgentChangePayload::InteractionChanged { interaction } => {
            if !projection.turns.contains_key(&interaction.turn_id) {
                return Err(CompleteAgentStateError::InvalidChange {
                    reason: "interaction change references an unknown turn".to_owned(),
                });
            }
            if !interaction.validate() {
                return Err(CompleteAgentStateError::InvalidChange {
                    reason: "interaction status and resolution evidence do not agree".to_owned(),
                });
            }
            projection
                .interactions
                .insert(interaction.id.clone(), interaction.clone());
        }
        AgentChangePayload::SurfaceApplied { applied } => {
            projection.applied_surface = Some(applied.clone());
        }
        AgentChangePayload::SnapshotInvalidated { .. } => {
            return Err(CompleteAgentStateError::InvalidChange {
                reason: "snapshot invalidation requires an authoritative reload".to_owned(),
            });
        }
    }
    Ok(())
}

fn source_state_payload(payload: &AgentChangePayload) -> &AgentChangePayload {
    match payload {
        AgentChangePayload::SourceObservation { state, .. } => state,
        payload => payload,
    }
}

fn changed_sections(
    before: &NormalizedAgentProjection,
    after: &NormalizedAgentProjection,
    payload: &AgentChangePayload,
) -> BTreeSet<ManagedRuntimeProjectionSection> {
    let mut sections = BTreeSet::new();
    match payload {
        AgentChangePayload::SourceObservation {
            state,
            presentation,
        } => {
            sections.extend(changed_sections(before, after, state));
            if !presentation.is_empty() {
                sections.insert(ManagedRuntimeProjectionSection::ConversationPresentation);
            }
        }
        AgentChangePayload::ThreadNameChanged { .. }
            if before.thread_name_source.is_none() || before.thread_name != after.thread_name =>
        {
            sections.insert(ManagedRuntimeProjectionSection::ThreadName);
        }
        AgentChangePayload::LifecycleChanged { .. } if before.lifecycle != after.lifecycle => {
            sections.insert(ManagedRuntimeProjectionSection::Lifecycle);
        }
        AgentChangePayload::TurnChanged { .. } => {
            if before.turns != after.turns {
                sections.insert(ManagedRuntimeProjectionSection::Turns);
            }
            if before.items != after.items {
                sections.insert(ManagedRuntimeProjectionSection::Items);
            }
        }
        AgentChangePayload::ActiveTurnChanged { .. }
            if before.active_turn_id != after.active_turn_id =>
        {
            sections.insert(ManagedRuntimeProjectionSection::ActiveTurn);
        }
        AgentChangePayload::ItemChanged { .. } | AgentChangePayload::ItemTransitioned { .. } => {
            if before.turns != after.turns {
                sections.insert(ManagedRuntimeProjectionSection::Turns);
            }
            if before.items != after.items {
                sections.insert(ManagedRuntimeProjectionSection::Items);
            }
        }
        AgentChangePayload::InteractionChanged { .. }
            if before.interactions != after.interactions =>
        {
            sections.insert(ManagedRuntimeProjectionSection::Interactions);
        }
        AgentChangePayload::SurfaceApplied { .. }
            if before.applied_surface != after.applied_surface =>
        {
            sections.insert(ManagedRuntimeProjectionSection::Surface);
        }
        AgentChangePayload::ThreadNameChanged { .. }
        | AgentChangePayload::SnapshotInvalidated { .. }
        | AgentChangePayload::LifecycleChanged { .. }
        | AgentChangePayload::ActiveTurnChanged { .. }
        | AgentChangePayload::InteractionChanged { .. }
        | AgentChangePayload::SurfaceApplied { .. } => {}
    }
    sections
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct CompleteAgentRuntimeItemIdentity {
    source_turn_id: AgentTurnId,
    runtime_item_id: RuntimeItemId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct CompleteAgentRuntimeInteractionIdentity {
    source_turn_id: AgentTurnId,
    runtime_interaction_id: RuntimeInteractionId,
}

/// Stable Runtime-owned identity map for one Complete Agent source.
///
/// Runtime identities must be allocated independently and then bound explicitly. A source
/// coordinate is never parsed or copied into a Runtime identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompleteAgentRuntimeIdentityMap {
    source: AgentSourceCoordinate,
    thread_id: RuntimeThreadId,
    turns: BTreeMap<AgentTurnId, RuntimeTurnId>,
    items: BTreeMap<AgentItemId, CompleteAgentRuntimeItemIdentity>,
    interactions: BTreeMap<AgentInteractionId, CompleteAgentRuntimeInteractionIdentity>,
    surface_revisions: BTreeMap<agentdash_agent_service_api::AgentSurfaceRevision, SurfaceRevision>,
}

impl CompleteAgentRuntimeIdentityMap {
    pub fn new(source: AgentSourceCoordinate, thread_id: RuntimeThreadId) -> Self {
        Self {
            source,
            thread_id,
            turns: BTreeMap::new(),
            items: BTreeMap::new(),
            interactions: BTreeMap::new(),
            surface_revisions: BTreeMap::new(),
        }
    }

    pub fn source(&self) -> &AgentSourceCoordinate {
        &self.source
    }

    pub fn thread_id(&self) -> &RuntimeThreadId {
        &self.thread_id
    }

    pub fn source_turn_id(
        &self,
        runtime_turn_id: &RuntimeTurnId,
    ) -> Result<AgentTurnId, CompleteAgentRuntimeProjectionError> {
        self.turns
            .iter()
            .find_map(|(source, runtime)| (runtime == runtime_turn_id).then(|| source.clone()))
            .ok_or_else(|| CompleteAgentRuntimeProjectionError::MissingIdentity {
                kind: "Runtime turn",
                source_identity: runtime_turn_id.to_string(),
            })
    }

    pub fn source_interaction_id(
        &self,
        runtime_interaction_id: &RuntimeInteractionId,
    ) -> Result<AgentInteractionId, CompleteAgentRuntimeProjectionError> {
        self.interactions
            .iter()
            .find_map(|(source, identity)| {
                (&identity.runtime_interaction_id == runtime_interaction_id).then(|| source.clone())
            })
            .ok_or_else(|| CompleteAgentRuntimeProjectionError::MissingIdentity {
                kind: "Runtime interaction",
                source_identity: runtime_interaction_id.to_string(),
            })
    }

    pub(crate) fn validate_extension_of(
        &self,
        current: &Self,
    ) -> Result<(), CompleteAgentRuntimeProjectionError> {
        if self.source != current.source {
            return Err(CompleteAgentRuntimeProjectionError::SourceMismatch);
        }
        if self.thread_id != current.thread_id {
            return Err(CompleteAgentRuntimeProjectionError::ThreadMismatch);
        }
        if current
            .turns
            .iter()
            .any(|(source, runtime)| self.turns.get(source) != Some(runtime))
            || current
                .items
                .iter()
                .any(|(source, runtime)| self.items.get(source) != Some(runtime))
            || current
                .interactions
                .iter()
                .any(|(source, runtime)| self.interactions.get(source) != Some(runtime))
            || current
                .surface_revisions
                .iter()
                .any(|(source, runtime)| self.surface_revisions.get(source) != Some(runtime))
        {
            return Err(CompleteAgentRuntimeProjectionError::IdentityDrift {
                kind: "source mapping",
                source_identity: self.source.to_string(),
                expected_runtime_identity: current.thread_id.to_string(),
                received_runtime_identity: self.thread_id.to_string(),
            });
        }
        Ok(())
    }

    pub fn bind_turn(
        &mut self,
        source_turn_id: AgentTurnId,
        runtime_turn_id: RuntimeTurnId,
    ) -> Result<(), CompleteAgentRuntimeProjectionError> {
        bind_identity("turn", &source_turn_id, &runtime_turn_id, &mut self.turns)
    }

    pub fn bind_item(
        &mut self,
        source_item_id: AgentItemId,
        source_turn_id: AgentTurnId,
        runtime_item_id: RuntimeItemId,
    ) -> Result<(), CompleteAgentRuntimeProjectionError> {
        if !self.turns.contains_key(&source_turn_id) {
            return Err(CompleteAgentRuntimeProjectionError::MissingIdentity {
                kind: "turn",
                source_identity: source_turn_id.to_string(),
            });
        }
        if let Some(existing) = self.items.get(&source_item_id) {
            if existing.source_turn_id != source_turn_id {
                return Err(CompleteAgentRuntimeProjectionError::ParentIdentityDrift {
                    kind: "item",
                    source_identity: source_item_id.to_string(),
                    expected_parent: existing.source_turn_id.to_string(),
                    received_parent: source_turn_id.to_string(),
                });
            }
            if existing.runtime_item_id != runtime_item_id {
                return Err(CompleteAgentRuntimeProjectionError::IdentityDrift {
                    kind: "item",
                    source_identity: source_item_id.to_string(),
                    expected_runtime_identity: existing.runtime_item_id.to_string(),
                    received_runtime_identity: runtime_item_id.to_string(),
                });
            }
            return Ok(());
        }
        if self
            .items
            .values()
            .any(|identity| identity.runtime_item_id == runtime_item_id)
        {
            return Err(CompleteAgentRuntimeProjectionError::RuntimeIdentityReused {
                kind: "item",
                runtime_identity: runtime_item_id.to_string(),
            });
        }
        self.items.insert(
            source_item_id,
            CompleteAgentRuntimeItemIdentity {
                source_turn_id,
                runtime_item_id,
            },
        );
        Ok(())
    }

    pub fn bind_interaction(
        &mut self,
        source_interaction_id: AgentInteractionId,
        source_turn_id: AgentTurnId,
        runtime_interaction_id: RuntimeInteractionId,
    ) -> Result<(), CompleteAgentRuntimeProjectionError> {
        if !self.turns.contains_key(&source_turn_id) {
            return Err(CompleteAgentRuntimeProjectionError::MissingIdentity {
                kind: "turn",
                source_identity: source_turn_id.to_string(),
            });
        }
        if let Some(existing) = self.interactions.get(&source_interaction_id) {
            if existing.source_turn_id != source_turn_id {
                return Err(CompleteAgentRuntimeProjectionError::ParentIdentityDrift {
                    kind: "interaction",
                    source_identity: source_interaction_id.to_string(),
                    expected_parent: existing.source_turn_id.to_string(),
                    received_parent: source_turn_id.to_string(),
                });
            }
            if existing.runtime_interaction_id != runtime_interaction_id {
                return Err(CompleteAgentRuntimeProjectionError::IdentityDrift {
                    kind: "interaction",
                    source_identity: source_interaction_id.to_string(),
                    expected_runtime_identity: existing.runtime_interaction_id.to_string(),
                    received_runtime_identity: runtime_interaction_id.to_string(),
                });
            }
            return Ok(());
        }
        if self
            .interactions
            .values()
            .any(|identity| identity.runtime_interaction_id == runtime_interaction_id)
        {
            return Err(CompleteAgentRuntimeProjectionError::RuntimeIdentityReused {
                kind: "interaction",
                runtime_identity: runtime_interaction_id.to_string(),
            });
        }
        self.interactions.insert(
            source_interaction_id,
            CompleteAgentRuntimeInteractionIdentity {
                source_turn_id,
                runtime_interaction_id,
            },
        );
        Ok(())
    }

    pub fn bind_surface_revision(
        &mut self,
        source_revision: agentdash_agent_service_api::AgentSurfaceRevision,
        runtime_revision: SurfaceRevision,
    ) -> Result<(), CompleteAgentRuntimeProjectionError> {
        if let Some(existing) = self.surface_revisions.get(&source_revision) {
            if existing != &runtime_revision {
                return Err(CompleteAgentRuntimeProjectionError::IdentityDrift {
                    kind: "surface revision",
                    source_identity: source_revision.0.to_string(),
                    expected_runtime_identity: existing.0.to_string(),
                    received_runtime_identity: runtime_revision.0.to_string(),
                });
            }
            return Ok(());
        }
        if self
            .surface_revisions
            .values()
            .any(|revision| revision == &runtime_revision)
        {
            return Err(CompleteAgentRuntimeProjectionError::RuntimeIdentityReused {
                kind: "surface revision",
                runtime_identity: runtime_revision.0.to_string(),
            });
        }
        self.surface_revisions
            .insert(source_revision, runtime_revision);
        Ok(())
    }

    fn runtime_turn_id(
        &self,
        source_turn_id: &AgentTurnId,
    ) -> Result<RuntimeTurnId, CompleteAgentRuntimeProjectionError> {
        self.turns.get(source_turn_id).cloned().ok_or_else(|| {
            CompleteAgentRuntimeProjectionError::MissingIdentity {
                kind: "turn",
                source_identity: source_turn_id.to_string(),
            }
        })
    }

    fn runtime_item_id(
        &self,
        source_item_id: &AgentItemId,
        source_turn_id: &AgentTurnId,
    ) -> Result<RuntimeItemId, CompleteAgentRuntimeProjectionError> {
        let identity = self.items.get(source_item_id).ok_or_else(|| {
            CompleteAgentRuntimeProjectionError::MissingIdentity {
                kind: "item",
                source_identity: source_item_id.to_string(),
            }
        })?;
        if &identity.source_turn_id != source_turn_id {
            return Err(CompleteAgentRuntimeProjectionError::ParentIdentityDrift {
                kind: "item",
                source_identity: source_item_id.to_string(),
                expected_parent: identity.source_turn_id.to_string(),
                received_parent: source_turn_id.to_string(),
            });
        }
        Ok(identity.runtime_item_id.clone())
    }

    fn runtime_interaction_id(
        &self,
        source_interaction_id: &AgentInteractionId,
        source_turn_id: &AgentTurnId,
    ) -> Result<RuntimeInteractionId, CompleteAgentRuntimeProjectionError> {
        let identity = self
            .interactions
            .get(source_interaction_id)
            .ok_or_else(|| CompleteAgentRuntimeProjectionError::MissingIdentity {
                kind: "interaction",
                source_identity: source_interaction_id.to_string(),
            })?;
        if &identity.source_turn_id != source_turn_id {
            return Err(CompleteAgentRuntimeProjectionError::ParentIdentityDrift {
                kind: "interaction",
                source_identity: source_interaction_id.to_string(),
                expected_parent: identity.source_turn_id.to_string(),
                received_parent: source_turn_id.to_string(),
            });
        }
        Ok(identity.runtime_interaction_id.clone())
    }

    fn runtime_surface_revision(
        &self,
        source_revision: agentdash_agent_service_api::AgentSurfaceRevision,
    ) -> Result<SurfaceRevision, CompleteAgentRuntimeProjectionError> {
        self.surface_revisions
            .get(&source_revision)
            .copied()
            .ok_or_else(|| CompleteAgentRuntimeProjectionError::MissingIdentity {
                kind: "surface revision",
                source_identity: source_revision.0.to_string(),
            })
    }
}

pub(crate) fn validate_complete_agent_source_facts(
    current: &ManagedRuntimeFacts,
    candidate: &ManagedRuntimeFacts,
) -> Result<(), ManagedRuntimeStateStoreError> {
    let source_facts_present = candidate.source_projection.is_some()
        || candidate.source_identities.is_some()
        || !candidate.source_changes.is_empty();
    if !source_facts_present {
        if current.source_projection.is_some()
            || current.source_identities.is_some()
            || !current.source_changes.is_empty()
        {
            return Err(ManagedRuntimeStateStoreError::Invariant {
                reason: "Complete Agent source facts cannot be removed".to_owned(),
            });
        }
        return Ok(());
    }
    let source = candidate.source_projection.as_ref().ok_or_else(|| {
        ManagedRuntimeStateStoreError::Invariant {
            reason: "Complete Agent source changes require a normalized projection".to_owned(),
        }
    })?;
    let identities = candidate.source_identities.as_ref().ok_or_else(|| {
        ManagedRuntimeStateStoreError::Invariant {
            reason: "Complete Agent source projection requires Runtime identities".to_owned(),
        }
    })?;
    let managed =
        candidate
            .projection
            .as_ref()
            .ok_or_else(|| ManagedRuntimeStateStoreError::Invariant {
                reason: "Complete Agent source projection requires a Managed Runtime projection"
                    .to_owned(),
            })?;
    if source.source != *identities.source()
        || managed.thread_id != *identities.thread_id()
        || source.platform_revision > managed.revision.0
    {
        return Err(ManagedRuntimeStateStoreError::Invariant {
            reason: "Complete Agent source and Managed Runtime coordinates are inconsistent"
                .to_owned(),
        });
    }
    if let Some(current_identities) = &current.source_identities {
        identities
            .validate_extension_of(current_identities)
            .map_err(|error| ManagedRuntimeStateStoreError::Invariant {
                reason: error.to_string(),
            })?;
    }
    if let Some(current_source) = &current.source_projection
        && (source.source != current_source.source
            || source.snapshot_revision.0 < current_source.snapshot_revision.0
            || authority_rank(source.source_info.authority)
                < authority_rank(current_source.source_info.authority)
            || !source
                .source_info
                .fidelity
                .satisfies(current_source.source_info.fidelity)
            || source.platform_revision < current_source.platform_revision)
    {
        return Err(ManagedRuntimeStateStoreError::Invariant {
            reason: "Complete Agent source authority, fidelity, or revision moved backwards"
                .to_owned(),
        });
    }
    if !candidate
        .source_changes
        .starts_with(&current.source_changes)
    {
        return Err(ManagedRuntimeStateStoreError::Invariant {
            reason: "Complete Agent source change history is append-only".to_owned(),
        });
    }
    let mut previous_sequence = None;
    let mut previous_revision = None;
    for change in &candidate.source_changes {
        if change.sequence == 0
            || previous_sequence.is_some_and(|previous| change.sequence != previous + 1)
            || previous_revision.is_some_and(|previous| change.platform_revision < previous)
            || change.platform_revision > source.platform_revision
        {
            return Err(ManagedRuntimeStateStoreError::Invariant {
                reason: "Complete Agent source change coordinates are invalid".to_owned(),
            });
        }
        previous_sequence = Some(change.sequence);
        previous_revision = Some(change.platform_revision);
    }
    if previous_revision.is_some_and(|revision| revision != source.platform_revision) {
        return Err(ManagedRuntimeStateStoreError::Invariant {
            reason: "Complete Agent source projection has no matching latest change".to_owned(),
        });
    }
    let expected_projection_changes =
        validate_appended_source_sections(current, candidate, identities)?;

    let expected_observations = candidate
        .source_changes
        .iter()
        .map(|change| {
            source_observation_delta(&source.source, change).map_err(|error| {
                ManagedRuntimeStateStoreError::Invariant {
                    reason: error.to_string(),
                }
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let actual_observations = candidate
        .changes
        .iter()
        .filter_map(|change| {
            matches!(
                change.delta,
                ManagedRuntimeChangeDelta::SourceObservationApplied { .. }
            )
            .then_some((&change.delta, change.revision))
        })
        .collect::<Vec<_>>();
    if actual_observations.len() != expected_observations.len()
        || expected_observations.iter().any(|expected| {
            let ManagedRuntimeChangeDelta::SourceObservationApplied {
                source_projection_revision,
                ..
            } = expected
            else {
                unreachable!("source observation helper always returns its typed delta");
            };
            actual_observations
                .iter()
                .filter(|(actual, revision)| {
                    *actual == expected && revision == source_projection_revision
                })
                .count()
                != 1
        })
    {
        return Err(ManagedRuntimeStateStoreError::Invariant {
            reason: "Complete Agent source observation has no exact causal Runtime change"
                .to_owned(),
        });
    }

    let appended_runtime_changes =
        candidate
            .changes
            .get(current.changes.len()..)
            .ok_or_else(|| ManagedRuntimeStateStoreError::Invariant {
                reason: "Runtime change history cannot be removed".to_owned(),
            })?;
    let actual_projection_changes = appended_runtime_changes
        .iter()
        .filter_map(|change| {
            matches!(
                change.delta,
                ManagedRuntimeChangeDelta::SourceProjectionChanged { .. }
                    | ManagedRuntimeChangeDelta::ThreadNameChanged { .. }
            )
            .then_some((&change.delta, change.revision))
        })
        .collect::<Vec<_>>();
    if actual_projection_changes.len() != expected_projection_changes.len()
        || expected_projection_changes.iter().any(|expected| {
            let source_projection_revision = match expected {
                ManagedRuntimeChangeDelta::SourceProjectionChanged {
                    source_projection_revision,
                    ..
                }
                | ManagedRuntimeChangeDelta::ThreadNameChanged {
                    source_projection_revision,
                    ..
                } => source_projection_revision,
                _ => unreachable!("source projection helper always returns a typed source delta"),
            };
            actual_projection_changes
                .iter()
                .filter(|(actual, revision)| {
                    *actual == expected && revision == source_projection_revision
                })
                .count()
                != 1
        })
    {
        return Err(ManagedRuntimeStateStoreError::Invariant {
            reason:
                "Complete Agent changed sections have no exact causal Runtime projection changes"
                    .to_owned(),
        });
    }

    let expected = project_managed_runtime_snapshot(
        source,
        identities,
        CompleteAgentRuntimeProjectionInput {
            thread_id: managed.thread_id.clone(),
            projection_revision: managed.revision,
            latest_change_sequence: managed.latest_change_sequence,
            captured_at_ms: managed.captured_at_ms,
            operations: candidate
                .operations
                .values()
                .map(|record| record.operation.clone())
                .collect(),
            command_availability: managed.command_availability.clone(),
        },
    )
    .map_err(|error| ManagedRuntimeStateStoreError::Invariant {
        reason: error.to_string(),
    })?;
    if &expected != managed {
        return Err(ManagedRuntimeStateStoreError::Invariant {
            reason:
                "Managed Runtime projection does not exactly project source and operation facts"
                    .to_owned(),
        });
    }
    Ok(())
}

fn apply_normalized_observation(
    previous: Option<&NormalizedAgentProjection>,
    change: &NormalizedAgentPlatformChange,
    snapshot_projection: &NormalizedAgentProjection,
) -> Result<NormalizedAgentProjection, CompleteAgentStateError> {
    match &change.payload {
        NormalizedAgentPlatformChangePayload::SnapshotReplaced { .. } => {
            Ok(snapshot_projection.clone())
        }
        NormalizedAgentPlatformChangePayload::SourceChangeApplied {
            source_cursor,
            source_revision,
            payload,
        } => {
            let mut projection =
                previous
                    .cloned()
                    .ok_or_else(|| CompleteAgentStateError::InvalidChange {
                        reason: "source delta has no preceding normalized projection".to_owned(),
                    })?;
            apply_source_change(&mut projection, payload)?;
            projection.source_cursor = Some(source_cursor.clone());
            if let Some(source_revision) = source_revision {
                projection.source_info.source_revision = Some(source_revision.clone());
            }
            projection.platform_revision = change.platform_revision;
            Ok(projection)
        }
    }
}

fn validate_appended_source_sections(
    current: &ManagedRuntimeFacts,
    candidate: &ManagedRuntimeFacts,
    identities: &CompleteAgentRuntimeIdentityMap,
) -> Result<Vec<ManagedRuntimeChangeDelta>, ManagedRuntimeStateStoreError> {
    let appended = &candidate.source_changes[current.source_changes.len()..];
    let mut projection = current.source_projection.clone();
    let mut expected_projection_changes = Vec::new();
    for change in appended {
        match &change.payload {
            NormalizedAgentPlatformChangePayload::SnapshotReplaced { .. } => {
                let mut expected = BTreeSet::from([ManagedRuntimeProjectionSection::Snapshot]);
                if candidate.source_projection.as_ref().is_some_and(|after| {
                    after.thread_name_source.is_some()
                        && projection.as_ref().is_none_or(|before| {
                            before.thread_name_source.is_none()
                                || before.thread_name != after.thread_name
                        })
                }) {
                    expected.insert(ManagedRuntimeProjectionSection::ThreadName);
                }
                if change.changed_sections != expected {
                    return Err(ManagedRuntimeStateStoreError::Invariant {
                        reason: "snapshot observation changed-section evidence is invalid"
                            .to_owned(),
                    });
                }
                projection = candidate.source_projection.clone();
            }
            NormalizedAgentPlatformChangePayload::SourceChangeApplied {
                source_cursor,
                source_revision,
                payload,
            } => {
                let before = projection.as_ref().ok_or_else(|| {
                    ManagedRuntimeStateStoreError::Invariant {
                        reason: "source delta has no preceding normalized projection".to_owned(),
                    }
                })?;
                let mut after = before.clone();
                apply_source_change(&mut after, payload).map_err(|error| {
                    ManagedRuntimeStateStoreError::Invariant {
                        reason: error.to_string(),
                    }
                })?;
                let expected = changed_sections(before, &after, payload);
                if change.changed_sections != expected {
                    return Err(ManagedRuntimeStateStoreError::Invariant {
                        reason: "source observation changed-section evidence is inconsistent"
                            .to_owned(),
                    });
                }
                after.source_cursor = Some(source_cursor.clone());
                if let Some(source_revision) = source_revision {
                    after.source_info.source_revision = Some(source_revision.clone());
                }
                after.platform_revision = change.platform_revision;
                projection = Some(after);
            }
        }
        let after =
            projection
                .as_ref()
                .ok_or_else(|| ManagedRuntimeStateStoreError::Invariant {
                    reason: "source observation has no resulting normalized projection".to_owned(),
                })?;
        expected_projection_changes.extend(
            project_source_projection_deltas(change, after, identities).map_err(|error| {
                ManagedRuntimeStateStoreError::Invariant {
                    reason: error.to_string(),
                }
            })?,
        );
    }
    Ok(expected_projection_changes)
}

fn bind_identity<S, R>(
    kind: &'static str,
    source: &S,
    runtime: &R,
    identities: &mut BTreeMap<S, R>,
) -> Result<(), CompleteAgentRuntimeProjectionError>
where
    S: Clone + Ord + ToString,
    R: Clone + PartialEq + ToString,
{
    if let Some(existing) = identities.get(source) {
        if existing != runtime {
            return Err(CompleteAgentRuntimeProjectionError::IdentityDrift {
                kind,
                source_identity: source.to_string(),
                expected_runtime_identity: existing.to_string(),
                received_runtime_identity: runtime.to_string(),
            });
        }
        return Ok(());
    }
    if identities.values().any(|identity| identity == runtime) {
        return Err(CompleteAgentRuntimeProjectionError::RuntimeIdentityReused {
            kind,
            runtime_identity: runtime.to_string(),
        });
    }
    identities.insert(source.clone(), runtime.clone());
    Ok(())
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompleteAgentRuntimeProjectionInput {
    pub thread_id: RuntimeThreadId,
    pub projection_revision: RuntimeProjectionRevision,
    pub latest_change_sequence: RuntimeChangeSequence,
    pub captured_at_ms: u64,
    pub operations: Vec<ManagedRuntimeOperation>,
    pub command_availability:
        BTreeMap<ManagedRuntimeCommandKind, ManagedRuntimeCommandAvailability>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CompleteAgentRuntimeProjectionError {
    #[error("Agent projection source does not match the Runtime identity map")]
    SourceMismatch,
    #[error("Runtime projection thread does not match the Runtime identity map")]
    ThreadMismatch,
    #[error(
        "Runtime projection revision mismatch: source projection {source_revision}, requested Runtime revision {runtime_revision}"
    )]
    RevisionMismatch {
        source_revision: u64,
        runtime_revision: u64,
    },
    #[error("missing {kind} Runtime identity for source coordinate {source_identity}")]
    MissingIdentity {
        kind: &'static str,
        source_identity: String,
    },
    #[error(
        "{kind} source coordinate {source_identity} drifted from Runtime identity {expected_runtime_identity} to {received_runtime_identity}"
    )]
    IdentityDrift {
        kind: &'static str,
        source_identity: String,
        expected_runtime_identity: String,
        received_runtime_identity: String,
    },
    #[error("{kind} Runtime identity {runtime_identity} is already bound")]
    RuntimeIdentityReused {
        kind: &'static str,
        runtime_identity: String,
    },
    #[error(
        "{kind} source coordinate {source_identity} changed parent from {expected_parent} to {received_parent}"
    )]
    ParentIdentityDrift {
        kind: &'static str,
        source_identity: String,
        expected_parent: String,
        received_parent: String,
    },
    #[error("Runtime command availability is missing {command:?}")]
    MissingCommandAvailability { command: ManagedRuntimeCommandKind },
    #[error(
        "Runtime command availability for {command:?} was decided at revision {decided_revision}, snapshot revision is {snapshot_revision}"
    )]
    AvailabilityRevisionMismatch {
        command: ManagedRuntimeCommandKind,
        decided_revision: u64,
        snapshot_revision: u64,
    },
    #[error("Runtime operation references unknown turn {turn_id}")]
    OperationTurnMissing { turn_id: RuntimeTurnId },
    #[error("Agent payload digest could not be represented by the Runtime contract")]
    InvalidPayloadDigest,
    #[error("Agent source observation could not be encoded for causal evidence")]
    ObservationEncoding,
    #[error("Agent source observation section does not match its normalized payload")]
    InvalidSourceProjectionSection,
    #[error("Agent source emitted a snapshot invalidation as a committed delta")]
    SnapshotInvalidationCommitted,
}

pub fn project_managed_runtime_snapshot(
    projection: &NormalizedAgentProjection,
    identities: &CompleteAgentRuntimeIdentityMap,
    input: CompleteAgentRuntimeProjectionInput,
) -> Result<ManagedRuntimeSnapshot, CompleteAgentRuntimeProjectionError> {
    validate_projection_boundary(projection, identities, &input)?;

    let turns = projection
        .turns
        .values()
        .map(|turn| project_turn(turn, projection, identities))
        .collect::<Result<Vec<_>, _>>()?;
    let items = projection
        .items
        .values()
        .map(|item| project_item(&item.item, &item.turn_id, identities))
        .collect::<Result<Vec<_>, _>>()?;
    let interactions = projection
        .interactions
        .values()
        .map(|interaction| project_interaction(interaction, identities))
        .collect::<Result<Vec<_>, _>>()?;
    let active_turn_id = projection
        .active_turn_id
        .as_ref()
        .map(|turn_id| identities.runtime_turn_id(turn_id))
        .transpose()?;

    Ok(ManagedRuntimeSnapshot {
        thread_id: input.thread_id,
        revision: input.projection_revision,
        latest_change_sequence: input.latest_change_sequence,
        captured_at_ms: input.captured_at_ms,
        lifecycle: project_lifecycle(projection.lifecycle),
        active_turn_id,
        turns,
        items,
        interactions,
        thread_name: projection.thread_name.clone(),
        thread_name_source: projection
            .thread_name_source
            .as_ref()
            .map(|source_info| project_thread_name_source(source_info, identities.source()))
            .transpose()?,
        operations: input.operations,
        source_binding: None,
        authority: project_authority(projection.source_info.authority),
        fidelity: project_fidelity(projection.source_info.fidelity),
        command_availability: input.command_availability,
        conversation_history: projection.conversation_history.clone(),
    })
}

pub fn project_managed_runtime_change_page(
    page: &NormalizedAgentChangePage,
    identities: &CompleteAgentRuntimeIdentityMap,
    snapshot_revision: RuntimeProjectionRevision,
) -> Result<ManagedRuntimeChangePage, CompleteAgentRuntimeProjectionError> {
    if page.source != identities.source {
        return Err(CompleteAgentRuntimeProjectionError::SourceMismatch);
    }
    let changes = page
        .changes
        .iter()
        .map(|change| project_platform_change(change, identities))
        .collect::<Result<Vec<_>, _>>()?;
    let requested_after = RuntimeChangeSequence(page.requested_after_sequence);
    let gap = page
        .earliest_available_sequence
        .filter(|earliest| page.requested_after_sequence.saturating_add(1) < *earliest)
        .map(|earliest| ManagedRuntimeChangeGap {
            requested_after: (page.requested_after_sequence != 0).then_some(requested_after),
            earliest_available: RuntimeChangeSequence(earliest),
            latest_available: RuntimeChangeSequence(
                page.latest_available_sequence.unwrap_or(page.next_sequence),
            ),
            snapshot_revision,
        });

    Ok(ManagedRuntimeChangePage {
        thread_id: identities.thread_id.clone(),
        changes,
        next: RuntimeChangeSequence(page.next_sequence),
        gap,
    })
}

fn validate_projection_boundary(
    projection: &NormalizedAgentProjection,
    identities: &CompleteAgentRuntimeIdentityMap,
    input: &CompleteAgentRuntimeProjectionInput,
) -> Result<(), CompleteAgentRuntimeProjectionError> {
    if projection.source != identities.source {
        return Err(CompleteAgentRuntimeProjectionError::SourceMismatch);
    }
    if input.thread_id != identities.thread_id {
        return Err(CompleteAgentRuntimeProjectionError::ThreadMismatch);
    }
    if input.projection_revision.0 < projection.platform_revision {
        return Err(CompleteAgentRuntimeProjectionError::RevisionMismatch {
            source_revision: projection.platform_revision,
            runtime_revision: input.projection_revision.0,
        });
    }
    for command in ManagedRuntimeCommandKind::ALL {
        let availability = input
            .command_availability
            .get(&command)
            .ok_or(CompleteAgentRuntimeProjectionError::MissingCommandAvailability { command })?;
        if availability.evidence().decided_at_revision != input.projection_revision {
            return Err(
                CompleteAgentRuntimeProjectionError::AvailabilityRevisionMismatch {
                    command,
                    decided_revision: availability.evidence().decided_at_revision.0,
                    snapshot_revision: input.projection_revision.0,
                },
            );
        }
    }
    let known_turns = identities.turns.values().collect::<BTreeSet<_>>();
    for operation in &input.operations {
        if let Some(turn_id) = &operation.turn_id
            && !known_turns.contains(turn_id)
        {
            return Err(CompleteAgentRuntimeProjectionError::OperationTurnMissing {
                turn_id: turn_id.clone(),
            });
        }
    }
    Ok(())
}

fn project_platform_change(
    change: &NormalizedAgentPlatformChange,
    identities: &CompleteAgentRuntimeIdentityMap,
) -> Result<ManagedRuntimePlatformChange, CompleteAgentRuntimeProjectionError> {
    let delta = source_observation_delta(identities.source(), change)?;
    Ok(ManagedRuntimePlatformChange {
        thread_id: identities.thread_id.clone(),
        sequence: RuntimeChangeSequence(change.sequence),
        revision: RuntimeProjectionRevision(change.platform_revision),
        delta,
    })
}

fn project_source_projection_deltas(
    change: &NormalizedAgentPlatformChange,
    projection: &NormalizedAgentProjection,
    identities: &CompleteAgentRuntimeIdentityMap,
) -> Result<Vec<ManagedRuntimeChangeDelta>, CompleteAgentRuntimeProjectionError> {
    let observation_digest = serialized_digest(&change.payload)?;
    change
        .changed_sections
        .iter()
        .map(|section| {
            if *section == ManagedRuntimeProjectionSection::ConversationPresentation {
                let records = source_observation_presentation(&change.payload)
                    .ok_or(CompleteAgentRuntimeProjectionError::InvalidSourceProjectionSection)?;
                return Ok(
                    ManagedRuntimeChangeDelta::ConversationPresentationAppended {
                        source_change_sequence: change.sequence,
                        source_projection_revision: RuntimeProjectionRevision(
                            change.platform_revision,
                        ),
                        records: records.to_vec(),
                    },
                );
            }
            if *section == ManagedRuntimeProjectionSection::ThreadName {
                let source_info = projection
                    .thread_name_source
                    .as_ref()
                    .ok_or(CompleteAgentRuntimeProjectionError::InvalidSourceProjectionSection)?;
                return Ok(ManagedRuntimeChangeDelta::ThreadNameChanged {
                    source_change_sequence: change.sequence,
                    source_projection_revision: RuntimeProjectionRevision(change.platform_revision),
                    thread_name: projection.thread_name.clone(),
                    source: project_thread_name_source(source_info, identities.source())?,
                });
            }
            let delta = project_source_projection_section(
                &change.payload,
                projection,
                *section,
                identities,
            )?;
            let section_digest = serialized_digest(&delta)?;
            Ok(ManagedRuntimeChangeDelta::SourceProjectionChanged {
                source_change_sequence: change.sequence,
                source_projection_revision: RuntimeProjectionRevision(change.platform_revision),
                observation_digest: observation_digest.clone(),
                section: *section,
                section_digest,
                delta,
            })
        })
        .collect()
}

fn project_source_projection_section(
    payload: &NormalizedAgentPlatformChangePayload,
    projection: &NormalizedAgentProjection,
    section: ManagedRuntimeProjectionSection,
    identities: &CompleteAgentRuntimeIdentityMap,
) -> Result<ManagedRuntimeSourceProjectionDelta, CompleteAgentRuntimeProjectionError> {
    if !source_payload_can_change_section(payload, section) {
        return Err(CompleteAgentRuntimeProjectionError::InvalidSourceProjectionSection);
    }
    let turns = || {
        projection
            .turns
            .values()
            .map(|turn| project_turn(turn, projection, identities))
            .collect::<Result<Vec<_>, _>>()
    };
    let items = || {
        projection
            .items
            .values()
            .map(|item| project_item(&item.item, &item.turn_id, identities))
            .collect::<Result<Vec<_>, _>>()
    };
    let interactions = || {
        projection
            .interactions
            .values()
            .map(|interaction| project_interaction(interaction, identities))
            .collect::<Result<Vec<_>, _>>()
    };
    let applied_surface_revision = || {
        projection
            .applied_surface
            .as_ref()
            .map(|applied| identities.runtime_surface_revision(applied.revision))
            .transpose()
    };

    match section {
        ManagedRuntimeProjectionSection::ThreadName => {
            Err(CompleteAgentRuntimeProjectionError::InvalidSourceProjectionSection)
        }
        ManagedRuntimeProjectionSection::Snapshot => {
            Ok(ManagedRuntimeSourceProjectionDelta::SnapshotReplaced {
                lifecycle: project_lifecycle(projection.lifecycle),
                active_turn_id: projection
                    .active_turn_id
                    .as_ref()
                    .map(|turn_id| identities.runtime_turn_id(turn_id))
                    .transpose()?,
                turns: turns()?,
                items: items()?,
                interactions: interactions()?,
                authority: project_authority(projection.source_info.authority),
                fidelity: project_fidelity(projection.source_info.fidelity),
                applied_surface_revision: applied_surface_revision()?,
            })
        }
        ManagedRuntimeProjectionSection::Lifecycle => {
            Ok(ManagedRuntimeSourceProjectionDelta::LifecycleChanged {
                lifecycle: project_lifecycle(projection.lifecycle),
            })
        }
        ManagedRuntimeProjectionSection::ActiveTurn => {
            Ok(ManagedRuntimeSourceProjectionDelta::ActiveTurnChanged {
                active_turn_id: projection
                    .active_turn_id
                    .as_ref()
                    .map(|turn_id| identities.runtime_turn_id(turn_id))
                    .transpose()?,
            })
        }
        ManagedRuntimeProjectionSection::Turns => {
            Ok(ManagedRuntimeSourceProjectionDelta::TurnsChanged { turns: turns()? })
        }
        ManagedRuntimeProjectionSection::Items => {
            if let NormalizedAgentPlatformChangePayload::SourceChangeApplied {
                payload: source_payload,
                ..
            } = payload
                && let AgentChangePayload::ItemTransitioned {
                    turn_id,
                    item_id,
                    transition,
                } = source_state_payload(source_payload)
            {
                return Ok(ManagedRuntimeSourceProjectionDelta::ItemTransitioned {
                    item_id: identities.runtime_item_id(item_id, turn_id)?,
                    transition: project_item_transition(transition)?,
                });
            }
            Ok(ManagedRuntimeSourceProjectionDelta::ItemsChanged { items: items()? })
        }
        ManagedRuntimeProjectionSection::Interactions => {
            Ok(ManagedRuntimeSourceProjectionDelta::InteractionsChanged {
                interactions: interactions()?,
            })
        }
        ManagedRuntimeProjectionSection::Surface => {
            Ok(ManagedRuntimeSourceProjectionDelta::SurfaceChanged {
                applied_surface_revision: applied_surface_revision()?,
            })
        }
        ManagedRuntimeProjectionSection::ConversationPresentation => {
            Err(CompleteAgentRuntimeProjectionError::InvalidSourceProjectionSection)
        }
    }
}

fn source_observation_presentation(
    payload: &NormalizedAgentPlatformChangePayload,
) -> Option<&[CanonicalConversationRecord]> {
    let NormalizedAgentPlatformChangePayload::SourceChangeApplied { payload, .. } = payload else {
        return None;
    };
    let AgentChangePayload::SourceObservation { presentation, .. } = payload.as_ref() else {
        return None;
    };
    Some(presentation)
}

fn source_payload_can_change_section(
    payload: &NormalizedAgentPlatformChangePayload,
    section: ManagedRuntimeProjectionSection,
) -> bool {
    match payload {
        NormalizedAgentPlatformChangePayload::SnapshotReplaced { .. } => {
            matches!(
                section,
                ManagedRuntimeProjectionSection::Snapshot
                    | ManagedRuntimeProjectionSection::ThreadName
            )
        }
        NormalizedAgentPlatformChangePayload::SourceChangeApplied {
            payload: source_payload,
            ..
        } => {
            let source_payload = source_state_payload(source_payload);
            matches!(
                (source_payload, section),
                (
                    AgentChangePayload::ThreadNameChanged { .. },
                    ManagedRuntimeProjectionSection::ThreadName
                ) | (
                    AgentChangePayload::LifecycleChanged { .. },
                    ManagedRuntimeProjectionSection::Lifecycle
                ) | (
                    AgentChangePayload::TurnChanged { .. },
                    ManagedRuntimeProjectionSection::Turns | ManagedRuntimeProjectionSection::Items
                ) | (
                    AgentChangePayload::ActiveTurnChanged { .. },
                    ManagedRuntimeProjectionSection::ActiveTurn
                ) | (
                    AgentChangePayload::ItemChanged { .. },
                    ManagedRuntimeProjectionSection::Turns | ManagedRuntimeProjectionSection::Items
                ) | (
                    AgentChangePayload::ItemTransitioned { .. },
                    ManagedRuntimeProjectionSection::Turns | ManagedRuntimeProjectionSection::Items
                ) | (
                    AgentChangePayload::InteractionChanged { .. },
                    ManagedRuntimeProjectionSection::Interactions
                ) | (
                    AgentChangePayload::SurfaceApplied { .. },
                    ManagedRuntimeProjectionSection::Surface
                )
            ) || (section == ManagedRuntimeProjectionSection::ConversationPresentation
                && source_observation_presentation(payload)
                    .is_some_and(|records| !records.is_empty()))
        }
    }
}

fn project_thread_name_source(
    source_info: &AgentSnapshotSource,
    source: &AgentSourceCoordinate,
) -> Result<ManagedRuntimeThreadNameSource, CompleteAgentRuntimeProjectionError> {
    Ok(ManagedRuntimeThreadNameSource {
        authority: project_authority(source_info.authority),
        fidelity: project_fidelity(source_info.fidelity),
        source_identity_digest: opaque_digest(source.as_str())?,
        source_revision_digest: source_info
            .source_revision
            .as_ref()
            .map(|revision| opaque_digest(revision.as_str()))
            .transpose()?,
        observed_at_ms: source_info.observed_at_ms,
    })
}

fn source_observation_delta(
    source: &AgentSourceCoordinate,
    change: &NormalizedAgentPlatformChange,
) -> Result<ManagedRuntimeChangeDelta, CompleteAgentRuntimeProjectionError> {
    let (source_revision, source_cursor) = match &change.payload {
        NormalizedAgentPlatformChangePayload::SnapshotReplaced {
            source_revision,
            source_cursor,
            ..
        } => (source_revision.as_ref(), source_cursor.as_ref()),
        NormalizedAgentPlatformChangePayload::SourceChangeApplied {
            source_revision,
            source_cursor,
            ..
        } => (source_revision.as_ref(), Some(source_cursor)),
    };
    Ok(ManagedRuntimeChangeDelta::SourceObservationApplied {
        source_change_sequence: change.sequence,
        source_projection_revision: RuntimeProjectionRevision(change.platform_revision),
        source_identity_digest: opaque_digest(source.as_str())?,
        observation_digest: serialized_digest(&change.payload)?,
        source_revision_digest: source_revision
            .map(|revision| opaque_digest(revision.as_str()))
            .transpose()?,
        source_cursor_digest: source_cursor
            .map(|cursor| opaque_digest(cursor.as_str()))
            .transpose()?,
        changed_sections: change.changed_sections.clone(),
    })
}

fn serialized_digest(
    value: &impl Serialize,
) -> Result<RuntimePayloadDigest, CompleteAgentRuntimeProjectionError> {
    let encoded = agentdash_agent_runtime_contract::canonical_json_bytes(value)
        .map_err(|_| CompleteAgentRuntimeProjectionError::ObservationEncoding)?;
    runtime_digest(&encoded)
}

fn opaque_digest(value: &str) -> Result<RuntimePayloadDigest, CompleteAgentRuntimeProjectionError> {
    runtime_digest(value.as_bytes())
}

fn runtime_digest(
    value: &[u8],
) -> Result<RuntimePayloadDigest, CompleteAgentRuntimeProjectionError> {
    RuntimePayloadDigest::new(format!("sha256:{:x}", Sha256::digest(value)))
        .map_err(|_| CompleteAgentRuntimeProjectionError::InvalidPayloadDigest)
}

fn project_turn(
    turn: &NormalizedAgentTurn,
    projection: &NormalizedAgentProjection,
    identities: &CompleteAgentRuntimeIdentityMap,
) -> Result<ManagedRuntimeTurn, CompleteAgentRuntimeProjectionError> {
    let runtime_turn_id = identities.runtime_turn_id(&turn.id)?;
    let item_ids = turn
        .item_ids
        .iter()
        .map(|item_id| {
            let item = projection.items.get(item_id).ok_or_else(|| {
                CompleteAgentRuntimeProjectionError::MissingIdentity {
                    kind: "normalized item",
                    source_identity: item_id.to_string(),
                }
            })?;
            identities.runtime_item_id(item_id, &item.turn_id)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ManagedRuntimeTurn {
        id: runtime_turn_id,
        status: project_entity_status(turn.status),
        item_ids,
    })
}

fn project_item(
    item: &AgentItemSnapshot,
    source_turn_id: &AgentTurnId,
    identities: &CompleteAgentRuntimeIdentityMap,
) -> Result<ManagedRuntimeItem, CompleteAgentRuntimeProjectionError> {
    Ok(ManagedRuntimeItem {
        id: identities.runtime_item_id(&item.id, source_turn_id)?,
        turn_id: identities.runtime_turn_id(source_turn_id)?,
        status: project_entity_status(item.status),
        presentation: project_item_presentation(&item.presentation)?,
    })
}

fn project_interaction(
    interaction: &AgentInteractionSnapshot,
    identities: &CompleteAgentRuntimeIdentityMap,
) -> Result<ManagedRuntimeInteraction, CompleteAgentRuntimeProjectionError> {
    Ok(ManagedRuntimeInteraction {
        id: identities.runtime_interaction_id(&interaction.id, &interaction.turn_id)?,
        turn_id: identities.runtime_turn_id(&interaction.turn_id)?,
        item_id: interaction
            .item_id
            .as_ref()
            .map(|item_id| identities.runtime_item_id(item_id, &interaction.turn_id))
            .transpose()?,
        request: project_interaction_request(&interaction.request),
        status: match interaction.status {
            agentdash_agent_service_api::AgentInteractionStatus::Pending => {
                ManagedRuntimeInteractionStatus::Pending
            }
            agentdash_agent_service_api::AgentInteractionStatus::Resolved => {
                ManagedRuntimeInteractionStatus::Resolved
            }
            agentdash_agent_service_api::AgentInteractionStatus::Cancelled => {
                ManagedRuntimeInteractionStatus::Cancelled
            }
            agentdash_agent_service_api::AgentInteractionStatus::Expired => {
                ManagedRuntimeInteractionStatus::Expired
            }
            agentdash_agent_service_api::AgentInteractionStatus::Lost => {
                ManagedRuntimeInteractionStatus::Lost
            }
        },
        resolution: interaction
            .resolution
            .as_ref()
            .map(project_interaction_resolution),
    })
}

fn project_item_presentation(
    presentation: &agentdash_agent_service_api::AgentItemPresentation,
) -> Result<ManagedRuntimeItemPresentation, CompleteAgentRuntimeProjectionError> {
    let projected = ManagedRuntimeItemPresentation::new(
        project_item_body(&presentation.body)?,
        presentation.started_at_ms.map(|value| value.0),
        presentation.updated_at_ms.map(|value| value.0),
        presentation
            .terminal
            .as_ref()
            .map(project_terminal_evidence),
    )
    .map_err(|_| CompleteAgentRuntimeProjectionError::InvalidPayloadDigest)?;
    if projected.body_digest.as_str() != presentation.body_digest.as_str()
        || projected.presentation_digest.as_str() != presentation.presentation_digest.as_str()
    {
        return Err(CompleteAgentRuntimeProjectionError::InvalidPayloadDigest);
    }
    Ok(projected)
}

fn project_item_transition(
    transition: &agentdash_agent_service_api::AgentItemTransition,
) -> Result<ManagedRuntimeItemTransition, CompleteAgentRuntimeProjectionError> {
    use agentdash_agent_service_api::AgentItemTransition as Source;
    Ok(match transition {
        Source::Started { presentation } => ManagedRuntimeItemTransition::Started {
            presentation: project_item_presentation(presentation)?,
        },
        Source::Updated {
            update,
            presentation,
        } => ManagedRuntimeItemTransition::Updated {
            update: project_item_update(update)?,
            presentation: project_item_presentation(presentation)?,
        },
        Source::Terminal { presentation } => ManagedRuntimeItemTransition::Terminal {
            presentation: project_item_presentation(presentation)?,
        },
    })
}

fn project_item_update(
    update: &agentdash_agent_service_api::AgentItemUpdate,
) -> Result<ManagedRuntimeItemUpdate, CompleteAgentRuntimeProjectionError> {
    use agentdash_agent_service_api::AgentItemUpdate as Source;
    Ok(match update {
        Source::TextAppended { text } => {
            ManagedRuntimeItemUpdate::TextAppended { text: text.clone() }
        }
        Source::ReasoningAppended { text } => {
            ManagedRuntimeItemUpdate::ReasoningAppended { text: text.clone() }
        }
        Source::ContentAppended { content } => ManagedRuntimeItemUpdate::ContentAppended {
            content: project_blocks(content)?,
        },
        Source::CommandOutputAppended { output } => {
            ManagedRuntimeItemUpdate::CommandOutputAppended {
                output: project_command_output(output),
            }
        }
        Source::PatchChanged { changes } => ManagedRuntimeItemUpdate::PatchChanged {
            changes: changes.iter().map(project_file_patch).collect(),
        },
        Source::PlanChanged { explanation, steps } => ManagedRuntimeItemUpdate::PlanChanged {
            explanation: explanation.clone(),
            steps: steps.iter().map(project_plan_step).collect(),
        },
        Source::ToolProgress { content } => ManagedRuntimeItemUpdate::ToolProgress {
            content: project_blocks(content)?,
        },
        Source::CollaborationChanged { status, result } => {
            ManagedRuntimeItemUpdate::CollaborationChanged {
                status: status.clone(),
                result: result.clone(),
            }
        }
        Source::BodyReplaced { body } => ManagedRuntimeItemUpdate::BodyReplaced {
            body: project_item_body(body)?,
        },
    })
}

fn project_content_block(
    content: &agentdash_agent_service_api::AgentContentBlock,
) -> Result<ManagedRuntimePresentationContentBlock, CompleteAgentRuntimeProjectionError> {
    Ok(match content {
        agentdash_agent_service_api::AgentContentBlock::Text { text } => {
            ManagedRuntimePresentationContentBlock::Text { text: text.clone() }
        }
        agentdash_agent_service_api::AgentContentBlock::Image {
            media_type,
            source,
            detail,
            digest,
        } => ManagedRuntimePresentationContentBlock::Image {
            media_type: media_type.clone(),
            source: source.clone(),
            detail: detail.clone(),
            digest: RuntimePayloadDigest::new(digest.as_str())
                .map_err(|_| CompleteAgentRuntimeProjectionError::InvalidPayloadDigest)?,
        },
        agentdash_agent_service_api::AgentContentBlock::LocalResource {
            path,
            media_type,
            digest,
        } => ManagedRuntimePresentationContentBlock::LocalResource {
            path: path.clone(),
            media_type: media_type.clone(),
            digest: digest
                .as_ref()
                .map(|digest| RuntimePayloadDigest::new(digest.as_str()))
                .transpose()
                .map_err(|_| CompleteAgentRuntimeProjectionError::InvalidPayloadDigest)?,
        },
        agentdash_agent_service_api::AgentContentBlock::ResourceLink {
            uri,
            title,
            media_type,
            digest,
        } => ManagedRuntimePresentationContentBlock::ResourceLink {
            uri: uri.clone(),
            title: title.clone(),
            media_type: media_type.clone(),
            digest: digest
                .as_ref()
                .map(|digest| RuntimePayloadDigest::new(digest.as_str()))
                .transpose()
                .map_err(|_| CompleteAgentRuntimeProjectionError::InvalidPayloadDigest)?,
        },
        agentdash_agent_service_api::AgentContentBlock::SkillReference { name, path } => {
            ManagedRuntimePresentationContentBlock::SkillReference {
                name: name.clone(),
                path: path.clone(),
            }
        }
        agentdash_agent_service_api::AgentContentBlock::Mention { label, reference } => {
            ManagedRuntimePresentationContentBlock::Mention {
                label: label.clone(),
                reference: reference.clone(),
            }
        }
        agentdash_agent_service_api::AgentContentBlock::Structured {
            schema,
            schema_version,
            value,
        } => ManagedRuntimePresentationContentBlock::Structured {
            schema: schema.clone(),
            schema_version: *schema_version,
            value: value.clone(),
        },
    })
}

fn project_blocks(
    content: &[agentdash_agent_service_api::AgentContentBlock],
) -> Result<Vec<ManagedRuntimePresentationContentBlock>, CompleteAgentRuntimeProjectionError> {
    content.iter().map(project_content_block).collect()
}

fn project_command_output(
    output: &agentdash_agent_service_api::AgentCommandOutput,
) -> agentdash_agent_runtime_contract::ManagedRuntimeCommandOutput {
    agentdash_agent_runtime_contract::ManagedRuntimeCommandOutput {
        stream: match output.stream {
            agentdash_agent_service_api::AgentCommandOutputStream::Stdout => {
                agentdash_agent_runtime_contract::ManagedRuntimeCommandOutputStream::Stdout
            }
            agentdash_agent_service_api::AgentCommandOutputStream::Stderr => {
                agentdash_agent_runtime_contract::ManagedRuntimeCommandOutputStream::Stderr
            }
            agentdash_agent_service_api::AgentCommandOutputStream::Combined => {
                agentdash_agent_runtime_contract::ManagedRuntimeCommandOutputStream::Combined
            }
        },
        text: output.text.clone(),
    }
}

fn project_file_patch(
    change: &agentdash_agent_service_api::AgentFilePatch,
) -> agentdash_agent_runtime_contract::ManagedRuntimeFilePatch {
    agentdash_agent_runtime_contract::ManagedRuntimeFilePatch {
        path: change.path.clone(),
        change_kind: match change.change_kind {
            agentdash_agent_service_api::AgentFileChangeKind::Add => {
                agentdash_agent_runtime_contract::ManagedRuntimeFileChangeKind::Add
            }
            agentdash_agent_service_api::AgentFileChangeKind::Update => {
                agentdash_agent_runtime_contract::ManagedRuntimeFileChangeKind::Update
            }
            agentdash_agent_service_api::AgentFileChangeKind::Delete => {
                agentdash_agent_runtime_contract::ManagedRuntimeFileChangeKind::Delete
            }
            agentdash_agent_service_api::AgentFileChangeKind::Move => {
                agentdash_agent_runtime_contract::ManagedRuntimeFileChangeKind::Move
            }
        },
        patch: change.patch.clone(),
        moved_to: change.moved_to.clone(),
    }
}

fn project_plan_step(
    step: &agentdash_agent_service_api::AgentPlanStep,
) -> agentdash_agent_runtime_contract::ManagedRuntimePlanStep {
    agentdash_agent_runtime_contract::ManagedRuntimePlanStep {
        id: step.id.clone(),
        text: step.text.clone(),
        status: match step.status {
            agentdash_agent_service_api::AgentPlanStepStatus::Pending => {
                agentdash_agent_runtime_contract::ManagedRuntimePlanStepStatus::Pending
            }
            agentdash_agent_service_api::AgentPlanStepStatus::InProgress => {
                agentdash_agent_runtime_contract::ManagedRuntimePlanStepStatus::InProgress
            }
            agentdash_agent_service_api::AgentPlanStepStatus::Completed => {
                agentdash_agent_runtime_contract::ManagedRuntimePlanStepStatus::Completed
            }
            agentdash_agent_service_api::AgentPlanStepStatus::Failed => {
                agentdash_agent_runtime_contract::ManagedRuntimePlanStepStatus::Failed
            }
        },
    }
}

fn project_item_body(
    body: &agentdash_agent_service_api::AgentItemBody,
) -> Result<ManagedRuntimeItemBody, CompleteAgentRuntimeProjectionError> {
    use agentdash_agent_service_api::AgentItemBody as Source;
    Ok(match body {
        Source::UserMessage { content } => ManagedRuntimeItemBody::UserMessage {
            content: project_blocks(content)?,
        },
        Source::HookPrompt {
            hook_point,
            content,
        } => ManagedRuntimeItemBody::HookPrompt {
            hook_point: hook_point.clone(),
            content: project_blocks(content)?,
        },
        Source::AgentMessage { content, phase } => ManagedRuntimeItemBody::AgentMessage {
            content: project_blocks(content)?,
            phase: phase.clone(),
        },
        Source::Reasoning { summary, content } => ManagedRuntimeItemBody::Reasoning {
            summary: project_blocks(summary)?,
            content: project_blocks(content)?,
        },
        Source::Plan { explanation, steps } => ManagedRuntimeItemBody::Plan {
            explanation: explanation.clone(),
            steps: steps.iter().map(project_plan_step).collect(),
        },
        Source::CommandExecution {
            command,
            cwd,
            output,
        } => ManagedRuntimeItemBody::CommandExecution {
            command: command.clone(),
            cwd: cwd.clone(),
            output: output.iter().map(project_command_output).collect(),
        },
        Source::FileChange { changes, output } => ManagedRuntimeItemBody::FileChange {
            changes: changes.iter().map(project_file_patch).collect(),
            output: project_blocks(output)?,
        },
        Source::FileRead {
            path,
            line_start,
            line_end,
            content,
        } => ManagedRuntimeItemBody::FileRead {
            path: path.clone(),
            line_start: *line_start,
            line_end: *line_end,
            content: project_blocks(content)?,
        },
        Source::FileSearch {
            mode,
            query,
            path,
            matches,
        } => ManagedRuntimeItemBody::FileSearch {
            mode: match mode {
                agentdash_agent_service_api::AgentFileSearchMode::Grep => {
                    agentdash_agent_runtime_contract::ManagedRuntimeFileSearchMode::Grep
                }
                agentdash_agent_service_api::AgentFileSearchMode::Glob => {
                    agentdash_agent_runtime_contract::ManagedRuntimeFileSearchMode::Glob
                }
            },
            query: query.clone(),
            path: path.clone(),
            matches: matches
                .iter()
                .map(
                    |item| agentdash_agent_runtime_contract::ManagedRuntimeFileSearchMatch {
                        path: item.path.clone(),
                        line: item.line,
                        column: item.column,
                        preview: item.preview.clone(),
                    },
                )
                .collect(),
        },
        Source::McpToolCall {
            server,
            tool,
            arguments,
            result,
            progress,
        } => ManagedRuntimeItemBody::McpToolCall {
            server: server.clone(),
            tool: tool.clone(),
            arguments: arguments.clone(),
            result: result.clone(),
            progress: project_blocks(progress)?,
        },
        Source::DynamicToolCall {
            namespace,
            tool,
            arguments,
            result,
            progress,
        } => ManagedRuntimeItemBody::DynamicToolCall {
            namespace: namespace.clone(),
            tool: tool.clone(),
            arguments: arguments.clone(),
            result: result.clone(),
            progress: project_blocks(progress)?,
        },
        Source::CollaborationToolCall {
            action,
            target,
            prompt,
            result,
        } => ManagedRuntimeItemBody::CollaborationToolCall {
            action: action.clone(),
            target: target.clone(),
            prompt: prompt.clone(),
            result: result.clone(),
        },
        Source::SubagentActivity {
            agent_id,
            task,
            status,
            result,
        } => ManagedRuntimeItemBody::SubagentActivity {
            agent_id: agent_id.clone(),
            task: task.clone(),
            status: status.clone(),
            result: project_blocks(result)?,
        },
        Source::WebSearch {
            action,
            query,
            url,
            results,
        } => ManagedRuntimeItemBody::WebSearch {
            action: action.clone(),
            query: query.clone(),
            url: url.clone(),
            results: project_blocks(results)?,
        },
        Source::ImageView { path, detail } => ManagedRuntimeItemBody::ImageView {
            path: path.clone(),
            detail: detail.clone(),
        },
        Source::ImageGeneration {
            prompt,
            revised_prompt,
            outputs,
        } => ManagedRuntimeItemBody::ImageGeneration {
            prompt: prompt.clone(),
            revised_prompt: revised_prompt.clone(),
            outputs: project_blocks(outputs)?,
        },
        Source::Sleep { duration_ms } => ManagedRuntimeItemBody::Sleep {
            duration_ms: agentdash_agent_runtime_contract::RuntimeU64(duration_ms.0),
        },
        Source::Review { findings, summary } => ManagedRuntimeItemBody::Review {
            findings: findings
                .iter()
                .map(
                    |finding| agentdash_agent_runtime_contract::ManagedRuntimeReviewFinding {
                        title: finding.title.clone(),
                        body: finding.body.clone(),
                        path: finding.path.clone(),
                        line: finding.line,
                        severity: finding.severity.clone(),
                    },
                )
                .collect(),
            summary: summary.clone(),
        },
        Source::TerminalControl {
            terminal_id,
            action,
            text,
        } => ManagedRuntimeItemBody::TerminalControl {
            terminal_id: terminal_id.clone(),
            action: action.clone(),
            text: text.clone(),
        },
        Source::ContextCompaction {
            summary,
            source_digest,
        } => ManagedRuntimeItemBody::ContextCompaction {
            summary: summary
                .as_ref()
                .map(|content| project_blocks(content))
                .transpose()?,
            source_digest: source_digest
                .as_ref()
                .map(|digest| RuntimePayloadDigest::new(digest.as_str()))
                .transpose()
                .map_err(|_| CompleteAgentRuntimeProjectionError::InvalidPayloadDigest)?,
        },
        Source::GenericToolActivity {
            name,
            arguments,
            result,
            progress,
        } => ManagedRuntimeItemBody::GenericToolActivity {
            name: name.clone(),
            arguments: arguments.clone(),
            result: result.clone(),
            progress: project_blocks(progress)?,
        },
        Source::Error {
            code,
            message,
            details,
        } => ManagedRuntimeItemBody::Error {
            code: code.clone(),
            message: message.clone(),
            details: details
                .as_ref()
                .map(|content| project_blocks(content))
                .transpose()?,
        },
    })
}

fn project_terminal_evidence(
    terminal: &agentdash_agent_service_api::AgentItemTerminalEvidence,
) -> ManagedRuntimeItemTerminalEvidence {
    ManagedRuntimeItemTerminalEvidence {
        outcome: match terminal.outcome {
            agentdash_agent_service_api::AgentTerminalStatus::Completed => {
                agentdash_agent_runtime_contract::ManagedRuntimeTerminalStatus::Completed
            }
            agentdash_agent_service_api::AgentTerminalStatus::Failed => {
                agentdash_agent_runtime_contract::ManagedRuntimeTerminalStatus::Failed
            }
            agentdash_agent_service_api::AgentTerminalStatus::Interrupted => {
                agentdash_agent_runtime_contract::ManagedRuntimeTerminalStatus::Interrupted
            }
            agentdash_agent_service_api::AgentTerminalStatus::Lost => {
                agentdash_agent_runtime_contract::ManagedRuntimeTerminalStatus::Lost
            }
        },
        completed_at_ms: terminal
            .completed_at_ms
            .map(|value| agentdash_agent_runtime_contract::RuntimeU64(value.0)),
        duration_ms: terminal
            .duration_ms
            .map(|value| agentdash_agent_runtime_contract::RuntimeU64(value.0)),
        process_exit: terminal.process_exit.as_ref().map(|value| {
            agentdash_agent_runtime_contract::ManagedRuntimeProcessExitEvidence {
                exit_code: value.exit_code,
                signal: value.signal.clone(),
                success: value.success,
            }
        }),
        error: terminal.error.as_ref().map(|value| {
            agentdash_agent_runtime_contract::ManagedRuntimePresentationError {
                code: value.code.clone(),
                message: value.message.clone(),
                retryable: value.retryable,
            }
        }),
    }
}

fn project_interaction_request(
    request: &agentdash_agent_service_api::AgentInteractionRequest,
) -> ManagedRuntimeInteractionRequest {
    match request {
        agentdash_agent_service_api::AgentInteractionRequest::Approval {
            prompt,
            reason,
            proposed_action,
        } => ManagedRuntimeInteractionRequest::Approval {
            prompt: prompt.clone(),
            reason: reason.clone(),
            proposed_action: proposed_action.clone(),
        },
        agentdash_agent_service_api::AgentInteractionRequest::UserInput { prompt, questions } => {
            ManagedRuntimeInteractionRequest::UserInput {
                prompt: prompt.clone(),
                questions: questions
                    .iter()
                    .map(|question| {
                        agentdash_agent_runtime_contract::ManagedRuntimeInteractionQuestion {
                            id: question.id.clone(),
                            prompt: question.prompt.clone(),
                            options: question.options.clone(),
                            allows_free_form: question.allows_free_form,
                        }
                    })
                    .collect(),
            }
        }
        agentdash_agent_service_api::AgentInteractionRequest::McpElicitation {
            server,
            prompt,
            schema,
        } => ManagedRuntimeInteractionRequest::McpElicitation {
            server: server.clone(),
            prompt: prompt.clone(),
            schema: schema.clone(),
        },
        agentdash_agent_service_api::AgentInteractionRequest::DynamicTool {
            namespace,
            tool,
            prompt,
            arguments,
        } => ManagedRuntimeInteractionRequest::DynamicTool {
            namespace: namespace.clone(),
            tool: tool.clone(),
            prompt: prompt.clone(),
            arguments: arguments.clone(),
        },
    }
}

fn project_interaction_resolution(
    resolution: &agentdash_agent_service_api::AgentInteractionResolution,
) -> ManagedRuntimeInteractionResolution {
    match resolution {
        agentdash_agent_service_api::AgentInteractionResolution::Approved => {
            ManagedRuntimeInteractionResolution::Approved
        }
        agentdash_agent_service_api::AgentInteractionResolution::Denied { reason } => {
            ManagedRuntimeInteractionResolution::Denied {
                reason: reason.clone(),
            }
        }
        agentdash_agent_service_api::AgentInteractionResolution::UserInput { answers } => {
            ManagedRuntimeInteractionResolution::UserInput {
                answers: answers.clone(),
            }
        }
        agentdash_agent_service_api::AgentInteractionResolution::McpElicitation { response } => {
            ManagedRuntimeInteractionResolution::McpElicitation {
                response: response.clone(),
            }
        }
        agentdash_agent_service_api::AgentInteractionResolution::DynamicToolResult { result } => {
            ManagedRuntimeInteractionResolution::DynamicToolResult {
                result: result.clone(),
            }
        }
        agentdash_agent_service_api::AgentInteractionResolution::Cancelled { reason } => {
            ManagedRuntimeInteractionResolution::Cancelled {
                reason: reason.clone(),
            }
        }
        agentdash_agent_service_api::AgentInteractionResolution::Expired => {
            ManagedRuntimeInteractionResolution::Expired
        }
        agentdash_agent_service_api::AgentInteractionResolution::Lost { reason } => {
            ManagedRuntimeInteractionResolution::Lost {
                reason: reason.clone(),
            }
        }
    }
}

fn project_lifecycle(status: AgentLifecycleStatus) -> ManagedRuntimeLifecycleStatus {
    match status {
        AgentLifecycleStatus::Creating => ManagedRuntimeLifecycleStatus::Provisioning,
        AgentLifecycleStatus::Active => ManagedRuntimeLifecycleStatus::Active,
        AgentLifecycleStatus::Suspended => ManagedRuntimeLifecycleStatus::Suspended,
        AgentLifecycleStatus::Closed => ManagedRuntimeLifecycleStatus::Closed,
        AgentLifecycleStatus::Lost => ManagedRuntimeLifecycleStatus::Lost,
    }
}

fn project_entity_status(status: AgentEntityStatus) -> ManagedRuntimeEntityStatus {
    match status {
        AgentEntityStatus::Accepted => ManagedRuntimeEntityStatus::Accepted,
        AgentEntityStatus::Running => ManagedRuntimeEntityStatus::Running,
        AgentEntityStatus::Completed => ManagedRuntimeEntityStatus::Completed,
        AgentEntityStatus::Failed => ManagedRuntimeEntityStatus::Failed,
        AgentEntityStatus::Interrupted => ManagedRuntimeEntityStatus::Interrupted,
        AgentEntityStatus::Lost => ManagedRuntimeEntityStatus::Lost,
    }
}

fn project_authority(authority: AgentSnapshotAuthority) -> ManagedRuntimeProjectionAuthority {
    match authority {
        AgentSnapshotAuthority::AgentAuthoritative => {
            ManagedRuntimeProjectionAuthority::SourceAuthoritative
        }
        AgentSnapshotAuthority::AgentObserved => ManagedRuntimeProjectionAuthority::SourceObserved,
        AgentSnapshotAuthority::Derived => ManagedRuntimeProjectionAuthority::RuntimeDerived,
    }
}

fn project_fidelity(
    fidelity: agentdash_agent_service_api::SemanticFidelity,
) -> ManagedRuntimeProjectionFidelity {
    match fidelity {
        agentdash_agent_service_api::SemanticFidelity::Unsupported => {
            ManagedRuntimeProjectionFidelity::Unsupported
        }
        agentdash_agent_service_api::SemanticFidelity::Observed => {
            ManagedRuntimeProjectionFidelity::Observed
        }
        agentdash_agent_service_api::SemanticFidelity::Approximation => {
            ManagedRuntimeProjectionFidelity::Approximation
        }
        agentdash_agent_service_api::SemanticFidelity::Exact => {
            ManagedRuntimeProjectionFidelity::Exact
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        sync::atomic::{AtomicUsize, Ordering},
    };

    use agentdash_agent_runtime_contract::ManagedRuntimeAvailabilityEvidence;
    use agentdash_agent_service_api::{
        AgentCapabilityProfile, AgentCommandEnvelope, AgentCommandOutput, AgentCommandOutputStream,
        AgentCommandReceipt, AgentConfigurationBoundary, AgentContentBlock, AgentEffectIdentity,
        AgentEffectInspection, AgentFileChangeKind, AgentFilePatch, AgentForkCapability,
        AgentItemBody, AgentItemPresentation, AgentItemTerminalEvidence, AgentItemTransition,
        AgentItemUpdate, AgentPayloadDigest, AgentPlanStep, AgentPlanStepStatus,
        AgentProfileDigest, AgentServiceDefinitionId, AgentServiceDescriptor,
        AgentServiceErrorCode, AgentSnapshotSource, AgentSourceRevision, AgentSurfaceProfile,
        AgentTerminalStatus, AgentTurnSnapshot, AppliedAgentSurfaceReceipt, ApplyBoundAgentSurface,
        CreateAgentCommand, ForkAgentCommand, ForkAgentReceipt, InitialContextAppliedEvidence,
        InitialContextProfile, ResumeAgentCommand, RevokeBoundAgentSurface, SemanticFidelity,
    };
    use async_trait::async_trait;
    use tokio::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct FixtureManagedRuntimeStateRepository {
        states: Mutex<BTreeMap<RuntimeThreadId, ManagedRuntimeStateSnapshot>>,
        fail_next_commit: AtomicUsize,
    }

    #[async_trait]
    impl ManagedRuntimeStateRepository for FixtureManagedRuntimeStateRepository {
        async fn load(
            &self,
            thread_id: &RuntimeThreadId,
        ) -> Result<ManagedRuntimeStateSnapshot, ManagedRuntimeStateStoreError> {
            Ok(self
                .states
                .lock()
                .await
                .get(thread_id)
                .cloned()
                .unwrap_or_default())
        }

        async fn commit(
            &self,
            commit: ManagedRuntimeStateCommit,
        ) -> Result<ManagedRuntimeStateSnapshot, ManagedRuntimeStateStoreError> {
            if self.fail_next_commit.swap(0, Ordering::SeqCst) != 0 {
                return Err(ManagedRuntimeStateStoreError::Persistence {
                    reason: "injected before atomic commit".to_owned(),
                });
            }
            let mut states = self.states.lock().await;
            let state = states.entry(commit.thread_id.clone()).or_default();
            crate::apply_managed_runtime_state_commit(state, commit)
        }
    }

    fn fixture_reconciler(
        repository: Arc<FixtureManagedRuntimeStateRepository>,
    ) -> CompleteAgentStateReconciler<FixtureManagedRuntimeStateRepository> {
        CompleteAgentStateReconciler::new(
            repository,
            identity_map(),
            projection_input(1, RuntimeChangeSequence(0)).command_availability,
        )
    }

    async fn fixture_state(
        repository: &FixtureManagedRuntimeStateRepository,
    ) -> ManagedRuntimeStateSnapshot {
        repository
            .load(identity_map().thread_id())
            .await
            .expect("load Runtime state")
    }

    async fn fixture_source_projection(
        repository: &FixtureManagedRuntimeStateRepository,
    ) -> NormalizedAgentProjection {
        fixture_state(repository)
            .await
            .facts
            .source_projection
            .expect("source projection")
    }

    fn assert_source_invariant(result: Result<(), ManagedRuntimeStateStoreError>) {
        assert!(matches!(
            result,
            Err(ManagedRuntimeStateStoreError::Invariant { .. })
        ));
    }

    #[tokio::test]
    async fn snapshot_is_normalized_and_reconnects_from_platform_changes() {
        let repository = Arc::new(FixtureManagedRuntimeStateRepository::default());
        let reconciler = fixture_reconciler(repository.clone());
        let source = source();
        reconciler
            .reconcile_snapshot(
                snapshot(1, AgentSnapshotAuthority::AgentAuthoritative),
                None,
            )
            .await
            .expect("snapshot");
        let page = AgentChangePage {
            source: source.clone(),
            changes: vec![agentdash_agent_service_api::AgentChange {
                cursor: cursor("cursor-1"),
                source_revision: Some(AgentSourceRevision::new("source-rev-2").expect("revision")),
                occurred_at_ms: 10,
                payload: AgentChangePayload::LifecycleChanged {
                    status: AgentLifecycleStatus::Closed,
                },
            }],
            next: Some(cursor("cursor-1")),
            gap: false,
        };
        reconciler
            .reconcile_change_page(
                &AgentChangesQuery {
                    source: source.clone(),
                    after: None,
                    limit: 10,
                },
                page,
            )
            .await
            .expect("changes");

        let projection = fixture_source_projection(&repository).await;
        assert_eq!(projection.lifecycle, AgentLifecycleStatus::Closed);
        assert_eq!(projection.turns.len(), 1);
        assert_eq!(projection.items.len(), 1);
        let reconnect = fixture_state(&repository).await;
        assert_eq!(reconnect.facts.source_changes.len(), 2);
        assert_eq!(
            reconnect
                .facts
                .source_changes
                .last()
                .expect("change")
                .sequence,
            2
        );
    }

    #[tokio::test]
    async fn thread_name_snapshot_set_duplicate_and_clear_have_one_semantic_delta_each() {
        let repository = Arc::new(FixtureManagedRuntimeStateRepository::default());
        let reconciler = fixture_reconciler(repository.clone());
        reconciler
            .reconcile_snapshot(snapshot_with_thread_name(1, Some(Some("初始标题"))), None)
            .await
            .expect("initial named snapshot");

        let set_cursor = cursor("name-cursor-1");
        reconciler
            .reconcile_change_page(
                &AgentChangesQuery {
                    source: source(),
                    after: None,
                    limit: 1,
                },
                AgentChangePage {
                    source: source(),
                    changes: vec![agentdash_agent_service_api::AgentChange {
                        cursor: set_cursor.clone(),
                        source_revision: Some(
                            AgentSourceRevision::new("source-name-rev-2").expect("revision"),
                        ),
                        occurred_at_ms: 2,
                        payload: AgentChangePayload::ThreadNameChanged {
                            thread_name: Some("更新标题".to_owned()),
                            source_info: AgentSnapshotSource {
                                authority: AgentSnapshotAuthority::AgentAuthoritative,
                                source_revision: Some(
                                    AgentSourceRevision::new("name-rev-2").expect("revision"),
                                ),
                                fidelity: SemanticFidelity::Exact,
                                observed_at_ms: 2,
                            },
                        },
                    }],
                    next: Some(set_cursor.clone()),
                    gap: false,
                },
            )
            .await
            .expect("set name");

        let duplicate_cursor = cursor("name-cursor-2");
        reconciler
            .reconcile_change_page(
                &AgentChangesQuery {
                    source: source(),
                    after: Some(set_cursor),
                    limit: 1,
                },
                AgentChangePage {
                    source: source(),
                    changes: vec![agentdash_agent_service_api::AgentChange {
                        cursor: duplicate_cursor.clone(),
                        source_revision: Some(
                            AgentSourceRevision::new("source-name-rev-3").expect("revision"),
                        ),
                        occurred_at_ms: 3,
                        payload: AgentChangePayload::ThreadNameChanged {
                            thread_name: Some("更新标题".to_owned()),
                            source_info: AgentSnapshotSource {
                                authority: AgentSnapshotAuthority::AgentAuthoritative,
                                source_revision: Some(
                                    AgentSourceRevision::new("name-rev-3").expect("revision"),
                                ),
                                fidelity: SemanticFidelity::Exact,
                                observed_at_ms: 3,
                            },
                        },
                    }],
                    next: Some(duplicate_cursor.clone()),
                    gap: false,
                },
            )
            .await
            .expect("duplicate name observation");

        let clear_cursor = cursor("name-cursor-3");
        reconciler
            .reconcile_change_page(
                &AgentChangesQuery {
                    source: source(),
                    after: Some(duplicate_cursor),
                    limit: 1,
                },
                AgentChangePage {
                    source: source(),
                    changes: vec![agentdash_agent_service_api::AgentChange {
                        cursor: clear_cursor.clone(),
                        source_revision: Some(
                            AgentSourceRevision::new("source-name-rev-4").expect("revision"),
                        ),
                        occurred_at_ms: 4,
                        payload: AgentChangePayload::ThreadNameChanged {
                            thread_name: None,
                            source_info: AgentSnapshotSource {
                                authority: AgentSnapshotAuthority::AgentAuthoritative,
                                source_revision: Some(
                                    AgentSourceRevision::new("name-rev-4").expect("revision"),
                                ),
                                fidelity: SemanticFidelity::Exact,
                                observed_at_ms: 4,
                            },
                        },
                    }],
                    next: Some(clear_cursor),
                    gap: false,
                },
            )
            .await
            .expect("clear name");

        let state = fixture_state(&repository).await;
        let projection = state.facts.projection.expect("managed projection");
        assert_eq!(projection.thread_name, None);
        assert!(projection.thread_name_source.is_some());
        let name_changes = state
            .facts
            .changes
            .iter()
            .filter_map(|change| match &change.delta {
                ManagedRuntimeChangeDelta::ThreadNameChanged { thread_name, .. } => {
                    Some(thread_name.clone())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            name_changes,
            vec![
                Some("初始标题".to_owned()),
                Some("更新标题".to_owned()),
                None
            ]
        );
        for pair in state.facts.changes.windows(2) {
            assert_eq!(pair[1].sequence.0, pair[0].sequence.0 + 1);
        }
    }

    #[tokio::test]
    async fn authoritative_initial_clear_is_a_typed_thread_name_projection() {
        let repository = Arc::new(FixtureManagedRuntimeStateRepository::default());
        fixture_reconciler(repository.clone())
            .reconcile_snapshot(snapshot_with_thread_name(1, Some(None)), None)
            .await
            .expect("initial authoritative clear");

        let state = fixture_state(&repository).await;
        let projection = state.facts.projection.expect("managed projection");
        assert_eq!(projection.thread_name, None);
        assert!(projection.thread_name_source.is_some());
        assert!(state.facts.changes.iter().any(|change| {
            matches!(
                &change.delta,
                ManagedRuntimeChangeDelta::ThreadNameChanged {
                    thread_name: None,
                    ..
                }
            )
        }));
    }

    #[tokio::test]
    async fn non_authoritative_initial_thread_name_is_rejected_before_any_runtime_mutation() {
        let repository = Arc::new(FixtureManagedRuntimeStateRepository::default());
        let reconciler = fixture_reconciler(repository.clone());
        let mut observed = snapshot_with_thread_name(1, Some(Some("观察标题")));
        observed
            .thread_name
            .as_mut()
            .expect("name section")
            .source_info
            .authority = AgentSnapshotAuthority::AgentObserved;

        assert!(matches!(
            reconciler.reconcile_snapshot(observed, None).await,
            Err(CompleteAgentStateError::InvalidSnapshot { .. })
        ));
        assert_eq!(
            fixture_state(&repository).await,
            ManagedRuntimeStateSnapshot::default()
        );
    }

    #[tokio::test]
    async fn lower_authority_or_fidelity_cannot_overwrite_an_authoritative_thread_name() {
        let repository = Arc::new(FixtureManagedRuntimeStateRepository::default());
        let reconciler = fixture_reconciler(repository.clone());
        reconciler
            .reconcile_snapshot(snapshot_with_thread_name(1, Some(Some("权威标题"))), None)
            .await
            .expect("authoritative initial name");
        let committed = fixture_state(&repository).await;

        for (cursor_value, authority, fidelity, thread_name) in [
            (
                "name-observed",
                AgentSnapshotAuthority::AgentObserved,
                SemanticFidelity::Exact,
                Some("观察覆盖"),
            ),
            (
                "name-approximate",
                AgentSnapshotAuthority::AgentAuthoritative,
                SemanticFidelity::Approximation,
                None,
            ),
        ] {
            let result = reconciler
                .reconcile_change_page(
                    &AgentChangesQuery {
                        source: source(),
                        after: None,
                        limit: 1,
                    },
                    AgentChangePage {
                        source: source(),
                        changes: vec![agentdash_agent_service_api::AgentChange {
                            cursor: cursor(cursor_value),
                            source_revision: Some(
                                AgentSourceRevision::new(format!("source-{cursor_value}"))
                                    .expect("revision"),
                            ),
                            occurred_at_ms: 5,
                            payload: AgentChangePayload::ThreadNameChanged {
                                thread_name: thread_name.map(str::to_owned),
                                source_info: AgentSnapshotSource {
                                    authority,
                                    source_revision: Some(
                                        AgentSourceRevision::new(format!("name-{cursor_value}"))
                                            .expect("revision"),
                                    ),
                                    fidelity,
                                    observed_at_ms: 5,
                                },
                            },
                        }],
                        next: Some(cursor(cursor_value)),
                        gap: false,
                    },
                )
                .await;
            assert!(matches!(
                result,
                Err(CompleteAgentStateError::InvalidChange { .. })
            ));
            assert_eq!(fixture_state(&repository).await, committed);
        }
    }

    #[tokio::test]
    async fn no_op_source_observation_keeps_causal_change_and_empty_sections() {
        let repository = Arc::new(FixtureManagedRuntimeStateRepository::default());
        let reconciler = fixture_reconciler(repository.clone());
        reconciler
            .reconcile_snapshot(
                snapshot(1, AgentSnapshotAuthority::AgentAuthoritative),
                None,
            )
            .await
            .expect("snapshot");
        reconciler
            .reconcile_change_page(
                &AgentChangesQuery {
                    source: source(),
                    after: None,
                    limit: 10,
                },
                AgentChangePage {
                    source: source(),
                    changes: vec![agentdash_agent_service_api::AgentChange {
                        cursor: cursor("cursor-no-op"),
                        source_revision: Some(
                            AgentSourceRevision::new("source-revision-no-op").expect("revision"),
                        ),
                        occurred_at_ms: 10,
                        payload: AgentChangePayload::LifecycleChanged {
                            status: AgentLifecycleStatus::Active,
                        },
                    }],
                    next: Some(cursor("cursor-no-op")),
                    gap: false,
                },
            )
            .await
            .expect("no-op observation");

        let state = fixture_state(&repository).await;
        let source_change = state.facts.source_changes.last().expect("source change");
        assert!(source_change.changed_sections.is_empty());
        let causal = state
            .facts
            .changes
            .iter()
            .find(|change| {
                matches!(
                    &change.delta,
                    ManagedRuntimeChangeDelta::SourceObservationApplied {
                        source_change_sequence: 2,
                        ..
                    }
                )
            })
            .expect("causal Runtime change");
        let ManagedRuntimeChangeDelta::SourceObservationApplied {
            changed_sections, ..
        } = &causal.delta
        else {
            unreachable!("filtered causal change");
        };
        assert!(changed_sections.is_empty());
        assert!(
            !state.facts.changes.iter().any(|change| {
                change.revision == RuntimeProjectionRevision(2)
                    && matches!(
                        change.delta,
                        ManagedRuntimeChangeDelta::SourceProjectionChanged { .. }
                    )
            }),
            "a no-op observation has no concrete projection delta"
        );
        assert_eq!(
            state
                .facts
                .outbox
                .iter()
                .filter(|entry| entry.change == *causal)
                .count(),
            1,
            "the causal Runtime change has exactly one matching outbox entry"
        );

        let current = repository
            .load(identity_map().thread_id())
            .await
            .expect("state")
            .facts;
        let mut fake = current.clone();
        let causal = fake
            .changes
            .iter()
            .find_map(|change| match &change.delta {
                ManagedRuntimeChangeDelta::SourceObservationApplied {
                    source_change_sequence: 2,
                    observation_digest,
                    ..
                } => Some(observation_digest.clone()),
                _ => None,
            })
            .expect("no-op causal digest");
        let delta = ManagedRuntimeSourceProjectionDelta::LifecycleChanged {
            lifecycle: ManagedRuntimeLifecycleStatus::Active,
        };
        fake.changes.push(ManagedRuntimePlatformChange {
            thread_id: identity_map().thread_id().clone(),
            sequence: RuntimeChangeSequence(
                fake.changes.last().expect("change head").sequence.0 + 1,
            ),
            revision: RuntimeProjectionRevision(2),
            delta: ManagedRuntimeChangeDelta::SourceProjectionChanged {
                source_change_sequence: 2,
                source_projection_revision: RuntimeProjectionRevision(2),
                observation_digest: causal,
                section: ManagedRuntimeProjectionSection::Lifecycle,
                section_digest: serialized_digest(&delta).expect("section digest"),
                delta,
            },
        });
        assert_source_invariant(validate_complete_agent_source_facts(&current, &fake));
    }

    #[tokio::test]
    async fn source_observation_and_projection_deltas_reject_incomplete_or_wrong_evidence() {
        let repository = Arc::new(FixtureManagedRuntimeStateRepository::default());
        let reconciler = fixture_reconciler(repository.clone());
        reconciler
            .reconcile_snapshot(
                snapshot(1, AgentSnapshotAuthority::AgentAuthoritative),
                None,
            )
            .await
            .expect("snapshot");
        let current = fixture_state(&repository).await.facts;
        reconciler
            .reconcile_change_page(
                &AgentChangesQuery {
                    source: source(),
                    after: None,
                    limit: 10,
                },
                AgentChangePage {
                    source: source(),
                    changes: vec![agentdash_agent_service_api::AgentChange {
                        cursor: cursor("cursor-2"),
                        source_revision: Some(
                            AgentSourceRevision::new("source-revision-2").expect("revision"),
                        ),
                        occurred_at_ms: 10,
                        payload: AgentChangePayload::LifecycleChanged {
                            status: AgentLifecycleStatus::Closed,
                        },
                    }],
                    next: Some(cursor("cursor-2")),
                    gap: false,
                },
            )
            .await
            .expect("source change");
        let candidate = fixture_state(&repository).await.facts;
        validate_complete_agent_source_facts(&current, &candidate).expect("valid causal evidence");
        let causal_index = candidate
            .changes
            .iter()
            .position(|change| {
                matches!(
                    change.delta,
                    ManagedRuntimeChangeDelta::SourceObservationApplied {
                        source_change_sequence: 2,
                        ..
                    }
                )
            })
            .expect("causal change");
        let concrete_index = candidate
            .changes
            .iter()
            .position(|change| {
                matches!(
                    change.delta,
                    ManagedRuntimeChangeDelta::SourceProjectionChanged {
                        source_change_sequence: 2,
                        section: ManagedRuntimeProjectionSection::Lifecycle,
                        ..
                    }
                )
            })
            .expect("concrete source projection change");

        let mut omitted = candidate.clone();
        omitted.changes.remove(causal_index);
        assert_source_invariant(validate_complete_agent_source_facts(&current, &omitted));

        let mut duplicated = candidate.clone();
        duplicated
            .changes
            .push(candidate.changes[causal_index].clone());
        assert_source_invariant(validate_complete_agent_source_facts(&current, &duplicated));

        let mut wrong_revision = candidate.clone();
        let ManagedRuntimeChangeDelta::SourceObservationApplied {
            source_projection_revision,
            ..
        } = &mut wrong_revision.changes[causal_index].delta
        else {
            unreachable!("causal change");
        };
        *source_projection_revision = RuntimeProjectionRevision(99);
        assert_source_invariant(validate_complete_agent_source_facts(
            &current,
            &wrong_revision,
        ));

        let mut wrong_digest = candidate.clone();
        let ManagedRuntimeChangeDelta::SourceObservationApplied {
            observation_digest, ..
        } = &mut wrong_digest.changes[causal_index].delta
        else {
            unreachable!("causal change");
        };
        *observation_digest = RuntimePayloadDigest::new("sha256:wrong").expect("digest");
        assert_source_invariant(validate_complete_agent_source_facts(
            &current,
            &wrong_digest,
        ));

        let mut wrong_source = candidate.clone();
        let ManagedRuntimeChangeDelta::SourceObservationApplied {
            source_identity_digest,
            ..
        } = &mut wrong_source.changes[causal_index].delta
        else {
            unreachable!("causal change");
        };
        *source_identity_digest = RuntimePayloadDigest::new("sha256:wrong-source").expect("digest");
        assert_source_invariant(validate_complete_agent_source_facts(
            &current,
            &wrong_source,
        ));

        let mut wrong_source_revision = candidate.clone();
        let ManagedRuntimeChangeDelta::SourceObservationApplied {
            source_revision_digest,
            ..
        } = &mut wrong_source_revision.changes[causal_index].delta
        else {
            unreachable!("causal change");
        };
        *source_revision_digest =
            Some(RuntimePayloadDigest::new("sha256:wrong-revision").expect("digest"));
        assert_source_invariant(validate_complete_agent_source_facts(
            &current,
            &wrong_source_revision,
        ));

        let mut wrong_sections = candidate.clone();
        let ManagedRuntimeChangeDelta::SourceObservationApplied {
            changed_sections, ..
        } = &mut wrong_sections.changes[causal_index].delta
        else {
            unreachable!("causal change");
        };
        changed_sections.clear();
        assert_source_invariant(validate_complete_agent_source_facts(
            &current,
            &wrong_sections,
        ));

        let mut omitted_concrete = candidate.clone();
        omitted_concrete.changes.remove(concrete_index);
        assert_source_invariant(validate_complete_agent_source_facts(
            &current,
            &omitted_concrete,
        ));

        let mut duplicated_concrete = candidate.clone();
        duplicated_concrete
            .changes
            .push(candidate.changes[concrete_index].clone());
        assert_source_invariant(validate_complete_agent_source_facts(
            &current,
            &duplicated_concrete,
        ));

        let mut wrong_concrete_section = candidate.clone();
        let ManagedRuntimeChangeDelta::SourceProjectionChanged { section, .. } =
            &mut wrong_concrete_section.changes[concrete_index].delta
        else {
            unreachable!("concrete change");
        };
        *section = ManagedRuntimeProjectionSection::Items;
        assert_source_invariant(validate_complete_agent_source_facts(
            &current,
            &wrong_concrete_section,
        ));

        let mut wrong_concrete_sequence = candidate.clone();
        let ManagedRuntimeChangeDelta::SourceProjectionChanged {
            source_change_sequence,
            ..
        } = &mut wrong_concrete_sequence.changes[concrete_index].delta
        else {
            unreachable!("concrete change");
        };
        *source_change_sequence = 99;
        assert_source_invariant(validate_complete_agent_source_facts(
            &current,
            &wrong_concrete_sequence,
        ));

        let mut wrong_concrete_revision = candidate.clone();
        let ManagedRuntimeChangeDelta::SourceProjectionChanged {
            source_projection_revision,
            ..
        } = &mut wrong_concrete_revision.changes[concrete_index].delta
        else {
            unreachable!("concrete change");
        };
        *source_projection_revision = RuntimeProjectionRevision(99);
        assert_source_invariant(validate_complete_agent_source_facts(
            &current,
            &wrong_concrete_revision,
        ));

        let mut wrong_change_revision = candidate.clone();
        wrong_change_revision.changes[concrete_index].revision = RuntimeProjectionRevision(99);
        assert_source_invariant(validate_complete_agent_source_facts(
            &current,
            &wrong_change_revision,
        ));

        let mut wrong_concrete_payload = candidate.clone();
        let ManagedRuntimeChangeDelta::SourceProjectionChanged {
            section_digest,
            delta,
            ..
        } = &mut wrong_concrete_payload.changes[concrete_index].delta
        else {
            unreachable!("concrete change");
        };
        *delta = ManagedRuntimeSourceProjectionDelta::LifecycleChanged {
            lifecycle: ManagedRuntimeLifecycleStatus::Active,
        };
        *section_digest = serialized_digest(delta).expect("tampered section digest");
        assert_source_invariant(validate_complete_agent_source_facts(
            &current,
            &wrong_concrete_payload,
        ));
    }

    #[tokio::test]
    async fn source_observation_commit_failure_leaves_no_normalized_or_platform_gap() {
        let repository = Arc::new(FixtureManagedRuntimeStateRepository::default());
        repository.fail_next_commit.store(1, Ordering::SeqCst);
        let reconciler = fixture_reconciler(repository.clone());

        assert!(matches!(
            reconciler
                .reconcile_snapshot(
                    snapshot(1, AgentSnapshotAuthority::AgentAuthoritative),
                    None,
                )
                .await,
            Err(CompleteAgentStateError::Store(
                ManagedRuntimeStateStoreError::Persistence { .. }
            ))
        ));
        assert_eq!(
            fixture_state(&repository).await,
            ManagedRuntimeStateSnapshot::default()
        );
    }

    #[tokio::test]
    async fn stale_source_candidate_cannot_commit_without_its_platform_change() {
        let repository = Arc::new(FixtureManagedRuntimeStateRepository::default());
        let reconciler = fixture_reconciler(repository.clone());
        let base = fixture_state(&repository).await;
        let first = prepare_snapshot_observation(
            None,
            snapshot(1, AgentSnapshotAuthority::AgentObserved),
            None,
            1,
        )
        .expect("prepare first")
        .expect("first candidate");
        let stale = prepare_snapshot_observation(
            None,
            snapshot(2, AgentSnapshotAuthority::AgentAuthoritative),
            None,
            1,
        )
        .expect("prepare stale")
        .expect("stale candidate");

        reconciler
            .commit_observation(base.clone(), first)
            .await
            .expect("commit first");
        assert!(matches!(
            reconciler.commit_observation(base, stale).await,
            Err(CompleteAgentStateError::Store(
                ManagedRuntimeStateStoreError::Conflict
            ))
        ));

        let committed = fixture_state(&repository).await;
        assert_eq!(
            committed
                .facts
                .source_projection
                .as_ref()
                .expect("source")
                .snapshot_revision,
            AgentSnapshotRevision(1)
        );
        assert_eq!(committed.facts.changes.len(), committed.facts.outbox.len());
        assert_eq!(
            committed
                .facts
                .projection
                .as_ref()
                .expect("managed")
                .latest_change_sequence
                .0 as usize,
            committed.facts.changes.len()
        );
    }

    #[tokio::test]
    async fn restarted_reconciler_continues_from_one_atomic_source_and_runtime_state() {
        let repository = Arc::new(FixtureManagedRuntimeStateRepository::default());
        fixture_reconciler(repository.clone())
            .reconcile_snapshot(
                snapshot(1, AgentSnapshotAuthority::AgentObserved),
                Some(cursor("cursor-0")),
            )
            .await
            .expect("initial snapshot");
        let restarted = fixture_reconciler(repository.clone());
        restarted
            .reconcile_change_page(
                &AgentChangesQuery {
                    source: source(),
                    after: Some(cursor("cursor-0")),
                    limit: 10,
                },
                AgentChangePage {
                    source: source(),
                    changes: vec![agentdash_agent_service_api::AgentChange {
                        cursor: cursor("cursor-1"),
                        source_revision: Some(
                            AgentSourceRevision::new("source-rev-2").expect("revision"),
                        ),
                        occurred_at_ms: 20,
                        payload: AgentChangePayload::LifecycleChanged {
                            status: AgentLifecycleStatus::Closed,
                        },
                    }],
                    next: Some(cursor("cursor-1")),
                    gap: false,
                },
            )
            .await
            .expect("reconcile after restart");

        let committed = fixture_state(&repository).await;
        assert_eq!(
            committed
                .facts
                .source_projection
                .as_ref()
                .expect("source")
                .lifecycle,
            AgentLifecycleStatus::Closed
        );
        assert_eq!(
            committed
                .facts
                .projection
                .as_ref()
                .expect("managed")
                .lifecycle,
            ManagedRuntimeLifecycleStatus::Closed
        );
        assert_eq!(committed.facts.changes.len(), committed.facts.outbox.len());
        let encoded = crate::encode_managed_runtime_state_snapshot(&committed)
            .expect("encode source projection state");
        let decoded =
            crate::decode_managed_runtime_state_snapshot(identity_map().thread_id(), encoded)
                .expect("decode source projection state");
        assert_eq!(decoded, committed);
        assert!(decoded.facts.source_projection.is_some());
        assert!(decoded.facts.source_identities.is_some());
        assert_eq!(decoded.facts.source_changes.len(), 2);
    }

    #[tokio::test]
    async fn managed_snapshot_uses_explicit_runtime_ids_and_committed_availability() {
        let repository = Arc::new(FixtureManagedRuntimeStateRepository::default());
        let reconciler = fixture_reconciler(repository.clone());
        reconciler
            .reconcile_snapshot(
                snapshot(1, AgentSnapshotAuthority::AgentAuthoritative),
                None,
            )
            .await
            .expect("snapshot");
        let projection = fixture_source_projection(&repository).await;
        let mut identities = identity_map();
        let snapshot = project_managed_runtime_snapshot(
            &projection,
            &identities,
            projection_input(1, RuntimeChangeSequence(1)),
        )
        .expect("managed snapshot");

        assert_eq!(snapshot.thread_id.as_str(), "runtime-thread-7");
        assert_eq!(snapshot.turns[0].id.as_str(), "runtime-turn-11");
        assert_eq!(snapshot.items[0].id.as_str(), "runtime-item-13");
        assert_eq!(
            snapshot.command_availability[&ManagedRuntimeCommandKind::RequestCompaction]
                .evidence()
                .decided_at_revision,
            RuntimeProjectionRevision(1)
        );
        let json = serde_json::to_string(&snapshot).expect("serialize managed snapshot");
        for source_coordinate in ["source-1", "turn-1", "item-1"] {
            assert!(
                !json.contains(&format!("\"{source_coordinate}\"")),
                "source coordinate leaked: {source_coordinate}"
            );
        }

        identities
            .bind_turn(
                AgentTurnId::new("turn-1").expect("turn"),
                RuntimeTurnId::new("runtime-turn-11").expect("runtime turn"),
            )
            .expect("idempotent identity bind");
    }

    #[tokio::test]
    async fn missing_or_drifting_identity_is_rejected() {
        let repository = Arc::new(FixtureManagedRuntimeStateRepository::default());
        let reconciler = fixture_reconciler(repository.clone());
        reconciler
            .reconcile_snapshot(
                snapshot(1, AgentSnapshotAuthority::AgentAuthoritative),
                None,
            )
            .await
            .expect("snapshot");
        let projection = fixture_source_projection(&repository).await;
        let mut identities = CompleteAgentRuntimeIdentityMap::new(
            source(),
            RuntimeThreadId::new("runtime-thread-7").expect("runtime thread"),
        );
        identities
            .bind_turn(
                AgentTurnId::new("turn-1").expect("turn"),
                RuntimeTurnId::new("runtime-turn-11").expect("runtime turn"),
            )
            .expect("turn identity");

        assert!(matches!(
            project_managed_runtime_snapshot(
                &projection,
                &identities,
                projection_input(1, RuntimeChangeSequence(1)),
            ),
            Err(CompleteAgentRuntimeProjectionError::MissingIdentity { kind: "item", .. })
        ));
        assert!(matches!(
            identities.bind_turn(
                AgentTurnId::new("turn-1").expect("turn"),
                RuntimeTurnId::new("runtime-turn-drift").expect("runtime turn"),
            ),
            Err(CompleteAgentRuntimeProjectionError::IdentityDrift { kind: "turn", .. })
        ));
    }

    #[tokio::test]
    async fn availability_must_be_complete_and_committed_at_snapshot_revision() {
        let repository = Arc::new(FixtureManagedRuntimeStateRepository::default());
        let reconciler = fixture_reconciler(repository.clone());
        reconciler
            .reconcile_snapshot(
                snapshot(1, AgentSnapshotAuthority::AgentAuthoritative),
                None,
            )
            .await
            .expect("snapshot");
        let projection = fixture_source_projection(&repository).await;
        let mut input = projection_input(1, RuntimeChangeSequence(1));
        input.command_availability.insert(
            ManagedRuntimeCommandKind::SubmitInput,
            ManagedRuntimeCommandAvailability::Available {
                evidence: availability_evidence(2),
            },
        );

        assert!(matches!(
            project_managed_runtime_snapshot(&projection, &identity_map(), input),
            Err(
                CompleteAgentRuntimeProjectionError::AvailabilityRevisionMismatch {
                    command: ManagedRuntimeCommandKind::SubmitInput,
                    ..
                }
            )
        ));
    }

    #[test]
    fn managed_change_page_maps_runtime_ids_and_exposes_typed_retention_gap() {
        let identities = identity_map();
        let page = NormalizedAgentChangePage {
            source: source(),
            requested_after_sequence: 1,
            earliest_available_sequence: Some(5),
            latest_available_sequence: Some(9),
            changes: vec![NormalizedAgentPlatformChange {
                sequence: 5,
                platform_revision: 3,
                payload: NormalizedAgentPlatformChangePayload::SourceChangeApplied {
                    source_cursor: cursor("source-cursor-5"),
                    source_revision: None,
                    payload: Box::new(AgentChangePayload::ItemChanged {
                        turn_id: AgentTurnId::new("turn-1").expect("turn"),
                        item: AgentItemSnapshot {
                            id: AgentItemId::new("item-1").expect("item"),
                            status: AgentEntityStatus::Running,
                            presentation: AgentItemPresentation::new(
                                AgentItemBody::ContextCompaction {
                                    summary: None,
                                    source_digest: None,
                                },
                                None,
                                None,
                                None,
                            )
                            .expect("presentation"),
                        },
                    }),
                },
                changed_sections: BTreeSet::from([ManagedRuntimeProjectionSection::Items]),
            }],
            next_sequence: 5,
        };

        let projected =
            project_managed_runtime_change_page(&page, &identities, RuntimeProjectionRevision(3))
                .expect("managed change page");
        assert_eq!(
            projected.gap,
            Some(ManagedRuntimeChangeGap {
                requested_after: Some(RuntimeChangeSequence(1)),
                earliest_available: RuntimeChangeSequence(5),
                latest_available: RuntimeChangeSequence(9),
                snapshot_revision: RuntimeProjectionRevision(3),
            })
        );
        let ManagedRuntimeChangeDelta::SourceObservationApplied {
            source_change_sequence,
            source_projection_revision,
            changed_sections,
            ..
        } = &projected.changes[0].delta
        else {
            panic!("source observation delta");
        };
        assert_eq!(*source_change_sequence, 5);
        assert_eq!(*source_projection_revision, RuntimeProjectionRevision(3));
        assert_eq!(
            changed_sections,
            &BTreeSet::from([ManagedRuntimeProjectionSection::Items])
        );
    }

    #[tokio::test]
    async fn active_turn_change_is_applied_as_an_explicit_source_fact() {
        let repository = Arc::new(FixtureManagedRuntimeStateRepository::default());
        let reconciler = fixture_reconciler(repository.clone());
        let source = source();
        reconciler
            .reconcile_snapshot(
                snapshot(1, AgentSnapshotAuthority::AgentAuthoritative),
                None,
            )
            .await
            .expect("snapshot");

        reconciler
            .reconcile_change_page(
                &AgentChangesQuery {
                    source: source.clone(),
                    after: None,
                    limit: 10,
                },
                AgentChangePage {
                    source: source.clone(),
                    changes: vec![
                        agentdash_agent_service_api::AgentChange {
                            cursor: cursor("cursor-1"),
                            source_revision: None,
                            occurred_at_ms: 10,
                            payload: AgentChangePayload::TurnChanged {
                                turn: AgentTurnSnapshot {
                                    id: AgentTurnId::new("turn-1").expect("turn"),
                                    status: AgentEntityStatus::Completed,
                                    items: Vec::new(),
                                },
                            },
                        },
                        agentdash_agent_service_api::AgentChange {
                            cursor: cursor("cursor-2"),
                            source_revision: None,
                            occurred_at_ms: 11,
                            payload: AgentChangePayload::ActiveTurnChanged {
                                active_turn_id: None,
                            },
                        },
                    ],
                    next: Some(cursor("cursor-2")),
                    gap: false,
                },
            )
            .await
            .expect("changes");

        let projection = fixture_source_projection(&repository).await;
        assert_eq!(projection.active_turn_id, None);
        assert_eq!(
            projection
                .turns
                .get(&AgentTurnId::new("turn-1").expect("turn"))
                .expect("normalized turn")
                .status,
            AgentEntityStatus::Completed
        );
        let state = fixture_state(&repository).await;
        let turn_sections = state
            .facts
            .changes
            .iter()
            .filter_map(|change| match &change.delta {
                ManagedRuntimeChangeDelta::SourceProjectionChanged {
                    source_change_sequence: 2,
                    section,
                    delta,
                    ..
                } => Some((*section, delta)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(turn_sections.len(), 2);
        assert!(turn_sections.iter().any(|(section, delta)| {
            *section == ManagedRuntimeProjectionSection::Turns
                && matches!(
                    delta,
                    ManagedRuntimeSourceProjectionDelta::TurnsChanged { .. }
                )
        }));
        assert!(turn_sections.iter().any(|(section, delta)| {
            *section == ManagedRuntimeProjectionSection::Items
                && matches!(
                    delta,
                    ManagedRuntimeSourceProjectionDelta::ItemsChanged { .. }
                )
        }));
        assert_eq!(
            state
                .facts
                .changes
                .iter()
                .filter(|change| {
                    matches!(
                        change.delta,
                        ManagedRuntimeChangeDelta::SourceProjectionChanged {
                            source_change_sequence: 3,
                            section: ManagedRuntimeProjectionSection::ActiveTurn,
                            ..
                        }
                    )
                })
                .count(),
            1
        );
    }

    #[test]
    fn every_item_update_projects_into_the_independent_runtime_vocabulary() {
        let content = || AgentContentBlock::Text {
            text: "delta".to_owned(),
        };
        let updates = vec![
            AgentItemUpdate::TextAppended {
                text: "text".to_owned(),
            },
            AgentItemUpdate::ReasoningAppended {
                text: "reasoning".to_owned(),
            },
            AgentItemUpdate::ContentAppended {
                content: vec![content()],
            },
            AgentItemUpdate::CommandOutputAppended {
                output: AgentCommandOutput {
                    stream: AgentCommandOutputStream::Stdout,
                    text: "output".to_owned(),
                },
            },
            AgentItemUpdate::PatchChanged {
                changes: vec![AgentFilePatch {
                    path: "src/lib.rs".to_owned(),
                    change_kind: AgentFileChangeKind::Update,
                    patch: "@@".to_owned(),
                    moved_to: None,
                }],
            },
            AgentItemUpdate::PlanChanged {
                explanation: Some("plan".to_owned()),
                steps: vec![AgentPlanStep {
                    id: Some("step-1".to_owned()),
                    text: "work".to_owned(),
                    status: AgentPlanStepStatus::InProgress,
                }],
            },
            AgentItemUpdate::ToolProgress {
                content: vec![content()],
            },
            AgentItemUpdate::CollaborationChanged {
                status: "completed".to_owned(),
                result: Some(serde_json::json!({"ok": true})),
            },
            AgentItemUpdate::BodyReplaced {
                body: AgentItemBody::ContextCompaction {
                    summary: Some(vec![content()]),
                    source_digest: Some(
                        AgentPayloadDigest::new("sha256:source").expect("source digest"),
                    ),
                },
            },
        ];

        for update in updates {
            let source_kind = serde_json::to_value(&update).expect("source update")["kind"].clone();
            let projected = project_item_update(&update).expect("Runtime item update");
            let runtime_kind =
                serde_json::to_value(&projected).expect("Runtime update")["kind"].clone();
            assert_eq!(runtime_kind, source_kind);
        }
    }

    #[tokio::test]
    async fn item_transitions_commit_as_typed_runtime_deltas_and_fold_to_the_snapshot() {
        for terminal_status in [
            AgentTerminalStatus::Completed,
            AgentTerminalStatus::Failed,
            AgentTerminalStatus::Interrupted,
            AgentTerminalStatus::Lost,
        ] {
            let repository = Arc::new(FixtureManagedRuntimeStateRepository::default());
            let reconciler = fixture_reconciler(repository.clone());
            let mut initial = snapshot(1, AgentSnapshotAuthority::AgentAuthoritative);
            initial.turns[0].items.clear();
            reconciler
                .reconcile_snapshot(initial, None)
                .await
                .expect("initial item-free snapshot");

            let running = AgentItemPresentation::new(
                AgentItemBody::ContextCompaction {
                    summary: None,
                    source_digest: None,
                },
                Some(u64::MAX - 1),
                Some(u64::MAX - 1),
                None,
            )
            .expect("running presentation");
            let updated = AgentItemPresentation::new(
                AgentItemBody::ContextCompaction {
                    summary: Some(vec![AgentContentBlock::Text {
                        text: "compact summary".to_owned(),
                    }]),
                    source_digest: Some(
                        AgentPayloadDigest::new("sha256:compaction-source").expect("source digest"),
                    ),
                },
                Some(u64::MAX - 1),
                Some(u64::MAX - 1),
                None,
            )
            .expect("updated presentation");
            let terminal = AgentItemPresentation::new(
                updated.body.clone(),
                Some(u64::MAX - 1),
                Some(u64::MAX),
                Some(AgentItemTerminalEvidence {
                    outcome: terminal_status,
                    completed_at_ms: Some(agentdash_agent_service_api::AgentServiceU64(u64::MAX)),
                    duration_ms: Some(agentdash_agent_service_api::AgentServiceU64(1)),
                    process_exit: None,
                    error: None,
                }),
            )
            .expect("terminal presentation");
            let item_id = AgentItemId::new("item-1").expect("item");
            let turn_id = AgentTurnId::new("turn-1").expect("turn");
            let changes = vec![
                AgentItemTransition::Started {
                    presentation: running,
                },
                AgentItemTransition::Updated {
                    update: AgentItemUpdate::BodyReplaced {
                        body: updated.body.clone(),
                    },
                    presentation: updated,
                },
                AgentItemTransition::Terminal {
                    presentation: terminal,
                },
            ]
            .into_iter()
            .enumerate()
            .map(
                |(index, transition)| agentdash_agent_service_api::AgentChange {
                    cursor: cursor(&format!("item-transition-{terminal_status:?}-{index}")),
                    source_revision: None,
                    occurred_at_ms: index as u64 + 10,
                    payload: AgentChangePayload::ItemTransitioned {
                        turn_id: turn_id.clone(),
                        item_id: item_id.clone(),
                        transition,
                    },
                },
            )
            .collect::<Vec<_>>();
            let next = changes.last().expect("transition head").cursor.clone();

            reconciler
                .reconcile_change_page(
                    &AgentChangesQuery {
                        source: source(),
                        after: None,
                        limit: 3,
                    },
                    AgentChangePage {
                        source: source(),
                        changes,
                        next: Some(next),
                        gap: false,
                    },
                )
                .await
                .expect("item transition page");

            let state = fixture_state(&repository).await;
            let typed_transitions = state
                .facts
                .changes
                .iter()
                .filter_map(|change| match &change.delta {
                    ManagedRuntimeChangeDelta::SourceProjectionChanged {
                        section: ManagedRuntimeProjectionSection::Items,
                        delta:
                            ManagedRuntimeSourceProjectionDelta::ItemTransitioned {
                                item_id,
                                transition,
                            },
                        ..
                    } => Some((item_id, transition)),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(typed_transitions.len(), 3);
            let typed_transition_changes = state
                .facts
                .changes
                .iter()
                .filter(|change| {
                    matches!(
                        change.delta,
                        ManagedRuntimeChangeDelta::SourceProjectionChanged {
                            section: ManagedRuntimeProjectionSection::Items,
                            delta: ManagedRuntimeSourceProjectionDelta::ItemTransitioned { .. },
                            ..
                        }
                    )
                })
                .collect::<Vec<_>>();
            assert_eq!(typed_transition_changes.len(), 3);
            for change in typed_transition_changes {
                let outbox = state
                    .facts
                    .outbox
                    .iter()
                    .find(|entry| entry.sequence == change.sequence)
                    .expect("typed transition outbox");
                assert_eq!(&outbox.change, change);
            }
            assert_eq!(typed_transitions[0].0.as_str(), "runtime-item-13");
            assert!(matches!(
                typed_transitions[0].1,
                ManagedRuntimeItemTransition::Started { .. }
            ));
            assert!(matches!(
                typed_transitions[1].1,
                ManagedRuntimeItemTransition::Updated { .. }
            ));
            let ManagedRuntimeItemTransition::Terminal { presentation } = typed_transitions[2].1
            else {
                panic!("terminal Runtime transition");
            };
            assert_eq!(
                serde_json::to_value(presentation).expect("terminal wire")["terminal"]["completed_at_ms"],
                serde_json::Value::String(u64::MAX.to_string())
            );

            let source_projection = state
                .facts
                .source_projection
                .as_ref()
                .expect("source projection");
            let identities = state
                .facts
                .source_identities
                .as_ref()
                .expect("source identities");
            let managed = state.facts.projection.as_ref().expect("Runtime projection");
            let expected = project_managed_runtime_snapshot(
                source_projection,
                identities,
                CompleteAgentRuntimeProjectionInput {
                    thread_id: managed.thread_id.clone(),
                    projection_revision: managed.revision,
                    latest_change_sequence: managed.latest_change_sequence,
                    captured_at_ms: managed.captured_at_ms,
                    operations: managed.operations.clone(),
                    command_availability: managed.command_availability.clone(),
                },
            )
            .expect("snapshot fold");
            assert_eq!(&expected, managed);
            assert_eq!(
                managed.items[0].presentation.presentation_digest,
                presentation.presentation_digest
            );
        }
    }

    #[tokio::test]
    async fn weaker_snapshot_authority_cannot_replace_authoritative_projection() {
        let repository = Arc::new(FixtureManagedRuntimeStateRepository::default());
        let reconciler = fixture_reconciler(repository);
        reconciler
            .reconcile_snapshot(
                snapshot(1, AgentSnapshotAuthority::AgentAuthoritative),
                None,
            )
            .await
            .expect("authoritative");

        let error = reconciler
            .reconcile_snapshot(snapshot(2, AgentSnapshotAuthority::Derived), None)
            .await
            .expect_err("authority downgrade");
        assert_eq!(error, CompleteAgentStateError::AuthorityDowngrade);
    }

    #[tokio::test]
    async fn stronger_authority_can_confirm_the_same_snapshot_revision() {
        let repository = Arc::new(FixtureManagedRuntimeStateRepository::default());
        let reconciler = fixture_reconciler(repository);
        reconciler
            .reconcile_snapshot(
                snapshot(1, AgentSnapshotAuthority::AgentObserved),
                Some(cursor("cursor-known")),
            )
            .await
            .expect("observed");

        let outcome = reconciler
            .reconcile_snapshot(
                snapshot(1, AgentSnapshotAuthority::AgentAuthoritative),
                None,
            )
            .await
            .expect("authority upgrade");
        let projection = outcome_projection(outcome).expect("projection");
        assert_eq!(
            projection.source_info.authority,
            AgentSnapshotAuthority::AgentAuthoritative
        );
    }

    #[tokio::test]
    async fn cursor_gap_requires_snapshot_reload_without_partial_change_apply() {
        let repository = Arc::new(FixtureManagedRuntimeStateRepository::default());
        let reconciler = fixture_reconciler(repository.clone());
        let source = source();
        reconciler
            .reconcile_snapshot(snapshot(1, AgentSnapshotAuthority::AgentObserved), None)
            .await
            .expect("snapshot");

        let outcome = reconciler
            .reconcile_change_page(
                &AgentChangesQuery {
                    source: source.clone(),
                    after: None,
                    limit: 10,
                },
                AgentChangePage {
                    source: source.clone(),
                    changes: Vec::new(),
                    next: Some(cursor("cursor-head")),
                    gap: true,
                },
            )
            .await
            .expect("gap outcome");
        assert_eq!(
            outcome,
            CompleteAgentReconcileOutcome::SnapshotReloadRequired
        );
        assert_eq!(
            fixture_source_projection(&repository)
                .await
                .platform_revision,
            1
        );
    }

    #[tokio::test]
    async fn source_sync_reloads_snapshot_at_gap_cursor() {
        let repository = Arc::new(FixtureManagedRuntimeStateRepository::default());
        let reconciler = fixture_reconciler(repository.clone());
        reconciler
            .reconcile_snapshot(
                snapshot(1, AgentSnapshotAuthority::AgentObserved),
                Some(cursor("cursor-known")),
            )
            .await
            .expect("initial snapshot");
        let service = GapService {
            reads: AtomicUsize::new(0),
            changes: AtomicUsize::new(0),
            source_changes: AgentSourceChangeLevel::OrderedDurableTail,
            pages: Mutex::new(VecDeque::new()),
        };

        let outcome = reconciler
            .synchronize_source(&service, source(), 10)
            .await
            .expect("synchronize gap");

        assert!(outcome.reloaded_snapshot);
        assert_eq!(
            outcome.projection.snapshot_revision,
            AgentSnapshotRevision(2)
        );
        assert_eq!(outcome.projection.source_cursor, None);
        assert_eq!(service.reads.load(Ordering::SeqCst), 1);
        assert_eq!(service.changes.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn snapshot_only_sync_never_calls_the_unsupported_changes_endpoint() {
        let repository = Arc::new(FixtureManagedRuntimeStateRepository::default());
        let reconciler = fixture_reconciler(repository);
        let service = GapService {
            reads: AtomicUsize::new(0),
            changes: AtomicUsize::new(0),
            source_changes: AgentSourceChangeLevel::SnapshotOnly,
            pages: Mutex::new(VecDeque::new()),
        };

        let outcome = reconciler
            .synchronize_source(&service, source(), 10)
            .await
            .expect("synchronize snapshot-only source");

        assert!(outcome.reloaded_snapshot);
        assert_eq!(outcome.projection.source_cursor, None);
        assert_eq!(service.reads.load(Ordering::SeqCst), 1);
        assert_eq!(service.changes.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn first_partial_ordered_page_is_not_used_as_snapshot_head() {
        let repository = Arc::new(FixtureManagedRuntimeStateRepository::default());
        let reconciler = fixture_reconciler(repository);
        let service = GapService {
            reads: AtomicUsize::new(0),
            changes: AtomicUsize::new(0),
            source_changes: AgentSourceChangeLevel::OrderedDurableTail,
            pages: Mutex::new(VecDeque::from([AgentChangePage {
                source: source(),
                changes: vec![agentdash_agent_service_api::AgentChange {
                    cursor: cursor("partial-1"),
                    source_revision: None,
                    occurred_at_ms: 10,
                    payload: AgentChangePayload::LifecycleChanged {
                        status: AgentLifecycleStatus::Suspended,
                    },
                }],
                next: Some(cursor("partial-1")),
                gap: false,
            }])),
        };

        let outcome = reconciler
            .synchronize_source(&service, source(), 1)
            .await
            .expect("snapshot-first synchronization");

        assert!(outcome.reloaded_snapshot);
        assert_eq!(outcome.projection.source_cursor, None);
        assert_eq!(service.reads.load(Ordering::SeqCst), 1);
        assert_eq!(service.changes.load(Ordering::SeqCst), 0);
        assert_eq!(service.pages.lock().await.len(), 1);
    }

    #[tokio::test]
    async fn trusted_ordered_cursor_drains_multiple_pages_without_replay_or_regression() {
        let repository = Arc::new(FixtureManagedRuntimeStateRepository::default());
        let reconciler = fixture_reconciler(repository);
        reconciler
            .reconcile_snapshot(
                snapshot(1, AgentSnapshotAuthority::AgentObserved),
                Some(cursor("cursor-0")),
            )
            .await
            .expect("trusted snapshot cursor");
        let service = GapService {
            reads: AtomicUsize::new(0),
            changes: AtomicUsize::new(0),
            source_changes: AgentSourceChangeLevel::OrderedDurableTail,
            pages: Mutex::new(VecDeque::from([
                AgentChangePage {
                    source: source(),
                    changes: vec![agentdash_agent_service_api::AgentChange {
                        cursor: cursor("cursor-1"),
                        source_revision: None,
                        occurred_at_ms: 10,
                        payload: AgentChangePayload::LifecycleChanged {
                            status: AgentLifecycleStatus::Suspended,
                        },
                    }],
                    next: Some(cursor("cursor-1")),
                    gap: false,
                },
                AgentChangePage {
                    source: source(),
                    changes: vec![agentdash_agent_service_api::AgentChange {
                        cursor: cursor("cursor-2"),
                        source_revision: None,
                        occurred_at_ms: 11,
                        payload: AgentChangePayload::LifecycleChanged {
                            status: AgentLifecycleStatus::Closed,
                        },
                    }],
                    next: Some(cursor("cursor-2")),
                    gap: false,
                },
                AgentChangePage {
                    source: source(),
                    changes: Vec::new(),
                    next: Some(cursor("cursor-2")),
                    gap: false,
                },
            ])),
        };

        let outcome = reconciler
            .synchronize_source(&service, source(), 1)
            .await
            .expect("drain ordered pages");

        assert!(!outcome.reloaded_snapshot);
        assert_eq!(outcome.projection.source_cursor, Some(cursor("cursor-2")));
        assert_eq!(outcome.projection.lifecycle, AgentLifecycleStatus::Closed);
        assert_eq!(service.reads.load(Ordering::SeqCst), 0);
        assert_eq!(service.changes.load(Ordering::SeqCst), 3);
    }

    fn identity_map() -> CompleteAgentRuntimeIdentityMap {
        let mut identities = CompleteAgentRuntimeIdentityMap::new(
            source(),
            RuntimeThreadId::new("runtime-thread-7").expect("runtime thread"),
        );
        identities
            .bind_turn(
                AgentTurnId::new("turn-1").expect("turn"),
                RuntimeTurnId::new("runtime-turn-11").expect("runtime turn"),
            )
            .expect("turn identity");
        identities
            .bind_item(
                AgentItemId::new("item-1").expect("item"),
                AgentTurnId::new("turn-1").expect("turn"),
                RuntimeItemId::new("runtime-item-13").expect("runtime item"),
            )
            .expect("item identity");
        identities
    }

    fn projection_input(
        revision: u64,
        latest_change_sequence: RuntimeChangeSequence,
    ) -> CompleteAgentRuntimeProjectionInput {
        let mut command_availability = BTreeMap::new();
        for command in ManagedRuntimeCommandKind::ALL {
            command_availability.insert(
                command,
                ManagedRuntimeCommandAvailability::Available {
                    evidence: availability_evidence(revision),
                },
            );
        }
        CompleteAgentRuntimeProjectionInput {
            thread_id: RuntimeThreadId::new("runtime-thread-7").expect("runtime thread"),
            projection_revision: RuntimeProjectionRevision(revision),
            latest_change_sequence,
            captured_at_ms: 1000 + revision,
            operations: Vec::new(),
            command_availability,
        }
    }

    fn availability_evidence(revision: u64) -> ManagedRuntimeAvailabilityEvidence {
        ManagedRuntimeAvailabilityEvidence {
            decided_at_revision: RuntimeProjectionRevision(revision),
            blocking_operation_id: None,
            bound_surface_revision: Some(SurfaceRevision(4)),
            applied_surface_revision: Some(SurfaceRevision(4)),
        }
    }

    fn snapshot(revision: u64, authority: AgentSnapshotAuthority) -> AgentSnapshot {
        let turn_id = AgentTurnId::new("turn-1").expect("turn");
        AgentSnapshot {
            source: source(),
            revision: AgentSnapshotRevision(revision),
            lifecycle: AgentLifecycleStatus::Active,
            active_turn_id: Some(turn_id.clone()),
            turns: vec![AgentTurnSnapshot {
                id: turn_id,
                status: AgentEntityStatus::Running,
                items: vec![AgentItemSnapshot {
                    id: AgentItemId::new("item-1").expect("item"),
                    status: AgentEntityStatus::Completed,
                    presentation: AgentItemPresentation::new(
                        AgentItemBody::ContextCompaction {
                            summary: None,
                            source_digest: None,
                        },
                        None,
                        None,
                        Some(AgentItemTerminalEvidence {
                            outcome: AgentTerminalStatus::Completed,
                            completed_at_ms: None,
                            duration_ms: None,
                            process_exit: None,
                            error: None,
                        }),
                    )
                    .expect("presentation"),
                }],
            }],
            interactions: Vec::new(),
            thread_name: None,
            source_info: AgentSnapshotSource {
                authority,
                source_revision: Some(
                    AgentSourceRevision::new(format!("source-rev-{revision}")).expect("revision"),
                ),
                fidelity: SemanticFidelity::Exact,
                observed_at_ms: revision,
            },
            applied_surface: None,
            initial_context: None,
            conversation_history: Vec::new(),
        }
    }

    fn snapshot_with_thread_name(
        revision: u64,
        thread_name: Option<Option<&str>>,
    ) -> AgentSnapshot {
        let mut snapshot = snapshot(revision, AgentSnapshotAuthority::AgentAuthoritative);
        snapshot.thread_name = thread_name.map(|thread_name| AgentThreadNameSnapshot {
            thread_name: thread_name.map(str::to_owned),
            source_info: AgentSnapshotSource {
                authority: AgentSnapshotAuthority::AgentAuthoritative,
                source_revision: Some(
                    AgentSourceRevision::new(format!("name-rev-{revision}")).expect("revision"),
                ),
                fidelity: SemanticFidelity::Exact,
                observed_at_ms: revision,
            },
        });
        snapshot
    }

    fn source() -> AgentSourceCoordinate {
        AgentSourceCoordinate::new("source-1").expect("source")
    }

    fn cursor(value: &str) -> AgentSourceCursor {
        AgentSourceCursor::new(value).expect("cursor")
    }

    struct GapService {
        reads: AtomicUsize,
        changes: AtomicUsize,
        source_changes: AgentSourceChangeLevel,
        pages: Mutex<VecDeque<AgentChangePage>>,
    }

    #[async_trait]
    impl CompleteAgentService for GapService {
        async fn describe(&self) -> Result<AgentServiceDescriptor, AgentServiceError> {
            Ok(sync_descriptor(self.source_changes))
        }

        async fn create(
            &self,
            _command: CreateAgentCommand,
        ) -> Result<AgentCommandReceipt, AgentServiceError> {
            Err(unsupported())
        }

        async fn resume(
            &self,
            _command: ResumeAgentCommand,
        ) -> Result<AgentCommandReceipt, AgentServiceError> {
            Err(unsupported())
        }

        async fn fork(
            &self,
            _command: ForkAgentCommand,
        ) -> Result<ForkAgentReceipt, AgentServiceError> {
            Err(unsupported())
        }

        async fn execute(
            &self,
            _command: AgentCommandEnvelope,
        ) -> Result<AgentCommandReceipt, AgentServiceError> {
            Err(unsupported())
        }

        async fn read(&self, _query: AgentReadQuery) -> Result<AgentSnapshot, AgentServiceError> {
            self.reads.fetch_add(1, Ordering::SeqCst);
            Ok(snapshot(2, AgentSnapshotAuthority::AgentAuthoritative))
        }

        async fn changes(
            &self,
            query: AgentChangesQuery,
        ) -> Result<AgentChangePage, AgentServiceError> {
            self.changes.fetch_add(1, Ordering::SeqCst);
            if self.source_changes == AgentSourceChangeLevel::SnapshotOnly {
                return Err(unsupported());
            }
            if let Some(page) = self.pages.lock().await.pop_front() {
                return Ok(page);
            }
            Ok(AgentChangePage {
                source: query.source,
                changes: Vec::new(),
                next: Some(cursor("cursor-head")),
                gap: true,
            })
        }

        async fn inspect(
            &self,
            _identity: AgentEffectIdentity,
        ) -> Result<AgentEffectInspection, AgentServiceError> {
            Err(unsupported())
        }

        async fn apply_surface(
            &self,
            _command: ApplyBoundAgentSurface,
        ) -> Result<AppliedAgentSurfaceReceipt, AgentServiceError> {
            Err(unsupported())
        }

        async fn revoke_surface(
            &self,
            _command: RevokeBoundAgentSurface,
        ) -> Result<AgentCommandReceipt, AgentServiceError> {
            Err(unsupported())
        }
    }

    fn sync_descriptor(source_changes: AgentSourceChangeLevel) -> AgentServiceDescriptor {
        AgentServiceDescriptor {
            definition_id: AgentServiceDefinitionId::new("sync-test").expect("definition"),
            title: "Sync test".to_owned(),
            protocol_revision: 1,
            profile: AgentCapabilityProfile {
                lifecycle: BTreeSet::new(),
                commands: BTreeSet::new(),
                fork: AgentForkCapability {
                    cutoffs: BTreeMap::new(),
                    lineage_fidelity: SemanticFidelity::Unsupported,
                    native_durability: SemanticFidelity::Unsupported,
                },
                compaction: BTreeMap::new(),
                source_changes,
                initial_context: InitialContextProfile {
                    contribution_fidelity: BTreeMap::new(),
                    applied_evidence: InitialContextAppliedEvidence::Unsupported,
                    renderer_versions: BTreeSet::new(),
                },
                surface: AgentSurfaceProfile { facets: Vec::new() },
                inspect_effects: SemanticFidelity::Unsupported,
            },
            profile_digest: AgentProfileDigest::new("sync-profile").expect("profile"),
            configuration_boundary: AgentConfigurationBoundary::Binding,
        }
    }

    fn unsupported() -> AgentServiceError {
        AgentServiceError::new(
            AgentServiceErrorCode::Unsupported,
            "not used by test",
            false,
        )
    }
}
