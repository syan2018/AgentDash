use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use agentdash_domain::task::{AgentBinding, Artifact, Task, TaskExecutionMode, TaskStatus};

#[derive(Debug, Serialize)]
pub struct TaskResponse {
    pub id: Uuid,
    pub story_id: Uuid,
    pub workspace_id: Option<Uuid>,
    pub session_id: Option<String>,
    pub executor_session_id: Option<String>,
    pub title: String,
    pub description: String,
    pub status: TaskStatus,
    pub execution_mode: TaskExecutionMode,
    pub agent_binding: AgentBinding,
    pub artifacts: Vec<Artifact>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<Task> for TaskResponse {
    fn from(t: Task) -> Self {
        Self {
            id: t.id,
            story_id: t.story_id,
            workspace_id: t.workspace_id,
            session_id: t.session_id,
            executor_session_id: t.executor_session_id,
            title: t.title,
            description: t.description,
            status: t.status,
            execution_mode: t.execution_mode,
            agent_binding: t.agent_binding,
            artifacts: t.artifacts,
            created_at: t.created_at,
            updated_at: t.updated_at,
        }
    }
}
