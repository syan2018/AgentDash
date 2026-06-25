use std::collections::BTreeMap;

use agentdash_spi::hooks::{
    HookControlTarget, PendingExecutionLogEntry, RuntimeAdapterProvenance, SubjectRunContext,
};
use async_trait::async_trait;

use crate::lifecycle_surface_projection::ActiveWorkflowProjection;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookWorkflowProjectionQuery {
    pub target: HookControlTarget,
    pub provenance: RuntimeAdapterProvenance,
}

#[derive(Debug, Clone)]
pub struct HookWorkflowProjection {
    pub run_context: Option<SubjectRunContext>,
    pub active_workflow: Option<HookActiveWorkflowFacts>,
}

#[derive(Debug, Clone)]
pub struct HookActiveWorkflowFacts {
    pub projection: ActiveWorkflowProjection,
    pub fulfilled_output_ports: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HookExecutionLogAppendCommand {
    pub entries: Vec<PendingExecutionLogEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HookWorkflowProjectionError {
    #[error("hook workflow projection failed: {message}")]
    Projection { message: String },
    #[error("hook workflow projection repository failed: operation={operation}, message={message}")]
    Repository {
        operation: &'static str,
        message: String,
    },
    #[error("hook workflow effect failed: {message}")]
    Effect { message: String },
}

#[async_trait]
pub trait HookWorkflowProjectionPort: Send + Sync {
    async fn load_hook_workflow_projection(
        &self,
        query: HookWorkflowProjectionQuery,
    ) -> Result<HookWorkflowProjection, HookWorkflowProjectionError>;

    async fn append_execution_log(
        &self,
        command: HookExecutionLogAppendCommand,
    ) -> Result<(), HookWorkflowProjectionError>;
}
