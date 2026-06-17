use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::context_source::ContextSourceRef;

use super::super::lifecycle_subject_association::SubjectRef;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskPlanStatus {
    Open,
    Active,
    Review,
    Blocked,
    Done,
    Dropped,
}

impl TaskPlanStatus {
    pub fn can_transition_to(self, next: Self) -> bool {
        use TaskPlanStatus::*;

        if self == next {
            return true;
        }

        !matches!(self, Dropped)
    }
}

impl Default for TaskPlanStatus {
    fn default() -> Self {
        Self::Open
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskPriority {
    P0,
    P1,
    P2,
    P3,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifecycleTaskPlanItem {
    pub id: Uuid,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    pub status: TaskPlanStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<TaskPriority>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_by_agent_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_agent_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assigned_agent_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_task_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub context_refs: Vec<ContextSourceRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub story_ref: Option<SubjectRef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleTaskPlanItemDraft {
    pub id: Option<Uuid>,
    pub title: String,
    pub body: Option<String>,
    pub status: TaskPlanStatus,
    pub priority: Option<TaskPriority>,
    pub created_by_agent_id: Option<Uuid>,
    pub owner_agent_id: Option<Uuid>,
    pub assigned_agent_id: Option<Uuid>,
    pub source_task_id: Option<Uuid>,
    pub context_refs: Vec<ContextSourceRef>,
    pub story_ref: Option<SubjectRef>,
}

impl LifecycleTaskPlanItemDraft {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            id: None,
            title: title.into(),
            body: None,
            status: TaskPlanStatus::Open,
            priority: None,
            created_by_agent_id: None,
            owner_agent_id: None,
            assigned_agent_id: None,
            source_task_id: None,
            context_refs: Vec::new(),
            story_ref: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LifecycleTaskPlanItemPatch {
    pub title: Option<String>,
    pub body: Option<Option<String>>,
    pub priority: Option<Option<TaskPriority>>,
    pub owner_agent_id: Option<Option<Uuid>>,
    pub assigned_agent_id: Option<Option<Uuid>>,
    pub source_task_id: Option<Option<Uuid>>,
    pub context_refs: Option<Vec<ContextSourceRef>>,
    pub story_ref: Option<Option<SubjectRef>>,
}
