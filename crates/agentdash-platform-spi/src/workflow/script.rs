//! SPI port for evaluating workflow scripts.
//!
//! Workflow scripts compile through a builder-document frontend. The concrete
//! scripting engine lives in infrastructure; application code depends only on
//! this port and parses the returned JSON document into typed workflow script
//! builder structures before any future compiler step.

/// Evaluates workflow builder scripts against a prebuilt context value.
///
/// The returned value must be a serializable builder document. Evaluators must
/// not execute AgentRun, FunctionRun, local effect, filesystem, network, or
/// other workflow side effects while evaluating scripts.
pub trait WorkflowScriptEvaluator: Send + Sync {
    /// Compiles the script without executing it, for syntax validation.
    fn validate_workflow_script(&self, script: &str) -> Result<(), Vec<String>>;

    /// Executes the script against the prebuilt context value and returns the
    /// raw builder document JSON.
    fn eval_workflow_script(
        &self,
        script: &str,
        ctx: &serde_json::Value,
    ) -> Result<serde_json::Value, String>;
}
