mod admission_delegate;
mod assignment_context_frame;
mod auto_resume_context_frame;
pub mod bootstrap;
mod branching;
mod compaction_checkpoint;
mod compaction_context_frame;
#[cfg(test)]
pub(crate) mod construction;
pub mod context;
mod context_frame;
mod context_projector;
mod context_usage_marking;
mod context_usage_projection;
pub(crate) mod runtime_transition_service;
// context_query_use_case 已删除：所有 API 消费者已迁移至 frame-based read model
pub mod control;
pub mod core;
pub(crate) mod dimension;
pub(crate) mod effects_service;
mod environment_context_frame;
pub mod eventing;
mod guidelines_context_frame;
pub(crate) mod hook_delegate;
pub(crate) mod hook_events;
pub(crate) mod hook_injection_sink;
mod hook_messages;
pub(crate) mod hooks_service;
pub(crate) mod hub;
mod hub_support;
mod identity_context_frame;
pub mod launch;
mod memory_context_frame;
mod memory_inventory_entries;
#[cfg(test)]
#[path = "../../test-support/session_memory_persistence.rs"]
mod memory_persistence;
pub mod path_policy;
mod pending_action_context_frame;
pub mod persistence;
pub mod plan;
pub(crate) mod post_turn_handler;
mod prompt_vfs;
pub(crate) mod runtime_builder;
pub(crate) mod runtime_capability;
pub(crate) mod runtime_commands;
pub(crate) mod runtime_control;
mod runtime_registry;
pub(crate) mod runtime_services;
pub mod stall_detector;
pub mod terminal_cache;
pub(crate) mod terminal_effects;
pub(crate) mod title_generator;
pub(crate) mod title_service;
pub(crate) mod tool_assembly;
pub(crate) mod tool_result_cache;
mod transcript_restore;
pub(crate) mod turn_processor;
mod turn_supervisor;
pub mod types;

pub use crate::runtime::McpServerSummary;
pub use branching::{
    SessionBranchingService, SessionForkRequest, SessionForkResult, SessionLineageView,
    SessionProjectionRollbackRequest, SessionProjectionRollbackResult,
};
pub use context::ExecutorResolution;
pub use context_projector::ContextProjector;
pub use context_usage_projection::{
    SessionAttachmentContextContribution, SessionContextProjectionReadModel,
    SessionContextUsageCategory, SessionContextUsageItem, SessionContextUsageReadModel,
    SessionMessageContextBreakdown, SessionProjectionMessageRefReadModel,
    SessionProjectionSegmentProvenanceReadModel, SessionProjectionSegmentReadModel,
    SessionProjectionSourceRangeReadModel, SessionToolContextContribution,
};
pub use control::{SessionControlService, SessionTurnSteerCommand};
pub use core::SessionCoreService;
pub use effects_service::SessionEffectsService;
pub use eventing::SessionEventingService;
pub use hook_delegate::HookRuntimeDelegate;
pub use hook_events::build_hook_trace_envelope;
pub use hooks_service::SessionHookService;
pub use hub_support::TurnTerminalKind;
pub use launch::{LaunchCommandOutcome, SessionLaunchService};
#[cfg(test)]
pub use memory_persistence::MemorySessionPersistence;
pub use persistence::{
    PersistedSessionEvent, SessionCompactionStore, SessionEventBacklog, SessionEventPage,
    SessionEventStore, SessionLineageRecord, SessionLineageRelationKind, SessionLineageStatus,
    SessionLineageStore, SessionMetaStore, SessionPersistence, SessionProjectionStore,
    SessionRuntimeCommandStore, SessionTerminalEffectStore,
};
pub use post_turn_handler::{
    DynPostTurnHandler, DynSessionTerminalCallback, DynTerminalHookEffectHandlerRegistry,
    EmptyTerminalHookEffectHandlerRegistry, PostTurnHandler, SessionTerminalCallback,
    TerminalHookEffectBinding, TerminalHookEffectHandlerRegistry,
};
pub use prompt_vfs::local_workspace_vfs;
pub use runtime_builder::SessionRuntimeBuilder;
pub use runtime_commands::{
    AgentFrameTransitionRecord, RuntimeCommandRecord, RuntimeCommandStatus, RuntimeDeliveryCommand,
    RuntimeDeliveryCommandKind,
};
pub use runtime_control::SessionRuntimeService;
pub use runtime_services::SessionRuntimeServices;
pub use runtime_transition_service::SessionRuntimeTransitionService;
pub use terminal_effects::{
    NewTerminalEffectRecord, TerminalEffectRecord, TerminalEffectStatus, TerminalEffectType,
};
pub use title_service::SessionTitleService;
pub use tool_result_cache::{
    SESSION_TOOL_RESULT_CACHE_DEFAULT_TTL, SessionToolResultCache, SessionToolResultCacheMetadata,
    SessionToolResultCachePut, SessionToolResultCacheRead, SessionToolResultCacheStatus,
    SessionToolResultCacheStatusKind, lifecycle_path_for_tool_result,
    readable_aliases_from_item_id,
};
pub use turn_processor::{SessionTurnProcessor, SessionTurnProcessorConfig, TurnEvent};
pub use types::{
    ApplyMountOperationsEffect, ApplyVfsOverlayEffect, CapabilityArtifactSource,
    CapabilityContributionRecord, CapabilityDeclarationRecord, CapabilityDimensionKey,
    CapabilityState, DECLARATION_TYPE_CAPABILITY_DIRECTIVE, DECLARATION_TYPE_MOUNT_OPERATION,
    EFFECT_TYPE_APPLY_MOUNT_OPERATIONS, EFFECT_TYPE_APPLY_VFS_OVERLAY,
    EFFECT_TYPE_SET_COMPANION_AGENT_ROSTER, EFFECT_TYPE_SET_MCP_SERVER_SET,
    EFFECT_TYPE_SET_TOOL_ACCESS, ExecutionStatus, HookSnapshotReloadTrigger,
    PendingCapabilityStateTransition, PromptLaunchPath, ResolvedPromptPayload,
    RuntimeCapabilityEffectRecord, RuntimeCapabilityTransition, RuntimeTraceLaunchState,
    SessionExecutionState, SessionMeta, SessionRepositoryRehydrateMode,
    SetCompanionAgentRosterEffect, SetMcpServerSetEffect, SetToolAccessEffect, TitleSource,
    UserPromptInput, resolve_launch_prompt_payload, resolve_prompt_launch_path,
};
