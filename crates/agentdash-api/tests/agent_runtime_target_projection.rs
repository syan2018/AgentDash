//! Target-only App Server projection evidence.
//!
//! The production router remains unchanged. This fixture is serialized
//! directly from the dependency-light Runtime Contract and consumed verbatim
//! by the frontend target test.

use std::collections::BTreeMap;

use agentdash_agent_runtime_contract::managed_projection::{
    ManagedRuntimeAvailabilityEvidence, ManagedRuntimeChangeDelta, ManagedRuntimeChangeGap,
    ManagedRuntimeChangePage, ManagedRuntimeCommandAvailability, ManagedRuntimeCommandKind,
    ManagedRuntimeEntityStatus, ManagedRuntimeItem, ManagedRuntimeItemContent,
    ManagedRuntimeLifecycleStatus, ManagedRuntimeOperation, ManagedRuntimeOperationStatus,
    ManagedRuntimePlatformChange, ManagedRuntimeProjectionAuthority,
    ManagedRuntimeProjectionFidelity, ManagedRuntimeSnapshot, ManagedRuntimeTurn,
    ManagedRuntimeUnavailabilityReason,
};
use agentdash_agent_runtime_contract::{
    RuntimeChangeSequence, RuntimeItemId, RuntimeOperationId, RuntimePayloadDigest,
    RuntimeProjectionRevision, RuntimeThreadId, RuntimeTurnId,
};
use agentdash_application_agentrun::agent_run::target_product_protocol::{
    consume_managed_runtime_change_page, consume_managed_runtime_snapshot,
    managed_runtime_change_page_requires_snapshot_reload,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Deserialize, Serialize)]
struct CanonicalFrontendFixture {
    snapshots: BTreeMap<String, ManagedRuntimeSnapshot>,
    change_page: ManagedRuntimeChangePage,
    gap_page: ManagedRuntimeChangePage,
}

fn id<T>(
    value: &str,
    constructor: impl FnOnce(String) -> Result<T, agentdash_agent_runtime_contract::InvalidRuntimeId>,
) -> T {
    constructor(value.to_owned()).expect("valid Runtime identity")
}

fn availability(
    revision: RuntimeProjectionRevision,
    unavailable_reason: Option<ManagedRuntimeUnavailabilityReason>,
) -> BTreeMap<ManagedRuntimeCommandKind, ManagedRuntimeCommandAvailability> {
    ManagedRuntimeCommandKind::ALL
        .into_iter()
        .map(|command| {
            let evidence = ManagedRuntimeAvailabilityEvidence {
                decided_at_revision: revision,
                blocking_operation_id: matches!(
                    unavailable_reason,
                    Some(ManagedRuntimeUnavailabilityReason::OperationInFlight)
                )
                .then(|| id("operation-compaction", RuntimeOperationId::new)),
                bound_surface_revision: None,
                applied_surface_revision: None,
            };
            let availability = if let Some(reason) = unavailable_reason {
                ManagedRuntimeCommandAvailability::Unavailable { reason, evidence }
            } else {
                ManagedRuntimeCommandAvailability::Available { evidence }
            };
            (command, availability)
        })
        .collect()
}

fn snapshot(
    item_status: ManagedRuntimeEntityStatus,
    operation_status: ManagedRuntimeOperationStatus,
    revision: u64,
    unavailable_reason: Option<ManagedRuntimeUnavailabilityReason>,
) -> ManagedRuntimeSnapshot {
    let thread_id = id("runtime-thread-child", RuntimeThreadId::new);
    let turn_id = id("turn-compaction", RuntimeTurnId::new);
    let item_id = id("item-compaction", RuntimeItemId::new);
    let revision = RuntimeProjectionRevision(revision);
    ManagedRuntimeSnapshot {
        thread_id,
        revision,
        latest_change_sequence: RuntimeChangeSequence(revision.0 + 3),
        captured_at_ms: 1_000 + revision.0,
        lifecycle: ManagedRuntimeLifecycleStatus::Active,
        active_turn_id: matches!(item_status, ManagedRuntimeEntityStatus::Running)
            .then_some(turn_id.clone()),
        turns: vec![ManagedRuntimeTurn {
            id: turn_id.clone(),
            status: item_status,
            item_ids: vec![item_id.clone()],
        }],
        items: vec![ManagedRuntimeItem {
            id: item_id,
            turn_id: turn_id.clone(),
            status: item_status,
            content: ManagedRuntimeItemContent::ContextCompaction,
            content_digest: id(
                &format!("sha256:compaction-{}", revision.0),
                RuntimePayloadDigest::new,
            ),
        }],
        interactions: Vec::new(),
        operations: vec![ManagedRuntimeOperation {
            id: id("operation-compaction", RuntimeOperationId::new),
            turn_id: Some(turn_id),
            status: operation_status,
        }],
        authority: ManagedRuntimeProjectionAuthority::SourceAuthoritative,
        fidelity: ManagedRuntimeProjectionFidelity::Exact,
        command_availability: availability(revision, unavailable_reason),
    }
}

