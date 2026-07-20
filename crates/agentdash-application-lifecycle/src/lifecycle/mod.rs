pub mod applied_resource_surface;
mod completion;
mod dispatch;
mod dispatch_facade;
pub mod dispatch_service;
pub mod execution_log;
pub mod history_projection;
mod orchestrator;
mod projection;
pub(crate) mod run;
pub mod run_command_service;
pub mod run_view_builder;
mod runtime_thread_association;
mod session_run_context_resolver;
mod session_tool_result_cache;
pub mod surface;
pub mod tools;
pub mod vfs_catalog;
pub mod vfs_mount;
pub mod vfs_provider;

pub use agentdash_application_workflow::WorkflowApplicationError;
pub use applied_resource_surface::{
    AgentRunLifecycleAppliedResourceSurfaceCompiler, AgentRunLifecycleMountFacts,
    AgentRunLifecycleMountFactsQueryPort, install_agent_run_lifecycle_applied_mount,
};
pub use completion::{session_terminal_state_tag, session_terminal_summary};
pub use dispatch_facade::LifecycleDispatchFacade;
pub use dispatch_service::LifecycleDispatchService;
pub use execution_log::{
    RuntimeNodeArtifactScope, RuntimeNodePortArtifactRef, load_scoped_port_output_map,
    materialize_activity_summary,
};
pub use history_projection::{
    DeferredLifecycleHistoryQuery, LifecycleHistoryProjection, LifecycleHistoryQueryError,
    LifecycleHistoryQueryPort, ProductRuntimeLifecycleHistoryQuery,
};
pub use orchestrator::{
    AdvanceCurrentActivityInput, AdvanceCurrentNodeResult, AdvanceCurrentNodeStatus,
    AdvanceCurrentRuntimeThreadActivityInput, LifecycleNodeAdvanceOutcome, LifecycleOrchestrator,
    LifecycleOrchestratorDeps, OrchestrationResult,
};
pub use projection::{
    ActiveWorkflowProjection, resolve_active_workflow_projection_for_target,
    resolve_active_workflow_projection_from_message_stream_trace,
};
pub use run::select_active_run;
pub use run_command_service::{
    ContinueLifecycleRunResult, CreateLifecycleRunCommand, LifecycleRunCommandDeps,
    LifecycleRunCommandService,
};
pub use runtime_thread_association::{
    LIFECYCLE_ACTIVITY_LABEL_PREFIX, LIFECYCLE_NODE_LABEL_PREFIX, RuntimeThreadCurrentFrame,
    build_lifecycle_activity_label, build_lifecycle_node_label,
    lifecycle_activity_parts_from_label, resolve_activity_runtime_association_from_runtime_thread,
    resolve_current_frame_from_delivery_trace_ref,
};
pub use session_run_context_resolver::{SubjectRunContextResolver, build_subject_run_context};
pub use session_tool_result_cache::{
    SessionToolResultCache, SessionToolResultCacheRead, SessionToolResultCacheStatus,
    SessionToolResultCacheStatusKind, lifecycle_path_for_tool_result,
    readable_aliases_from_item_id,
};
pub use surface::AgentRunLifecycleSurfaceProjector;
pub use vfs_provider::LifecycleMountProvider;
