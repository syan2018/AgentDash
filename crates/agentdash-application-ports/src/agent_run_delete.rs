use async_trait::async_trait;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeleteAgentRunCommand {
    pub project_id: Uuid,
    pub run_id: Uuid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeleteAgentRunOutcome {
    pub project_id: Uuid,
    pub run_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DeleteAgentRunError {
    #[error("AgentRun 不存在或不属于当前 Project: {run_id}")]
    NotFound { run_id: Uuid },
    #[error("AgentRun {run_id} 仍有活动中的 Runtime turn，不能删除")]
    RuntimeActive { run_id: Uuid },
    #[error("AgentRun 删除失败: {0}")]
    Persistence(String),
}

/// AgentRun product graph and its canonical runtime owner graph are deleted as one unit of work.
#[async_trait]
pub trait AgentRunDeleteStore: Send + Sync {
    async fn delete(
        &self,
        command: DeleteAgentRunCommand,
    ) -> Result<DeleteAgentRunOutcome, DeleteAgentRunError>;
}
