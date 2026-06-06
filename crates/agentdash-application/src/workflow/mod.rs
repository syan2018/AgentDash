pub(crate) mod activity_activation;
pub mod agent_message;
pub mod agent_steering;
mod catalog;
mod completion;
mod definition;
pub mod dispatch_service;
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
pub mod orchestration;
pub mod orchestrator;
mod project_agent_session_start;
pub mod projection;
pub(crate) mod run;
pub mod runtime_launch;
mod session_association;
mod session_run_context_resolver;
mod subject_execution_control;
pub mod tools;

#[cfg(test)]
pub(crate) use activity_activation::KickoffPromptFragment;
pub(crate) use activity_activation::{
    ActivityActivation, ActivityActivationInput, activate_activity_with_platform,
};
pub use agent_message::{
    LifecycleAgentMessageCommand, LifecycleAgentMessageDelivery, LifecycleAgentMessageDeliveryPort,
    LifecycleAgentMessageDispatch, LifecycleAgentMessageService,
    SessionLaunchLifecycleAgentMessageDeliveryPort,
};
pub use agent_steering::{
    LifecycleAgentSteeringCommand, LifecycleAgentSteeringDispatch, LifecycleAgentSteeringService,
};
pub use agentdash_domain::workflow::{
    AgentReusePolicy, RuntimeSessionPolicy, WorkflowSessionTerminalState,
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
pub use error::WorkflowApplicationError;
pub use execution_log::{
    RuntimeNodeArtifactScope, RuntimeNodePortArtifactRef, load_scoped_port_output_map,
    materialize_activity_summary,
};
pub use frame_builder::AgentFrameBuilder;
pub use frame_construction::FrameConstructionService;
pub use frame_hook_runtime::AgentFrameHookRuntime;
pub use frame_surface::{AgentFrameSurfaceExt, FrameContextBundleSummary};
pub use graph_resolver::{ResolvedWorkflowGraph, WorkflowGraphResolver};
pub use lifecycle::mount::{
    LifecycleMountSurface, append_active_workflow_lifecycle_mount,
    ensure_active_workflow_lifecycle_mount, lifecycle_mount_surface_for_active_workflow,
    writable_port_keys_for_active_workflow,
};
pub use lifecycle_gate_service::LifecycleGateService;
pub use orchestration::{
    OrchestrationExecutorDrainResult, OrchestrationExecutorLauncher, SubmitHumanGateDecisionInput,
    SubmitHumanGateDecisionResult,
};
pub use orchestration::{
    WORKFLOW_GRAPH_COMPILER_SCHEMA_VERSION, WorkflowGraphCompileDiagnostic,
    WorkflowGraphCompileInput, WorkflowGraphCompileMode, WorkflowGraphCompileOutput,
    WorkflowGraphCompileSourceMetadata, WorkflowGraphCompiler, compile_workflow_graph,
};
pub use orchestrator::{
    AdvanceCurrentActivityInput, AdvanceCurrentNodeResult, AdvanceCurrentNodeStatus,
    LifecycleNodeAdvanceOutcome, LifecycleOrchestrator,
};
pub use project_agent_session_start::{
    ProjectAgentSessionStartCommand, ProjectAgentSessionStartDispatch,
    ProjectAgentSessionStartRepos, ProjectAgentSessionStartService,
};
#[cfg(test)]
pub(crate) use projection::activity_projection;
pub use projection::{
    ActiveWorkflowProjection, resolve_active_workflow_projection_for_session,
    resolve_active_workflow_projection_for_target,
};
pub use run::select_active_run;
pub use runtime_launch::{FrameLaunchEnvelope, FrameLaunchIntent, FrameRuntimeSurface};
pub use session_association::{
    LIFECYCLE_ACTIVITY_LABEL_PREFIX, LIFECYCLE_NODE_LABEL_PREFIX, build_lifecycle_activity_label,
    build_lifecycle_node_label, lifecycle_activity_parts_from_label,
    resolve_activity_session_association, resolve_current_frame_for_runtime_session,
};
pub use session_run_context_resolver::{SubjectRunContextResolver, build_subject_run_context};
pub use subject_execution_control::{
    CancelSubjectExecutionCommand, RuntimeCancelDeliveryCommand, SubjectExecutionCancelResult,
    SubjectExecutionControlService,
};
