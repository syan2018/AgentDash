use agentdash_application::project::project_authorization_context_from_identity;
use agentdash_domain::project::{
    Project, ProjectAuthorizationService, ProjectPermission as ApplicationProjectPermission,
};
use agentdash_spi::platform::auth::AuthIdentity;
use uuid::Uuid;

use crate::error::McpError;
use crate::services::McpServices;

pub use agentdash_domain::project::ProjectPermission as McpProjectPermission;

pub async fn require_project_permission(
    services: &McpServices,
    identity: &AuthIdentity,
    project_id: Uuid,
    permission: ApplicationProjectPermission,
) -> Result<Project, McpError> {
    let project = services
        .project_repo
        .get_by_id(project_id)
        .await
        .map_err(McpError::from)?
        .ok_or_else(|| McpError::not_found("Project", project_id))?;
    let authz = ProjectAuthorizationService::new(services.project_repo.as_ref());
    let context = project_authorization_context_from_identity(identity);
    if authz
        .can_access_project(&context, &project, permission)
        .await
        .map_err(McpError::from)?
    {
        return Ok(project);
    }

    let action = match permission {
        ApplicationProjectPermission::View => "查看",
        ApplicationProjectPermission::Edit => "编辑",
        ApplicationProjectPermission::ManageSharing => "管理共享",
    };
    Err(McpError::forbidden(format!(
        "当前用户无权{action} Project {}",
        project.id
    )))
}

pub async fn list_accessible_projects(
    services: &McpServices,
    identity: &AuthIdentity,
) -> Result<Vec<Project>, McpError> {
    let authz = ProjectAuthorizationService::new(services.project_repo.as_ref());
    authz
        .list_accessible_projects(&project_authorization_context_from_identity(identity))
        .await
        .map_err(McpError::from)
}
