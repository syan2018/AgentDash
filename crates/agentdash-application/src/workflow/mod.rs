mod catalog;
mod completion;
mod definition;
pub mod engine;
mod error;
pub mod execution_log;
pub mod lifecycle;
pub mod orchestrator;
pub mod projection;
pub(crate) mod run;
pub mod scheduler;
mod session_association;
pub mod step_activation;
pub mod tools;

pub use agentdash_domain::workflow::WorkflowSessionTerminalState;
pub use catalog::{ActivityLifecycleCatalogService, WorkflowCatalogService};
pub use completion::{session_terminal_state_tag, session_terminal_summary};
pub use definition::{
    BuiltinLifecycleTemplate, BuiltinWorkflowBundle, BuiltinWorkflowTemplate,
    BuiltinWorkflowTemplateBundle, TRELLIS_DAG_TASK_TEMPLATE_KEY, build_builtin_workflow_bundle,
    get_builtin_workflow_template, list_builtin_workflow_templates,
};
pub use engine::{
    ActivityEvent, ActivityInputArtifact, ActivityLifecycleRunState, ActivityOutputArtifact,
    ActivityPortValue, ActivityRunStatus, LifecycleEngine, LifecycleEngineError,
};
pub use error::WorkflowApplicationError;
pub use execution_log::{load_port_output_map, materialize_step_summary};
pub use lifecycle::mount::{
    append_active_workflow_lifecycle_mount, ensure_active_workflow_lifecycle_mount,
    writable_port_keys_for_active_workflow,
};
pub use orchestrator::{
    AdvanceCurrentNodeInput, AdvanceCurrentNodeResult, AdvanceCurrentNodeStatus,
    LifecycleNodeAdvanceOutcome, LifecycleOrchestrator,
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
pub use scheduler::ActivityExecutorScheduler;
pub use session_association::{LIFECYCLE_NODE_LABEL_PREFIX, build_lifecycle_node_label};
pub use step_activation::{
    KickoffPromptFragment, StepActivation, StepActivationInput, activate_step_with_platform,
    agent_mcp_entries_from_servers, build_capability_state_for_activation,
    capability_delta_directives, capability_keys_sorted, empty_presets, tool_directives_from_keys,
};