fn canonical_frontend_fixture() -> CanonicalFrontendFixture {
    let snapshots = BTreeMap::from([
        (
            "completed".to_owned(),
            snapshot(
                ManagedRuntimeEntityStatus::Completed,
                ManagedRuntimeOperationStatus::Succeeded,
                6,
                None,
            ),
        ),
        (
            "failed".to_owned(),
            snapshot(
                ManagedRuntimeEntityStatus::Failed,
                ManagedRuntimeOperationStatus::Failed,
                7,
                None,
            ),
        ),
        (
            "lost".to_owned(),
            snapshot(
                ManagedRuntimeEntityStatus::Lost,
                ManagedRuntimeOperationStatus::Lost,
                8,
                Some(ManagedRuntimeUnavailabilityReason::SourceUnavailable),
            ),
        ),
        (
            "started".to_owned(),
            snapshot(
                ManagedRuntimeEntityStatus::Running,
                ManagedRuntimeOperationStatus::Running,
                5,
                Some(ManagedRuntimeUnavailabilityReason::OperationInFlight),
            ),
        ),
    ]);
    let thread_id = id("runtime-thread-child", RuntimeThreadId::new);
    let change_page = ManagedRuntimeChangePage {
        thread_id: thread_id.clone(),
        changes: vec![ManagedRuntimePlatformChange {
            thread_id: thread_id.clone(),
            sequence: RuntimeChangeSequence(9),
            revision: RuntimeProjectionRevision(6),
            delta: ManagedRuntimeChangeDelta::ItemUpserted {
                item: snapshot(
                    ManagedRuntimeEntityStatus::Completed,
                    ManagedRuntimeOperationStatus::Succeeded,
                    6,
                    None,
                )
                .items
                .into_iter()
                .next()
                .expect("fixture item"),
            },
        }],
        next: RuntimeChangeSequence(9),
        gap: None,
    };
    let gap_page = ManagedRuntimeChangePage {
        thread_id,
        changes: Vec::new(),
        next: RuntimeChangeSequence(12),
        gap: Some(ManagedRuntimeChangeGap {
            requested_after: Some(RuntimeChangeSequence(4)),
            earliest_available: RuntimeChangeSequence(9),
            latest_available: RuntimeChangeSequence(12),
            snapshot_revision: RuntimeProjectionRevision(8),
        }),
    };

    CanonicalFrontendFixture {
        snapshots,
        change_page,
        gap_page,
    }
}

#[test]
fn app_server_projection_serializes_the_canonical_runtime_contract_losslessly() {
    let fixture = canonical_frontend_fixture();
    for snapshot in fixture.snapshots.values() {
        assert_eq!(
            consume_managed_runtime_snapshot(snapshot.clone()).expect("canonical snapshot"),
            *snapshot
        );
    }
    assert_eq!(
        consume_managed_runtime_change_page(fixture.change_page.clone())
            .expect("canonical change page"),
        fixture.change_page
    );
    assert!(managed_runtime_change_page_requires_snapshot_reload(
        &fixture.gap_page
    ));
}

#[test]
fn frontend_fixture_is_the_exact_canonical_rust_serialization() {
    let expected: CanonicalFrontendFixture = serde_json::from_str(include_str!(
        "../../../packages/app-web/src/features/session/model/fixtures/managedRuntimeProjection.json"
    ))
    .expect("typed frontend fixture");
    assert_eq!(expected, canonical_frontend_fixture());
}
