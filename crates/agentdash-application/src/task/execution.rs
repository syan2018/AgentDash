use uuid::Uuid;

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
    pub identity: Option<agentdash_spi::auth::AuthIdentity>,
}

#[derive(Debug, Clone)]
pub struct TaskExecutionResult {
    pub task_id: Uuid,
    /// AgentDash 内部 execution session id（Task child session）。
    pub session_id: String,
    /// 执行器原生 follow-up / resume id；不是 AgentDash 内部 session id。
    pub executor_session_id: Option<String>,
    pub turn_id: String,
    pub status: TaskStatus,
    pub context_sources: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct SessionOverview {
    pub title: String,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct TaskSessionResult {
    pub task_id: Uuid,
    /// AgentDash 内部 execution session id（通过 SessionBinding 解析）。
    pub session_id: Option<String>,
    /// 执行器原生 follow-up / resume id；不是 AgentDash 内部 session id。
    pub executor_session_id: Option<String>,
    pub task_status: TaskStatus,
    pub session_execution_status: Option<String>,
    pub agent_binding: agentdash_domain::task::AgentBinding,
    pub session_title: Option<String>,
    pub last_activity: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct StartedTurn {
    pub turn_id: String,
    pub context_sources: Vec<String>,
}
