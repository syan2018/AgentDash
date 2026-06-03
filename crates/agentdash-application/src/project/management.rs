use agentdash_domain::project::ProjectRepository;
use uuid::Uuid;

use agentdash_domain::context_container::{
    ContextContainerDefinition, validate_context_containers,
};
use agentdash_domain::inline_file::InlineFileOwnerKind;
use agentdash_domain::project::{Project, ProjectConfig, ProjectVisibility};
use agentdash_domain::story::{Story, StoryRepository};
use agentdash_domain::workspace::{Workspace, WorkspaceRepository};

use crate::ApplicationError;
use crate::repository_set::RepositorySet;
use crate::workflow::{FreeformLifecycleService, WorkflowApplicationError};

#[derive(Debug, Clone, Default)]
pub struct ProjectMutationInput {
    pub name: Option<String>,
    pub description: Option<String>,
    pub config: Option<ProjectConfig>,
    pub visibility: Option<ProjectVisibility>,
    pub is_template: Option<bool>,
    pub cloned_from_project_id: Option<Uuid>,
    pub context_containers: Option<Vec<ContextContainerDefinition>>,
}

#[derive(Debug, Clone)]
pub struct CreateProjectInput {
    pub creator_user_id: String,
    pub name: String,
    pub description: Option<String>,
    pub mutation: ProjectMutationInput,
}

#[derive(Debug, Clone)]
pub struct UpdateProjectInput {
    pub updated_by_user_id: String,
    pub mutation: ProjectMutationInput,
}

