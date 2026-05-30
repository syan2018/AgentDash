use serde::{Deserialize, Serialize};

use agentdash_application::session::context::SessionContextSnapshot;

#[derive(Debug, Serialize)]
pub struct StorySessionDetailResponse {
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
pub struct SessionBindingResponse {
    pub id: String,
    pub project_id: String,
    pub session_id: String,
    pub owner_type: String,
    pub owner_id: String,
    pub label: String,
    pub created_at: String,
    pub session_title: Option<String>,
    pub session_updated_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateStorySessionRequest {
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
}
