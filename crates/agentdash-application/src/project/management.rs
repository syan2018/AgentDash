use agentdash_domain::project::ProjectRepository;
use uuid::Uuid;

use agentdash_domain::context_container::ContextContainerDefinition;
use agentdash_domain::project::{Project, ProjectConfig, ProjectVisibility};
use agentdash_domain::story::StoryRepository;
use agentdash_domain::workspace::WorkspaceRepository;

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

pub fn normalize_clone_name(raw_name: Option<String>, source_name: &str) -> Result<String, String> {
    match raw_name {
        Some(name) => {
            let trimmed = name.trim();
            if trimmed.is_empty() {
                Err("clone 后的 Project 名称不能为空".to_string())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_clone_name_rejects_blank_name() {
        let err = normalize_clone_name(Some("   ".to_string()), "Source")
            .expect_err("blank name should be rejected");
        assert!(err.contains("不能为空"));
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
