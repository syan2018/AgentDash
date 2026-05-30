use serde::{Deserialize, Serialize};
use uuid::Uuid;

use agentdash_application::session::context::SessionContextSnapshot;
use agentdash_domain::task::TaskStatus;

#[derive(Debug, Deserialize, Default)]
pub struct StartTaskRequest {
    #[serde(default)]
    pub override_prompt: Option<String>,
    #[serde(default)]
    pub executor_config: Option<agentdash_spi::AgentConfig>,
}

#[derive(Debug, Serialize)]
pub struct StartTaskResponse {
    pub task_id: Uuid,
    pub session_id: String,
    pub turn_id: String,
    pub status: TaskStatus,
    pub context_sources: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct ContinueTaskRequest {
    #[serde(default)]
    pub additional_prompt: Option<String>,
    #[serde(default)]
    pub executor_config: Option<agentdash_spi::AgentConfig>,
}

#[derive(Debug, Serialize)]
pub struct ContinueTaskResponse {
    pub task_id: Uuid,
    pub session_id: String,
    pub turn_id: String,
    pub status: TaskStatus,
    pub context_sources: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct TaskSessionResponse {
    pub task_id: Uuid,
    pub workspace_id: Option<Uuid>,
    pub session_id: Option<String>,
    pub task_status: TaskStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_execution_status: Option<String>,
    pub agent_binding: agentdash_domain::task::AgentBinding,
    pub session_title: Option<String>,
    pub last_activity: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vfs: Option<agentdash_spi::Vfs>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_surface: Option<agentdash_application::vfs::ResolvedVfsSurface>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_snapshot: Option<SessionContextSnapshot>,
}
