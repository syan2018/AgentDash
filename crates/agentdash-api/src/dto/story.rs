use serde::Deserialize;

use agentdash_domain::context_container::ContextContainerDefinition;
use agentdash_domain::context_source::ContextSourceRef;
use agentdash_domain::session_composition::SessionComposition;
use agentdash_domain::story::{StoryPriority, StoryStatus, StoryType};

#[derive(Deserialize)]
pub struct ListStoriesQuery {
    pub project_id: Option<String>,
}

#[derive(Deserialize)]
pub struct CreateStoryRequest {
    pub project_id: String,
    pub title: String,
    pub description: Option<String>,
    pub status: Option<StoryStatus>,
    pub priority: Option<StoryPriority>,
    pub story_type: Option<StoryType>,
    pub tags: Option<Vec<String>>,
    pub default_workspace_id: Option<String>,
    pub context_source_refs: Option<Vec<ContextSourceRef>>,
    pub context_containers: Option<Vec<ContextContainerDefinition>>,
    pub disabled_container_ids: Option<Vec<String>>,
    pub session_composition: Option<SessionComposition>,
}

#[derive(Deserialize, Default)]
pub struct UpdateStoryRequest {
    pub title: Option<String>,
    pub description: Option<String>,
    pub default_workspace_id: Option<String>,
    pub status: Option<StoryStatus>,
    pub priority: Option<StoryPriority>,
    pub story_type: Option<StoryType>,
    pub tags: Option<Vec<String>>,
    pub context_source_refs: Option<Vec<ContextSourceRef>>,
    pub context_containers: Option<Vec<ContextContainerDefinition>>,
    pub disabled_container_ids: Option<Vec<String>>,
    pub session_composition: Option<SessionComposition>,
    pub clear_session_composition: Option<bool>,
}

#[derive(Deserialize, Default)]
pub struct CreateTaskAgentBindingRequest {
    pub agent_type: Option<String>,
    pub agent_pid: Option<String>,
    pub preset_name: Option<String>,
    pub prompt_template: Option<String>,
    pub initial_context: Option<String>,
    pub context_sources: Option<Vec<ContextSourceRef>>,
}

#[derive(Deserialize)]
pub struct CreateTaskRequest {
    pub title: String,
    pub description: Option<String>,
    pub workspace_id: Option<String>,
    pub lifecycle_step_key: Option<String>,
    pub agent_binding: Option<CreateTaskAgentBindingRequest>,
}

#[derive(Deserialize, Default)]
pub struct UpdateTaskRequest {
    pub title: Option<String>,
    pub description: Option<String>,
    pub workspace_id: Option<String>,
    pub lifecycle_step_key: Option<String>,
    pub agent_binding: Option<CreateTaskAgentBindingRequest>,
}
