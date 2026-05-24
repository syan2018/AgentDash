use serde::{Deserialize, Serialize};

/// BackboneEnvelope 透传（JSON 编码的 BackboneEnvelope）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionNotificationPayload {
    pub session_id: String,
    /// BackboneEnvelope JSON — 本机 serializes BackboneEnvelope, 云端 deserialize 后直接使用
    pub notification: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStateChangedPayload {
    pub session_id: String,
    pub turn_id: String,
    /// started | completed | failed | cancelled
    pub state: SessionState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Started,
    Completed,
    Failed,
    Cancelled,
}
