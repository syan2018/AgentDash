use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use agentdash_application::project::ProjectAuthorization;
use agentdash_domain::project::{
    Project, ProjectConfig, ProjectRole, ProjectSubjectGrant, ProjectSubjectType, ProjectVisibility,
};
use agentdash_domain::story::Story;
use agentdash_domain::workspace::Workspace;

use super::{StoryResponse, WorkspaceResponse};

#[derive(Debug, Serialize)]
pub struct ProjectResponse {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub config: ProjectConfig,
    pub created_by_user_id: String,
    pub updated_by_user_id: String,
    pub visibility: ProjectVisibility,
    pub is_template: bool,
    pub cloned_from_project_id: Option<Uuid>,
    pub access: ProjectAccessSummaryResponse,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ProjectResponse {
    pub fn new(p: Project, access: ProjectAuthorization) -> Self {
        Self {
            id: p.id,
            name: p.name,
            description: p.description,
            config: p.config,
            created_by_user_id: p.created_by_user_id,
            updated_by_user_id: p.updated_by_user_id,
            visibility: p.visibility,
            is_template: p.is_template,
            cloned_from_project_id: p.cloned_from_project_id,
            access: ProjectAccessSummaryResponse::from(access),
            created_at: p.created_at,
            updated_at: p.updated_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ProjectAccessSummaryResponse {
    pub role: Option<ProjectRole>,
    pub can_view: bool,
    pub can_edit: bool,
    pub can_manage_sharing: bool,
    pub via_admin_bypass: bool,
    pub via_template_visibility: bool,
}

impl From<ProjectAuthorization> for ProjectAccessSummaryResponse {
    fn from(access: ProjectAuthorization) -> Self {
        Self {
            role: access.role,
            can_view: access.can_view_project(),
            can_edit: access.can_edit_project(),
            can_manage_sharing: access.can_manage_project_sharing(),
            via_admin_bypass: access.via_admin_bypass,
            via_template_visibility: access.via_template_visibility,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ProjectSubjectGrantResponse {
    pub project_id: Uuid,
    pub subject_type: ProjectSubjectType,
    pub subject_id: String,
    pub role: ProjectRole,
    pub granted_by_user_id: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<ProjectSubjectGrant> for ProjectSubjectGrantResponse {
    fn from(grant: ProjectSubjectGrant) -> Self {
        Self {
            project_id: grant.project_id,
            subject_type: grant.subject_type,
            subject_id: grant.subject_id,
            role: grant.role,
            granted_by_user_id: grant.granted_by_user_id,
            created_at: grant.created_at,
            updated_at: grant.updated_at,
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
    pub fn new(
        project: Project,
        access: ProjectAuthorization,
        workspaces: Vec<Workspace>,
        stories: Vec<Story>,
    ) -> Self {
        Self {
            project: ProjectResponse::new(project, access),
            workspaces: workspaces
                .into_iter()
                .map(WorkspaceResponse::from)
                .collect(),
            stories: stories.into_iter().map(StoryResponse::from).collect(),
        }
    }
}
