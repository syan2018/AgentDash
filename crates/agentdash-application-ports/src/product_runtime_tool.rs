use agentdash_agent_runtime_contract::RuntimeThreadId;
use async_trait::async_trait;
use serde_json::Value;
use uuid::Uuid;

/// Product-owned tool families exposed to an Agent Runtime through the typed Tool Broker.
///
/// The enum is intentionally limited to Product commands. VFS, Task and dynamic MCP tools keep
/// their dedicated execution ports and authorization grants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProductRuntimeToolKind {
    Wait,
    CompleteLifecycleNode,
    CompanionRequest,
    CompanionRespond,
    WorkspaceModuleList,
    WorkspaceModuleDescribe,
    WorkspaceModuleOperate,
    WorkspaceModuleInvoke,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductRuntimeToolTarget {
    pub project_id: Uuid,
    pub run_id: Uuid,
    pub agent_id: Uuid,
}

/// Stable Product coordinates resolved and authorized before a tool reaches Application code.
///
/// RuntimeThread and callback coordinates are delivery evidence. Product services resolve any
/// richer business owner (for example a lifecycle node or companion gate) from their own durable
/// projections instead of depending on AgentSession state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductRuntimeToolContext {
    pub runtime_thread_id: RuntimeThreadId,
    pub target: ProductRuntimeToolTarget,
    pub turn_id: String,
    pub item_id: Option<String>,
    pub effect_id: String,
    pub invocation_id: String,
    pub deadline_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProductRuntimeToolRequest {
    pub context: ProductRuntimeToolContext,
    pub arguments: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ProductRuntimeToolOutcome {
    Completed { output: Value },
    Rejected { code: String, message: String },
    Failed { code: String, message: String },
}

/// Application-owned command boundary for one Product tool.
///
/// Implementations retain the existing Product business service and only adapt the typed Runtime
/// callback into that service's command model.
#[async_trait]
pub trait ProductRuntimeToolService: Send + Sync {
    fn kind(&self) -> ProductRuntimeToolKind;

    fn parameters_schema(&self) -> Value;

    async fn execute(&self, request: ProductRuntimeToolRequest) -> ProductRuntimeToolOutcome;
}
