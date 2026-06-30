use std::future::Future;
use std::time::Duration;

use agentdash_contracts::backend::{BackendShareScopeKind, BackendVisibility};
use chrono::{DateTime, Utc};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};

use crate::desktop_profile::DesktopRuntimeStartRequest;
use crate::runner_redaction::redact_secret;
use crate::runtime::LocalRuntimeConfig;

const DESKTOP_ENSURE_PATH: &str = "/api/local-runtime/ensure";
const DEFAULT_CAPABILITY_SLOT: &str = "default";
const DESKTOP_REGISTRATION_SOURCE: &str = "desktop_access_token";

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct DesktopEnsureLocalRuntimePayload {
    pub machine_id: String,
    pub machine_label: Option<String>,
    pub profile_id: String,
    pub scope: DesktopLocalRuntimeScopePayload,
    pub capability_slot: String,
    pub name: Option<String>,
    pub executor_enabled: bool,
    pub client_version: Option<String>,
    pub device: serde_json::Value,
    pub rotate_token: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct DesktopLocalRuntimeScopePayload {
    pub kind: BackendShareScopeKind,
    pub id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DesktopEnsureLocalRuntimeResponse {
    pub backend_id: String,
    pub name: String,
    pub relay_ws_url: String,
    pub auth_token: String,
    #[serde(default)]
    pub backend_enabled: bool,
    pub profile_id: String,
    pub machine_id: String,
    pub machine_label: String,
    pub visibility: BackendVisibility,
    pub share_scope_kind: BackendShareScopeKind,
    pub share_scope_id: Option<String>,
    pub capability_slot: String,
    pub registration_source: String,
    pub claimed_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct DesktopEnsureRetryPolicy {
    pub retry_until_server_ready: bool,
    pub max_attempts: u32,
    pub delay: Duration,
}

impl DesktopEnsureRetryPolicy {
    pub fn single_attempt() -> Self {
        Self {
            retry_until_server_ready: false,
            max_attempts: 1,
            delay: Duration::from_secs(0),
        }
    }

    pub fn wait_for_server_ready() -> Self {
        Self {
            retry_until_server_ready: true,
            max_attempts: 30,
            delay: Duration::from_secs(1),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopEnsureRetryEvent {
    pub attempt: u32,
    pub next_retry_at: String,
    pub error: String,
}

#[derive(Debug, thiserror::Error)]
pub enum DesktopClaimError {
    #[error("fatal desktop claim error {code}: {message}")]
    Fatal { code: String, message: String },
    #[error("retryable desktop claim error {code}: {message}")]
    Retryable { code: String, message: String },
}

impl DesktopClaimError {
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

pub async fn ensure_desktop_runtime_config<F, Fut>(
    request: DesktopRuntimeStartRequest,
    retry_policy: DesktopEnsureRetryPolicy,
    on_retry: F,
) -> Result<LocalRuntimeConfig, DesktopClaimError>
where
    F: FnMut(DesktopEnsureRetryEvent) -> Fut,
    Fut: Future<Output = ()>,
{
    let response = ensure_desktop_local_runtime(&request, retry_policy, on_retry).await?;
    desktop_runtime_config_from_ensure(&request, response).map_err(|error| {
        DesktopClaimError::Fatal {
            code: "desktop_claim_response_invalid".to_string(),
            message: error.to_string(),
        }
    })
}

pub async fn ensure_desktop_local_runtime<F, Fut>(
    request: &DesktopRuntimeStartRequest,
    retry_policy: DesktopEnsureRetryPolicy,
    mut on_retry: F,
) -> Result<DesktopEnsureLocalRuntimeResponse, DesktopClaimError>
where
    F: FnMut(DesktopEnsureRetryEvent) -> Fut,
    Fut: Future<Output = ()>,
{
    let server_url = normalize_server_url(&request.server_url);
    let payload = desktop_ensure_payload_from_request(request);
    let max_attempts = retry_policy.max_attempts.max(1);
    let mut attempt = 0;

    loop {
        attempt += 1;
        match post_desktop_local_runtime_claim(&server_url, &request.access_token, &payload).await {
            Ok(response) => {
                validate_desktop_ensure_response(&response, request).map_err(|error| {
                    DesktopClaimError::Fatal {
                        code: "desktop_claim_response_invalid".to_string(),
                        message: error.to_string(),
                    }
                })?;
                return Ok(response);
            }
            Err(error)
                if retry_policy.retry_until_server_ready
                    && error.is_retryable()
                    && attempt < max_attempts =>
            {
                let next_retry_at = next_retry_at(retry_policy.delay);
                on_retry(DesktopEnsureRetryEvent {
                    attempt,
                    next_retry_at,
                    error: error.to_string(),
                })
                .await;
                tokio::time::sleep(retry_policy.delay).await;
            }
            Err(error) => return Err(error),
        }
    }
}

pub fn desktop_ensure_payload_from_request(
    request: &DesktopRuntimeStartRequest,
) -> DesktopEnsureLocalRuntimePayload {
    DesktopEnsureLocalRuntimePayload {
        machine_id: request.machine_id.clone(),
        machine_label: request.machine_label.clone(),
        profile_id: request.profile_id.clone(),
        scope: DesktopLocalRuntimeScopePayload {
            kind: BackendShareScopeKind::User,
            id: None,
        },
        capability_slot: DEFAULT_CAPABILITY_SLOT.to_string(),
        name: request.name.clone(),
        executor_enabled: request.executor_enabled,
        client_version: Some(env!("CARGO_PKG_VERSION").to_string()),
        device: local_device_payload(),
        rotate_token: false,
    }
}

pub fn validate_desktop_ensure_response(
    response: &DesktopEnsureLocalRuntimeResponse,
    request: &DesktopRuntimeStartRequest,
) -> anyhow::Result<()> {
    if response.machine_id != request.machine_id {
        anyhow::bail!(
            "server 返回的 machine_id 与本机 profile 不一致: expected={}, actual={}",
            request.machine_id,
            response.machine_id
        );
    }
    if response.machine_label.trim().is_empty() {
        anyhow::bail!("server 返回的 machine_label 为空");
    }
    if response.share_scope_kind != BackendShareScopeKind::User {
        anyhow::bail!(
            "当前桌面端只支持 personal runtime scope，server 返回: {:?}",
            response.share_scope_kind
        );
    }
    if response.capability_slot != DEFAULT_CAPABILITY_SLOT {
        anyhow::bail!(
            "当前桌面端只支持 default capability slot，server 返回: {}",
            response.capability_slot
        );
    }
    if response.registration_source != DESKTOP_REGISTRATION_SOURCE {
        anyhow::bail!(
            "桌面端 ensure 返回了非桌面注册来源: {}",
            response.registration_source
        );
    }
    Ok(())
}

pub fn desktop_runtime_config_from_ensure(
    request: &DesktopRuntimeStartRequest,
    response: DesktopEnsureLocalRuntimeResponse,
) -> anyhow::Result<LocalRuntimeConfig> {
    validate_desktop_ensure_response(&response, request)?;
    Ok(LocalRuntimeConfig::new(
        response.relay_ws_url,
        response.auth_token,
        response.backend_id,
        response.name,
        request.workspace_roots.clone(),
        request.executor_enabled,
    ))
}

async fn post_desktop_local_runtime_claim(
    server_url: &str,
    access_token: &str,
    payload: &DesktopEnsureLocalRuntimePayload,
) -> Result<DesktopEnsureLocalRuntimeResponse, DesktopClaimError> {
    let endpoint = format!("{server_url}{DESKTOP_ENSURE_PATH}");
    let client = reqwest::Client::new();
    let mut request = client.post(&endpoint);
    let access_token = access_token.trim();
    if !access_token.is_empty() {
        request = request.bearer_auth(access_token);
    }

    let response =
        request
            .json(payload)
            .send()
            .await
            .map_err(|error| DesktopClaimError::Retryable {
                code: "desktop_claim_unavailable".to_string(),
                message: redact_secret(&error.to_string()),
            })?;

    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| DesktopClaimError::Retryable {
            code: "desktop_claim_response_read_failed".to_string(),
            message: redact_secret(&error.to_string()),
        })?;

    if !status.is_success() {
        return Err(desktop_claim_error_from_http(status, &body));
    }

    serde_json::from_str::<DesktopEnsureLocalRuntimeResponse>(&body).map_err(|error| {
        DesktopClaimError::Fatal {
            code: "desktop_claim_response_invalid".to_string(),
            message: redact_secret(&error.to_string()),
        }
    })
}

pub fn desktop_claim_error_from_http(status: StatusCode, body: &str) -> DesktopClaimError {
    let message = redact_secret(body);
    match status {
        StatusCode::BAD_REQUEST => DesktopClaimError::Fatal {
            code: "desktop_claim_bad_request".to_string(),
            message,
        },
        StatusCode::UNAUTHORIZED => DesktopClaimError::Fatal {
            code: "desktop_claim_unauthorized".to_string(),
            message,
        },
        StatusCode::FORBIDDEN => DesktopClaimError::Fatal {
            code: "desktop_claim_forbidden".to_string(),
            message,
        },
        StatusCode::CONFLICT => DesktopClaimError::Fatal {
            code: "desktop_claim_conflict".to_string(),
            message,
        },
        StatusCode::TOO_MANY_REQUESTS => DesktopClaimError::Retryable {
            code: "desktop_claim_rate_limited".to_string(),
            message,
        },
        status if status.is_server_error() => DesktopClaimError::Retryable {
            code: "desktop_claim_server_error".to_string(),
            message,
        },
        _ => DesktopClaimError::Fatal {
            code: format!("desktop_claim_http_{}", status.as_u16()),
            message,
        },
    }
}

fn normalize_server_url(value: &str) -> String {
    value.trim().trim_end_matches('/').to_string()
}

fn next_retry_at(delay: Duration) -> String {
    let chrono_delay =
        chrono::Duration::from_std(delay).unwrap_or_else(|_| chrono::Duration::seconds(1));
    (Utc::now() + chrono_delay).to_rfc3339()
}

fn local_device_payload() -> serde_json::Value {
    serde_json::json!({
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "family": std::env::consts::FAMILY,
        "hostname": local_hostname(),
    })
}

fn local_hostname() -> Option<String> {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn start_request() -> DesktopRuntimeStartRequest {
        DesktopRuntimeStartRequest {
            server_url: "https://cloud.example.test".to_string(),
            access_token: "user-access-token".to_string(),
            profile_id: "default".to_string(),
            machine_id: "machine-1".to_string(),
            machine_label: Some("host-1".to_string()),
            name: Some("Desktop Runtime".to_string()),
            workspace_roots: vec![PathBuf::from("workspace")],
            executor_enabled: true,
        }
    }

    fn ensure_response() -> DesktopEnsureLocalRuntimeResponse {
        DesktopEnsureLocalRuntimeResponse {
            backend_id: "backend-1".to_string(),
            name: "Desktop Runtime".to_string(),
            relay_ws_url: "wss://cloud.example.test/ws/backend".to_string(),
            auth_token: "relay-token".to_string(),
            backend_enabled: true,
            profile_id: "default".to_string(),
            machine_id: "machine-1".to_string(),
            machine_label: "host-1".to_string(),
            visibility: BackendVisibility::Private,
            share_scope_kind: BackendShareScopeKind::User,
            share_scope_id: None,
            capability_slot: DEFAULT_CAPABILITY_SLOT.to_string(),
            registration_source: DESKTOP_REGISTRATION_SOURCE.to_string(),
            claimed_at: Utc::now(),
        }
    }

    #[test]
    fn ensure_payload_uses_user_scope_default_slot_and_desktop_source_shape() {
        let payload = desktop_ensure_payload_from_request(&start_request());

        assert_eq!(payload.scope.kind, BackendShareScopeKind::User);
        assert_eq!(payload.scope.id, None);
        assert_eq!(payload.capability_slot, DEFAULT_CAPABILITY_SLOT);
        assert!(!payload.rotate_token);
        assert_eq!(
            payload.client_version.as_deref(),
            Some(env!("CARGO_PKG_VERSION"))
        );
        assert_eq!(payload.device["os"], std::env::consts::OS);
    }

    #[test]
    fn validate_rejects_machine_mismatch() {
        let request = start_request();
        let mut response = ensure_response();
        response.machine_id = "other-machine".to_string();

        let error = validate_desktop_ensure_response(&response, &request)
            .expect_err("machine mismatch 应被拒绝");

        assert!(error.to_string().contains("machine_id"));
    }

    #[test]
    fn validate_rejects_non_user_scope() {
        let request = start_request();
        let mut response = ensure_response();
        response.share_scope_kind = BackendShareScopeKind::Project;

        let error = validate_desktop_ensure_response(&response, &request)
            .expect_err("非 user scope 应被拒绝");

        assert!(error.to_string().contains("personal runtime scope"));
    }

    #[test]
    fn validate_rejects_non_default_slot() {
        let request = start_request();
        let mut response = ensure_response();
        response.capability_slot = "build".to_string();

        let error = validate_desktop_ensure_response(&response, &request)
            .expect_err("非 default slot 应被拒绝");

        assert!(error.to_string().contains("default capability slot"));
    }

    #[test]
    fn validate_rejects_wrong_registration_source() {
        let request = start_request();
        let mut response = ensure_response();
        response.registration_source = "runner_registration_token".to_string();

        let error = validate_desktop_ensure_response(&response, &request)
            .expect_err("非 desktop access-token 来源应被拒绝");

        assert!(error.to_string().contains("非桌面注册来源"));
    }

    #[test]
    fn runtime_config_projection_uses_relay_credentials() {
        let request = start_request();
        let config = desktop_runtime_config_from_ensure(&request, ensure_response())
            .expect("合法 ensure response 应能投影 runtime config");

        assert_eq!(config.cloud_url, "wss://cloud.example.test/ws/backend");
        assert_eq!(config.token, "relay-token");
        assert_eq!(config.backend_id, "backend-1");
        assert_eq!(config.name, "Desktop Runtime");
        assert_eq!(config.workspace_roots, vec![PathBuf::from("workspace")]);
        assert!(config.executor_enabled);
    }

    #[test]
    fn unauthorized_http_error_is_fatal_and_redacts_body() {
        let error = desktop_claim_error_from_http(StatusCode::UNAUTHORIZED, "bad token=secret");

        assert!(!error.is_retryable());
        assert_eq!(error.code(), "desktop_claim_unauthorized");
        assert_eq!(error.message(), "bad token=***");
    }

    #[test]
    fn server_http_error_is_retryable() {
        let error = desktop_claim_error_from_http(StatusCode::SERVICE_UNAVAILABLE, "down");

        assert!(error.is_retryable());
        assert_eq!(error.code(), "desktop_claim_server_error");
    }
}
