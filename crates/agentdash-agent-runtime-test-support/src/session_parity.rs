//! Strict presentation-event parity support.
//!
//! The normalizers in this module understand only the two explicitly allowed
//! transport placements. They never traverse or rewrite the protected event
//! body, so JSON object fields, explicit nulls, array order, scalar types, IDs,
//! and timestamps all participate in equality.

use serde::Deserialize;
use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresentationDurability {
    Durable,
    Ephemeral,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NormalizedPresentationEvent {
    pub durability: PresentationDurability,
    pub event: Value,
}

#[derive(Debug, Error, PartialEq)]
pub enum SessionParityError {
    #[error("session parity frame must be a JSON object")]
    FrameMustBeObject,
    #[error("main session frame does not match the allowlisted typed wrapper")]
    InvalidMainSessionWrapper,
    #[error("current session frame does not match the allowlisted typed wrapper")]
    InvalidCurrentPresentationWrapper,
    #[error("unsupported main NDJSON control or event frame type: {0}")]
    UnsupportedMainFrame(String),
    #[error("presentation event count differs: main={main}, current={current}")]
    EventCount { main: usize, current: usize },
    #[error("presentation durability differs at index {index}: main={main:?}, current={current:?}")]
    Durability {
        index: usize,
        main: PresentationDurability,
        current: PresentationDurability,
    },
    #[error("protected presentation body differs at index {index}")]
    ProtectedBody {
        index: usize,
        main: Value,
        current: Value,
    },
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[allow(dead_code)]
struct MainSourceInfo {
    connector_id: String,
    connector_type: String,
    executor_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[allow(dead_code)]
struct MainTraceInfo {
    turn_id: Option<String>,
    entry_index: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[allow(dead_code)]
struct MainNotification {
    session_id: String,
    source: MainSourceInfo,
    trace: MainTraceInfo,
    observed_at: String,
    event: Value,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct MainSessionFrame {
    #[serde(rename = "type")]
    frame_type: Option<String>,
    session_id: String,
    event_seq: u64,
    occurred_at_ms: i64,
    committed_at_ms: i64,
    session_update_type: String,
    turn_id: Option<String>,
    entry_index: Option<u32>,
    tool_call_id: Option<String>,
    notification: MainNotification,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct CurrentPresentationFrame {
    runtime_thread_id: String,
    runtime_revision: u64,
    durable_sequence: u64,
    presentation_event: Value,
}

/// Unwraps the main `SessionEventResponse.notification.event` placement.
///
/// Wrapper fields are deserialized and discarded as a unit. The event itself
/// is moved out without field-level normalization.
pub fn normalize_main_session_event(
    frame: Value,
    durability: PresentationDurability,
) -> Result<NormalizedPresentationEvent, SessionParityError> {
    if !frame.is_object() {
        return Err(SessionParityError::FrameMustBeObject);
    }
    let frame: MainSessionFrame =
        serde_json::from_value(frame).map_err(|_| SessionParityError::InvalidMainSessionWrapper)?;
    Ok(NormalizedPresentationEvent {
        durability,
        event: frame.notification.event,
    })
}

/// Unwraps the current immutable carrier's `presentation_event` placement.
///
/// This is intentionally a separate typed entry point: accepting both shapes
/// in one permissive parser would hide placement regressions.
pub fn normalize_current_presentation_event(
    frame: Value,
    durability: PresentationDurability,
) -> Result<NormalizedPresentationEvent, SessionParityError> {
    if !frame.is_object() {
        return Err(SessionParityError::FrameMustBeObject);
    }
    let frame: CurrentPresentationFrame = serde_json::from_value(frame)
        .map_err(|_| SessionParityError::InvalidCurrentPresentationWrapper)?;
    Ok(NormalizedPresentationEvent {
        durability,
        event: frame.presentation_event,
    })
}

/// Normalizes a main NDJSON event frame and keeps control frames on a separate
/// channel by returning `None` for `connected` and `heartbeat`.
pub fn normalize_main_ndjson_frame(
    frame: Value,
) -> Result<Option<NormalizedPresentationEvent>, SessionParityError> {
    let Some(frame_type) = frame.get("type").and_then(Value::as_str) else {
        return Err(SessionParityError::UnsupportedMainFrame(
            "<missing>".to_string(),
        ));
    };
    match frame_type {
        "event" => normalize_main_session_event(frame, PresentationDurability::Durable).map(Some),
        "ephemeral_event" => {
            normalize_main_session_event(frame, PresentationDurability::Ephemeral).map(Some)
        }
        "connected" | "heartbeat" => Ok(None),
        other => Err(SessionParityError::UnsupportedMainFrame(other.to_string())),
    }
}

/// Compares ordered presentation streams without an ignore list or semantic
/// coercion. `serde_json::Value` equality preserves null-vs-omitted, scalar
/// types, and array order.
pub fn compare_ordered_presentation_events(
    main: &[NormalizedPresentationEvent],
    current: &[NormalizedPresentationEvent],
) -> Result<(), SessionParityError> {
    if main.len() != current.len() {
        return Err(SessionParityError::EventCount {
            main: main.len(),
            current: current.len(),
        });
    }
    for (index, (main, current)) in main.iter().zip(current).enumerate() {
        if main.durability != current.durability {
            return Err(SessionParityError::Durability {
                index,
                main: main.durability,
                current: current.durability,
            });
        }
        if main.event != current.event {
            return Err(SessionParityError::ProtectedBody {
                index,
                main: main.event.clone(),
                current: current.event.clone(),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{BTreeMap, BTreeSet},
        fs,
        io::Read,
        path::{Path, PathBuf},
        process::Command,
    };

    use base64::Engine;
    use flate2::read::GzDecoder;
    use serde_json::{Value, json};
    use sha2::{Digest, Sha256};

    use super::*;

    fn main_frame(event: Value) -> Value {
        json!({
            "session_id": "main-session",
            "event_seq": 41,
            "occurred_at_ms": 1720000000000_i64,
            "committed_at_ms": 1720000000001_i64,
            "session_update_type": "notification",
            "notification": {
                "sessionId": "source-session",
                "source": { "connectorId": "native", "connectorType": "pi_agent" },
                "trace": { "turnId": "turn-1", "entryIndex": 0 },
                "observedAt": "2026-07-10T12:00:00Z",
                "event": event
            }
        })
    }

    fn current_frame(event: Value) -> Value {
        json!({
            "runtime_thread_id": "runtime-thread",
            "runtime_revision": 9,
            "durable_sequence": 300,
            "presentation_event": event
        })
    }

    fn event(event_type: &str, payload: Value) -> Value {
        json!({ "type": event_type, "payload": payload })
    }

    fn normalized(event: Value) -> NormalizedPresentationEvent {
        NormalizedPresentationEvent {
            durability: PresentationDurability::Durable,
            event,
        }
    }

    fn assert_protected_body_mismatch(
        main: &[NormalizedPresentationEvent],
        current: &[NormalizedPresentationEvent],
    ) {
        assert!(matches!(
            compare_ordered_presentation_events(main, current),
            Err(SessionParityError::ProtectedBody { .. })
        ));
    }

    #[test]
    fn wrapper_difference_is_allowed_but_body_is_untouched() {
        let body = event(
            "user_input_submitted",
            json!({
                "turn_id": "turn-1",
                "input": [{ "type": "text", "text": "hello" }],
                "nullable": null
            }),
        );
        let main =
            normalize_main_session_event(main_frame(body.clone()), PresentationDurability::Durable)
                .expect("main wrapper");
        let current = normalize_current_presentation_event(
            current_frame(body.clone()),
            PresentationDurability::Durable,
        )
        .expect("current wrapper");

        compare_ordered_presentation_events(&[main], &[current]).expect("protected body parity");
        assert_eq!(body, current_frame(body.clone())["presentation_event"]);
    }

    #[test]
    fn unknown_wrapper_fields_are_rejected() {
        let body = event("turn_started", json!({ "turn": { "id": "turn-1" } }));
        let mut main = main_frame(body.clone());
        main["unlisted_wrapper_field"] = json!(true);
        assert_eq!(
            normalize_main_session_event(main, PresentationDurability::Durable),
            Err(SessionParityError::InvalidMainSessionWrapper)
        );

        let mut current = current_frame(body);
        current["unlisted_wrapper_field"] = json!(true);
        assert_eq!(
            normalize_current_presentation_event(current, PresentationDurability::Durable),
            Err(SessionParityError::InvalidCurrentPresentationWrapper)
        );
    }

    #[test]
    fn nested_main_wrapper_fields_and_observed_at_are_typed() {
        let body = event("turn_started", json!({ "turn": { "id": "turn-1" } }));

        let mut unknown_source = main_frame(body.clone());
        unknown_source["notification"]["source"]["unlisted"] = json!(true);
        assert_eq!(
            normalize_main_session_event(unknown_source, PresentationDurability::Durable),
            Err(SessionParityError::InvalidMainSessionWrapper)
        );

        let mut unknown_trace = main_frame(body.clone());
        unknown_trace["notification"]["trace"]["unlisted"] = json!(true);
        assert_eq!(
            normalize_main_session_event(unknown_trace, PresentationDurability::Durable),
            Err(SessionParityError::InvalidMainSessionWrapper)
        );

        let mut invalid_observed_at = main_frame(body);
        invalid_observed_at["notification"]["observedAt"] = json!(1720000000000_i64);
        assert_eq!(
            normalize_main_session_event(invalid_observed_at, PresentationDurability::Durable),
            Err(SessionParityError::InvalidMainSessionWrapper)
        );
    }

    #[test]
    fn wrapper_named_fields_inside_protected_body_are_not_removed() {
        let main = normalized(event(
            "platform",
            json!({ "runtime_revision": 7, "trace": null, "durable_sequence": 3 }),
        ));
        let current = normalized(event(
            "platform",
            json!({ "runtime_revision": 8, "trace": null, "durable_sequence": 3 }),
        ));
        assert_protected_body_mismatch(&[main], &[current]);
    }

    #[test]
    fn ndjson_controls_are_not_presentation_events() {
        assert_eq!(
            normalize_main_ndjson_frame(json!({ "type": "connected", "last_event_id": 1 }))
                .expect("connected"),
            None
        );
        assert_eq!(
            normalize_main_ndjson_frame(json!({ "type": "heartbeat", "timestamp": 1 }))
                .expect("heartbeat"),
            None
        );
        let normalized = normalize_main_ndjson_frame({
            let mut value =
                main_frame(event("turn_started", json!({ "turn": { "id": "turn-1" } })));
            value["type"] = json!("event");
            value
        })
        .expect("event")
        .expect("presentation");
        assert_eq!(normalized.durability, PresentationDurability::Durable);
    }

    #[test]
    fn missing_and_added_events_fail() {
        let first = normalized(event("user_input_submitted", json!({ "id": "input-1" })));
        let second = normalized(event("turn_started", json!({ "id": "turn-1" })));
        assert!(matches!(
            compare_ordered_presentation_events(&[first.clone(), second.clone()], &[first.clone()]),
            Err(SessionParityError::EventCount {
                main: 2,
                current: 1
            })
        ));
        assert!(matches!(
            compare_ordered_presentation_events(&[first.clone()], &[first, second]),
            Err(SessionParityError::EventCount {
                main: 1,
                current: 2
            })
        ));
    }

    #[test]
    fn reordered_events_fail() {
        let first = normalized(event("user_input_submitted", json!({ "id": "input-1" })));
        let second = normalized(event("turn_started", json!({ "id": "turn-1" })));
        assert_protected_body_mismatch(&[first.clone(), second.clone()], &[second, first]);
    }

    #[test]
    fn changed_id_fails() {
        assert_protected_body_mismatch(
            &[normalized(event(
                "item_started",
                json!({ "item": { "id": "item-main" } }),
            ))],
            &[normalized(event(
                "item_started",
                json!({ "item": { "id": "item-current" } }),
            ))],
        );
    }

    #[test]
    fn changed_timestamp_fails() {
        assert_protected_body_mismatch(
            &[normalized(event(
                "platform",
                json!({ "kind": "status", "data": { "timestamp": 10 } }),
            ))],
            &[normalized(event(
                "platform",
                json!({ "kind": "status", "data": { "timestamp": 11 } }),
            ))],
        );
    }

    #[test]
    fn number_to_string_fails() {
        assert_protected_body_mismatch(
            &[normalized(event(
                "token_usage_updated",
                json!({ "total": 42 }),
            ))],
            &[normalized(event(
                "token_usage_updated",
                json!({ "total": "42" }),
            ))],
        );
    }

    #[test]
    fn null_to_omitted_fails() {
        assert_protected_body_mismatch(
            &[normalized(event(
                "error",
                json!({ "code": null, "message": "failed" }),
            ))],
            &[normalized(event("error", json!({ "message": "failed" })))],
        );
    }

    #[test]
    fn reordered_array_fails() {
        assert_protected_body_mismatch(
            &[normalized(event(
                "turn_plan_updated",
                json!({ "steps": ["one", "two"] }),
            ))],
            &[normalized(event(
                "turn_plan_updated",
                json!({ "steps": ["two", "one"] }),
            ))],
        );
    }

    #[test]
    fn durability_change_fails() {
        let main = normalized(event("agent_message_delta", json!({ "delta": "hi" })));
        let mut current = main.clone();
        current.durability = PresentationDurability::Ephemeral;
        assert!(matches!(
            compare_ordered_presentation_events(&[main], &[current]),
            Err(SessionParityError::Durability { index: 0, .. })
        ));
    }

    #[test]
    fn fixture_main_golden_normalizes_in_order() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../fixtures/session-parity/main/user-submit.json"
        ))
        .expect("valid golden fixture");
        let events = fixture["frames"]
            .as_array()
            .expect("fixture frames")
            .iter()
            .cloned()
            .map(normalize_main_ndjson_frame)
            .collect::<Result<Vec<_>, _>>()
            .expect("normalizable fixture")
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event["type"], "user_input_submitted");
        assert_eq!(events[1].event["type"], "turn_started");
    }

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn fixture_provenance(fixture: &Value) -> Option<&str> {
        fixture
            .get("oracle_commit")
            .or_else(|| fixture.pointer("/provenance/oracle_commit"))
            .and_then(Value::as_str)
    }

    fn sha256_file(path: &Path) -> String {
        let bytes = fs::read(path)
            .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
        format!("{:x}", Sha256::digest(bytes))
    }

    fn assert_oracle_source_hash(
        fixture_id: &str,
        fixture: &Value,
        reference_root: &Path,
        path_pointer: &str,
        hash_pointer: &str,
    ) -> bool {
        let Some(source_path) = fixture.pointer(path_pointer).and_then(Value::as_str) else {
            assert!(
                fixture.pointer(hash_pointer).is_none(),
                "fixture {fixture_id} has {hash_pointer} without {path_pointer}"
            );
            return false;
        };
        let Some(expected_hash) = fixture.pointer(hash_pointer).and_then(Value::as_str) else {
            panic!("fixture {fixture_id} has {path_pointer} without {hash_pointer}");
        };
        let source_path = reference_root.join(source_path);
        assert!(
            source_path.is_file(),
            "fixture {fixture_id} Main source is absent: {}",
            source_path.display()
        );
        assert_eq!(
            sha256_file(&source_path),
            expected_hash,
            "fixture {fixture_id} Main source hash drifted: {}",
            source_path.display()
        );
        true
    }

    fn assert_oracle_source_blob(
        fixture_id: &str,
        fixture: &Value,
        reference_root: &Path,
        oracle_commit: &str,
        path_pointer: &str,
        blob_pointer: &str,
    ) {
        let Some(expected_blob) = fixture.pointer(blob_pointer).and_then(Value::as_str) else {
            return;
        };
        let source_path = fixture
            .pointer(path_pointer)
            .and_then(Value::as_str)
            .unwrap_or_else(|| {
                panic!("fixture {fixture_id} has {blob_pointer} without {path_pointer}")
            });
        let actual_blob = oracle_blob(reference_root, oracle_commit, source_path)
            .unwrap_or_else(|error| panic!("cannot resolve Main blob for {fixture_id}: {error}"));
        assert_eq!(
            actual_blob, expected_blob,
            "fixture {fixture_id} Main source blob drifted: {source_path}"
        );
    }

    fn oracle_blob(
        reference_root: &Path,
        oracle_commit: &str,
        source_path: &str,
    ) -> Result<String, String> {
        let output = Command::new("git")
            .args(["-C"])
            .arg(reference_root)
            .args(["rev-parse", &format!("{oracle_commit}:{source_path}")])
            .output()
            .map_err(|error| error.to_string())?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).into_owned());
        }
        String::from_utf8(output.stdout)
            .map(|blob| blob.trim().to_string())
            .map_err(|error| error.to_string())
    }

    fn agent_run_route_registrations(source: &str) -> BTreeSet<(String, String)> {
        let lines = source.lines().map(str::trim).collect::<Vec<_>>();
        lines
            .windows(2)
            .filter_map(|pair| {
                let path = pair[0].strip_prefix('"')?.strip_suffix("\",")?;
                if !path.starts_with('/') || !pair[1].starts_with("axum::routing::") {
                    return None;
                }
                Some((path.to_string(), pair[1].trim_end_matches(',').to_string()))
            })
            .collect()
    }

    #[test]
    fn pinned_main_has_no_agentdash_interaction_response_route() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../fixtures/session-parity/main/interaction-extension-boundary.json"
        ))
        .expect("interaction extension fixture");
        let reference = fs::read_to_string(repo_root().join(
            "../AgentDash-main-reference/crates/agentdash-api/src/routes/lifecycle_agents.rs",
        ))
        .expect("pinned Main lifecycle routes");
        assert!(
            !reference.contains(
                fixture["main_absence"]["path"]
                    .as_str()
                    .expect("absent route path")
            )
        );
        assert!(
            !reference.contains(
                fixture["main_absence"]["handler"]
                    .as_str()
                    .expect("absent route handler")
            )
        );
    }

    #[test]
    fn current_preserves_every_pinned_main_agent_run_route_registration() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../fixtures/session-parity/main/interaction-extension-boundary.json"
        ))
        .expect("interaction extension fixture");
        let reference = fs::read_to_string(repo_root().join(
            "../AgentDash-main-reference/crates/agentdash-api/src/routes/lifecycle_agents.rs",
        ))
        .expect("pinned Main lifecycle routes");
        let current = fs::read_to_string(
            repo_root().join("crates/agentdash-api/src/routes/lifecycle_agents.rs"),
        )
        .expect("current lifecycle routes");
        let reference_routes = agent_run_route_registrations(&reference);
        let current_routes = agent_run_route_registrations(&current);
        assert_eq!(
            reference_routes.len() as u64,
            fixture["protected_main_surface"]["route_registration_count"]
                .as_u64()
                .expect("protected route count")
        );
        let missing = reference_routes
            .difference(&current_routes)
            .cloned()
            .collect::<Vec<_>>();
        assert!(
            missing.is_empty(),
            "current must preserve every pinned Main path/method/handler registration: {missing:?}"
        );
    }

    fn completion_errors(
        scenarios: &[Value],
        evidence_by_id: &BTreeMap<&str, &Value>,
        completion_groups: &BTreeMap<&str, Vec<&str>>,
        final_complete: bool,
    ) -> Vec<String> {
        let mut errors = Vec::new();
        for scenario in scenarios {
            let id = scenario["id"].as_str().expect("scenario id");
            let status = scenario["status"].as_str().expect("scenario status");
            let evidence_ids = scenario
                .get("evidence_ids")
                .and_then(Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or_default();
            let mut has_complete_evidence = false;
            for evidence_id in evidence_ids {
                let evidence_id = evidence_id.as_str().expect("evidence id");
                let Some(evidence) = evidence_by_id.get(evidence_id) else {
                    errors.push(format!("{id}: missing evidence {evidence_id}"));
                    continue;
                };
                has_complete_evidence |= evidence["complete_scenarios"]
                    .as_array()
                    .expect("complete scenarios")
                    .iter()
                    .any(|candidate| candidate == id);
            }
            if let Some(group) = completion_groups.get(id) {
                has_complete_evidence |= group.iter().all(|evidence_id| {
                    evidence_by_id.get(evidence_id).is_some_and(|evidence| {
                        ["complete_scenarios", "partial_scenarios"]
                            .iter()
                            .any(|field| {
                                evidence[*field]
                                    .as_array()
                                    .expect("group scenario evidence")
                                    .iter()
                                    .any(|candidate| candidate == id)
                            })
                    })
                });
            }
            match status {
                "golden_verified" if !has_complete_evidence => {
                    errors.push(format!("{id}: no complete strict evidence"));
                }
                "planned" if has_complete_evidence => {
                    errors.push(format!("{id}: complete evidence is still marked planned"));
                }
                "planned" if final_complete => {
                    errors.push(format!("{id}: remains planned"));
                }
                "planned" | "golden_verified" | "extension_verified" => {}
                other => errors.push(format!("{id}: unsupported status {other}")),
            }
        }
        errors
    }

    fn load_evidence_documents() -> (Value, Value, Value) {
        let catalog: Value = serde_json::from_str(include_str!(
            "../fixtures/session-parity/scenario-catalog.json"
        ))
        .expect("valid scenario catalog");
        let inventory: Value =
            serde_json::from_str(include_str!("../fixtures/session-parity/inventory.json"))
                .expect("valid inventory");
        let manifest: Value = serde_json::from_str(include_str!(
            "../fixtures/session-parity/evidence-manifest.json"
        ))
        .expect("valid evidence manifest");
        (catalog, inventory, manifest)
    }

    #[test]
    fn central_evidence_manifest_is_truthful_and_complete() {
        let (catalog, inventory, manifest) = load_evidence_documents();
        let oracle = catalog["oracle_commit"].as_str().expect("catalog oracle");
        let oracle_manifest_path = repo_root().join("scripts/session-parity/oracle-manifest.json");
        let oracle_manifest: Value =
            serde_json::from_slice(&fs::read(&oracle_manifest_path).unwrap_or_else(|error| {
                panic!("cannot read {}: {error}", oracle_manifest_path.display())
            }))
            .expect("valid oracle manifest");
        assert_eq!(oracle_manifest["oracle_commit"], oracle);
        let reference_root = PathBuf::from(
            oracle_manifest["reference_path"]
                .as_str()
                .expect("oracle reference path"),
        );
        assert!(
            reference_root.is_dir(),
            "Main oracle reference is absent: {}",
            reference_root.display()
        );
        let oracle_sources = oracle_manifest["source_files"]
            .as_array()
            .expect("oracle source files")
            .iter()
            .map(|source| {
                (
                    source["path"].as_str().expect("oracle source path"),
                    source["sha256"].as_str().expect("oracle source hash"),
                )
            })
            .collect::<BTreeMap<_, _>>();
        assert_eq!(inventory["oracle_commit"], oracle);
        assert_eq!(manifest["oracle_commit"], oracle);
        assert_eq!(inventory["status"], "final_complete");
        assert_eq!(inventory["evidence_manifest"], "evidence-manifest.json");
        assert_eq!(manifest["mode"], "final_complete");

        let scenarios = catalog["scenarios"].as_array().expect("scenario list");
        let fixture_ids = scenarios
            .iter()
            .map(|scenario| {
                assert!(
                    scenario["owner"]
                        .as_str()
                        .is_some_and(|owner| !owner.is_empty()),
                    "scenario owner must be explicit: {scenario}"
                );
                assert!(
                    matches!(
                        scenario["status"].as_str(),
                        Some("planned" | "golden_verified" | "extension_verified")
                    ),
                    "scenario status must distinguish planned from verified: {scenario}"
                );
                scenario["id"].as_str().expect("scenario id").to_string()
            })
            .collect::<BTreeSet<_>>();

        for section in [
            "backbone_events",
            "platform_events",
            "drivers",
            "tool_families",
            "application_producers",
            "frontend_surfaces",
        ] {
            for row in inventory[section].as_array().expect("inventory section") {
                assert!(
                    row.get("owner")
                        .or_else(|| row.get("current_owner"))
                        .and_then(Value::as_str)
                        .is_some_and(|owner| !owner.is_empty()),
                    "inventory owner must be explicit in {section}: {row}"
                );
                let referenced = row
                    .get("fixture_ids")
                    .and_then(Value::as_array)
                    .map(|values| values.iter().collect::<Vec<_>>())
                    .unwrap_or_else(|| vec![&row["fixture_id"]]);
                for fixture_id in referenced {
                    let fixture_id = fixture_id.as_str().expect("fixture id");
                    assert!(
                        fixture_ids.contains(fixture_id),
                        "inventory fixture {fixture_id} is absent from scenario catalog"
                    );
                }
            }
        }

        let evidence = manifest["evidence"].as_array().expect("evidence list");
        let manifest_fixture_paths = evidence
            .iter()
            .map(|row| row["fixture"].as_str().expect("evidence fixture path"))
            .collect::<BTreeSet<_>>();
        let mut evidence_by_id = BTreeMap::new();
        for row in evidence {
            let id = row["id"].as_str().expect("evidence id");
            assert!(
                evidence_by_id.insert(id, row).is_none(),
                "duplicate evidence id: {id}"
            );

            let fixture_path = repo_root().join(row["fixture"].as_str().expect("fixture path"));
            let fixture_bytes = fs::read(&fixture_path)
                .unwrap_or_else(|error| panic!("cannot read {}: {error}", fixture_path.display()));
            assert_eq!(
                format!("{:x}", Sha256::digest(&fixture_bytes)),
                row["fixture_sha256"].as_str().expect("fixture hash"),
                "fixture provenance hash drifted: {id}"
            );
            let fixture: Value = serde_json::from_slice(&fixture_bytes).expect("fixture JSON");
            let fixture_oracle = fixture_provenance(&fixture).expect("fixture oracle provenance");
            assert!(
                oracle.starts_with(fixture_oracle),
                "fixture {id} is not pinned to catalog oracle {oracle}: {fixture_oracle}"
            );
            assert_eq!(
                row["main_capture"], "fixture_provenance",
                "evidence {id} must name its Main capture source"
            );
            let capture_method = fixture
                .get("capture_method")
                .or_else(|| fixture.pointer("/provenance/capture_method"))
                .and_then(Value::as_str)
                .filter(|method| !method.trim().is_empty())
                .unwrap_or_else(|| panic!("fixture {id} has no Main capture method"));
            let capture_method_lower = capture_method.to_ascii_lowercase();
            for forbidden in [
                "self_roundtrip",
                "self roundtrip",
                "fixture_equals_itself",
                "fixture equals itself",
                "current shape",
                "current dto",
            ] {
                assert!(
                    !capture_method_lower.contains(forbidden),
                    "fixture {id} uses forbidden self-proving capture method: {capture_method}"
                );
            }
            let mut verified_source_hashes = 0;
            for (path_pointer, hash_pointer) in [
                ("/oracle_source_path", "/oracle_source_sha256"),
                ("/oracle_test_source", "/oracle_test_source_sha256"),
            ] {
                verified_source_hashes += usize::from(assert_oracle_source_hash(
                    id,
                    &fixture,
                    &reference_root,
                    path_pointer,
                    hash_pointer,
                ));
            }
            for (path_pointer, blob_pointer) in [
                ("/oracle_source_path", "/oracle_source_blob"),
                ("/oracle_test_source", "/oracle_test_source_blob"),
            ] {
                assert_oracle_source_blob(
                    id,
                    &fixture,
                    &reference_root,
                    oracle,
                    path_pointer,
                    blob_pointer,
                );
            }
            if fixture
                .get("source")
                .and_then(Value::as_str)
                .is_some_and(|path| path.starts_with("crates/") || path.starts_with("packages/"))
            {
                verified_source_hashes += usize::from(assert_oracle_source_hash(
                    id,
                    &fixture,
                    &reference_root,
                    "/source",
                    "/source_sha256",
                ));
            } else if let (Some(source_description), Some(source_hash)) = (
                fixture.get("source").and_then(Value::as_str),
                fixture.get("source_sha256").and_then(Value::as_str),
            ) {
                let manifest_source = oracle_sources.iter().find(|(path, hash)| {
                    source_description.contains(**path) && **hash == source_hash
                });
                let (source_path, expected_hash) = if let Some(source) = manifest_source {
                    (*source.0, *source.1)
                } else {
                    let source_path = ["crates/", "packages/"]
                        .iter()
                        .find_map(|prefix| {
                            source_description
                                .find(prefix)
                                .map(|index| &source_description[index..])
                        })
                        .unwrap_or_else(|| {
                            panic!(
                                "fixture {id} descriptive Main source has no repository path: {source_description}"
                            )
                        });
                    (source_path, source_hash)
                };
                assert_eq!(
                    sha256_file(&reference_root.join(source_path)),
                    expected_hash,
                    "fixture {id} Main source hash drifted: {source_path}"
                );
                verified_source_hashes += 1;
            }
            if let Some(producers) = fixture
                .pointer("/provenance/source_producers")
                .and_then(Value::as_array)
            {
                for producer in producers {
                    let (producer, expected_hash) = if let Some(producer) = producer.as_str() {
                        let expected_hash = oracle_sources.get(producer).unwrap_or_else(|| {
                            panic!(
                                "fixture {id} source producer is absent from oracle manifest: {producer}"
                            )
                        });
                        (producer, *expected_hash)
                    } else {
                        let producer_path =
                            producer["path"].as_str().expect("fixture producer path");
                        if let Some(expected_blob) =
                            producer.get("git_blob_sha").and_then(Value::as_str)
                        {
                            assert_eq!(
                                oracle_blob(&reference_root, oracle, producer_path)
                                    .expect("fixture producer Main blob"),
                                expected_blob,
                                "fixture {id} Main source blob drifted: {producer_path}"
                            );
                            verified_source_hashes += 1;
                            continue;
                        }
                        (
                            producer_path,
                            producer["sha256"].as_str().expect("fixture producer hash"),
                        )
                    };
                    assert_eq!(
                        sha256_file(&reference_root.join(producer)),
                        expected_hash,
                        "fixture {id} Main source hash drifted: {producer}"
                    );
                    verified_source_hashes += 1;
                }
            }
            if let Some(sources) = fixture
                .pointer("/provenance/source_files")
                .and_then(Value::as_array)
            {
                for source in sources {
                    let source_path = source["path"].as_str().expect("fixture source path");
                    let expected_hash = source["sha256"].as_str().expect("fixture source hash");
                    assert_eq!(
                        sha256_file(&reference_root.join(source_path)),
                        expected_hash,
                        "fixture {id} Main source hash drifted: {source_path}"
                    );
                    verified_source_hashes += 1;
                }
            }
            if let (Some(source_path), Some(expected_hash)) = (
                fixture
                    .pointer("/provenance/source_path")
                    .and_then(Value::as_str),
                fixture
                    .pointer("/provenance/source_sha256")
                    .and_then(Value::as_str),
            ) {
                assert!(
                    manifest_fixture_paths.contains(source_path),
                    "fixture {id} chained source is not independently registered: {source_path}"
                );
                let source_path = repo_root().join(source_path);
                assert_eq!(
                    sha256_file(&source_path),
                    expected_hash,
                    "fixture {id} chained source hash drifted: {}",
                    source_path.display()
                );
                let source_fixture: Value = serde_json::from_slice(
                    &fs::read(&source_path).expect("read chained source fixture"),
                )
                .expect("valid chained source fixture");
                let source_oracle =
                    fixture_provenance(&source_fixture).expect("chained source oracle provenance");
                assert!(
                    oracle.starts_with(source_oracle),
                    "fixture {id} chained source is not pinned to catalog oracle"
                );
                if let Some(expected_capture_hash) = fixture
                    .pointer("/provenance/source_capture_sha256")
                    .and_then(Value::as_str)
                {
                    assert_eq!(
                        source_fixture["capture_sha256"], expected_capture_hash,
                        "fixture {id} chained capture hash drifted"
                    );
                }
                if let (Some(frames), Some(source_frames)) =
                    (fixture.get("frames"), source_fixture.get("frames"))
                {
                    assert_eq!(
                        frames, source_frames,
                        "fixture {id} must copy chained Main frames verbatim"
                    );
                }
                if let (Some(encoded), Some(source_case), Some(protected_events)) = (
                    source_fixture
                        .get("protected_scenarios")
                        .and_then(Value::as_str),
                    fixture
                        .pointer("/provenance/source_case")
                        .and_then(Value::as_str),
                    fixture.get("protected_events"),
                ) {
                    let compressed = base64::engine::general_purpose::STANDARD
                        .decode(encoded)
                        .expect("decode chained Main capture");
                    let mut decoder = GzDecoder::new(compressed.as_slice());
                    let mut capture = Vec::new();
                    decoder
                        .read_to_end(&mut capture)
                        .expect("decompress chained Main capture");
                    let scenarios: Value =
                        serde_json::from_slice(&capture).expect("parse chained Main capture");
                    let source_scenario = scenarios
                        .as_array()
                        .expect("chained Main scenario list")
                        .iter()
                        .find(|scenario| scenario["fixture_id"] == source_case)
                        .unwrap_or_else(|| {
                            panic!("fixture {id} chained Main case is absent: {source_case}")
                        });
                    assert_eq!(
                        protected_events, &source_scenario["protected_events"],
                        "fixture {id} must copy chained Main protected events verbatim"
                    );
                }
                verified_source_hashes += 1;
            }
            assert!(
                verified_source_hashes > 0,
                "fixture {id} must carry machine-verifiable Main source provenance"
            );

            if let (Some(main_methods), Some(extension_methods), Some(scenarios)) = (
                fixture
                    .get("main_capture_methods")
                    .and_then(Value::as_array),
                fixture
                    .get("protocol_extension_methods")
                    .and_then(Value::as_array),
                fixture.get("scenarios").and_then(Value::as_array),
            ) {
                let source_path = fixture["oracle_source_path"]
                    .as_str()
                    .expect("classified oracle source path");
                let oracle_source = fs::read_to_string(reference_root.join(source_path))
                    .expect("classified oracle source");
                let main_methods = main_methods
                    .iter()
                    .map(|method| method.as_str().expect("Main capture method"))
                    .collect::<BTreeSet<_>>();
                let extension_methods = extension_methods
                    .iter()
                    .map(|method| method.as_str().expect("protocol extension method"))
                    .collect::<BTreeSet<_>>();
                assert!(
                    main_methods.is_disjoint(&extension_methods),
                    "fixture {id} method classifications overlap"
                );
                for method in &main_methods {
                    assert!(
                        oracle_source.contains(&format!("\"{method}\"")),
                        "fixture {id} claims absent Main method {method}"
                    );
                }
                for method in &extension_methods {
                    assert!(
                        !oracle_source.contains(&format!("\"{method}\"")),
                        "fixture {id} misclassifies Main method {method} as an extension"
                    );
                }
                for scenario in scenarios {
                    let method = scenario["method"]
                        .as_str()
                        .expect("fixture scenario method");
                    assert!(
                        main_methods.contains(method) || extension_methods.contains(method),
                        "fixture {id} scenario method is unclassified: {method}"
                    );
                }
            }

            let source_path = repo_root().join(row["test_source"].as_str().expect("test source"));
            let source = fs::read_to_string(&source_path)
                .unwrap_or_else(|error| panic!("cannot read {}: {error}", source_path.display()));
            match row
                .get("test_kind")
                .and_then(Value::as_str)
                .unwrap_or("rust")
            {
                "rust" => {
                    let function = row["test_function"].as_str().expect("test function");
                    assert!(
                        source.contains(&format!("fn {function}")),
                        "named evidence test is absent: {function} in {}",
                        source_path.display()
                    );
                }
                "vitest" => {
                    let test_name = row["test_name"].as_str().expect("Vitest test name");
                    assert!(
                        source.contains(&format!("it(\"{test_name}\"")),
                        "named Vitest evidence is absent: {test_name} in {}",
                        source_path.display()
                    );
                }
                kind => panic!("unsupported evidence test kind {kind}: {id}"),
            }
            if row["strength"] == "strict_main_current_deep_equality" {
                assert!(
                    source.contains("compare_ordered_presentation_events"),
                    "strict evidence must call the protected-body comparator: {id}"
                );
            }
            let has_cargo = row
                .get("cargo_args")
                .and_then(Value::as_array)
                .is_some_and(|args| !args.is_empty());
            let has_command = row
                .get("command")
                .and_then(Value::as_array)
                .is_some_and(|args| !args.is_empty());
            assert_ne!(
                has_cargo, has_command,
                "evidence {id} must declare exactly one executable command"
            );

            for field in ["complete_scenarios", "partial_scenarios"] {
                for scenario_id in row[field].as_array().expect("scenario evidence list") {
                    let scenario_id = scenario_id.as_str().expect("scenario evidence id");
                    assert!(
                        fixture_ids.contains(scenario_id),
                        "evidence {id} references unknown scenario {scenario_id}"
                    );
                }
            }
        }

        let mut completion_groups = BTreeMap::new();
        for group in manifest["completion_groups"]
            .as_array()
            .expect("completion groups")
        {
            let scenario_id = group["scenario"].as_str().expect("group scenario");
            assert!(
                fixture_ids.contains(scenario_id),
                "completion group references unknown scenario {scenario_id}"
            );
            let evidence_ids = group["evidence_ids"]
                .as_array()
                .expect("group evidence ids")
                .iter()
                .map(|value| value.as_str().expect("group evidence id"))
                .collect::<Vec<_>>();
            assert!(
                evidence_ids.len() >= 2,
                "completion group must be composite"
            );
            for evidence_id in &evidence_ids {
                let evidence = evidence_by_id.get(evidence_id).unwrap_or_else(|| {
                    panic!("completion group evidence is absent: {evidence_id}")
                });
                assert!(
                    ["complete_scenarios", "partial_scenarios"]
                        .iter()
                        .any(|field| evidence[*field]
                            .as_array()
                            .expect("scenario evidence")
                            .iter()
                            .any(|candidate| candidate == scenario_id)),
                    "completion group evidence {evidence_id} does not cover {scenario_id}"
                );
            }
            assert!(
                completion_groups
                    .insert(scenario_id, evidence_ids)
                    .is_none(),
                "duplicate completion group: {scenario_id}"
            );
        }

        let extension_roles = [
            ("main_absence_evidence_id", "pinned_main_absence"),
            (
                "current_typed_execution_evidence_id",
                "current_typed_execution",
            ),
            (
                "protected_surface_evidence_id",
                "protected_main_surface_unchanged",
            ),
        ];
        let mut extension_scenarios = BTreeSet::new();
        for group in manifest["extension_groups"]
            .as_array()
            .expect("extension groups")
        {
            let scenario_id = group["scenario"].as_str().expect("extension scenario");
            assert!(
                extension_scenarios.insert(scenario_id),
                "duplicate extension group: {scenario_id}"
            );
            assert_eq!(
                scenarios
                    .iter()
                    .find(|scenario| scenario["id"] == scenario_id)
                    .and_then(|scenario| scenario["status"].as_str()),
                Some("extension_verified"),
                "extension group must target extension_verified scenario"
            );
            for (field, strength) in extension_roles {
                let evidence_id = group[field].as_str().expect("extension evidence id");
                let evidence = evidence_by_id
                    .get(evidence_id)
                    .unwrap_or_else(|| panic!("extension evidence is absent: {evidence_id}"));
                assert_eq!(evidence["strength"], strength);
                assert!(
                    evidence["partial_scenarios"]
                        .as_array()
                        .expect("extension scenario evidence")
                        .iter()
                        .any(|candidate| candidate == scenario_id),
                    "extension evidence {evidence_id} does not cover {scenario_id}"
                );
            }
        }
        for scenario in scenarios
            .iter()
            .filter(|scenario| scenario["status"] == "extension_verified")
        {
            assert!(
                extension_scenarios.contains(scenario["id"].as_str().expect("scenario id")),
                "extension_verified scenario must have a three-proof extension group"
            );
        }

        assert!(
            completion_errors(scenarios, &evidence_by_id, &completion_groups, false).is_empty(),
            "complete evidence must be internally consistent"
        );
        let verified = scenarios
            .iter()
            .filter(|scenario| {
                matches!(
                    scenario["status"].as_str(),
                    Some("golden_verified" | "extension_verified")
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            verified.len(),
            manifest["verified_scenario_count"]
                .as_u64()
                .expect("verified scenario count") as usize,
            "only executable strict evidence is verified"
        );
        assert_eq!(
            scenarios.len() - verified.len(),
            manifest["planned_scenario_count"]
                .as_u64()
                .expect("planned scenario count") as usize,
            "planned scenario count must remain explicit"
        );
        for scenario in verified {
            let fixture = scenario["fixture"].as_str().expect("verified fixture path");
            assert!(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("fixtures/session-parity")
                    .join(fixture)
                    .is_file(),
                "verified fixture must exist: {fixture}"
            );
        }
    }

    #[test]
    fn final_complete_mode_rejects_planned_or_missing_evidence() {
        let (mut catalog, _, manifest) = load_evidence_documents();
        let scenarios = catalog["scenarios"].as_array().expect("scenario list");
        let evidence_by_id = manifest["evidence"]
            .as_array()
            .expect("evidence list")
            .iter()
            .map(|row| (row["id"].as_str().expect("evidence id"), row))
            .collect::<BTreeMap<_, _>>();
        let completion_groups = manifest["completion_groups"]
            .as_array()
            .expect("completion groups")
            .iter()
            .map(|group| {
                (
                    group["scenario"].as_str().expect("group scenario"),
                    group["evidence_ids"]
                        .as_array()
                        .expect("group evidence ids")
                        .iter()
                        .map(|value| value.as_str().expect("group evidence id"))
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let errors = completion_errors(scenarios, &evidence_by_id, &completion_groups, true);
        let planned_count = scenarios
            .iter()
            .filter(|scenario| scenario["status"] == "planned")
            .count();
        assert_eq!(
            planned_count,
            manifest["planned_scenario_count"]
                .as_u64()
                .expect("planned scenario count") as usize
        );
        assert_eq!(errors.len(), planned_count);
        assert!(
            errors
                .iter()
                .all(|error| error.ends_with("remains planned"))
        );

        let first = catalog["scenarios"]
            .as_array_mut()
            .expect("scenario list")
            .first_mut()
            .expect("first scenario");
        first["status"] = json!("golden_verified");
        first["evidence_ids"] = json!(["missing-evidence"]);
        let first_id = first["id"].as_str().expect("first scenario id");
        let mut incomplete_groups = completion_groups.clone();
        incomplete_groups.remove(first_id);
        let errors = completion_errors(
            catalog["scenarios"].as_array().expect("scenario list"),
            &evidence_by_id,
            &incomplete_groups,
            true,
        );
        assert!(
            errors
                .iter()
                .any(|error| error.contains("missing evidence missing-evidence"))
        );
        assert!(
            errors
                .iter()
                .any(|error| error.contains("no complete strict evidence"))
        );
    }
}
