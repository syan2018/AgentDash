pub mod adapters;
pub mod connector;
pub mod connectors;
mod hook_events;
pub mod hooks;
pub mod hub;
#[cfg(feature = "pi-agent")]
mod runtime_delegate;

pub use connector::{
    AgentConnector, AgentDashExecutorConfig, ConnectorCapabilities, ConnectorError, ConnectorType,
    ExecutionAddressSpace, ExecutionContext, ExecutionMount, ExecutionMountCapability,
    ExecutionStream, ExecutorInfo, FlowCapabilities, PromptPayload,
};
#[cfg(feature = "pi-agent")]
pub use connector::RuntimeToolProvider;
pub use hook_events::build_hook_trace_notification;
pub use hooks::{
    ExecutionHookProvider, HookApprovalRequest, HookCompletionStatus, HookConstraint,
    HookContextFragment, HookContributionSet, HookDiagnosticEntry, HookError, HookEvaluationQuery,
    HookOwnerSummary, HookPendingAction, HookPendingActionResolutionKind, HookPendingActionStatus,
    HookPhaseAdvanceRequest, HookPolicy, HookResolution, HookSessionRuntime,
    HookSessionRuntimeSnapshot, HookSourceLayer, HookSourceRef, HookTraceEntry, HookTrigger,
    NoopExecutionHookProvider, SessionHookRefreshQuery, SessionHookSnapshot,
    SessionHookSnapshotQuery, SharedHookSessionRuntime,
};
pub use hub::{
    CompanionSessionContext, ExecutorHub, PromptSessionRequest, SessionExecutionState, SessionMeta,
};
#[cfg(feature = "pi-agent")]
pub use runtime_delegate::HookRuntimeDelegate;
