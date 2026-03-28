pub mod adapters;
pub mod connector;
pub mod connectors;
mod hook_events;
pub mod hooks;
#[cfg(feature = "pi-agent")]
mod runtime_delegate;

pub use connector::{DynAgentTool, RuntimeToolProvider};
pub use connector::{
    AgentConnector, ConnectorCapabilities, ConnectorError, ConnectorType,
    ExecutionAddressSpace, ExecutionContext, ExecutionMount, ExecutionMountCapability,
    ExecutionStream, AgentConfig, AgentInfo, FlowCapabilities, PromptPayload,
    ThinkingLevel, is_native_agent, to_vibe_kanban_config,
};
pub use hooks::{
    ActiveTaskMeta, ActiveWorkflowMeta, ExecutionHookProvider, HookApprovalRequest,
    HookCompletionStatus, HookConstraint, HookContextFragment, HookContributionSet,
    HookDiagnosticEntry, HookError, HookEvaluationQuery, HookOwnerSummary, HookPendingAction,
    HookPendingActionResolutionKind, HookPendingActionStatus, HookPolicyView, HookResolution,
    HookSessionRuntime, HookSessionRuntimeSnapshot, HookSourceLayer, HookSourceRef,
    HookStepAdvanceRequest, HookTraceEntry, HookTrigger, NoopExecutionHookProvider,
    PendingExecutionLogEntry, SessionHookRefreshQuery, SessionHookSnapshot,
    SessionHookSnapshotQuery, SessionSnapshotMetadata, SharedHookSessionRuntime,
};
#[cfg(feature = "pi-agent")]
pub use runtime_delegate::HookRuntimeDelegate;
