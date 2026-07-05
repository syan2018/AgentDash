use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct NdjsonStreamQuery {
    pub since_id: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct SessionEventsQuery {
    pub after_seq: Option<u64>,
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSessionMetaRequest {
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RejectToolApprovalRequest {
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ContextAuditQuery {
    pub since_ms: Option<u64>,
    pub scope: Option<String>,
    pub slot: Option<String>,
    pub source_prefix: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ContextAuditEventDto {
    pub event_id: uuid::Uuid,
    pub bundle_id: uuid::Uuid,
    pub session_id: String,
    pub bundle_session_uuid: uuid::Uuid,
    pub at_ms: u64,
    pub trigger: String,
    pub slot: String,
    pub label: String,
    pub source: String,
    pub order: i32,
    pub scope: Vec<String>,
    pub content_preview: String,
    pub content_hash: u64,
    pub full_content_available: bool,
}
