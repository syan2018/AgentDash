//! Target-only App Server projection evidence.
//!
//! The production router remains unchanged. This test freezes the response
//! shape that the S5 composition root will expose from Runtime-owned state.

use agentdash_application_agentrun::agent_run::target_product_protocol::{
    AgentRunRuntimeChangePage, AgentRunRuntimeCommittedChange,
    AgentRunRuntimeCommittedChangePayload, AgentRunRuntimeFeedSnapshot,
    AgentRunTargetApiProjection, RuntimeCompactionLifecycle, RuntimeFeedItem,
    RuntimeFeedReduceOutcome, reduce_runtime_change_page,
};

fn snapshot() -> AgentRunRuntimeFeedSnapshot {
    AgentRunRuntimeFeedSnapshot::new(
        "runtime-source-child".to_owned(),
        12,
        20,
        Some("cursor-20".to_owned()),
        vec![RuntimeFeedItem {
            item_id: "child-item".to_owned(),
            turn_id: "child-turn".to_owned(),
            kind: "message".to_owned(),
            text: "child history".to_owned(),
        }],
        RuntimeCompactionLifecycle::Idle,
    )
}

#[test]
fn app_server_projection_is_built_from_target_runtime_snapshot() {
    let json = serde_json::to_value(AgentRunTargetApiProjection::from(snapshot()))
        .expect("serialize target projection");

    assert_eq!(json["source_coordinate"], "runtime-source-child");
    assert_eq!(json["revision"], 12);
    assert_eq!(json["feed"][0]["text"], "child history");
    assert_eq!(json["compaction"]["state"], "idle");
    assert_eq!(json["availability"]["submit_input"], true);
    assert!(json.get("journal_segments").is_none());
    assert!(json.get("ancestor_run_id").is_none());
}

#[test]
fn app_server_change_projection_exposes_compaction_lost_and_availability() {
    let outcome = reduce_runtime_change_page(
        snapshot(),
        AgentRunRuntimeChangePage {
            source_coordinate: "runtime-source-child".to_owned(),
            after_cursor: Some("cursor-20".to_owned()),
            next_cursor: Some("cursor-21".to_owned()),
            gap: false,
            changes: vec![AgentRunRuntimeCommittedChange {
                sequence: 21,
                previous_snapshot_revision: 12,
                snapshot_revision: 13,
                payload: AgentRunRuntimeCommittedChangePayload::CompactionLost {
                    operation_id: "compact-7".to_owned(),
                    reason: "inspection horizon expired".to_owned(),
                },
            }],
        },
    )
    .expect("reduce");
    let RuntimeFeedReduceOutcome::Applied(snapshot) = outcome else {
        panic!("change must apply");
    };
    let json = serde_json::to_value(AgentRunTargetApiProjection::from(snapshot))
        .expect("serialize target projection");

    assert_eq!(json["compaction"]["state"], "lost");
    assert_eq!(json["availability"]["submit_input"], false);
    assert_eq!(json["availability"]["reason"], "compaction_state_lost");
}

#[test]
fn app_server_projection_preserves_each_compaction_lifecycle_fact() {
    let cases = [
        (
            RuntimeCompactionLifecycle::Started {
                operation_id: "compact-1".to_owned(),
            },
            "started",
            false,
        ),
        (
            RuntimeCompactionLifecycle::Completed {
                operation_id: "compact-1".to_owned(),
            },
            "completed",
            true,
        ),
        (
            RuntimeCompactionLifecycle::Failed {
                operation_id: "compact-1".to_owned(),
                reason: "rejected".to_owned(),
            },
            "failed",
            true,
        ),
        (
            RuntimeCompactionLifecycle::Lost {
                operation_id: "compact-1".to_owned(),
                reason: "unknown final state".to_owned(),
            },
            "lost",
            false,
        ),
    ];

    for (compaction, state, submit_input) in cases {
        let projection = AgentRunTargetApiProjection::from(AgentRunRuntimeFeedSnapshot::new(
            "runtime-source-child".to_owned(),
            12,
            20,
            Some("cursor-20".to_owned()),
            Vec::new(),
            compaction,
        ));
        let json = serde_json::to_value(projection).expect("serialize");
        assert_eq!(json["compaction"]["state"], state);
        assert_eq!(json["availability"]["submit_input"], submit_input);
    }
}
