use serde::{Deserialize, Serialize};

use agentdash_application::session::construction::SessionConstructionPlan;
use agentdash_application::session::context::SessionContextSnapshot;

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
pub struct ListSessionsQuery {
    pub project_id: Option<String>,
    pub exclude_bound: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct CreateSessionRequest {
    #[serde(default)]
    pub title: Option<String>,
    pub project_id: uuid::Uuid,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct SessionExecutionStateResponse {
    pub session_id: String,
    pub status: String,
    pub turn_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct SessionContextResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_binding: Option<agentdash_domain::task::AgentBinding>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vfs: Option<agentdash_spi::Vfs>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_surface: Option<agentdash_application::vfs::ResolvedVfsSurface>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_snapshot: Option<SessionContextSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_capabilities: Option<agentdash_spi::SessionBaselineCapabilities>,
}

impl SessionContextResponse {
    pub(crate) fn empty() -> Self {
        Self {
            workspace_id: None,
            agent_binding: None,
            vfs: None,
            runtime_surface: None,
            context_snapshot: None,
            session_capabilities: None,
        }
    }

    pub(crate) fn from_construction_plan(plan: SessionConstructionPlan) -> Self {
        let projection = plan.context_projection;
        Self {
            workspace_id: projection.workspace_id.map(|id| id.to_string()),
            agent_binding: projection.agent_binding,
            vfs: projection.vfs,
            runtime_surface: projection.runtime_surface,
            context_snapshot: projection.context_snapshot,
            session_capabilities: projection.session_capabilities,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct UpdateSessionMetaRequest {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub tab_layout: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct RejectToolApprovalRequest {
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CompanionRespondRequest {
    pub payload: serde_json::Value,
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
