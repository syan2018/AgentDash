use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use agentdash_agent_service_api::{
    AgentChangePage, AgentChangePayload, AgentChangesQuery, AgentEntityStatus, AgentInteractionId,
    AgentInteractionSnapshot, AgentItemId, AgentItemSnapshot, AgentLifecycleStatus, AgentReadQuery,
    AgentServiceError, AgentSnapshot, AgentSnapshotAuthority, AgentSnapshotRevision,
    AgentSnapshotSource, AgentSourceChangeLevel, AgentSourceCoordinate, AgentSourceCursor,
    AgentSourceRevision, AgentTurnId, AgentTurnSnapshot, AppliedAgentSurface,
    AppliedInitialContextEvidence, CompleteAgentService,
};
use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::Mutex;

#[derive(Debug, Clone, PartialEq)]
pub struct NormalizedAgentTurn {
    pub id: AgentTurnId,
    pub status: AgentEntityStatus,
    pub item_ids: Vec<AgentItemId>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NormalizedAgentItem {
    pub turn_id: AgentTurnId,
    pub item: AgentItemSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NormalizedAgentProjection {
    pub source: AgentSourceCoordinate,
    pub platform_revision: u64,
    pub snapshot_revision: AgentSnapshotRevision,
    pub lifecycle: AgentLifecycleStatus,
    pub active_turn_id: Option<AgentTurnId>,
    pub turns: BTreeMap<AgentTurnId, NormalizedAgentTurn>,
    pub items: BTreeMap<AgentItemId, NormalizedAgentItem>,
    pub interactions: BTreeMap<AgentInteractionId, AgentInteractionSnapshot>,
    pub source_info: AgentSnapshotSource,
    pub source_cursor: Option<AgentSourceCursor>,
    pub applied_surface: Option<AppliedAgentSurface>,
    pub initial_context: Option<AppliedInitialContextEvidence>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum NormalizedAgentPlatformChangePayload {
    SnapshotReplaced {
        snapshot_revision: AgentSnapshotRevision,
        authority: AgentSnapshotAuthority,
    },
    SourceChangeApplied {
        source_cursor: AgentSourceCursor,
        source_revision: Option<AgentSourceRevision>,
        payload: Box<AgentChangePayload>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct NormalizedAgentPlatformChange {
    pub sequence: u64,
    pub platform_revision: u64,
    pub payload: NormalizedAgentPlatformChangePayload,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NormalizedAgentChangePage {
    pub source: AgentSourceCoordinate,
    pub changes: Vec<NormalizedAgentPlatformChange>,
    pub next_sequence: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NormalizedAgentProjectionCommit {
    pub expected_platform_revision: Option<u64>,
    pub projection: NormalizedAgentProjection,
    pub changes: Vec<NormalizedAgentPlatformChangePayload>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CompleteAgentStateStoreError {
    #[error(
        "normalized Agent projection revision conflict for {coordinate}: expected {expected:?}, actual {actual:?}"
    )]
    Conflict {
        coordinate: AgentSourceCoordinate,
        expected: Option<u64>,
        actual: Option<u64>,
    },
    #[error("normalized Agent state persistence failed: {reason}")]
    Persistence { reason: String },
}

#[async_trait]
pub trait CompleteAgentStateRepository: Send + Sync {
    async fn load_projection(
        &self,
        source: &AgentSourceCoordinate,
    ) -> Result<Option<NormalizedAgentProjection>, CompleteAgentStateStoreError>;

    async fn commit_projection(
        &self,
        commit: NormalizedAgentProjectionCommit,
    ) -> Result<NormalizedAgentProjection, CompleteAgentStateStoreError>;

    async fn platform_changes(
        &self,
        source: &AgentSourceCoordinate,
        after_sequence: u64,
        limit: usize,
    ) -> Result<NormalizedAgentChangePage, CompleteAgentStateStoreError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CompleteAgentStateError {
    #[error(transparent)]
    Store(#[from] CompleteAgentStateStoreError),
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
}

impl<R> CompleteAgentStateReconciler<R>
where
    R: CompleteAgentStateRepository,
{
    pub fn new(repository: Arc<R>) -> Self {
        Self { repository }
    }

    pub async fn reconcile_snapshot(
        &self,
        snapshot: AgentSnapshot,
        source_cursor: Option<AgentSourceCursor>,
    ) -> Result<CompleteAgentReconcileOutcome, CompleteAgentStateError> {
        let existing = self.repository.load_projection(&snapshot.source).await?;
        let next_platform_revision = existing
            .as_ref()
            .map_or(1, |projection| projection.platform_revision + 1);
        let normalized = normalize_snapshot(snapshot, source_cursor, next_platform_revision)?;

        if let Some(existing) = &existing {
            ensure_snapshot_can_replace(existing, &normalized)?;
            if same_projection_facts(existing, &normalized) {
                return Ok(CompleteAgentReconcileOutcome::Unchanged(existing.clone()));
            }
        }

        let committed = self
            .repository
            .commit_projection(NormalizedAgentProjectionCommit {
                expected_platform_revision: existing
                    .as_ref()
                    .map(|projection| projection.platform_revision),
                changes: vec![NormalizedAgentPlatformChangePayload::SnapshotReplaced {
                    snapshot_revision: normalized.snapshot_revision,
                    authority: normalized.source_info.authority,
                }],
                projection: normalized,
            })
            .await?;
        Ok(CompleteAgentReconcileOutcome::Committed(committed))
    }

    pub async fn reconcile_change_page(
        &self,
        query: &AgentChangesQuery,
        page: AgentChangePage,
    ) -> Result<CompleteAgentReconcileOutcome, CompleteAgentStateError> {
        if query.source != page.source {
            return Err(CompleteAgentStateError::SourceMismatch);
        }
        if page.gap
            || page.changes.iter().any(|change| {
                matches!(
                    &change.payload,
                    AgentChangePayload::SnapshotInvalidated { .. }
                )
            })
        {
            return Ok(CompleteAgentReconcileOutcome::SnapshotReloadRequired);
        }
        if query.limit == 0 || page.changes.len() > query.limit as usize {
            return Err(CompleteAgentStateError::InvalidChangePage);
        }

        let Some(mut projection) = self.repository.load_projection(&query.source).await? else {
            return Ok(CompleteAgentReconcileOutcome::SnapshotReloadRequired);
        };
        if projection.source_cursor != query.after {
            return Err(CompleteAgentStateError::CursorMismatch);
        }
        validate_page_cursors(query, &page)?;
        if page.changes.is_empty() && projection.source_cursor == page.next {
            return Ok(CompleteAgentReconcileOutcome::Unchanged(projection));
        }

        let expected_platform_revision = projection.platform_revision;
        let next_platform_revision = expected_platform_revision + 1;
        let mut changes = Vec::with_capacity(page.changes.len());
        for change in page.changes {
            apply_source_change(&mut projection, &change.payload)?;
            if let Some(source_revision) = &change.source_revision {
                projection.source_info.source_revision = Some(source_revision.clone());
            }
            changes.push(NormalizedAgentPlatformChangePayload::SourceChangeApplied {
                source_cursor: change.cursor,
                source_revision: change.source_revision,
                payload: Box::new(change.payload),
            });
        }
        projection.platform_revision = next_platform_revision;
        projection.source_cursor = page.next;

        let committed = self
            .repository
            .commit_projection(NormalizedAgentProjectionCommit {
                expected_platform_revision: Some(expected_platform_revision),
                projection,
                changes,
            })
            .await?;
        Ok(CompleteAgentReconcileOutcome::Committed(committed))
    }

    /// Synchronizes one source without replaying source history into a Runtime journal.
    ///
    /// Snapshot/observation sources reconcile directly from authoritative snapshots. Ordered
    /// sources use their change contract and align a gap reload to the returned cursor.
    pub async fn synchronize_source(
        &self,
        service: &dyn CompleteAgentService,
        source: AgentSourceCoordinate,
        limit: u32,
    ) -> Result<CompleteAgentSourceSyncOutcome, CompleteAgentStateError> {
        let descriptor = service.describe().await?;
        if matches!(
            descriptor.profile.source_changes,
            AgentSourceChangeLevel::SnapshotOnly | AgentSourceChangeLevel::ObservationOnly
        ) {
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

        let current = self.repository.load_projection(&source).await?;
        let query = AgentChangesQuery {
            source: source.clone(),
            after: current
                .as_ref()
                .and_then(|projection| projection.source_cursor.clone()),
            limit,
        };
        let page = service.changes(query.clone()).await?;
        if page.source != source {
            return Err(CompleteAgentStateError::SourceMismatch);
        }
        let page_cursor = page.next.clone();
        let must_reload = current.is_none()
            || page.gap
            || page.changes.iter().any(|change| {
                matches!(
                    &change.payload,
                    AgentChangePayload::SnapshotInvalidated { .. }
                )
            });
        if must_reload {
            let snapshot = service
                .read(AgentReadQuery {
                    source: source.clone(),
                    at_revision: None,
                })
                .await?;
            if snapshot.source != source {
                return Err(CompleteAgentStateError::SourceMismatch);
            }
            let outcome = self.reconcile_snapshot(snapshot, page_cursor).await?;
            return Ok(CompleteAgentSourceSyncOutcome {
                projection: outcome_projection(outcome)
                    .expect("snapshot reconciliation always returns a projection"),
                reloaded_snapshot: true,
            });
        }

        match self.reconcile_change_page(&query, page).await? {
            CompleteAgentReconcileOutcome::Unchanged(projection)
            | CompleteAgentReconcileOutcome::Committed(projection) => {
                Ok(CompleteAgentSourceSyncOutcome {
                    projection,
                    reloaded_snapshot: false,
                })
            }
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
                let outcome = self.reconcile_snapshot(snapshot, page_cursor).await?;
                Ok(CompleteAgentSourceSyncOutcome {
                    projection: outcome_projection(outcome)
                        .expect("snapshot reconciliation always returns a projection"),
                    reloaded_snapshot: true,
                })
            }
        }
    }
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
        source_info: snapshot.source_info,
        source_cursor,
        applied_surface: snapshot.applied_surface,
        initial_context: snapshot.initial_context,
    })
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
        && left.applied_surface == right.applied_surface
        && left.initial_context == right.initial_context
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
        && left.source_info == right.source_info
        && left.source_cursor == right.source_cursor
        && left.applied_surface == right.applied_surface
        && left.initial_context == right.initial_context
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
        AgentChangePayload::InteractionChanged { interaction } => {
            if !projection.turns.contains_key(&interaction.turn_id) {
                return Err(CompleteAgentStateError::InvalidChange {
                    reason: "interaction change references an unknown turn".to_owned(),
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

#[derive(Default)]
struct InMemoryCompleteAgentState {
    projections: BTreeMap<AgentSourceCoordinate, NormalizedAgentProjection>,
    changes: BTreeMap<AgentSourceCoordinate, Vec<NormalizedAgentPlatformChange>>,
}

#[derive(Default)]
pub struct InMemoryCompleteAgentStateRepository {
    state: Mutex<InMemoryCompleteAgentState>,
}

impl InMemoryCompleteAgentStateRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl CompleteAgentStateRepository for InMemoryCompleteAgentStateRepository {
    async fn load_projection(
        &self,
        source: &AgentSourceCoordinate,
    ) -> Result<Option<NormalizedAgentProjection>, CompleteAgentStateStoreError> {
        Ok(self.state.lock().await.projections.get(source).cloned())
    }

    async fn commit_projection(
        &self,
        commit: NormalizedAgentProjectionCommit,
    ) -> Result<NormalizedAgentProjection, CompleteAgentStateStoreError> {
        let mut state = self.state.lock().await;
        let source = commit.projection.source.clone();
        let actual = state
            .projections
            .get(&source)
            .map(|projection| projection.platform_revision);
        if actual != commit.expected_platform_revision {
            return Err(CompleteAgentStateStoreError::Conflict {
                coordinate: source,
                expected: commit.expected_platform_revision,
                actual,
            });
        }
        let stream = state.changes.entry(source.clone()).or_default();
        let base_sequence = stream.last().map_or(0, |change| change.sequence);
        for (offset, payload) in commit.changes.into_iter().enumerate() {
            let offset =
                u64::try_from(offset).map_err(|_| CompleteAgentStateStoreError::Persistence {
                    reason: "platform change sequence offset exceeds u64".to_owned(),
                })?;
            let sequence = base_sequence
                .checked_add(offset)
                .and_then(|value| value.checked_add(1))
                .ok_or_else(|| CompleteAgentStateStoreError::Persistence {
                    reason: "platform change sequence is exhausted".to_owned(),
                })?;
            stream.push(NormalizedAgentPlatformChange {
                sequence,
                platform_revision: commit.projection.platform_revision,
                payload,
            });
        }
        state.projections.insert(source, commit.projection.clone());
        Ok(commit.projection)
    }

    async fn platform_changes(
        &self,
        source: &AgentSourceCoordinate,
        after_sequence: u64,
        limit: usize,
    ) -> Result<NormalizedAgentChangePage, CompleteAgentStateStoreError> {
        let state = self.state.lock().await;
        let changes = state
            .changes
            .get(source)
            .into_iter()
            .flatten()
            .filter(|change| change.sequence > after_sequence)
            .take(limit)
            .cloned()
            .collect::<Vec<_>>();
        let next_sequence = changes
            .last()
            .map_or(after_sequence, |change| change.sequence);
        Ok(NormalizedAgentChangePage {
            source: source.clone(),
            changes,
            next_sequence,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use agentdash_agent_service_api::{
        AgentCapabilityProfile, AgentCommandEnvelope, AgentCommandReceipt,
        AgentConfigurationBoundary, AgentEffectIdentity, AgentEffectInspection,
        AgentForkCapability, AgentItemContent, AgentPayloadDigest, AgentProfileDigest,
        AgentServiceDefinitionId, AgentServiceDescriptor, AgentServiceErrorCode,
        AgentSnapshotSource, AgentSourceRevision, AgentSurfaceProfile, AgentTurnSnapshot,
        AppliedAgentSurfaceReceipt, ApplyBoundAgentSurface, CreateAgentCommand, ForkAgentCommand,
        ForkAgentReceipt, InitialContextAppliedEvidence, InitialContextProfile, ResumeAgentCommand,
        RevokeBoundAgentSurface, SemanticFidelity,
    };

    use super::*;

    #[tokio::test]
    async fn snapshot_is_normalized_and_reconnects_from_platform_changes() {
        let repository = Arc::new(InMemoryCompleteAgentStateRepository::new());
        let reconciler = CompleteAgentStateReconciler::new(repository.clone());
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

        let projection = repository
            .load_projection(&source)
            .await
            .expect("load")
            .expect("projection");
        assert_eq!(projection.lifecycle, AgentLifecycleStatus::Closed);
        assert_eq!(projection.turns.len(), 1);
        assert_eq!(projection.items.len(), 1);
        let reconnect = repository
            .platform_changes(&source, 0, 10)
            .await
            .expect("platform changes");
        assert_eq!(reconnect.changes.len(), 2);
        assert_eq!(reconnect.next_sequence, 2);
    }

    #[tokio::test]
    async fn active_turn_change_is_applied_as_an_explicit_source_fact() {
        let repository = Arc::new(InMemoryCompleteAgentStateRepository::new());
        let reconciler = CompleteAgentStateReconciler::new(repository.clone());
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

        let projection = repository
            .load_projection(&source)
            .await
            .expect("load")
            .expect("projection");
        assert_eq!(projection.active_turn_id, None);
        assert_eq!(
            projection
                .turns
                .get(&AgentTurnId::new("turn-1").expect("turn"))
                .expect("normalized turn")
                .status,
            AgentEntityStatus::Completed
        );
    }

    #[tokio::test]
    async fn weaker_snapshot_authority_cannot_replace_authoritative_projection() {
        let repository = Arc::new(InMemoryCompleteAgentStateRepository::new());
        let reconciler = CompleteAgentStateReconciler::new(repository);
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
        let repository = Arc::new(InMemoryCompleteAgentStateRepository::new());
        let reconciler = CompleteAgentStateReconciler::new(repository);
        reconciler
            .reconcile_snapshot(snapshot(1, AgentSnapshotAuthority::AgentObserved), None)
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
        let repository = Arc::new(InMemoryCompleteAgentStateRepository::new());
        let reconciler = CompleteAgentStateReconciler::new(repository.clone());
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
            repository
                .load_projection(&source)
                .await
                .expect("load")
                .expect("projection")
                .platform_revision,
            1
        );
    }

    #[tokio::test]
    async fn source_sync_reloads_snapshot_at_gap_cursor() {
        let repository = Arc::new(InMemoryCompleteAgentStateRepository::new());
        let reconciler = CompleteAgentStateReconciler::new(repository.clone());
        reconciler
            .reconcile_snapshot(snapshot(1, AgentSnapshotAuthority::AgentObserved), None)
            .await
            .expect("initial snapshot");
        let service = GapService {
            reads: AtomicUsize::new(0),
            changes: AtomicUsize::new(0),
            source_changes: AgentSourceChangeLevel::OrderedDurableTail,
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
        assert_eq!(
            outcome.projection.source_cursor,
            Some(cursor("cursor-head"))
        );
        assert_eq!(service.reads.load(Ordering::SeqCst), 1);
        assert_eq!(service.changes.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn snapshot_only_sync_never_calls_the_unsupported_changes_endpoint() {
        let repository = Arc::new(InMemoryCompleteAgentStateRepository::new());
        let reconciler = CompleteAgentStateReconciler::new(repository);
        let service = GapService {
            reads: AtomicUsize::new(0),
            changes: AtomicUsize::new(0),
            source_changes: AgentSourceChangeLevel::SnapshotOnly,
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
                    content: AgentItemContent::ContextCompaction,
                    content_digest: AgentPayloadDigest::new("sha256:item").expect("digest"),
                }],
            }],
            interactions: Vec::new(),
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
        }
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
