mod core_execution;
mod history;
mod lifecycle;
mod service;
mod store;

pub use core_execution::{
    DashBeforeToolDecision, DashCancellation, DashCoreContext, DashCoreError, DashCoreEvent,
    DashCoreOutput, DashCoreTurn, DashCoreTurnResult, DashExecutionCallbacks, DashExecutionEvent,
    DashExecutionFailure, DashFinishReason, DashMessage, DashMessageRole, DashProvider,
    DashProviderEvent, DashProviderEventStream, DashProviderRequest, DashToolCall,
    DashToolCallbacks, DashToolDefinition, DashToolResult, execution_tool_item_id,
};
pub use history::{
    ActivityStatus, AgentHistory, AgentHistoryEntry, AgentHistoryState, AgentItemId,
    AgentSessionId, AgentTurnId, BranchId, CompactionId, CompactionMode, CompactionState,
    ContextDeliveryFidelity, ContextRevision, DashSurface, DashSurfaceInstruction, ForkCutoff,
    ForkLineage, HistoryContribution, HistoryEntryId, HistoryError, HistoryPayload,
    InitialContextContribution, InitialContextInstallation, InitialContextMode, InteractionId,
    InteractionState, ItemDetails, ItemKind, ItemState, SessionStatus, ToolActivityResult,
    TurnState, fold_history,
};
pub use lifecycle::{
    CommandDependency, CommandId, CommandOutcome, CommandStatus, DashCommand, DashCommandKind,
    DashExecutionConsistency, DashLifecycle, EffectId, EffectOutcome, LifecycleError,
};
pub use service::{
    DashAgentRead, DashAgentRepository, DashAgentRepositoryState, DashAgentRepositoryStore,
    DashAgentService, DashCommandReceipt, DashCommandRequest, DashCompactionRequest,
    DashCompactionResult, DashCompactor, DashConversationNamer, DashConversationNamingRequest,
    DashEffectInspection, DashExecutionDependencies, DashHistoryCallbacks, DashHistoryCommit,
    DashPublicCommand, DashReceiptState, DashServiceError, DashTerminalOutcome,
    NoopDashConversationNamer, NoopDashHistoryCallbacks,
};
pub use store::{
    CommandSettlement, DashAgentChange, DashAgentChangePayload, DashAgentCommit, DashAgentStore,
    DashChangeCursor, DashExecutionInspection, EffectSettlement, StoreError,
};
