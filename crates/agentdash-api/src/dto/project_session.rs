use serde::{Deserialize, Serialize};

use agentdash_application::session::context::SessionContextSnapshot;

#[derive(Debug, Serialize)]
pub struct ProjectSessionDetailResponse {
    pub binding_id: String,
    pub session_id: String,
    pub label: String,
    pub session_title: Option<String>,
    pub last_activity: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vfs: Option<agentdash_spi::Vfs>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_surface: Option<agentdash_application::vfs::ResolvedVfsSurface>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_snapshot: Option<SessionContextSnapshot>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ProjectSessionEntry {
    pub session_id: String,
    pub session_title: Option<String>,
    pub last_activity: Option<i64>,
    pub execution_status: String,
    pub owner_type: String,
    pub owner_id: String,
    pub owner_title: Option<String>,
    pub story_id: Option<String>,
    pub story_title: Option<String>,
    pub agent_key: Option<String>,
    pub agent_display_name: Option<String>,
    pub parent_session_id: Option<String>,
    pub parent_relation_kind: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ListProjectSessionsQuery {
    pub status: Option<String>,
    pub limit: Option<i64>,
}
