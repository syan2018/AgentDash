mod catalog;
mod completion;
mod definition;
mod error;
pub mod execution_log;
pub mod projection;
mod run;
pub mod tools;

pub use agentdash_domain::workflow::WorkflowSessionTerminalState;
pub use catalog::{AssignLifecycleCommand, WorkflowCatalogService};
pub use completion::{
    WorkflowCompletionDecision, WorkflowCompletionEvidence, WorkflowCompletionSignalSet,
    evaluate_step_completion, session_terminal_state_tag, session_terminal_summary,
    workflow_artifact_type_tag,
};
pub use definition::{
    BuiltinLifecycleTemplate, BuiltinWorkflowBundle, BuiltinWorkflowTemplate,
    BuiltinWorkflowTemplateBundle, TRELLIS_DEV_PROJECT_TEMPLATE_KEY,
    TRELLIS_DEV_STORY_TEMPLATE_KEY, TRELLIS_DEV_TASK_TEMPLATE_KEY, build_builtin_workflow_bundle,
    get_builtin_workflow_template, list_builtin_workflow_templates,
};
pub use error::WorkflowApplicationError;
pub use projection::{
    ActiveWorkflowProjection, WorkflowProjectionSnapshot, WorkflowTargetSummary,
    resolve_active_workflow_projection,
};
pub use run::{
    ActivateLifecycleStepCommand, AppendLifecycleStepArtifactsCommand,
    CompleteLifecycleStepCommand, LifecycleRunService, StartLifecycleRunCommand,
    WorkflowRecordArtifactDraft, build_step_completion_artifact_drafts, select_active_run,
};
