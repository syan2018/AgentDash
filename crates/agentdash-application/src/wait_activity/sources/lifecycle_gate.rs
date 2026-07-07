use agentdash_domain::workflow::LifecycleGate;
use serde_json::{Map, Value, json};

use crate::wait_activity::types::{ResolvedWaitScope, WaitActivityItem};
use agentdash_application_ports::agent_run_control_effect::RuntimeTerminalDiagnostic;

pub(crate) fn gate_belongs_to_scope(gate: &LifecycleGate, scope: &ResolvedWaitScope) -> bool {
    scope.run_id.is_none_or(|run_id| gate.run_id == run_id)
}

pub(crate) fn gate_item_from_gate(gate: &LifecycleGate) -> WaitActivityItem {
    let projection = gate.waiting_projection();
    let diagnostic = gate_terminal_diagnostic(gate);
    let result_refs = gate_result_refs(gate);
    WaitActivityItem {
        activity_ref: gate.id.to_string(),
        kind: projection.kind,
        status: gate_status(gate).to_string(),
        source_ref: Some(gate.id.to_string()),
        correlation_ref: Some(gate.correlation_id.clone()),
        preview: projection.preview,
        diagnostic,
        result_refs,
        exec: None,
        cursor: None,
        next: Some(json!({
            "tool": "wait",
            "activity_refs": [gate.id.to_string()],
        })),
        updated_at_ms: gate
            .resolved_at
            .unwrap_or(gate.created_at)
            .timestamp_millis(),
    }
}

fn gate_result_refs(gate: &LifecycleGate) -> Value {
    let mut refs = Map::new();
    refs.insert("gate_id".to_string(), json!(gate.id.to_string()));
    refs.insert("run_id".to_string(), json!(gate.run_id.to_string()));
    refs.insert(
        "agent_id".to_string(),
        json!(gate.agent_id.map(|id| id.to_string())),
    );
    refs.insert(
        "frame_id".to_string(),
        json!(gate.frame_id.map(|id| id.to_string())),
    );
    refs.insert("gate_kind".to_string(), json!(gate.gate_kind));

    if let Some(payload_refs) = gate
        .payload_json
        .as_ref()
        .and_then(|payload| payload.get("result_refs"))
        .and_then(Value::as_object)
    {
        for (key, value) in payload_refs {
            refs.entry(key.clone()).or_insert_with(|| value.clone());
        }
    }

    if let Some(diagnostic) = gate
        .payload_json
        .as_ref()
        .and_then(|payload| payload.get("diagnostic"))
    {
        refs.entry("diagnostic".to_string())
            .or_insert_with(|| diagnostic.clone());
    }

    Value::Object(refs)
}

fn gate_terminal_diagnostic(gate: &LifecycleGate) -> Option<RuntimeTerminalDiagnostic> {
    gate.payload_json
        .as_ref()
        .and_then(|payload| payload.get("diagnostic"))
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok())
}

fn gate_status(gate: &LifecycleGate) -> String {
    if let Some(status) = gate.resolved_payload_status() {
        return status;
    }
    match gate.status.as_str() {
        "open" => "pending",
        other => other,
    }
    .to_string()
}
