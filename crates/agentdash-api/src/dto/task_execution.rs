use serde::{Deserialize, Serialize};
use uuid::Uuid;

use agentdash_contracts::core::TaskResponse;
use agentdash_domain::task::TaskStatus;

#[derive(Debug, Deserialize, Default)]
pub struct StartTaskRequest {
    #[serde(default)]
    pub override_prompt: Option<String>,
    #[serde(default)]
    pub executor_config: Option<agentdash_spi::AgentConfig>,
}

/// Task start 结果 — 返回 lifecycle 控制面锚点引用。
#[derive(Debug, Serialize)]
pub struct StartTaskResponse {
    pub task_id: Uuid,
    pub run_ref: Uuid,
    pub agent_ref: Uuid,
    pub frame_ref: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignment_ref: Option<Uuid>,
    pub subject_execution_ref: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delivery_runtime_ref: Option<Uuid>,
    pub status: TaskStatus,
}

#[derive(Debug, Deserialize, Default)]
pub struct ContinueTaskRequest {
    #[serde(default)]
    pub additional_prompt: Option<String>,
    #[serde(default)]
    pub executor_config: Option<agentdash_spi::AgentConfig>,
}

/// Task continue 结果 — 返回 lifecycle 控制面锚点引用。
#[derive(Debug, Serialize)]
pub struct ContinueTaskResponse {
    pub task_id: Uuid,
    pub run_ref: Uuid,
    pub agent_ref: Uuid,
    pub frame_ref: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignment_ref: Option<Uuid>,
    pub subject_execution_ref: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delivery_runtime_ref: Option<Uuid>,
    pub status: TaskStatus,
}

/// Task cancel 结果 — Task view 来自 lifecycle projection，runtime delivery 仅为追踪引用。
#[derive(Debug, Serialize)]
pub struct CancelTaskResponse {
    pub task: TaskResponse,
    pub run_ref: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub graph_instance_ref: Option<Uuid>,
    pub agent_ref: Uuid,
    pub frame_ref: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignment_ref: Option<Uuid>,
    pub subject_execution_ref: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_delivery_ref: Option<String>,
}

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
