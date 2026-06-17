use uuid::Uuid;

use agentdash_domain::context_container::{
    ContextContainerDefinition, validate_context_containers, validate_disabled_container_ids,
};
use agentdash_domain::context_source::ContextSourceRef;
use agentdash_domain::inline_file::InlineFileOwnerKind;
use agentdash_domain::project::Project;
use agentdash_domain::session_composition::SessionComposition;
use agentdash_domain::session_composition::validate_session_composition;
use agentdash_domain::story::{
    ChangeKind, StateChangeRepository, Story, StoryPriority, StoryRepository, StoryStatus,
    StoryType,
};

use crate::ApplicationError;
use crate::repository_set::RepositorySet;

#[derive(Debug, Clone, Default)]
pub struct StoryMutationInput {
    pub title: Option<String>,
    pub description: Option<String>,
    pub default_workspace_id: Option<Option<Uuid>>,
    pub status: Option<StoryStatus>,
    pub priority: Option<StoryPriority>,
    pub story_type: Option<StoryType>,
    pub tags: Option<Vec<String>>,
    pub context_source_refs: Option<Vec<ContextSourceRef>>,
    pub context_containers: Option<Vec<ContextContainerDefinition>>,
    pub disabled_container_ids: Option<Vec<String>>,
    pub session_composition: Option<Option<SessionComposition>>,
}

#[derive(Debug, Clone)]
pub struct CreateStoryInput {
    pub project_id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub mutation: StoryMutationInput,
}

pub async fn list_project_stories(
    repos: &RepositorySet,
    project_id: Uuid,
) -> Result<Vec<Story>, ApplicationError> {
    repos
        .story_repo
        .list_by_project(project_id)
        .await
        .map_err(ApplicationError::from)
}

pub async fn create_story_record(
    repos: &RepositorySet,
    project: &Project,
    input: CreateStoryInput,
) -> Result<Story, ApplicationError> {
    let title = input.title.trim();
    if title.is_empty() {
        return Err(ApplicationError::BadRequest(
            "Story 标题不能为空".to_string(),
        ));
    }

    let story = build_story(
        input.project_id,
        title.to_string(),
        input.description.unwrap_or_default(),
        input.mutation,
    );
    validate_story_context(&story, project)?;
    repos
        .story_repo
        .create(&story)
        .await
        .map_err(ApplicationError::from)?;
    sync_story_inline_files(repos, &story).await?;
    Ok(story)
}

pub async fn update_story_record(
    repos: &RepositorySet,
    mut story: Story,
    project: &Project,
    input: StoryMutationInput,
) -> Result<Story, ApplicationError> {
    apply_story_mutation(&mut story, input);
    validate_story_context(&story, project)?;
    repos
        .story_repo
        .update(&story)
        .await
        .map_err(ApplicationError::from)?;
    sync_story_inline_files(repos, &story).await?;
    Ok(story)
}

pub async fn delete_story_record(
    repos: &RepositorySet,
    story: &Story,
) -> Result<(), ApplicationError> {
    delete_story_aggregate(
        repos.story_repo.as_ref(),
        repos.state_change_repo.as_ref(),
        story,
    )
    .await
    .map_err(ApplicationError::from)
}

pub fn build_story(
    project_id: Uuid,
    title: String,
    description: String,
    input: StoryMutationInput,
) -> Story {
    let mut story = Story::new(project_id, title, description);
    apply_story_mutation(&mut story, input);
    story
}

pub fn validate_story_context(story: &Story, project: &Project) -> Result<(), ApplicationError> {
    validate_context_containers(&story.context.context_containers)
        .map_err(ApplicationError::BadRequest)?;
    validate_disabled_container_ids(
        &story.context.disabled_container_ids,
        &project.config.context_containers,
    )
    .map_err(ApplicationError::BadRequest)?;
    if let Some(session_composition) = &story.context.session_composition {
        validate_session_composition(session_composition).map_err(ApplicationError::BadRequest)?;
    }
    Ok(())
}

pub fn apply_story_mutation(story: &mut Story, input: StoryMutationInput) {
    if let Some(title) = input.title {
        story.title = title;
    }
    if let Some(description) = input.description {
        story.description = description;
    }
    if let Some(default_workspace_id) = input.default_workspace_id {
        story.default_workspace_id = default_workspace_id;
    }
    if let Some(status) = input.status {
        story.status = status;
    }
    if let Some(priority) = input.priority {
        story.priority = priority;
    }
    if let Some(story_type) = input.story_type {
        story.story_type = story_type;
    }
    if let Some(tags) = input.tags {
        story.tags = normalize_string_list(tags);
    }
    if let Some(context_source_refs) = input.context_source_refs {
        story.context.source_refs = context_source_refs;
    }
    if let Some(context_containers) = input.context_containers {
        story.context.context_containers = context_containers;
    }
    if let Some(disabled_container_ids) = input.disabled_container_ids {
        story.context.disabled_container_ids = normalize_string_list(disabled_container_ids);
    }
    if let Some(session_composition) = input.session_composition {
        story.context.session_composition = session_composition;
    }
}

fn normalize_string_list(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

pub async fn delete_story_aggregate(
    story_repo: &dyn StoryRepository,
    state_change_repo: &dyn StateChangeRepository,
    story: &Story,
) -> Result<(), agentdash_domain::DomainError> {
    story_repo.delete(story.id).await?;
    state_change_repo
        .append_change(
            story.project_id,
            story.id,
            ChangeKind::StoryDeleted,
            serde_json::json!({
                "project_id": story.project_id,
                "story_id": story.id,
                "reason": "story_deleted_by_user"
            }),
            None,
        )
        .await?;
    Ok(())
}

async fn sync_story_inline_files(
    repos: &RepositorySet,
    story: &Story,
) -> Result<(), ApplicationError> {
    crate::vfs::inline_persistence::sync_container_inline_files(
        repos.inline_file_repo.as_ref(),
        InlineFileOwnerKind::Story,
        story.id,
        &story.context.context_containers,
    )
    .await
    .map_err(ApplicationError::Internal)
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::story::StoryStatus;

    #[test]
    fn apply_story_mutation_normalizes_tag_and_disabled_lists() {
        let mut story = Story::new(Uuid::new_v4(), "title".to_string(), "desc".to_string());
        apply_story_mutation(
            &mut story,
            StoryMutationInput {
                tags: Some(vec![
                    " alpha ".to_string(),
                    " ".to_string(),
                    "beta".to_string(),
                ]),
                disabled_container_ids: Some(vec![
                    " one ".to_string(),
                    "".to_string(),
                    "two".to_string(),
                ]),
                status: Some(StoryStatus::Executing),
                ..StoryMutationInput::default()
            },
        );

        assert_eq!(story.tags, vec!["alpha".to_string(), "beta".to_string()]);
        assert_eq!(
            story.context.disabled_container_ids,
            vec!["one".to_string(), "two".to_string()]
        );
        assert_eq!(story.status, StoryStatus::Executing);
    }
}
