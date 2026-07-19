use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use uuid::Uuid;

const WAITING_PREVIEW_CHARS: usize = 280;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleGateWaitingProjection {
    pub kind: String,
    pub source_label: Option<String>,
    pub preview: Option<String>,
}

/// Durable wait/review/resume 点。
///
/// Gate 可跨进程重启恢复，并能恢复 agent/frame/run context。
/// correlation_id 用于 resume 时匹配。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LifecycleGate {
    pub id: Uuid,
    pub run_id: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame_id: Option<Uuid>,
    pub gate_kind: String,
    pub correlation_id: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload_json: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_by: Option<String>,
    pub created_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_at: Option<DateTime<Utc>>,
}

impl LifecycleGate {
    pub fn open(
        run_id: Uuid,
        agent_id: Option<Uuid>,
        frame_id: Option<Uuid>,
        gate_kind: impl Into<String>,
        correlation_id: impl Into<String>,
        payload: Option<serde_json::Value>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            run_id,
            agent_id,
            frame_id,
            gate_kind: gate_kind.into(),
            correlation_id: correlation_id.into(),
            status: "open".to_string(),
            payload_json: payload,
            resolved_by: None,
            created_at: Utc::now(),
            resolved_at: None,
        }
    }

    pub fn resolve(&mut self, resolved_by: impl Into<String>) {
        self.status = "resolved".to_string();
        self.resolved_by = Some(resolved_by.into());
        self.resolved_at = Some(Utc::now());
    }

    pub fn is_open(&self) -> bool {
        self.status == "open"
    }

    pub fn resolved_payload_status(&self) -> Option<String> {
        if self.status != "resolved" {
            return None;
        }
        let status = self
            .payload_json
            .as_ref()
            .and_then(|payload| payload.get("status"))
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("completed");
        Some(status.to_string())
    }

    pub fn waiting_projection(&self) -> LifecycleGateWaitingProjection {
        waiting_projection_from_gate_parts(&self.gate_kind, self.payload_json.as_ref())
    }
}

fn waiting_projection_from_gate_parts(
    gate_kind: &str,
    payload: Option<&Value>,
) -> LifecycleGateWaitingProjection {
    let kind = waiting_kind_from_gate(gate_kind, payload).to_string();
    let source_label = waiting_source_label(&kind, payload);
    let preview = waiting_preview(payload);
    LifecycleGateWaitingProjection {
        kind,
        source_label,
        preview,
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

fn waiting_source_label(kind: &str, payload: Option<&Value>) -> Option<String> {
    payload
        .and_then(|payload| {
            [
                "source_label",
                "companion_label",
                "label",
                "request_type",
                "plan_node_id",
            ]
            .iter()
            .find_map(|key| payload_string(payload, key))
        })
        .or_else(|| Some(kind.to_string()))
}

fn waiting_preview(payload: Option<&Value>) -> Option<String> {
    payload.and_then(|payload| {
        ["preview", "summary", "message", "title", "label"]
            .iter()
            .find_map(|key| bounded_payload_string(payload, key))
            .or_else(|| {
                payload.get("payload").and_then(|nested| {
                    ["preview", "summary", "message", "title"]
                        .iter()
                        .find_map(|key| bounded_payload_string(nested, key))
                })
            })
            .or_else(|| Some(bound_string(&payload.to_string(), WAITING_PREVIEW_CHARS)))
    })
}

fn payload_string(payload: &Value, key: &str) -> Option<String> {
    payload_direct_string(payload, key).or_else(|| {
        payload
            .get("display")
            .and_then(Value::as_object)
            .and_then(|display| object_string(display, key))
    })
}

fn payload_direct_string(payload: &Value, key: &str) -> Option<String> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn object_string(object: &Map<String, Value>, key: &str) -> Option<String> {
    object
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn bounded_payload_string(payload: &Value, key: &str) -> Option<String> {
    payload_string(payload, key).map(|value| bound_string(&value, WAITING_PREVIEW_CHARS))
}

fn bound_string(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let bounded = chars.by_ref().take(max_chars).collect::<String>();
    if chars.next().is_some() {
        format!("{bounded}...")
    } else {
        bounded
    }
}
