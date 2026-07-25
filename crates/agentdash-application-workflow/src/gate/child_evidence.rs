use serde_json::{Value, json};
use uuid::Uuid;

pub(crate) fn child_evidence_result_refs(
    gate_id: Uuid,
    child_run_id: Uuid,
    child_agent_id: Uuid,
    child_frame_id: Option<Uuid>,
    runtime_thread_id: Option<&str>,
) -> Value {
    let child_frame_id = child_frame_id.map(|id| id.to_string());
    let runtime_thread_id = runtime_thread_id.map(str::to_string);
    let evidence = runtime_thread_id
        .as_deref()
        .map(|session_id| {
            vec![json!({
                "kind": "lifecycle_file",
                "scope": "child_agent_run_messages",
                "child_run_id": child_run_id.to_string(),
                "child_agent_id": child_agent_id.to_string(),
                "child_frame_id": child_frame_id.clone(),
                "runtime_thread_id": session_id,
                "mount_id": "lifecycle",
                "uri": child_messages_uri(child_agent_id),
            })]
        })
        .unwrap_or_default();

    json!({
        "schema_version": 1,
        "gate_id": gate_id.to_string(),
        "child": {
            "run_id": child_run_id.to_string(),
            "agent_id": child_agent_id.to_string(),
            "frame_id": child_frame_id,
            "runtime_thread_id": runtime_thread_id,
        },
        "evidence": evidence,
    })
}

pub(crate) fn child_messages_uri(child_agent_id: Uuid) -> String {
    format!("lifecycle://agent-runs/{child_agent_id}/sessions/messages")
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
            entry.get("kind") == Some(&json!("lifecycle_file"))
                && entry.get("runtime_thread_id") == Some(&json!("child-session"))
                && entry.get("mount_id") == Some(&json!("lifecycle"))
                && entry
                    .get("uri")
                    .and_then(Value::as_str)
                    .is_some_and(|value| value.starts_with("lifecycle://agent-runs/"))
        }));
        assert!(
            !serde_json::to_string(&refs)
                .expect("serialize refs")
                .contains("session/events.json")
        );
        assert!(
            !serde_json::to_string(&refs)
                .expect("serialize refs")
                .contains("\"path\"")
        );
    }
}
