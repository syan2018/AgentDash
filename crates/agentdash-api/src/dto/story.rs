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
