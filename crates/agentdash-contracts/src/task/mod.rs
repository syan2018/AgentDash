use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use crate::context::ContextSourceRef;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    Assigned,
    Running,
    AwaitingVerification,
    Completed,
    Failed,
    Cancelled,
}

impl From<agentdash_domain::task::TaskStatus> for TaskStatus {
    fn from(value: agentdash_domain::task::TaskStatus) -> Self {
        match value {
            agentdash_domain::task::TaskStatus::Pending => Self::Pending,
            agentdash_domain::task::TaskStatus::Assigned => Self::Assigned,
            agentdash_domain::task::TaskStatus::Running => Self::Running,
            agentdash_domain::task::TaskStatus::AwaitingVerification => Self::AwaitingVerification,
            agentdash_domain::task::TaskStatus::Completed => Self::Completed,
            agentdash_domain::task::TaskStatus::Failed => Self::Failed,
            agentdash_domain::task::TaskStatus::Cancelled => Self::Cancelled,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactType {
    CodeChange,
    TestResult,
    LogOutput,
    File,
    ToolExecution,
}

impl From<agentdash_domain::task::ArtifactType> for ArtifactType {
    fn from(value: agentdash_domain::task::ArtifactType) -> Self {
        match value {
            agentdash_domain::task::ArtifactType::CodeChange => Self::CodeChange,
            agentdash_domain::task::ArtifactType::TestResult => Self::TestResult,
            agentdash_domain::task::ArtifactType::LogOutput => Self::LogOutput,
            agentdash_domain::task::ArtifactType::File => Self::File,
            agentdash_domain::task::ArtifactType::ToolExecution => Self::ToolExecution,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct Artifact {
    pub id: String,
    pub artifact_type: ArtifactType,
    pub content: Value,
    pub created_at: String,
}

impl From<agentdash_domain::task::Artifact> for Artifact {
    fn from(value: agentdash_domain::task::Artifact) -> Self {
        Self {
            id: value.id.to_string(),
            artifact_type: ArtifactType::from(value.artifact_type),
            content: value.content,
            created_at: value.created_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct TaskDispatchPreference {
    pub agent_type: Option<String>,
    pub agent_pid: Option<String>,
    pub preset_name: Option<String>,
    pub prompt_template: Option<String>,
    pub initial_context: Option<String>,
    pub context_sources: Vec<ContextSourceRef>,
}

impl From<agentdash_domain::task::TaskDispatchPreference> for TaskDispatchPreference {
    fn from(value: agentdash_domain::task::TaskDispatchPreference) -> Self {
        Self {
            agent_type: value.agent_type,
            agent_pid: value.agent_pid,
            preset_name: value.preset_name,
            prompt_template: value.prompt_template,
            initial_context: value.initial_context,
            context_sources: value
                .context_sources
                .into_iter()
                .map(ContextSourceRef::from)
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct TaskResponse {
    pub id: String,
    pub project_id: String,
    pub story_id: String,
    pub workspace_id: Option<String>,
    pub title: String,
    pub description: String,
    pub status: TaskStatus,
    pub dispatch_preference: TaskDispatchPreference,
    pub artifacts: Vec<Artifact>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<agentdash_domain::task::Task> for TaskResponse {
    fn from(value: agentdash_domain::task::Task) -> Self {
        Self {
            id: value.id.to_string(),
            project_id: value.project_id.to_string(),
            story_id: value.story_id.to_string(),
            workspace_id: value.workspace_id.map(|id| id.to_string()),
            title: value.title.clone(),
            description: value.description.clone(),
            status: TaskStatus::from(value.status().clone()),
            dispatch_preference: TaskDispatchPreference::from(value.dispatch_preference.clone()),
            artifacts: value
                .artifacts()
                .iter()
                .cloned()
                .map(Artifact::from)
                .collect(),
            created_at: value.created_at.to_rfc3339(),
            updated_at: value.updated_at.to_rfc3339(),
        }
    }
}
