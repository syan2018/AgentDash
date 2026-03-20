use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use agentdash_domain::story::{Story, StoryContext, StoryPriority, StoryStatus, StoryType};

#[derive(Debug, Serialize)]
pub struct StoryResponse {
    pub id: Uuid,
    pub project_id: Uuid,
    pub backend_id: String,
    pub title: String,
    pub description: String,
    pub status: StoryStatus,
    pub priority: StoryPriority,
    pub story_type: StoryType,
    pub tags: Vec<String>,
    pub task_count: u32,
    pub context: StoryContext,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<Story> for StoryResponse {
    fn from(s: Story) -> Self {
        Self {
            id: s.id,
            project_id: s.project_id,
            backend_id: s.backend_id,
            title: s.title,
            description: s.description,
            status: s.status,
            priority: s.priority,
            story_type: s.story_type,
            tags: s.tags,
            task_count: s.task_count,
            context: s.context,
            created_at: s.created_at,
            updated_at: s.updated_at,
        }
    }
}
