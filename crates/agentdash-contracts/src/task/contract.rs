use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::context::ContextSourceRef;
use crate::workflow::SubjectRefDto;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskPlanStatus {
    Open,
    Active,
    Review,
    Blocked,
    Done,
    Dropped,
}

impl From<agentdash_domain::workflow::TaskPlanStatus> for TaskPlanStatus {
    fn from(value: agentdash_domain::workflow::TaskPlanStatus) -> Self {
        match value {
            agentdash_domain::workflow::TaskPlanStatus::Open => Self::Open,
            agentdash_domain::workflow::TaskPlanStatus::Active => Self::Active,
            agentdash_domain::workflow::TaskPlanStatus::Review => Self::Review,
            agentdash_domain::workflow::TaskPlanStatus::Blocked => Self::Blocked,
            agentdash_domain::workflow::TaskPlanStatus::Done => Self::Done,
            agentdash_domain::workflow::TaskPlanStatus::Dropped => Self::Dropped,
        }
    }
}

/// Browser-facing Task status name kept as a plan-status union.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Open,
    Active,
    Review,
    Blocked,
    Done,
    Dropped,
}

impl From<agentdash_domain::workflow::TaskPlanStatus> for TaskStatus {
    fn from(value: agentdash_domain::workflow::TaskPlanStatus) -> Self {
        match TaskPlanStatus::from(value) {
            TaskPlanStatus::Open => Self::Open,
            TaskPlanStatus::Active => Self::Active,
            TaskPlanStatus::Review => Self::Review,
            TaskPlanStatus::Blocked => Self::Blocked,
            TaskPlanStatus::Done => Self::Done,
            TaskPlanStatus::Dropped => Self::Dropped,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskPriority {
    P0,
    P1,
    P2,
    P3,
}

impl From<agentdash_domain::workflow::TaskPriority> for TaskPriority {
    fn from(value: agentdash_domain::workflow::TaskPriority) -> Self {
        match value {
            agentdash_domain::workflow::TaskPriority::P0 => Self::P0,
            agentdash_domain::workflow::TaskPriority::P1 => Self::P1,
            agentdash_domain::workflow::TaskPriority::P2 => Self::P2,
            agentdash_domain::workflow::TaskPriority::P3 => Self::P3,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct TaskResponse {
    pub id: String,
    pub project_id: String,
    pub owning_run_id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub body: Option<String>,
    pub status: TaskStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub priority: Option<TaskPriority>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub created_by_agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub owner_agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub assigned_agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub source_task_id: Option<String>,
    #[serde(default)]
    pub context_refs: Vec<ContextSourceRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub story_ref: Option<SubjectRefDto>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub archived_at: Option<String>,
}

impl TaskResponse {
    pub fn from_plan_item(
        project_id: impl Into<String>,
        owning_run_id: impl Into<String>,
        value: agentdash_domain::workflow::LifecycleTaskPlanItem,
    ) -> Self {
        Self {
            id: value.id.to_string(),
            project_id: project_id.into(),
            owning_run_id: owning_run_id.into(),
            title: value.title,
            body: value.body,
            status: TaskStatus::from(value.status),
            priority: value.priority.map(TaskPriority::from),
            created_by_agent_id: value.created_by_agent_id.map(|id| id.to_string()),
            owner_agent_id: value.owner_agent_id.map(|id| id.to_string()),
            assigned_agent_id: value.assigned_agent_id.map(|id| id.to_string()),
            source_task_id: value.source_task_id.map(|id| id.to_string()),
            context_refs: value
                .context_refs
                .into_iter()
                .map(ContextSourceRef::from)
                .collect(),
            story_ref: value.story_ref.map(subject_ref_to_dto),
            created_at: value.created_at.to_rfc3339(),
            updated_at: value.updated_at.to_rfc3339(),
            archived_at: value.archived_at.map(|value| value.to_rfc3339()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct RunTaskPlanResponse {
    pub project_id: String,
    pub run_id: String,
    #[serde(default)]
    pub tasks: Vec<TaskResponse>,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct CreateRunTaskRequest {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub body: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub status: Option<TaskPlanStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub priority: Option<TaskPriority>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub created_by_agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub owner_agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub assigned_agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub source_task_id: Option<String>,
    #[serde(default)]
    pub context_refs: Vec<ContextSourceRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub story_ref: Option<SubjectRefDto>,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct UpdateRunTaskRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub body: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub priority: Option<Option<TaskPriority>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub owner_agent_id: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub assigned_agent_id: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub source_task_id: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub context_refs: Option<Vec<ContextSourceRef>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub story_ref: Option<Option<SubjectRefDto>>,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct UpdateRunTaskStatusRequest {
    pub status: TaskPlanStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct RunTaskCommandResponse {
    pub project_id: String,
    pub run_id: String,
    pub task: TaskResponse,
}

fn subject_ref_to_dto(value: agentdash_domain::workflow::SubjectRef) -> SubjectRefDto {
    SubjectRefDto {
        kind: value.kind,
        id: value.id.to_string(),
    }
}
