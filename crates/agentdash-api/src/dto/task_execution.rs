use serde::Serialize;
use uuid::Uuid;

use agentdash_domain::task::TaskStatus;

/// Task 执行视图 — 从 lifecycle facts 投影。
#[derive(Debug, Serialize)]
pub struct TaskExecutionViewResponse {
    pub task_id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_ref: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_ref: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame_ref: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delivery_runtime_ref: Option<Uuid>,
    pub task_status: TaskStatus,
}