#[derive(Debug, Clone)]
pub struct CloneProjectInput {
    pub creator_user_id: String,
    pub name: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ProjectDetailFacts {
    pub workspaces: Vec<Workspace>,
    pub stories: Vec<Story>,
}

pub async fn create_project_record(
    repos: &RepositorySet,
    input: CreateProjectInput,
) -> Result<Project, ApplicationError> {
    let project = build_project(
        input.creator_user_id,
        input.name,
        input.description.unwrap_or_default(),
        input.mutation,
    );
    validate_project_config(&project.config)?;
    validate_project_contract(&project)?;
    repos
        .project_repo
        .create(&project)
        .await
        .map_err(ApplicationError::from)?;
    ensure_project_freeform_lifecycle(repos, project.id).await?;
    sync_project_inline_files(repos, &project).await?;
    Ok(project)
}

pub async fn load_project_by_id(
    repos: &RepositorySet,
    project_id: Uuid,
    raw_id: &str,
) -> Result<Project, ApplicationError> {
    repos
        .project_repo
        .get_by_id(project_id)
        .await
        .map_err(ApplicationError::from)?
        .ok_or_else(|| ApplicationError::NotFound(format!("Project {raw_id} 不存在")))
}

pub async fn load_project_detail_facts(
    repos: &RepositorySet,
    project_id: Uuid,
) -> Result<ProjectDetailFacts, ApplicationError> {
    let workspaces = repos
        .workspace_repo
        .list_by_project(project_id)
        .await
        .map_err(ApplicationError::from)?;
    let stories = repos
        .story_repo
        .list_by_project(project_id)
        .await
        .map_err(ApplicationError::from)?;
    Ok(ProjectDetailFacts {
        workspaces,
        stories,
    })
}

pub async fn update_project_record(
    repos: &RepositorySet,
    mut project: Project,
    input: UpdateProjectInput,
) -> Result<Project, ApplicationError> {
    apply_project_mutation(&mut project, input.mutation, Some(input.updated_by_user_id));
    validate_project_config(&project.config)?;
    validate_project_contract(&project)?;
    repos
        .project_repo
        .update(&project)
        .await
        .map_err(ApplicationError::from)?;
    sync_project_inline_files(repos, &project).await?;
    Ok(project)
}

pub async fn delete_project_record(
    repos: &RepositorySet,
    project_id: Uuid,
) -> Result<(), ApplicationError> {
    delete_project_aggregate(
        repos.project_repo.as_ref(),
        repos.story_repo.as_ref(),
        repos.workspace_repo.as_ref(),
        project_id,
    )
    .await
    .map_err(ApplicationError::from)
}

pub async fn clone_project_record(
    repos: &RepositorySet,
    source_project: &Project,
    input: CloneProjectInput,
) -> Result<Project, ApplicationError> {
    if !source_project.is_template {
        return Err(ApplicationError::BadRequest(
            "仅模板 Project 支持 clone；请先将源 Project 标记为模板".to_string(),
        ));
    }

    let clone_name = normalize_clone_name(input.name, &source_project.name)?;
    let cloned_project = build_cloned_project(
        source_project,
        input.creator_user_id,
        clone_name,
        input.description,
    );
    validate_project_config(&cloned_project.config)?;
    validate_project_contract(&cloned_project)?;
    repos
        .project_repo
        .create(&cloned_project)
        .await
        .map_err(ApplicationError::from)?;
    ensure_project_freeform_lifecycle(repos, cloned_project.id).await?;
    Ok(cloned_project)
}

pub fn build_project(
    creator_user_id: String,
    name: String,
    description: String,
    input: ProjectMutationInput,
) -> Project {
    let mut project = Project::new_with_creator(name, description, creator_user_id);
    apply_project_mutation(&mut project, input, None);
    project
}

pub fn validate_project_config(config: &ProjectConfig) -> Result<(), ApplicationError> {
    validate_context_containers(&config.context_containers)
        .map_err(ApplicationError::BadRequest)?;
    Ok(())
}

pub fn validate_project_contract(project: &Project) -> Result<(), ApplicationError> {
    if matches!(project.visibility, ProjectVisibility::TemplateVisible) && !project.is_template {
        return Err(ApplicationError::BadRequest(
            "template_visible 仅适用于模板 Project；请同时设置 is_template=true".to_string(),
        ));
    }

    Ok(())
}

pub fn apply_project_mutation(
    project: &mut Project,
    input: ProjectMutationInput,
    updated_by_user_id: Option<String>,
) {
    if let Some(name) = input.name {
        project.name = name;
    }
    if let Some(description) = input.description {
        project.description = description;
    }
    if let Some(config) = input.config {
        project.config = config;
    }
    if let Some(visibility) = input.visibility {
        project.visibility = visibility;
    }
    if let Some(is_template) = input.is_template {
        project.is_template = is_template;
    }
    if let Some(cloned_from_project_id) = input.cloned_from_project_id {
        project.cloned_from_project_id = Some(cloned_from_project_id);
    }
    if let Some(context_containers) = input.context_containers {
        project.config.context_containers = context_containers;
    }
    if let Some(updated_by_user_id) = updated_by_user_id {
        project.touch_updated_by(updated_by_user_id);
    }
}

pub fn normalize_clone_name(
    raw_name: Option<String>,
    source_name: &str,
) -> Result<String, ApplicationError> {
    match raw_name {
        Some(name) => {
            let trimmed = name.trim();
            if trimmed.is_empty() {
                Err(ApplicationError::BadRequest(
                    "clone 后的 Project 名称不能为空".to_string(),
                ))
            } else {
                Ok(trimmed.to_string())
            }
        }
        None => Ok(format!("{source_name}（副本）")),
    }
}

pub fn build_cloned_project(
    source_project: &Project,
    creator_user_id: String,
    name: String,
    description: Option<String>,
) -> Project {
    let mut cloned_project = Project::new_with_creator(
        name,
        description.unwrap_or_else(|| source_project.description.clone()),
        creator_user_id,
    );
    cloned_project.config = source_project.config.clone();
    cloned_project.config.default_workspace_id = None;
    cloned_project.visibility = ProjectVisibility::Private;
    cloned_project.is_template = false;
    cloned_project.cloned_from_project_id = Some(source_project.id);
    cloned_project
}

pub async fn delete_project_aggregate(
    project_repo: &dyn ProjectRepository,
    story_repo: &dyn StoryRepository,
    workspace_repo: &dyn WorkspaceRepository,
    project_id: Uuid,
) -> Result<(), agentdash_domain::DomainError> {
    // Story aggregate 已持有 Vec<Task>（stories.tasks JSONB）；删除 story 即级联清理 tasks。
    let stories = story_repo.list_by_project(project_id).await?;
    for story in stories {
        story_repo.delete(story.id).await?;
    }

    let workspaces = workspace_repo.list_by_project(project_id).await?;
    for workspace in workspaces {
        workspace_repo.delete(workspace.id).await?;
    }

    project_repo.delete(project_id).await?;
    Ok(())
}

async fn sync_project_inline_files(
    repos: &RepositorySet,
    project: &Project,
) -> Result<(), ApplicationError> {
    crate::vfs::inline_persistence::sync_container_inline_files(
        repos.inline_file_repo.as_ref(),
        InlineFileOwnerKind::Project,
        project.id,
        &project.config.context_containers,
    )
    .await
    .map_err(ApplicationError::Internal)
}

async fn ensure_project_freeform_lifecycle(
    repos: &RepositorySet,
    project_id: Uuid,
) -> Result<(), ApplicationError> {
    let service = FreeformLifecycleService::new(
        repos.agent_procedure_repo.as_ref(),
        repos.workflow_graph_repo.as_ref(),
    );
    service
        .ensure_definition(project_id)
        .await
        .map(|_| ())
        .map_err(map_workflow_error)
}

fn map_workflow_error(error: WorkflowApplicationError) -> ApplicationError {
    match error {
        WorkflowApplicationError::BadRequest(message) => ApplicationError::BadRequest(message),
        WorkflowApplicationError::NotFound(message) => ApplicationError::NotFound(message),
        WorkflowApplicationError::Conflict(message) => ApplicationError::Conflict(message),
        WorkflowApplicationError::Internal(message) => ApplicationError::Internal(message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_clone_name_rejects_blank_name() {
        let err = normalize_clone_name(Some("   ".to_string()), "Source")
            .expect_err("blank name should be rejected");
        assert!(err.to_string().contains("不能为空"));
    }

    #[test]
    fn build_cloned_project_resets_template_visibility_and_workspace() {
        let mut source = Project::new_with_creator(
            "Source".to_string(),
            "desc".to_string(),
            "creator".to_string(),
        );
        source.is_template = true;
        source.visibility = ProjectVisibility::TemplateVisible;
        source.config.default_workspace_id = Some(Uuid::new_v4());

        let cloned = build_cloned_project(&source, "user-2".to_string(), "Clone".to_string(), None);

        assert_eq!(cloned.name, "Clone");
        assert_eq!(cloned.description, source.description);
        assert_eq!(cloned.cloned_from_project_id, Some(source.id));
        assert_eq!(cloned.visibility, ProjectVisibility::Private);
        assert!(!cloned.is_template);
        assert_eq!(cloned.config.default_workspace_id, None);
    }
}
