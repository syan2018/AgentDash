use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use agentdash_domain::workspace::{GitConfig, Workspace, WorkspaceStatus, WorkspaceType};

#[derive(Debug, Serialize)]
pub struct WorkspaceResponse {
    pub id: Uuid,
    pub project_id: Uuid,
    pub backend_id: String,
    pub name: String,
    pub container_ref: String,
    pub workspace_type: WorkspaceType,
    pub status: WorkspaceStatus,
    pub git_config: Option<GitConfig>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<Workspace> for WorkspaceResponse {
    fn from(w: Workspace) -> Self {
        Self {
            id: w.id,
            project_id: w.project_id,
            backend_id: w.backend_id,
            name: w.name,
            container_ref: w.container_ref,
            workspace_type: w.workspace_type,
            status: w.status,
            git_config: w.git_config,
            created_at: w.created_at,
            updated_at: w.updated_at,
        }
    }
}
