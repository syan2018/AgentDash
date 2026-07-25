use async_trait::async_trait;
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeTaskToolScope {
    Project { project_id: Uuid },
    Task { project_id: Uuid, task_id: Uuid },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeTaskToolKind {
    Read,
    Write,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeTaskToolRequest {
    pub kind: RuntimeTaskToolKind,
    pub scope: RuntimeTaskToolScope,
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub arguments: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeTaskToolOutcome {
    Completed { output: Value },
    Rejected { code: String, message: String },
    Failed { code: String, message: String },
}

#[async_trait]
pub trait RuntimeTaskToolService: Send + Sync {
    fn parameters_schema(&self, kind: RuntimeTaskToolKind) -> Value;

    async fn execute(&self, request: RuntimeTaskToolRequest) -> RuntimeTaskToolOutcome;
}
