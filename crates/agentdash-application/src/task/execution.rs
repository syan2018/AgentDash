use uuid::Uuid;

use agentdash_domain::workflow::SubjectExecutionRef;
use agentdash_domain::{common::AgentConfig, task::TaskStatus};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionPhase {
    Start,
    Continue,
}

#[derive(Debug, Clone)]
pub struct TaskExecutionCommand {
    pub task_id: Uuid,
    pub phase: ExecutionPhase,
    /// Start 时为 override_prompt，Continue 时为 additional_prompt
    pub prompt: Option<String>,
    pub executor_config: Option<AgentConfig>,
    pub identity: Option<agentdash_spi::platform::auth::AuthIdentity>,
}

/// Task execution dispatch 结果。
///
/// session_id 不再作为业务主键，改为 lifecycle 控制面锚点引用。
#[derive(Debug, Clone)]
pub struct TaskExecutionResult {
    pub task_id: Uuid,
    pub run_ref: Uuid,
    pub agent_ref: Uuid,
    pub frame_ref: Uuid,
    pub assignment_ref: Option<Uuid>,
    pub subject_execution_ref: SubjectExecutionRef,
    pub delivery_runtime_ref: Option<Uuid>,
    pub status: TaskStatus,
}

/// Task cancel dispatch 结果。
///
/// cancel 的命令目标是 SubjectExecution / Assignment；runtime session 只作为
/// delivery/provenance ref 返回。
#[derive(Debug, Clone)]
pub struct TaskExecutionCancelResult {
    pub task: agentdash_domain::task::Task,
    pub run_ref: Uuid,
    pub graph_instance_ref: Option<Uuid>,
    pub agent_ref: Uuid,
    pub frame_ref: Uuid,
    pub assignment_ref: Option<Uuid>,
    pub subject_execution_ref: SubjectExecutionRef,
    pub runtime_delivery_ref: Option<String>,
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
