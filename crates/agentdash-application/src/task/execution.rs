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
pub struct StartTaskCommand {
    pub task_id: Uuid,
    pub override_prompt: Option<String>,
    pub executor_config: Option<AgentConfig>,
}

#[derive(Debug, Clone)]
pub struct ContinueTaskCommand {
    pub task_id: Uuid,
    pub additional_prompt: Option<String>,
    pub executor_config: Option<AgentConfig>,
}

#[derive(Debug, Clone)]
pub struct StartTaskResult {
    pub task_id: Uuid,
    pub session_id: String,
    pub executor_session_id: Option<String>,
    pub turn_id: String,
    pub status: TaskStatus,
    pub context_sources: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ContinueTaskResult {
    pub task_id: Uuid,
    pub session_id: String,
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
    pub session_id: Option<String>,
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
