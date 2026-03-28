pub mod adapters;
pub mod connector;
pub mod connectors;
mod hook_events;
pub mod hooks;
pub mod hub;
#[cfg(feature = "pi-agent")]
mod runtime_delegate;

pub use connector::{DynAgentTool, RuntimeToolProvider};
pub use connector::{
    AgentConnector, AgentDashExecutorConfig, ConnectorCapabilities, ConnectorError, ConnectorType,
    ExecutionAddressSpace, ExecutionContext, ExecutionMount, ExecutionMountCapability,
    ExecutionStream, ExecutorInfo, FlowCapabilities, PromptPayload, ThinkingLevel,
};
pub use hook_events::build_hook_trace_notification;
pub use hooks::{
    ExecutionHookProvider, HookApprovalRequest, HookCompletionStatus, HookConstraint,
    HookContextFragment, HookContributionSet, HookDiagnosticEntry, HookError, HookEvaluationQuery,
    HookOwnerSummary, HookPendingAction, HookPendingActionResolutionKind, HookPendingActionStatus,
    HookPolicyView, HookResolution, HookSessionRuntime, HookSessionRuntimeSnapshot,
    HookSourceLayer, HookSourceRef, HookStepAdvanceRequest, HookTraceEntry, HookTrigger,
    NoopExecutionHookProvider, PendingExecutionLogEntry, SessionHookRefreshQuery,
    SessionHookSnapshot, SessionHookSnapshotQuery, SharedHookSessionRuntime,
};
pub use hub::{
    CompanionSessionContext, ExecutorHub, PromptSessionRequest, SessionExecutionState, SessionMeta,
    UserPromptInput,
};
#[cfg(feature = "pi-agent")]
pub use runtime_delegate::HookRuntimeDelegate;
