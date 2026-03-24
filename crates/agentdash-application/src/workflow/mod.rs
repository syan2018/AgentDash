mod assignment_resolution;
pub mod binding;
mod catalog;
mod completion;
mod definition;
mod error;
pub mod projection;
mod run;

pub use assignment_resolution::{
    ResolveAssignmentInput, ResolvedAssignment, resolve_assignment_and_ensure_run,
};
pub use catalog::{AssignWorkflowCommand, WorkflowCatalogService};
pub use completion::{
    WorkflowCompletionDecision, WorkflowCompletionEvidence, WorkflowCompletionSignalSet,
    WorkflowSessionTerminalState, completion_mode_tag, evaluate_phase_completion,
    session_terminal_state_tag,
};
pub use definition::{
    BuiltinWorkflowTemplate, TRELLIS_DEV_PROJECT_TEMPLATE_KEY, TRELLIS_DEV_STORY_TEMPLATE_KEY,
    TRELLIS_DEV_TASK_TEMPLATE_KEY, build_builtin_workflow_definition,
    get_builtin_workflow_template, list_builtin_workflow_templates,
};
pub use error::WorkflowApplicationError;
pub use projection::{
    ActiveWorkflowProjection, WorkflowProjectionSnapshot, WorkflowTargetSummary,
    resolve_active_workflow_projection,
};
pub use run::{
    ActivateWorkflowPhaseCommand, AppendWorkflowPhaseArtifactsCommand,
    CompleteWorkflowPhaseCommand, StartWorkflowRunCommand, WorkflowRecordArtifactDraft,
    WorkflowRunService, build_phase_completion_artifact_drafts, select_active_run,
};
