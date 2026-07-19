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

#[derive(Debug, Clone, PartialEq)]
pub enum FunctionEffectSpec {
    ApiRequest(ApiRequestExecutorSpec),
    BashExec(BashExecExecutorSpec),
}

#[derive(Debug, Clone, PartialEq)]
pub struct FunctionEffectRequest {
    pub effect_id: String,
    pub payload_digest: String,
    pub spec: FunctionEffectSpec,
    pub context: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FunctionEffectRawOutcome {
    ApiRequest(ApiRequestOutcome),
    BashExec(BashExecOutcome),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FunctionEffectObservation {
    Unknown,
    Accepted,
    Succeeded(FunctionEffectRawOutcome),
    Failed { message: String, retryable: bool },
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

    /// Stable effect protocol used by Workflow production execution. The
    /// implementation must durably deduplicate `effect_id` + payload digest
    /// and make terminal observations inspectable after restart.
    async fn execute_effect(
        &self,
        _request: FunctionEffectRequest,
    ) -> Result<FunctionEffectObservation, String> {
        Err("stable Function effect protocol is not implemented".to_owned())
    }

    async fn inspect_effect(&self, _effect_id: &str) -> Result<FunctionEffectObservation, String> {
        Err("stable Function effect inspection is not implemented".to_owned())
    }
}
