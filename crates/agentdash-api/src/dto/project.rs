use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use agentdash_domain::project::{Project, ProjectConfig};
use agentdash_domain::story::Story;
use agentdash_domain::workspace::Workspace;

use super::{StoryResponse, WorkspaceResponse};

#[derive(Debug, Serialize)]
pub struct ProjectResponse {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub backend_id: String,
    pub config: ProjectConfig,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<Project> for ProjectResponse {
    fn from(p: Project) -> Self {
        Self {
            id: p.id,
            name: p.name,
            description: p.description,
            backend_id: p.backend_id,
            config: p.config,
            created_at: p.created_at,
            updated_at: p.updated_at,
        }
    }
}

/// GET /projects/:id 返回项目详情（含关联 Workspace 和 Story）
#[derive(Debug, Serialize)]
pub struct ProjectDetailResponse {
    #[serde(flatten)]
    pub project: ProjectResponse,
    pub workspaces: Vec<WorkspaceResponse>,
    pub stories: Vec<StoryResponse>,
}

impl ProjectDetailResponse {
    pub fn new(project: Project, workspaces: Vec<Workspace>, stories: Vec<Story>) -> Self {
        Self {
            project: ProjectResponse::from(project),
            workspaces: workspaces.into_iter().map(WorkspaceResponse::from).collect(),
            stories: stories.into_iter().map(StoryResponse::from).collect(),
        }
    }
}
