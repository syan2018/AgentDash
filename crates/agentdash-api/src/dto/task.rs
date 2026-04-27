use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use agentdash_domain::task::{AgentBinding, Artifact, Task, TaskStatus};

#[derive(Debug, Serialize)]
pub struct TaskResponse {
    pub id: Uuid,
    pub project_id: Uuid,
    pub story_id: Uuid,
    pub workspace_id: Option<Uuid>,
    pub title: String,
    pub description: String,
    pub status: TaskStatus,
    pub agent_binding: AgentBinding,
    pub artifacts: Vec<Artifact>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<Task> for TaskResponse {
    fn from(t: Task) -> Self {
        Self {
            id: t.id,
            project_id: t.project_id,
            story_id: t.story_id,
            workspace_id: t.workspace_id,
            title: t.title.clone(),
            description: t.description.clone(),
            status: t.status().clone(),
            agent_binding: t.agent_binding.clone(),
            artifacts: t.artifacts().to_vec(),
            created_at: t.created_at,
            updated_at: t.updated_at,
        }
    }
}
