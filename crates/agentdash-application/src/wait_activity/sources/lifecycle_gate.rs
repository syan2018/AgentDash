use agentdash_domain::workflow::LifecycleGate;
use serde_json::json;

use crate::wait_activity::types::{ResolvedWaitScope, WaitActivityItem};

pub(crate) fn gate_belongs_to_scope(gate: &LifecycleGate, scope: &ResolvedWaitScope) -> bool {
    scope.run_id.is_none_or(|run_id| gate.run_id == run_id)
}

pub(crate) fn gate_item_from_gate(gate: &LifecycleGate) -> WaitActivityItem {
    let projection = gate.waiting_projection();
    WaitActivityItem {
        activity_ref: gate.id.to_string(),
        kind: projection.kind,
        status: gate_status(gate).to_string(),
        source_ref: Some(gate.id.to_string()),
        correlation_ref: Some(gate.correlation_id.clone()),
        preview: projection.preview,
        result_refs: json!({
            "gate_id": gate.id.to_string(),
            "run_id": gate.run_id.to_string(),
            "agent_id": gate.agent_id.map(|id| id.to_string()),
            "frame_id": gate.frame_id.map(|id| id.to_string()),
            "gate_kind": gate.gate_kind,
        }),
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
