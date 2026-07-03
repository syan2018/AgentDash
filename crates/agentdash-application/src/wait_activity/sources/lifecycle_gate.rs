use agentdash_domain::workflow::LifecycleGate;
use serde_json::{Value, json};

use super::{payload_preview, payload_string};
use crate::wait_activity::types::{ResolvedWaitScope, WaitActivityItem};

pub(crate) fn gate_belongs_to_scope(gate: &LifecycleGate, scope: &ResolvedWaitScope) -> bool {
    scope.run_id.is_none_or(|run_id| gate.run_id == run_id)
}

pub(crate) fn gate_item_from_gate(gate: &LifecycleGate) -> WaitActivityItem {
    let kind = waiting_kind_from_gate(&gate.gate_kind, gate.payload_json.as_ref());
    WaitActivityItem {
        activity_ref: gate.id.to_string(),
        kind: kind.to_string(),
        status: gate_status(&gate.status).to_string(),
        source_ref: Some(gate.id.to_string()),
        correlation_ref: Some(gate.correlation_id.clone()),
        preview: payload_preview(gate.payload_json.as_ref()),
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

fn gate_status(status: &str) -> &str {
    match status {
        "open" => "pending",
        "resolved" => "completed",
        other => other,
    }
}

fn waiting_kind_from_gate(gate_kind: &str, payload: Option<&Value>) -> &'static str {
    if gate_kind == "companion_wait"
        && payload
            .and_then(|payload| payload_string(payload, "request_type"))
            .is_some()
    {
        return "human";
    }
    match gate_kind {
        "companion_human_request" | "orchestration_human_gate" => "human",
        "companion_wait" | "companion_wait_blocking" | "companion_wait_follow_up" => "subagent",
        "companion_parent_request" => "companion",
        kind if kind.starts_with("companion_") => "companion",
        kind if kind.starts_with("exec_") => "exec",
        _ => "workflow",
    }
}
