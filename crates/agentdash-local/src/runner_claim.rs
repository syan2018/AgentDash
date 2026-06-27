use chrono::Utc;

use agentdash_contracts::backend::RunnerRegistrationClaimResponse;
use reqwest::StatusCode;
use serde::Serialize;

use crate::machine_identity::LocalMachineIdentity;
use crate::runner_config::{ResolvedRunnerConfig, RunnerCredentials};
use crate::runner_redaction::redact_secret;

const CLAIM_PATH: &str = "/api/local-runtime/runner/claim";

#[derive(Debug, Serialize)]
struct RunnerRegistrationClaimRequestBody {
    registration_token: Option<String>,
    machine_id: String,
    machine_label: Option<String>,
    runner_name: Option<String>,
    client_version: Option<String>,
    device: serde_json::Value,
    executor_enabled: bool,
    capability_slot: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum RunnerClaimError {
    #[error("fatal claim error {code}: {message}")]
    Fatal { code: String, message: String },
    #[error("retryable claim error {code}: {message}")]
    Retryable { code: String, message: String },
}

impl RunnerClaimError {
    pub fn code(&self) -> &str {
        match self {
            Self::Fatal { code, .. } | Self::Retryable { code, .. } => code,
        }
    }

    pub fn message(&self) -> &str {
        match self {
            Self::Fatal { message, .. } | Self::Retryable { message, .. } => message,
        }
    }

    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Retryable { .. })
    }
}

pub async fn claim_runner(
    config: &ResolvedRunnerConfig,
    identity: &LocalMachineIdentity,
) -> Result<RunnerCredentials, RunnerClaimError> {
    let server_url = config
        .server_url
        .as_deref()
        .ok_or_else(|| RunnerClaimError::Fatal {
            code: "missing_server_url".to_string(),
            message: "缺少 server_url，无法领取 runner credentials".to_string(),
        })?;
    let registration_token =
        config
            .registration_token
            .as_deref()
            .ok_or_else(|| RunnerClaimError::Fatal {
                code: "missing_registration_token".to_string(),
                message: "缺少 registration token，无法领取 runner credentials".to_string(),
            })?;

    let request = RunnerRegistrationClaimRequestBody {
        registration_token: Some(registration_token.to_string()),
        machine_id: identity.machine_id.clone(),
        machine_label: Some(identity.machine_label.clone()),
        runner_name: Some(config.runner_name.clone()),
        client_version: Some(env!("CARGO_PKG_VERSION").to_string()),
        device: serde_json::json!({
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "family": std::env::consts::FAMILY,
        }),
        executor_enabled: config.executor_enabled,
        capability_slot: None,
    };

    let url = claim_url(server_url);
    let client = reqwest::Client::new();
    let response = client
        .post(&url)
        .json(&request)
        .send()
        .await
        .map_err(|error| RunnerClaimError::Retryable {
            code: "claim_unavailable".to_string(),
            message: redact_secret(&error.to_string()),
        })?;

    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| RunnerClaimError::Retryable {
            code: "claim_response_read_failed".to_string(),
            message: redact_secret(&error.to_string()),
        })?;

    if !status.is_success() {
        return Err(claim_error_from_http(status, &body));
    }

    let claimed =
        serde_json::from_str::<RunnerRegistrationClaimResponse>(&body).map_err(|error| {
            RunnerClaimError::Fatal {
                code: "claim_response_invalid".to_string(),
                message: redact_secret(&error.to_string()),
            }
        })?;

    Ok(credentials_from_claim(claimed))
}

pub fn credentials_from_claim(response: RunnerRegistrationClaimResponse) -> RunnerCredentials {
    RunnerCredentials {
        backend_id: Some(response.backend_id),
        relay_ws_url: Some(response.relay_ws_url),
        auth_token: Some(response.auth_token),
        claimed_at: Some(response.claimed_at),
        token_source: Some(response.registration_source),
    }
}

fn claim_url(server_url: &str) -> String {
    format!("{}{}", server_url.trim_end_matches('/'), CLAIM_PATH)
}

fn claim_error_from_http(status: StatusCode, body: &str) -> RunnerClaimError {
    let message = redact_secret(body);
    match status {
        StatusCode::BAD_REQUEST => RunnerClaimError::Fatal {
            code: "claim_bad_request".to_string(),
            message,
        },
        StatusCode::UNAUTHORIZED => RunnerClaimError::Fatal {
            code: "claim_unauthorized".to_string(),
            message,
        },
        StatusCode::FORBIDDEN => RunnerClaimError::Fatal {
            code: "claim_forbidden".to_string(),
            message,
        },
        StatusCode::CONFLICT => RunnerClaimError::Fatal {
            code: "claim_conflict".to_string(),
            message,
        },
        StatusCode::TOO_MANY_REQUESTS => RunnerClaimError::Retryable {
            code: "claim_rate_limited".to_string(),
            message,
        },
        status if status.is_server_error() => RunnerClaimError::Retryable {
            code: "claim_server_error".to_string(),
            message,
        },
        _ => RunnerClaimError::Fatal {
            code: format!("claim_http_{}", status.as_u16()),
            message,
        },
    }
}

pub fn direct_credentials(
    backend_id: String,
    relay_ws_url: String,
    auth_token: String,
) -> RunnerCredentials {
    RunnerCredentials {
        backend_id: Some(backend_id),
        relay_ws_url: Some(relay_ws_url),
        auth_token: Some(auth_token),
        claimed_at: Some(Utc::now()),
        token_source: Some("direct_credentials".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_contracts::backend::BackendShareScopeKind;

    #[test]
    fn claim_response_becomes_runtime_credentials() {
        let credentials = credentials_from_claim(RunnerRegistrationClaimResponse {
            backend_id: "backend-1".to_string(),
            name: "runner".to_string(),
            relay_ws_url: "wss://example/ws/backend".to_string(),
            auth_token: "relay-secret".to_string(),
            machine_id: "machine-1".to_string(),
            machine_label: "host".to_string(),
            share_scope_kind: BackendShareScopeKind::Project,
            share_scope_id: Some("project-1".to_string()),
            capability_slot: "default".to_string(),
            registration_source: "runner_registration_token".to_string(),
            claimed_at: Utc::now(),
        });

        assert!(credentials.is_complete());
        assert_eq!(
            credentials.token_source.as_deref(),
            Some("runner_registration_token")
        );
    }

    #[test]
    fn unauthorized_claim_error_is_fatal() {
        let error = claim_error_from_http(StatusCode::UNAUTHORIZED, "invalid token=secret");

        assert!(!error.is_retryable());
        assert_eq!(error.code(), "claim_unauthorized");
        assert_eq!(error.message(), "invalid token=***");
    }

    #[test]
    fn server_claim_error_is_retryable() {
        let error = claim_error_from_http(StatusCode::SERVICE_UNAVAILABLE, "down");

        assert!(error.is_retryable());
    }
}
