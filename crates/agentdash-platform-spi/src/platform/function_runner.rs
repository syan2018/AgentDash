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
    /// No durable dispatch intent exists. This is the only observation that
    /// authorizes Workflow to call `execute_effect`.
    NotApplied,
    /// The durable dispatch intent was accepted. The external side effect may
    /// already have happened, so recovery may only inspect/reconcile it.
    Accepted,
    /// A worker owns the durable dispatch intent. Claim or lease expiry does
    /// not prove that repeating the external side effect is safe.
    InFlight,
    Succeeded(FunctionEffectRawOutcome),
    Failed {
        message: String,
        retryable: bool,
    },
    /// The durable intent cannot be reconciled with its external outcome.
    /// Workflow must block the node for explicit operator resolution.
    Lost {
        reason: String,
        evidence: Value,
    },
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

    /// Accepts a stable Function effect only after `inspect_effect` returned
    /// `NotApplied`.
    ///
    /// The implementation must atomically commit a durable dispatch intent,
    /// keyed by `effect_id` + payload digest, before crossing the HTTP/process
    /// boundary. Once that intent exists, it must return/inspect as
    /// `Accepted`, `InFlight`, terminal, or `Lost`; it must never expose the
    /// effect as `NotApplied` again. A claim or lease can coordinate one
    /// worker, but expiry never authorizes automatic external re-execution.
    async fn execute_effect(
        &self,
        _request: FunctionEffectRequest,
    ) -> Result<FunctionEffectObservation, String> {
        Err("stable Function effect protocol is not implemented".to_owned())
    }

    /// Reads the durable runner lifecycle. `NotApplied` proves that no dispatch
    /// intent was committed. `Accepted`/`InFlight` are inspect-only states.
    /// Sources that cannot prove a terminal outcome after dispatch must
    /// converge to `Lost` with durable reason/evidence.
    async fn inspect_effect(&self, _effect_id: &str) -> Result<FunctionEffectObservation, String> {
        Err("stable Function effect inspection is not implemented".to_owned())
    }
}
