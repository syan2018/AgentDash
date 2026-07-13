use agentdash_domain::workflow::{
    AgentProcedureContract, AgentRuntimeRefs, ExecutionDispatchResult, ExecutionIntent,
    OrchestrationBindingRefs, RuntimePolicy,
};
use async_trait::async_trait;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct LifecycleDispatchRequest {
    pub intent: ExecutionIntent,
}

#[derive(Debug, Clone)]
pub struct LifecycleDispatchPortResult {
    pub result: ExecutionDispatchResult,
}

#[derive(Debug, Clone)]
pub struct WorkflowAgentNodeMaterializationRequest {
    pub run_id: Uuid,
    pub orchestration_binding: OrchestrationBindingRefs,
    pub runtime_policy: RuntimePolicy,
    pub frame_created_by_id: Option<String>,
    pub workflow_contract: Option<AgentProcedureContract>,
}

#[derive(Debug, Clone)]
pub struct WorkflowAgentNodeMaterializationResult {
    pub runtime_refs: AgentRuntimeRefs,
    pub delivery_runtime_ref: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LifecycleMaterializationError {
    #[error("lifecycle materialization rejected: {message}")]
    Rejected { message: String },
    #[error("lifecycle materialization missing dependency: {message}")]
    MissingDependency { message: String },
    #[error(
        "lifecycle materialization repository failed: operation={operation}, message={message}"
    )]
    Repository {
        operation: &'static str,
        message: String,
    },
    #[error("lifecycle materialization failed: {message}")]
    Internal { message: String },
}

#[async_trait]
pub trait LifecycleDispatchPort: Send + Sync {
    async fn dispatch_lifecycle(
        &self,
        request: LifecycleDispatchRequest,
    ) -> Result<LifecycleDispatchPortResult, LifecycleMaterializationError>;
}

#[async_trait]
pub trait WorkflowAgentNodeMaterializationPort: Send + Sync {
    async fn materialize_workflow_agent_node(
        &self,
        request: WorkflowAgentNodeMaterializationRequest,
    ) -> Result<WorkflowAgentNodeMaterializationResult, LifecycleMaterializationError>;
}
