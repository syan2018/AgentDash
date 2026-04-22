mod catalog;
mod completion;
mod definition;
mod error;
pub mod execution_log;
pub mod orchestrator;
pub mod projection;
pub(crate) mod run;
mod session_association;
pub mod step_activation;
pub mod tools;

pub use agentdash_domain::workflow::WorkflowSessionTerminalState;
pub use catalog::WorkflowCatalogService;
pub use completion::{
    WorkflowCompletionDecision, WorkflowCompletionEvidence, WorkflowCompletionSignalSet,
    evaluate_step_completion, session_terminal_state_tag, session_terminal_summary,
};
pub use definition::{
    BuiltinLifecycleTemplate, BuiltinWorkflowBundle, BuiltinWorkflowTemplate,
    BuiltinWorkflowTemplateBundle, TRELLIS_DAG_TASK_TEMPLATE_KEY, build_builtin_workflow_bundle,
    get_builtin_workflow_template, list_builtin_workflow_templates,
};
pub use error::WorkflowApplicationError;
pub use execution_log::{load_port_output_map, materialize_step_summary};
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
    LifecycleRunService, RecordGateCollisionCommand, StartLifecycleRunCommand, select_active_run,
};
pub use session_association::LIFECYCLE_NODE_LABEL_PREFIX;
pub use step_activation::{
    KickoffPromptFragment, StepActivation, StepActivationInput, activate_step_with_platform,
    agent_mcp_entries_from_servers, apply_to_prompt_request, apply_to_running_session,
    capability_delta_directives, capability_directives_from_keys, capability_keys_sorted,
    empty_presets,
};
