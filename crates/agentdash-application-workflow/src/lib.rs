mod catalog;
mod definition;
mod error;
pub mod gate;
mod graph_planner;
pub mod graph_resolver;
pub mod orchestration;
mod repository_set;
pub mod script;

pub use agentdash_domain::workflow::{
    AgentReusePolicy, RuntimeSessionPolicy, WorkflowSessionTerminalState,
};
pub use catalog::{ActivityLifecycleCatalogService, WorkflowCatalogService};
pub use definition::{
    BuiltinLifecycleTemplate, BuiltinWorkflowBundle, BuiltinWorkflowTemplate,
    BuiltinWorkflowTemplateBundle, TRELLIS_DAG_TASK_TEMPLATE_KEY, build_builtin_workflow_bundle,
    get_builtin_workflow_template, list_builtin_workflow_templates,
};
pub use error::WorkflowApplicationError;
pub use graph_planner::ApplicationWorkflowGraphPlanner;
pub use graph_resolver::{ResolvedWorkflowGraph, WorkflowGraphResolver};
pub use orchestration::{
    OrchestrationExecutorDrainResult, OrchestrationExecutorLauncher, SubmitHumanGateDecisionInput,
    SubmitHumanGateDecisionResult,
};
pub use orchestration::{
    ScriptCompileDiagnostic, ScriptCompileInput, ScriptCompileOutput, ScriptCompiler,
    WORKFLOW_SCRIPT_COMPILER_SCHEMA_VERSION, compile_workflow_script_builder_document,
};
pub use orchestration::{
    WORKFLOW_GRAPH_COMPILER_SCHEMA_VERSION, WorkflowGraphCompileDiagnostic,
    WorkflowGraphCompileInput, WorkflowGraphCompileMode, WorkflowGraphCompileOutput,
    WorkflowGraphCompileSourceMetadata, WorkflowGraphCompiler, compile_workflow_graph,
};
pub use repository_set::WorkflowRepositorySet;
pub use script::{
    WorkflowScriptAgent, WorkflowScriptBuilderDiagnostic, WorkflowScriptBuilderDocument,
    WorkflowScriptBuilderParseOutput, WorkflowScriptCompileDiagnostic, WorkflowScriptCompileInput,
    WorkflowScriptCompileOutput, WorkflowScriptCompiler, WorkflowScriptEffect,
    WorkflowScriptFunction, WorkflowScriptHumanGate, WorkflowScriptLocalEffect,
    WorkflowScriptParallel, WorkflowScriptPhase, WorkflowScriptPipeline, WorkflowScriptPlanPreview,
    WorkflowScriptPlanPreviewNode, WorkflowScriptPreflightDiagnostic, WorkflowScriptPreflightInput,
    WorkflowScriptPreflightOutput, WorkflowScriptPreflightService, WorkflowScriptRequest,
    WorkflowScriptStatement, extract_workflow_script_capability_summary,
    parse_workflow_script_builder_document, preflight_workflow_script,
};
