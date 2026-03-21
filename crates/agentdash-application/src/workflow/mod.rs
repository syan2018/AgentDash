mod catalog;
mod definition;
mod error;
mod run;

pub use catalog::{AssignWorkflowCommand, WorkflowCatalogService};
pub use definition::{
    BuiltinWorkflowTemplate, TRELLIS_DEV_PROJECT_TEMPLATE_KEY, TRELLIS_DEV_STORY_TEMPLATE_KEY,
    TRELLIS_DEV_TASK_TEMPLATE_KEY, build_builtin_workflow_definition,
    get_builtin_workflow_template, list_builtin_workflow_templates,
};
pub use error::WorkflowApplicationError;
pub use run::{
    ActivateWorkflowPhaseCommand, CompleteWorkflowPhaseCommand, StartWorkflowRunCommand,
    WorkflowRecordArtifactDraft, WorkflowRunService,
};
