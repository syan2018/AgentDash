use uuid::Uuid;

use agentdash_domain::task::TaskStatus;

#[derive(Debug, thiserror::Error)]
pub enum TaskExecutionError {
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    UnprocessableEntity(String),
    #[error("{0}")]
    Internal(String),
}

/// Task 执行视图（替代原 TaskSessionResult）。
///
/// 所有字段从 lifecycle facts 投影得到，附带 source refs。
#[derive(Debug, Clone)]
pub struct TaskExecutionView {
    pub task_id: Uuid,
    pub execution_status: Option<String>,
    pub agent_ref: Option<Uuid>,
    pub run_ref: Option<Uuid>,
    pub frame_ref: Option<Uuid>,
    pub delivery_runtime_ref: Option<Uuid>,
    pub task_status: TaskStatus,
}
