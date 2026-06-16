use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::context::{ContextContainerDefinition, ContextSourceRef, SessionComposition};
use crate::task::TaskResponse;
use crate::workflow::SubjectRefDto;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct StoryContext {
    pub source_refs: Vec<ContextSourceRef>,
    pub context_containers: Vec<ContextContainerDefinition>,
    pub disabled_container_ids: Vec<String>,
    pub session_composition: Option<SessionComposition>,
}

impl From<agentdash_domain::story::StoryContext> for StoryContext {
    fn from(value: agentdash_domain::story::StoryContext) -> Self {
        Self {
            source_refs: value
                .source_refs
                .into_iter()
                .map(ContextSourceRef::from)
                .collect(),
            context_containers: value
                .context_containers
                .into_iter()
                .map(ContextContainerDefinition::from)
                .collect(),
            disabled_container_ids: value.disabled_container_ids,
            session_composition: value.session_composition.map(SessionComposition::from),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StoryStatus {
    Created,
    ContextReady,
    Decomposed,
    Executing,
    Completed,
    Failed,
    Cancelled,
}

impl From<agentdash_domain::story::StoryStatus> for StoryStatus {
    fn from(value: agentdash_domain::story::StoryStatus) -> Self {
        match value {
            agentdash_domain::story::StoryStatus::Created => Self::Created,
            agentdash_domain::story::StoryStatus::ContextReady => Self::ContextReady,
            agentdash_domain::story::StoryStatus::Decomposed => Self::Decomposed,
            agentdash_domain::story::StoryStatus::Executing => Self::Executing,
            agentdash_domain::story::StoryStatus::Completed => Self::Completed,
            agentdash_domain::story::StoryStatus::Failed => Self::Failed,
            agentdash_domain::story::StoryStatus::Cancelled => Self::Cancelled,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StoryPriority {
    P0,
    P1,
    P2,
    P3,
}

impl From<agentdash_domain::story::StoryPriority> for StoryPriority {
    fn from(value: agentdash_domain::story::StoryPriority) -> Self {
        match value {
            agentdash_domain::story::StoryPriority::P0 => Self::P0,
            agentdash_domain::story::StoryPriority::P1 => Self::P1,
            agentdash_domain::story::StoryPriority::P2 => Self::P2,
            agentdash_domain::story::StoryPriority::P3 => Self::P3,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StoryType {
    Feature,
    Bugfix,
    Refactor,
    Docs,
    Test,
    Other,
}

impl From<agentdash_domain::story::StoryType> for StoryType {
    fn from(value: agentdash_domain::story::StoryType) -> Self {
        match value {
            agentdash_domain::story::StoryType::Feature => Self::Feature,
            agentdash_domain::story::StoryType::Bugfix => Self::Bugfix,
            agentdash_domain::story::StoryType::Refactor => Self::Refactor,
            agentdash_domain::story::StoryType::Docs => Self::Docs,
            agentdash_domain::story::StoryType::Test => Self::Test,
            agentdash_domain::story::StoryType::Other => Self::Other,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct StoryResponse {
    pub id: String,
    pub project_id: String,
    pub default_workspace_id: Option<String>,
    pub title: String,
    pub description: String,
    pub status: StoryStatus,
    pub priority: StoryPriority,
    pub story_type: StoryType,
    pub tags: Vec<String>,
    pub task_count: u32,
    pub context: StoryContext,
    pub created_at: String,
    pub updated_at: String,
}

impl From<agentdash_domain::story::Story> for StoryResponse {
    fn from(value: agentdash_domain::story::Story) -> Self {
        Self {
            id: value.id.to_string(),
            project_id: value.project_id.to_string(),
            default_workspace_id: value.default_workspace_id.map(|id| id.to_string()),
            title: value.title,
            description: value.description,
            status: StoryStatus::from(value.status),
            priority: StoryPriority::from(value.priority),
            story_type: StoryType::from(value.story_type),
            tags: value.tags,
            task_count: value.task_count,
            context: StoryContext::from(value.context),
            created_at: value.created_at.to_rfc3339(),
            updated_at: value.updated_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StoryTaskProjectionSourceKind {
    OwningRun,
    LinkedRun,
    StoryRef,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct StoryTaskProjectionSource {
    pub kind: StoryTaskProjectionSourceKind,
    pub run_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub story_ref: Option<SubjectRefDto>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct StoryTaskProjectionItem {
    pub task: TaskResponse,
    #[serde(default)]
    pub sources: Vec<StoryTaskProjectionSource>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct StoryTaskProjectionResponse {
    pub story_id: String,
    #[serde(default)]
    pub tasks: Vec<StoryTaskProjectionItem>,
}
