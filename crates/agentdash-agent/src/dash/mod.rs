mod core_execution;
mod history;
mod lifecycle;
mod service;
mod store;

pub use core_execution::{
    DashBeforeToolDecision, DashCancellation, DashCompactionTurn, DashCompactionTurnOutput,
    DashCoreContext, DashCoreError, DashCoreEvent, DashCoreOutput, DashCoreTurn,
    DashCoreTurnResult, DashExecutionCallbacks, DashExecutionEvent, DashExecutionFailure,
    DashFinishReason, DashMessage, DashMessageRole, DashProvider, DashProviderEvent,
    DashProviderEventStream, DashProviderRequest, DashProviderRoundMaterializer,
    DashProviderRoundSnapshots, DashToolCall, DashToolCallDeltaContent, DashToolCallbacks,
    DashToolDefinition, DashToolExecutionEvent, DashToolExecutionSequence, DashToolExecutionStream,
    DashToolResult, execution_assistant_item_id, execution_tool_item_id,
};
pub use history::{
    ActivityStatus, AgentHistory, AgentHistoryEntry, AgentHistoryReplayer, AgentHistoryState,
    AgentItemId, AgentSessionId, AgentTurnId, BranchId, CompactionId, CompactionMode,
    CompactionState, ContextDeliveryFidelity, ContextRevision, DashSurface, DashSurfaceInstruction,
    ForkCutoff, ForkLineage, HistoryContribution, HistoryEntryId, HistoryError, HistoryPayload,
    InitialContextContribution, InitialContextInstallation, InitialContextMode, InteractionId,
    InteractionState, ItemDetails, ItemKind, ItemState, ProjectedAgentHistoryEntry, SessionStatus,
    ToolActivityResult, TurnState, accepted_compaction_summary_frame, compaction_context_revision,
    fold_history,
};
pub use lifecycle::{
    CommandDependency, CommandId, CommandOutcome, CommandStatus, DashCommand, DashCommandKind,
    DashExecutionConsistency, DashLifecycle, EffectId, LifecycleError,
};
pub use service::{
    DashAgentChanges, DashAgentRead, DashAgentRepository, DashAgentRepositoryState,
    DashAgentRepositoryStore, DashAgentService, DashCommandReceipt, DashCommandRequest,
    DashCompactionRequest, DashCompactionResult, DashCompactor, DashContextMessageUsage,
    DashContextRecipe, DashContextRecipeMessage, DashContextToolUsage, DashContextUsageAnalysis,
    DashContextUsageCategory, DashConversationNamer, DashConversationNamingRequest,
    DashEffectInspection, DashExecutionDependencies, DashHistoryCallbacks, DashHistoryCommit,
    DashPublicCommand, DashReceiptState, DashServiceError, DashTerminalOutcome,
    NoopDashConversationNamer, NoopDashHistoryCallbacks,
    compaction_context_frames_from_history_state, context_recipe_from_history_state,
};
pub use store::{
    CommandSettlement, DashAgentChange, DashAgentCommit, DashAgentStore, DashChangeCursor,
    StoreError,
};
