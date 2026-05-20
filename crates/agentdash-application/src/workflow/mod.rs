mod activity_run;
pub mod agent_executor;
mod catalog;
mod completion;
mod definition;
pub mod engine;
mod error;
pub mod execution_log;
pub mod freeform;
pub mod lifecycle;
pub mod orchestrator;
pub mod projection;
pub(crate) mod run;
pub mod scheduler;
mod session_association;
pub mod step_activation;
pub mod tools;

pub use activity_run::ActivityLifecycleRunService;
pub use agent_executor::{
    AgentActivityExecutorLauncher, AgentActivityLaunchContext, AgentActivityRuntimePort,
    AgentActivitySessionPort,
};
pub use agentdash_domain::workflow::{
    ActivityInputArtifact, ActivityLifecycleRunState, ActivityOutputArtifact, ActivityPortValue,
    ActivityRunStatus, WorkflowSessionTerminalState,
};
pub use catalog::{ActivityLifecycleCatalogService, WorkflowCatalogService};
pub use completion::{session_terminal_state_tag, session_terminal_summary};
pub use definition::{
    BuiltinLifecycleTemplate, BuiltinWorkflowBundle, BuiltinWorkflowTemplate,
    BuiltinWorkflowTemplateBundle, TRELLIS_DAG_TASK_TEMPLATE_KEY, build_builtin_workflow_bundle,
    get_builtin_workflow_template, list_builtin_workflow_templates,
};
pub use engine::{ActivityEvent, LifecycleEngine, LifecycleEngineError};
pub use error::WorkflowApplicationError;
pub use execution_log::{load_port_output_map, materialize_step_summary};
pub use freeform::{
    FREEFORM_ACTIVITY_KEY, FREEFORM_AGENT_WORKFLOW_KEY, FREEFORM_LIFECYCLE_KEY,
    FREEFORM_SESSION_LABEL, FreeformLifecycleService, build_freeform_lifecycle,
    build_freeform_workflow,
};
pub use lifecycle::mount::{
    append_active_workflow_lifecycle_mount, ensure_active_workflow_lifecycle_mount,
    writable_port_keys_for_active_workflow,
};
pub use orchestrator::{
    AdvanceCurrentActivityInput, AdvanceCurrentNodeInput, AdvanceCurrentNodeResult,
    AdvanceCurrentNodeStatus, LifecycleNodeAdvanceOutcome, LifecycleOrchestrator,
};
pub use projection::{
    ActiveWorkflowProjection, resolve_active_workflow_projection,
    resolve_active_workflow_projection_for_session, resolve_workflow_projection_by_run,
};
pub use run::{
    ActivateLifecycleStepCommand, BindAndActivateLifecycleStepCommand,
    BindLifecycleStepSessionCommand, CompleteLifecycleStepCommand, FailLifecycleStepCommand,
    LifecycleRunService, LifecycleStepProjector, RecordGateCollisionCommand,
    StartLifecycleRunCommand, build_step_projector_from_repos, select_active_run,
};
pub use scheduler::{
    ActivityExecutorLaunchOutcome, ActivityExecutorLauncher, ActivityExecutorScheduler,
    ActivityExecutorStartError,
};
pub use session_association::{
    LIFECYCLE_ACTIVITY_LABEL_PREFIX, LIFECYCLE_NODE_LABEL_PREFIX, build_lifecycle_activity_label,
    build_lifecycle_node_label, lifecycle_activity_parts_from_label,
    resolve_activity_session_association,
};
pub use step_activation::{
    KickoffPromptFragment, StepActivation, StepActivationInput, activate_step_with_platform,
    agent_mcp_entries_from_servers, build_capability_state_for_activation,
    capability_delta_directives, capability_keys_sorted, empty_presets, tool_directives_from_keys,
};
