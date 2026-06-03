pub(crate) mod activity_activation;
mod activity_run;
pub mod agent_executor;
pub mod agent_message;
mod catalog;
mod completion;
mod definition;
pub mod dispatch_service;
pub mod engine;
mod error;
pub mod execution_log;
pub mod frame_builder;
pub mod frame_construction;
pub mod frame_hook_runtime;
pub mod frame_surface;
pub mod graph_resolver;
pub mod lifecycle;
pub mod lifecycle_gate_service;
pub mod lifecycle_run_view_builder;
pub mod orchestrator;
pub mod projection;
pub(crate) mod run;
pub mod runtime_launch;
pub mod scheduler;
mod session_association;
mod session_run_context_resolver;
mod subject_execution_control;
pub mod tools;

#[cfg(test)]
pub(crate) use activity_activation::KickoffPromptFragment;
pub(crate) use activity_activation::{
    ActivityActivation, ActivityActivationInput, activate_activity_with_platform,
    agent_mcp_entries_from_servers, build_capability_state_for_activation,
};
pub use activity_run::{ActivityGraphInstanceExecutionResult, ActivityLifecycleRunService};
pub use agent_executor::{
    AgentActivityAssignmentTarget, AgentActivityExecutorLauncher, AgentActivityLaunchContext,
    AgentActivityRuntimePort, AgentActivitySessionPort, ContinueRootExecutionPolicy,
    RuntimeSessionDeliveryPolicy,
};
pub use agent_message::{
    LifecycleAgentMessageCommand, LifecycleAgentMessageDelivery, LifecycleAgentMessageDeliveryPort,
    LifecycleAgentMessageDispatch, LifecycleAgentMessageService,
    SessionLaunchLifecycleAgentMessageDeliveryPort,
};
pub use agentdash_domain::workflow::{
    ActivityInputArtifact, ActivityLifecycleRunState, ActivityOutputArtifact, ActivityPortValue,
    ActivityRunStatus, AgentReusePolicy, RuntimeSessionPolicy, WorkflowSessionTerminalState,
};
pub use catalog::{ActivityLifecycleCatalogService, WorkflowCatalogService};
pub use completion::{session_terminal_state_tag, session_terminal_summary};
pub use definition::{
    BuiltinLifecycleTemplate, BuiltinWorkflowBundle, BuiltinWorkflowTemplate,
    BuiltinWorkflowTemplateBundle, TRELLIS_DAG_TASK_TEMPLATE_KEY, build_builtin_workflow_bundle,
    get_builtin_workflow_template, list_builtin_workflow_templates,
};
pub use dispatch_service::{
    LifecycleDispatchService, RuntimeSessionCreationRequest, RuntimeSessionCreator,
    SessionPersistenceRuntimeSessionCreator,
};
pub use engine::{ActivityEvent, LifecycleEngine, LifecycleEngineError};
pub use error::WorkflowApplicationError;
pub use execution_log::{
    ActivityPortArtifactRef, load_scoped_port_output_map, materialize_activity_summary,
};
pub use frame_builder::AgentFrameBuilder;
pub use frame_construction::FrameConstructionService;
pub use frame_hook_runtime::AgentFrameHookRuntime;
pub use frame_surface::{AgentFrameSurfaceExt, FrameContextBundleSummary};
pub use graph_resolver::{ResolvedWorkflowGraph, WorkflowGraphResolver};
pub use lifecycle::mount::{
    append_active_workflow_lifecycle_mount, ensure_active_workflow_lifecycle_mount,
    writable_port_keys_for_active_workflow,
};
pub use lifecycle_gate_service::LifecycleGateService;
pub use orchestrator::{
    AdvanceCurrentActivityInput, AdvanceCurrentNodeResult, AdvanceCurrentNodeStatus,
    LifecycleNodeAdvanceOutcome, LifecycleOrchestrator,
};
#[cfg(test)]
pub(crate) use projection::activity_projection;
pub use projection::{
    ActiveWorkflowProjection, resolve_active_workflow_projection_for_session,
    resolve_active_workflow_projection_for_target,
};
pub use run::select_active_run;
pub use runtime_launch::{FrameLaunchEnvelope, FrameLaunchIntent, FrameRuntimeSurface};
pub use scheduler::{
    ActivityExecutorLaunchOutcome, ActivityExecutorLauncher, ActivityExecutorScheduler,
    ActivityExecutorStartError,
};
pub(crate) use session_association::select_assignment_for_frame;
pub use session_association::{
    LIFECYCLE_ACTIVITY_LABEL_PREFIX, LIFECYCLE_NODE_LABEL_PREFIX, build_lifecycle_activity_label,
    build_lifecycle_node_label, lifecycle_activity_parts_from_label,
    resolve_activity_session_association,
};
pub use session_run_context_resolver::{SubjectRunContextResolver, build_subject_run_context};
pub use subject_execution_control::{
    CancelSubjectExecutionCommand, RuntimeCancelDeliveryCommand, SubjectExecutionCancelResult,
    SubjectExecutionControlService,
};
