use serde::Deserialize;
use uuid::Uuid;

use agentdash_domain::context_container::ContextContainerDefinition;
use agentdash_domain::project::{ProjectConfig, ProjectRole, ProjectVisibility};

#[derive(Deserialize)]
pub struct CreateProjectRequest {
    pub name: String,
    pub description: Option<String>,
    pub config: Option<ProjectConfig>,
    pub visibility: Option<ProjectVisibility>,
    pub is_template: Option<bool>,
    pub cloned_from_project_id: Option<Uuid>,
    pub context_containers: Option<Vec<ContextContainerDefinition>>,
}

#[derive(Deserialize)]
pub struct UpdateProjectRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub config: Option<ProjectConfig>,
    pub visibility: Option<ProjectVisibility>,
    pub is_template: Option<bool>,
    pub cloned_from_project_id: Option<Uuid>,
    pub context_containers: Option<Vec<ContextContainerDefinition>>,
}

#[derive(Deserialize)]
pub struct UpsertProjectGrantRequest {
    pub role: ProjectRole,
}

#[derive(Deserialize, Default)]
pub struct CloneProjectRequest {
    pub name: Option<String>,
    pub description: Option<String>,
}
