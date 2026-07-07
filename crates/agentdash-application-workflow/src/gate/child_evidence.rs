use serde_json::{Value, json};
use uuid::Uuid;

const LIFECYCLE_EVIDENCE_PATHS: &[&str] = &[
    "session/events.json",
    "session/messages",
    "session/tools",
    "session/turns",
    "session/terminal",
];

pub(crate) fn child_evidence_result_refs(
    gate_id: Uuid,
    child_run_id: Uuid,
    child_agent_id: Uuid,
    child_frame_id: Option<Uuid>,
    delivery_runtime_session_id: Option<&str>,
) -> Value {
    let child_frame_id = child_frame_id.map(|id| id.to_string());
    let delivery_runtime_session_id = delivery_runtime_session_id.map(str::to_string);
    let evidence = delivery_runtime_session_id
        .as_deref()
        .map(|session_id| {
            let mut refs = vec![json!({
                "kind": "agent_run_journal",
                "scope": "child_agent_run",
                "child_run_id": child_run_id.to_string(),
                "child_agent_id": child_agent_id.to_string(),
                "child_frame_id": child_frame_id,
                "delivery_runtime_session_id": session_id,
                "cursor": null,
            })];
            refs.extend(LIFECYCLE_EVIDENCE_PATHS.iter().map(|path| {
                json!({
                    "kind": "lifecycle_file",
                    "scope": "child_delivery_session",
                    "child_run_id": child_run_id.to_string(),
                    "child_agent_id": child_agent_id.to_string(),
                    "child_frame_id": child_frame_id,
                    "delivery_runtime_session_id": session_id,
                    "mount_id": "lifecycle",
                    "path": path,
                })
            }));
            refs.push(json!({
                "kind": "runtime_trace",
                "scope": "child_delivery_session",
                "child_run_id": child_run_id.to_string(),
                "child_agent_id": child_agent_id.to_string(),
                "child_frame_id": child_frame_id,
                "delivery_runtime_session_id": session_id,
            }));
            refs
        })
        .unwrap_or_default();

    json!({
        "schema_version": 1,
        "gate_id": gate_id.to_string(),
        "child": {
            "run_id": child_run_id.to_string(),
            "agent_id": child_agent_id.to_string(),
            "frame_id": child_frame_id,
            "delivery_runtime_session_id": delivery_runtime_session_id,
        },
        "evidence": evidence,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn child_evidence_refs_are_parent_visible_locators() {
        let refs = child_evidence_result_refs(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            Some(Uuid::new_v4()),
            Some("child-session"),
        );

        let evidence = refs
            .get("evidence")
            .and_then(Value::as_array)
            .expect("evidence refs");
        assert!(evidence.iter().any(|entry| {
            entry.get("kind") == Some(&json!("agent_run_journal"))
                && entry.get("delivery_runtime_session_id") == Some(&json!("child-session"))
        }));
        assert!(evidence.iter().any(|entry| {
            entry.get("kind") == Some(&json!("lifecycle_file"))
                && entry.get("mount_id") == Some(&json!("lifecycle"))
                && entry.get("path") == Some(&json!("session/events.json"))
        }));
        assert!(
            evidence
                .iter()
                .any(|entry| entry.get("kind") == Some(&json!("runtime_trace")))
        );
        assert!(
            !serde_json::to_string(&refs)
                .expect("serialize refs")
                .contains("lifecycle://session/")
        );
    }
}
