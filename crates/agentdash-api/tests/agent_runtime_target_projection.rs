//! Target-only App Server projection evidence.
//!
//! The production router remains unchanged. This fixture is serialized
//! directly from the dependency-light Runtime Contract and consumed verbatim
//! by the frontend target test.

use std::{collections::BTreeMap, fs, path::PathBuf};

use agentdash_agent_runtime_contract::managed_projection::{
    ManagedRuntimeAvailabilityEvidence, ManagedRuntimeChangeDelta, ManagedRuntimeChangeGap,
    ManagedRuntimeChangePage, ManagedRuntimeCommandAvailability, ManagedRuntimeCommandKind,
    ManagedRuntimeEntityStatus, ManagedRuntimeItem, ManagedRuntimeItemContent,
    ManagedRuntimeLifecycleStatus, ManagedRuntimeOperation, ManagedRuntimeOperationStatus,
    ManagedRuntimePlatformChange, ManagedRuntimeProjectionAuthority,
    ManagedRuntimeProjectionFidelity, ManagedRuntimeProjectionSchema, ManagedRuntimeSnapshot,
    ManagedRuntimeTurn, ManagedRuntimeUnavailabilityReason,
};
use agentdash_agent_runtime_contract::{
    RuntimeChangeSequence, RuntimeItemId, RuntimeOperationId, RuntimePayloadDigest,
    RuntimeProjectionRevision, RuntimeThreadId, RuntimeTurnId,
};
use agentdash_application_agentrun::agent_run::product_protocol::{
    consume_managed_runtime_change_page, consume_managed_runtime_snapshot,
    managed_runtime_change_page_requires_snapshot_reload,
};
use schemars::schema_for;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const UPDATE_ACTIVATION_ARTIFACTS: &str = "AGENTDASH_UPDATE_W7_ACTIVATION_ARTIFACTS";
const ACTIVATION_ARTIFACT_DIRECTORY: &str = ".trellis/tasks/07-17-agent-runtime-compaction-state-protocol-review/activation/w7-product-protocol";
const FRONTEND_FIXTURE: &str =
    "packages/app-web/src/features/session/model/fixtures/managedRuntimeProjection.json";

#[derive(Debug, PartialEq, Deserialize, Serialize)]
struct CanonicalFrontendFixture {
    snapshots: BTreeMap<String, ManagedRuntimeSnapshot>,
    change_page: ManagedRuntimeChangePage,
    gap_page: ManagedRuntimeChangePage,
}

#[derive(Debug, Serialize)]
struct W7GeneratedActivationManifest {
    generator: &'static str,
    canonical_root: &'static str,
    schema_path: &'static str,
    schema_sha256: String,
    frontend_fixture_path: &'static str,
    frontend_fixture_sha256: String,
    reproduction_command: &'static str,
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
        thread_name: None,
        thread_name_source: None,
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

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn canonical_projection_schema() -> String {
    format!(
        "{}\n",
        serde_json::to_string_pretty(&schema_for!(ManagedRuntimeProjectionSchema))
            .expect("serialize canonical managed Runtime projection schema")
    )
}

fn sha256(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}

fn check_or_update_activation_artifact(path: PathBuf, expected: &str) {
    if std::env::var(UPDATE_ACTIVATION_ARTIFACTS).as_deref() == Ok("1") {
        fs::create_dir_all(path.parent().expect("activation artifact parent"))
            .expect("create activation artifact directory");
        fs::write(path, expected).expect("write activation artifact");
        return;
    }
    assert_eq!(
        fs::read_to_string(&path).unwrap_or_else(|error| {
            panic!(
                "read {}: {error}; run the documented W7 activation artifact generator",
                path.display()
            )
        }),
        expected,
        "{} is out of date",
        path.display()
    );
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

#[test]
fn task_local_generated_artifacts_match_canonical_schema_and_frontend_fixture() {
    let root = repository_root();
    let schema = canonical_projection_schema();
    let fixture = fs::read(root.join(FRONTEND_FIXTURE)).expect("read canonical frontend fixture");
    let manifest = W7GeneratedActivationManifest {
        generator: "schemars::schema_for!(ManagedRuntimeProjectionSchema)",
        canonical_root: "agentdash_agent_runtime_contract::managed_projection::ManagedRuntimeProjectionSchema",
        schema_path: "managed-runtime-projection.schema.json",
        schema_sha256: sha256(schema.as_bytes()),
        frontend_fixture_path: FRONTEND_FIXTURE,
        frontend_fixture_sha256: sha256(&fixture),
        reproduction_command: "$env:AGENTDASH_UPDATE_W7_ACTIVATION_ARTIFACTS='1'; cargo test -p agentdash-api --test agent_runtime_target_projection task_local_generated_artifacts_match_canonical_schema_and_frontend_fixture -- --exact",
    };
    let manifest = format!(
        "{}\n",
        serde_json::to_string_pretty(&manifest).expect("serialize activation manifest")
    );
    let artifact_root = root.join(ACTIVATION_ARTIFACT_DIRECTORY);
    check_or_update_activation_artifact(
        artifact_root.join("managed-runtime-projection.schema.json"),
        &schema,
    );
    check_or_update_activation_artifact(artifact_root.join("manifest.json"), &manifest);

    let schema: serde_json::Value =
        serde_json::from_str(&schema).expect("generated canonical schema");
    assert!(schema.pointer("/properties/snapshot").is_some());
    assert!(schema.pointer("/properties/change_page").is_some());
    assert!(schema.pointer("/$defs/ManagedRuntimeChangeGap").is_some());
    assert!(
        schema
            .pointer("/$defs/ManagedRuntimeCommandAvailability")
            .is_some()
    );
}
