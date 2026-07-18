//! SPI port for executing workflow function activities (API request / bash).
//!
//! The concrete IO — `tera` template rendering, `reqwest` HTTP calls, and
//! `tokio::process` command execution — lives in infrastructure. Application
//! owns the activity-event shaping and the success/failure policy; it depends
//! only on this port for the raw side effects.

use agentdash_domain::workflow::{ApiRequestExecutorSpec, BashExecExecutorSpec};
use async_trait::async_trait;
use serde_json::Value;

/// Raw outcome of an API request function activity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiRequestOutcome {
    pub status: u16,
    pub body_text: String,
    pub body_json: Option<Value>,
}

/// Raw outcome of a bash exec function activity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BashExecOutcome {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub success: bool,
}

/// Executes function-activity side effects against a rendered template context.
///
/// Errors returned here cover template rendering, transport, and process
/// startup failures — the caller maps them to a failed activity event. The
/// success / failure interpretation of an HTTP status or process exit code is
/// left to the caller, which receives the raw outcome.
#[async_trait]
pub trait FunctionRunner: Send + Sync {
    async fn run_api_request(
        &self,
        spec: &ApiRequestExecutorSpec,
        context: &Value,
    ) -> Result<ApiRequestOutcome, String>;

    async fn run_bash(
        &self,
        spec: &BashExecExecutorSpec,
        context: &Value,
    ) -> Result<BashExecOutcome, String>;
}
