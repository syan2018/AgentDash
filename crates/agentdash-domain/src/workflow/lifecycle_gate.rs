use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Durable wait/review/resume 点。
///
/// Gate 可跨进程重启恢复，并能恢复 agent/frame/run context。
/// correlation_id 用于 resume 时匹配。
#[derive(Debug, Clone, Serialize, Deserialize)]
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
}
