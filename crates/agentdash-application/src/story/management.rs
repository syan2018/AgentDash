use uuid::Uuid;

use agentdash_domain::context_container::ContextContainerDefinition;
use agentdash_domain::context_source::ContextSourceRef;
use agentdash_domain::session_composition::SessionComposition;
use agentdash_domain::story::{
    ChangeKind, StateChangeRepository, Story, StoryPriority, StoryRepository, StoryStatus,
    StoryType,
};
use agentdash_domain::task::{AgentBinding, Task, TaskStatus};

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

#[derive(Debug, Clone, Default)]
pub struct TaskMutationInput {
    pub title: Option<String>,
    pub description: Option<String>,
    pub workspace_id: Option<Option<Uuid>>,
    pub lifecycle_step_key: Option<Option<String>>,
    pub status: Option<TaskStatus>,
    pub agent_binding: Option<AgentBinding>,
}

#[derive(Debug, Clone, Default)]
pub struct AgentBindingInput {
    pub agent_type: Option<String>,
    pub agent_pid: Option<String>,
    pub preset_name: Option<String>,
    pub prompt_template: Option<String>,
    pub initial_context: Option<String>,
    pub context_sources: Option<Vec<ContextSourceRef>>,
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

pub fn build_task(
    project_id: Uuid,
    story_id: Uuid,
    title: String,
    description: String,
    workspace_id: Option<Uuid>,
    lifecycle_step_key: Option<String>,
    agent_binding: AgentBinding,
) -> Task {
    let mut task = Task::new(project_id, story_id, title, description);
    task.workspace_id = workspace_id;
    task.lifecycle_step_key = lifecycle_step_key;
    task.agent_binding = agent_binding;
    task
}

pub fn apply_task_mutation(task: &mut Task, input: TaskMutationInput) {
    if let Some(title) = input.title {
        task.title = title;
    }
    if let Some(description) = input.description {
        task.description = description;
    }
    if let Some(workspace_id) = input.workspace_id {
        task.workspace_id = workspace_id;
    }
    if let Some(lifecycle_step_key) = input.lifecycle_step_key {
        task.lifecycle_step_key = lifecycle_step_key.and_then(normalize_string);
    }
    if let Some(status) = input.status {
        // M2：apply_task_mutation 保留 status 字段以兼容命令型编辑（管理 API）。
        // 运行时投影通过 `Story::apply_task_projection` 走，不经此路径。
        task.set_status(status);
    }
    if let Some(agent_binding) = input.agent_binding {
        task.agent_binding = agent_binding;
    }
}

pub fn build_agent_binding(input: Option<AgentBindingInput>) -> AgentBinding {
    if let Some(value) = input {
        AgentBinding {
            agent_type: normalize_option(value.agent_type),
            agent_pid: normalize_option(value.agent_pid),
            preset_name: normalize_option(value.preset_name),
            prompt_template: normalize_option(value.prompt_template),
            initial_context: normalize_option(value.initial_context),
            context_sources: value.context_sources.unwrap_or_default(),
        }
    } else {
        AgentBinding::default()
    }
}

fn normalize_option(value: Option<String>) -> Option<String> {
    value.and_then(normalize_string)
}

fn normalize_string(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
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
    // Story aggregate 持有 Vec<Task>，删 story 即级联清理 tasks（`stories.tasks` JSONB）。
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

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::story::StoryStatus;

    #[test]
    fn build_agent_binding_trims_empty_fields() {
        let binding = build_agent_binding(Some(AgentBindingInput {
            agent_type: Some("  gpt-5  ".to_string()),
            agent_pid: Some("   ".to_string()),
            preset_name: Some(" preset-a ".to_string()),
            prompt_template: Some("   tpl   ".to_string()),
            initial_context: Some(" ".to_string()),
            context_sources: None,
        }));

        assert_eq!(binding.agent_type.as_deref(), Some("gpt-5"));
        assert_eq!(binding.agent_pid, None);
        assert_eq!(binding.preset_name.as_deref(), Some("preset-a"));
        assert_eq!(binding.prompt_template.as_deref(), Some("tpl"));
        assert_eq!(binding.initial_context, None);
        assert!(binding.context_sources.is_empty());
    }

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

    #[test]
    fn apply_task_mutation_overwrites_workspace_status_and_binding() {
        let story_id = Uuid::new_v4();
        let mut task = Task::new(
            Uuid::new_v4(),
            story_id,
            "task".to_string(),
            "desc".to_string(),
        );
        let workspace_id = Uuid::new_v4();
        let binding = build_agent_binding(Some(AgentBindingInput {
            agent_type: Some("runner".to_string()),
            ..AgentBindingInput::default()
        }));

        apply_task_mutation(
            &mut task,
            TaskMutationInput {
                workspace_id: Some(Some(workspace_id)),
                status: Some(TaskStatus::Running),
                agent_binding: Some(binding.clone()),
                ..TaskMutationInput::default()
            },
        );

        assert_eq!(task.workspace_id, Some(workspace_id));
        assert_eq!(*task.status(), TaskStatus::Running);
        assert_eq!(task.agent_binding.agent_type, binding.agent_type);
    }
}
