use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use agentdash_agent_runtime_contract::{
    ManagedRuntimeChangeDelta, ManagedRuntimeChangeGap, ManagedRuntimeChangePage,
    ManagedRuntimeCommandAvailability, ManagedRuntimeCommandKind, ManagedRuntimeContentBlock,
    ManagedRuntimeEntityStatus, ManagedRuntimeInteraction, ManagedRuntimeInteractionKind,
    ManagedRuntimeInteractionStatus, ManagedRuntimeItem, ManagedRuntimeItemContent,
    ManagedRuntimeLifecycleStatus, ManagedRuntimeOperation, ManagedRuntimePlatformChange,
    ManagedRuntimeProjectionAuthority, ManagedRuntimeProjectionFidelity, ManagedRuntimeSnapshot,
    ManagedRuntimeTurn, RuntimeChangeSequence, RuntimeInteractionId, RuntimeItemId,
    RuntimePayloadDigest, RuntimeProjectionRevision, RuntimeThreadId, RuntimeTurnId,
    SurfaceRevision,
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
        fidelity: agentdash_agent_service_api::SemanticFidelity,
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
    pub requested_after_sequence: u64,
    pub earliest_available_sequence: Option<u64>,
    pub latest_available_sequence: Option<u64>,
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
                    fidelity: normalized.source_info.fidelity,
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct CompleteAgentRuntimeItemIdentity {
    source_turn_id: AgentTurnId,
    runtime_item_id: RuntimeItemId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CompleteAgentRuntimeInteractionIdentity {
    source_turn_id: AgentTurnId,
    runtime_interaction_id: RuntimeInteractionId,
}

/// Stable Runtime-owned identity map for one Complete Agent source.
///
/// Runtime identities must be allocated independently and then bound explicitly. A source
/// coordinate is never parsed or copied into a Runtime identity.
#[derive(Debug, Clone, PartialEq, Eq)]
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
        operations: input.operations,
        authority: project_authority(projection.source_info.authority),
        fidelity: project_fidelity(projection.source_info.fidelity),
        command_availability: input.command_availability,
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
    if input.projection_revision.0 != projection.platform_revision {
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
    let delta = match &change.payload {
        NormalizedAgentPlatformChangePayload::SnapshotReplaced {
            authority,
            fidelity,
            ..
        } => ManagedRuntimeChangeDelta::SnapshotReplaced {
            authority: project_authority(*authority),
            fidelity: project_fidelity(*fidelity),
        },
        NormalizedAgentPlatformChangePayload::SourceChangeApplied { payload, .. } => {
            project_source_delta(payload, identities)?
        }
    };
    Ok(ManagedRuntimePlatformChange {
        thread_id: identities.thread_id.clone(),
        sequence: RuntimeChangeSequence(change.sequence),
        revision: RuntimeProjectionRevision(change.platform_revision),
        delta,
    })
}

fn project_source_delta(
    payload: &AgentChangePayload,
    identities: &CompleteAgentRuntimeIdentityMap,
) -> Result<ManagedRuntimeChangeDelta, CompleteAgentRuntimeProjectionError> {
    match payload {
        AgentChangePayload::LifecycleChanged { status } => {
            Ok(ManagedRuntimeChangeDelta::LifecycleChanged {
                lifecycle: project_lifecycle(*status),
            })
        }
        AgentChangePayload::TurnChanged { turn } => {
            let projected_turn = project_turn_snapshot(turn, identities)?;
            let items = turn
                .items
                .iter()
                .map(|item| project_item(item, &turn.id, identities))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(ManagedRuntimeChangeDelta::TurnUpserted {
                turn: projected_turn,
                items,
            })
        }
        AgentChangePayload::ActiveTurnChanged { active_turn_id } => {
            Ok(ManagedRuntimeChangeDelta::ActiveTurnChanged {
                active_turn_id: active_turn_id
                    .as_ref()
                    .map(|turn_id| identities.runtime_turn_id(turn_id))
                    .transpose()?,
            })
        }
        AgentChangePayload::ItemChanged { turn_id, item } => {
            Ok(ManagedRuntimeChangeDelta::ItemUpserted {
                item: project_item(item, turn_id, identities)?,
            })
        }
        AgentChangePayload::InteractionChanged { interaction } => {
            Ok(ManagedRuntimeChangeDelta::InteractionUpserted {
                interaction: project_interaction(interaction, identities)?,
            })
        }
        AgentChangePayload::SurfaceApplied { applied } => {
            Ok(ManagedRuntimeChangeDelta::SurfaceEvidenceChanged {
                bound_surface_revision: None,
                applied_surface_revision: Some(
                    identities.runtime_surface_revision(applied.revision)?,
                ),
            })
        }
        AgentChangePayload::SnapshotInvalidated { .. } => {
            Err(CompleteAgentRuntimeProjectionError::SnapshotInvalidationCommitted)
        }
    }
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

fn project_turn_snapshot(
    turn: &AgentTurnSnapshot,
    identities: &CompleteAgentRuntimeIdentityMap,
) -> Result<ManagedRuntimeTurn, CompleteAgentRuntimeProjectionError> {
    Ok(ManagedRuntimeTurn {
        id: identities.runtime_turn_id(&turn.id)?,
        status: project_entity_status(turn.status),
        item_ids: turn
            .items
            .iter()
            .map(|item| identities.runtime_item_id(&item.id, &turn.id))
            .collect::<Result<Vec<_>, _>>()?,
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
        content: project_item_content(&item.content)?,
        content_digest: RuntimePayloadDigest::new(item.content_digest.as_str())
            .map_err(|_| CompleteAgentRuntimeProjectionError::InvalidPayloadDigest)?,
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
        kind: match interaction.kind {
            agentdash_agent_service_api::AgentInteractionKind::Approval => {
                ManagedRuntimeInteractionKind::Approval
            }
            agentdash_agent_service_api::AgentInteractionKind::UserInput => {
                ManagedRuntimeInteractionKind::UserInput
            }
            agentdash_agent_service_api::AgentInteractionKind::McpElicitation => {
                ManagedRuntimeInteractionKind::McpElicitation
            }
            agentdash_agent_service_api::AgentInteractionKind::DynamicTool => {
                ManagedRuntimeInteractionKind::DynamicTool
            }
        },
        prompt: interaction.prompt.clone(),
        status: if interaction.resolved {
            ManagedRuntimeInteractionStatus::Resolved
        } else {
            ManagedRuntimeInteractionStatus::Pending
        },
    })
}

fn project_item_content(
    content: &agentdash_agent_service_api::AgentItemContent,
) -> Result<ManagedRuntimeItemContent, CompleteAgentRuntimeProjectionError> {
    Ok(match content {
        agentdash_agent_service_api::AgentItemContent::UserInput { input } => {
            ManagedRuntimeItemContent::UserInput {
                content: input
                    .content
                    .iter()
                    .map(project_content_block)
                    .collect::<Result<Vec<_>, _>>()?,
            }
        }
        agentdash_agent_service_api::AgentItemContent::AgentOutput { content } => {
            ManagedRuntimeItemContent::AgentOutput {
                content: content
                    .iter()
                    .map(project_content_block)
                    .collect::<Result<Vec<_>, _>>()?,
            }
        }
        agentdash_agent_service_api::AgentItemContent::ToolCall { name, arguments } => {
            ManagedRuntimeItemContent::ToolCall {
                name: name.to_string(),
                arguments: arguments.clone(),
            }
        }
        agentdash_agent_service_api::AgentItemContent::ToolResult { name, result } => {
            ManagedRuntimeItemContent::ToolResult {
                name: name.to_string(),
                result: result.clone(),
            }
        }
        agentdash_agent_service_api::AgentItemContent::ContextCompaction => {
            ManagedRuntimeItemContent::ContextCompaction
        }
        agentdash_agent_service_api::AgentItemContent::Error { code, message } => {
            ManagedRuntimeItemContent::Error {
                code: code.clone(),
                message: message.clone(),
            }
        }
        agentdash_agent_service_api::AgentItemContent::Extension {
            namespace,
            schema,
            value,
        } => ManagedRuntimeItemContent::Extension {
            namespace: namespace.clone(),
            schema: schema.clone(),
            value: value.clone(),
        },
    })
}

fn project_content_block(
    content: &agentdash_agent_service_api::AgentInputContent,
) -> Result<ManagedRuntimeContentBlock, CompleteAgentRuntimeProjectionError> {
    Ok(match content {
        agentdash_agent_service_api::AgentInputContent::Text { text } => {
            ManagedRuntimeContentBlock::Text { text: text.clone() }
        }
        agentdash_agent_service_api::AgentInputContent::Image {
            media_type,
            source,
            digest,
        } => ManagedRuntimeContentBlock::Image {
            media_type: media_type.clone(),
            source: source.clone(),
            digest: RuntimePayloadDigest::new(digest.as_str())
                .map_err(|_| CompleteAgentRuntimeProjectionError::InvalidPayloadDigest)?,
        },
        agentdash_agent_service_api::AgentInputContent::Resource {
            uri,
            media_type,
            digest,
        } => ManagedRuntimeContentBlock::Resource {
            uri: uri.clone(),
            media_type: media_type.clone(),
            digest: digest
                .as_ref()
                .map(|digest| RuntimePayloadDigest::new(digest.as_str()))
                .transpose()
                .map_err(|_| CompleteAgentRuntimeProjectionError::InvalidPayloadDigest)?,
        },
        agentdash_agent_service_api::AgentInputContent::Structured { schema, value } => {
            ManagedRuntimeContentBlock::Structured {
                schema: schema.clone(),
                value: value.clone(),
            }
        }
    })
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
    use std::sync::atomic::{AtomicUsize, Ordering};

    use agentdash_agent_runtime_contract::ManagedRuntimeAvailabilityEvidence;
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
    use tokio::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct FixtureCompleteAgentState {
        projections: BTreeMap<AgentSourceCoordinate, NormalizedAgentProjection>,
        changes: BTreeMap<AgentSourceCoordinate, Vec<NormalizedAgentPlatformChange>>,
    }

    #[derive(Default)]
    struct FixtureCompleteAgentStateRepository {
        state: Mutex<FixtureCompleteAgentState>,
    }

    #[async_trait]
    impl CompleteAgentStateRepository for FixtureCompleteAgentStateRepository {
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
                let offset = u64::try_from(offset).map_err(|_| {
                    CompleteAgentStateStoreError::Persistence {
                        reason: "platform change sequence offset exceeds u64".to_owned(),
                    }
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
                requested_after_sequence: after_sequence,
                earliest_available_sequence: state
                    .changes
                    .get(source)
                    .and_then(|changes| changes.first())
                    .map(|change| change.sequence),
                latest_available_sequence: state
                    .changes
                    .get(source)
                    .and_then(|changes| changes.last())
                    .map(|change| change.sequence),
                changes,
                next_sequence,
            })
        }
    }

    #[tokio::test]
    async fn snapshot_is_normalized_and_reconnects_from_platform_changes() {
        let repository = Arc::new(FixtureCompleteAgentStateRepository::default());
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
    async fn managed_snapshot_uses_explicit_runtime_ids_and_committed_availability() {
        let repository = Arc::new(FixtureCompleteAgentStateRepository::default());
        let reconciler = CompleteAgentStateReconciler::new(repository.clone());
        reconciler
            .reconcile_snapshot(
                snapshot(1, AgentSnapshotAuthority::AgentAuthoritative),
                None,
            )
            .await
            .expect("snapshot");
        let projection = repository
            .load_projection(&source())
            .await
            .expect("load")
            .expect("projection");
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
        let repository = Arc::new(FixtureCompleteAgentStateRepository::default());
        let reconciler = CompleteAgentStateReconciler::new(repository.clone());
        reconciler
            .reconcile_snapshot(
                snapshot(1, AgentSnapshotAuthority::AgentAuthoritative),
                None,
            )
            .await
            .expect("snapshot");
        let projection = repository
            .load_projection(&source())
            .await
            .expect("load")
            .expect("projection");
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
        let repository = Arc::new(FixtureCompleteAgentStateRepository::default());
        let reconciler = CompleteAgentStateReconciler::new(repository.clone());
        reconciler
            .reconcile_snapshot(
                snapshot(1, AgentSnapshotAuthority::AgentAuthoritative),
                None,
            )
            .await
            .expect("snapshot");
        let projection = repository
            .load_projection(&source())
            .await
            .expect("load")
            .expect("projection");
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
                            content: AgentItemContent::ContextCompaction,
                            content_digest: AgentPayloadDigest::new("sha256:running")
                                .expect("digest"),
                        },
                    }),
                },
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
        let ManagedRuntimeChangeDelta::ItemUpserted { item } = &projected.changes[0].delta else {
            panic!("item delta");
        };
        assert_eq!(item.id.as_str(), "runtime-item-13");
        assert_eq!(item.turn_id.as_str(), "runtime-turn-11");
    }

    #[tokio::test]
    async fn active_turn_change_is_applied_as_an_explicit_source_fact() {
        let repository = Arc::new(FixtureCompleteAgentStateRepository::default());
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
        let repository = Arc::new(FixtureCompleteAgentStateRepository::default());
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
        let repository = Arc::new(FixtureCompleteAgentStateRepository::default());
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
        let repository = Arc::new(FixtureCompleteAgentStateRepository::default());
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
        let repository = Arc::new(FixtureCompleteAgentStateRepository::default());
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
        let repository = Arc::new(FixtureCompleteAgentStateRepository::default());
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
