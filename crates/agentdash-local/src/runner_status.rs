use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::runner_config::ResolvedRunnerConfig;
use crate::runner_redaction::{redact_optional, redact_secret};
use crate::runner_service::{SERVICE_NAME, ServiceCommandResult};

pub const RUNNER_STATUS_FILENAME: &str = "runner-status.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RunnerStatusSnapshot {
    pub version: String,
    pub pid: u32,
    pub service_name: String,
    pub service_state: String,
    pub status_stale: bool,
    pub config_path: String,
    pub config_sources: BTreeMap<String, String>,
    pub config_ready: bool,
    pub credential_state: String,
    pub credential_ready: bool,
    pub claim_ready: bool,
    pub relay_ready: bool,
    pub relay_state: String,
    pub registration_source: Option<String>,
    pub backend_id: Option<String>,
    pub runner_name: String,
    pub server_url: Option<String>,
    pub relay_ws_url: Option<String>,
    pub workspace_roots: Vec<String>,
    pub executor_enabled: bool,
    pub last_claim_attempt_at: Option<DateTime<Utc>>,
    pub last_claim_success_at: Option<DateTime<Utc>>,
    pub last_connected_at: Option<DateTime<Utc>>,
    pub last_disconnected_at: Option<DateTime<Utc>>,
    pub last_error_code: Option<String>,
    pub last_error_message: Option<String>,
    pub log_path: String,
    pub status_path: String,
    pub updated_at: DateTime<Utc>,
}

impl RunnerStatusSnapshot {
    pub fn from_config(config: &ResolvedRunnerConfig, service_state: impl Into<String>) -> Self {
        let status_path = status_path(&config.state_dir);
        let credential_ready = config.credentials.is_complete();
        let claim_ready = credential_ready || config.registration_token.is_some();
        Self {
            version: env!("CARGO_PKG_VERSION").to_string(),
            pid: std::process::id(),
            service_name: SERVICE_NAME.to_string(),
            service_state: service_state.into(),
            status_stale: false,
            config_path: config.config_path.to_string_lossy().to_string(),
            config_sources: config.sources.clone(),
            config_ready: config.server_url.is_some() || credential_ready,
            credential_state: config.credential_state().to_string(),
            credential_ready,
            claim_ready,
            relay_ready: credential_ready,
            relay_state: if credential_ready {
                "configured".to_string()
            } else {
                "unconfigured".to_string()
            },
            registration_source: config.registration_source(),
            backend_id: config.credentials.backend_id.clone(),
            runner_name: config.runner_name.clone(),
            server_url: redact_optional(config.server_url.as_deref()),
            relay_ws_url: redact_optional(config.credentials.relay_ws_url.as_deref()),
            workspace_roots: config
                .workspace_roots
                .iter()
                .map(|path| path.to_string_lossy().to_string())
                .collect(),
            executor_enabled: config.executor_enabled,
            last_claim_attempt_at: None,
            last_claim_success_at: config.credentials.claimed_at,
            last_connected_at: None,
            last_disconnected_at: None,
            last_error_code: None,
            last_error_message: None,
            log_path: config.log_path.to_string_lossy().to_string(),
            status_path: status_path.to_string_lossy().to_string(),
            updated_at: Utc::now(),
        }
    }

    pub fn with_error(mut self, code: impl Into<String>, message: impl AsRef<str>) -> Self {
        self.last_error_code = Some(code.into());
        self.last_error_message = Some(redact_secret(message.as_ref()));
        self.relay_state = "error".to_string();
        self.updated_at = Utc::now();
        self
    }

    pub fn mark_claim_attempt(mut self) -> Self {
        self.last_claim_attempt_at = Some(Utc::now());
        self.updated_at = Utc::now();
        self
    }

    pub fn mark_claim_success(mut self) -> Self {
        self.credential_state = "ready".to_string();
        self.credential_ready = true;
        self.claim_ready = true;
        self.relay_ready = true;
        self.last_claim_success_at = Some(Utc::now());
        self.updated_at = Utc::now();
        self
    }

    pub fn mark_connecting(mut self) -> Self {
        self.relay_ready = self.credential_ready;
        self.relay_state = "connecting".to_string();
        self.updated_at = Utc::now();
        self
    }

    pub fn mark_registered(mut self) -> Self {
        self.relay_ready = true;
        self.relay_state = "registered".to_string();
        self.last_connected_at = Some(Utc::now());
        self.last_error_code = None;
        self.last_error_message = None;
        self.updated_at = Utc::now();
        self
    }

    pub fn mark_retrying(mut self, code: impl Into<String>, message: impl AsRef<str>) -> Self {
        self.relay_ready = false;
        self.relay_state = "retrying".to_string();
        self.last_disconnected_at = Some(Utc::now());
        self.last_error_code = Some(code.into());
        self.last_error_message = Some(redact_secret(message.as_ref()));
        self.updated_at = Utc::now();
        self
    }

    pub fn mark_disconnected(mut self, message: impl AsRef<str>) -> Self {
        self.relay_ready = false;
        self.relay_state = "disconnected".to_string();
        self.last_disconnected_at = Some(Utc::now());
        self.last_error_code = Some("disconnected".to_string());
        self.last_error_message = Some(redact_secret(message.as_ref()));
        self.updated_at = Utc::now();
        self
    }

    pub fn mark_stopped(mut self) -> Self {
        self.relay_ready = false;
        self.relay_state = "stopped".to_string();
        self.last_disconnected_at = Some(Utc::now());
        self.updated_at = Utc::now();
        self
    }

    pub fn merge_service(mut self, service: &ServiceCommandResult) -> Self {
        self.service_state = service.state.clone();
        self
    }
}

#[derive(Clone)]
pub struct RunnerStatusReporter {
    snapshot: std::sync::Arc<Mutex<RunnerStatusSnapshot>>,
}

impl RunnerStatusReporter {
    pub fn new(snapshot: RunnerStatusSnapshot) -> Self {
        Self {
            snapshot: std::sync::Arc::new(Mutex::new(snapshot)),
        }
    }

    pub async fn mark_connecting(&self) -> anyhow::Result<()> {
        self.update(|snapshot| snapshot.mark_connecting()).await
    }

    pub async fn mark_registered(&self) -> anyhow::Result<()> {
        self.update(|snapshot| snapshot.mark_registered()).await
    }

    pub async fn mark_retrying(
        &self,
        code: impl Into<String>,
        message: impl AsRef<str>,
    ) -> anyhow::Result<()> {
        let code = code.into();
        let message = message.as_ref().to_string();
        self.update(|snapshot| snapshot.mark_retrying(code, message))
            .await
    }

    pub async fn mark_disconnected(&self, message: impl AsRef<str>) -> anyhow::Result<()> {
        let message = message.as_ref().to_string();
        self.update(|snapshot| snapshot.mark_disconnected(message))
            .await
    }

    pub async fn mark_stopped(&self) -> anyhow::Result<()> {
        self.update(|snapshot| snapshot.mark_stopped()).await
    }

    async fn update(
        &self,
        update: impl FnOnce(RunnerStatusSnapshot) -> RunnerStatusSnapshot,
    ) -> anyhow::Result<()> {
        let mut guard = self.snapshot.lock().await;
        let next = update(guard.clone());
        write_status(&next)?;
        *guard = next;
        Ok(())
    }
}

pub fn status_path(state_dir: &Path) -> PathBuf {
    state_dir.join(RUNNER_STATUS_FILENAME)
}

pub fn write_status(snapshot: &RunnerStatusSnapshot) -> anyhow::Result<()> {
    let path = PathBuf::from(&snapshot.status_path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temp_path = path.with_extension(format!("json.tmp-{}", uuid::Uuid::new_v4()));
    let content = serde_json::to_string_pretty(snapshot)?;
    {
        let mut file = std::fs::File::create(&temp_path)?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
    }
    match std::fs::rename(&temp_path, &path) {
        Ok(()) => Ok(()),
        Err(error) if cfg!(windows) && path.exists() => {
            std::fs::remove_file(&path)?;
            std::fs::rename(&temp_path, &path).map_err(|_| error.into())
        }
        Err(error) => Err(error.into()),
    }
}

pub fn read_status(path: &Path) -> anyhow::Result<Option<RunnerStatusSnapshot>> {
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(path)?;
    Ok(Some(serde_json::from_str(&content)?))
}

pub fn render_human(snapshot: &RunnerStatusSnapshot) -> String {
    format!(
        "service: {service}\nconfig: {config}\ncredentials: {credentials}\nclaim_ready: {claim_ready}\nrelay_ready: {relay_ready}\nrelay_state: {relay_state}\nbackend_id: {backend_id}\nserver_url: {server_url}\nrelay_ws_url: {relay_ws_url}\nlog_path: {log_path}\nstatus_path: {status_path}",
        service = snapshot.service_state,
        config = snapshot.config_path,
        credentials = snapshot.credential_state,
        claim_ready = snapshot.claim_ready,
        relay_ready = snapshot.relay_ready,
        relay_state = snapshot.relay_state,
        backend_id = snapshot.backend_id.as_deref().unwrap_or("<missing>"),
        server_url = snapshot.server_url.as_deref().unwrap_or("<missing>"),
        relay_ws_url = snapshot.relay_ws_url.as_deref().unwrap_or("<missing>"),
        log_path = snapshot.log_path,
        status_path = snapshot.status_path,
    )
}

pub fn is_stale(snapshot: &RunnerStatusSnapshot, now: DateTime<Utc>) -> bool {
    now.signed_duration_since(snapshot.updated_at).num_seconds() > 120
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner_config::{ResolvedRunnerConfig, RunnerCredentials};

    #[test]
    fn status_snapshot_redacts_targets_and_tokens() {
        let config = ResolvedRunnerConfig {
            config_path: PathBuf::from("runner.toml"),
            server_url: Some("https://example/api?token=server-secret".to_string()),
            registration_token: Some("adrt_secret".to_string()),
            credentials: RunnerCredentials {
                backend_id: Some("backend-1".to_string()),
                relay_ws_url: Some("wss://example/ws/backend?token=relay-secret".to_string()),
                auth_token: Some("auth-secret".to_string()),
                ..Default::default()
            },
            runner_name: "runner".to_string(),
            workspace_roots: Vec::new(),
            executor_enabled: true,
            log_path: PathBuf::from("runner.log"),
            state_dir: PathBuf::from("."),
            sources: BTreeMap::new(),
        };

        let snapshot = RunnerStatusSnapshot::from_config(&config, "unknown");
        let json = serde_json::to_string(&snapshot).expect("serialize");

        assert!(!json.contains("server-secret"));
        assert!(!json.contains("relay-secret"));
        assert!(!json.contains("auth-secret"));
        assert_eq!(
            snapshot.relay_ws_url.as_deref(),
            Some("wss://example/ws/backend?token=***")
        );
    }

    #[test]
    fn stale_detection_uses_updated_at() {
        let mut snapshot = RunnerStatusSnapshot {
            version: "test".to_string(),
            pid: 1,
            service_name: SERVICE_NAME.to_string(),
            service_state: "unknown".to_string(),
            status_stale: false,
            config_path: "runner.toml".to_string(),
            config_sources: BTreeMap::new(),
            config_ready: false,
            credential_state: "missing".to_string(),
            credential_ready: false,
            claim_ready: false,
            relay_ready: false,
            relay_state: "unknown".to_string(),
            registration_source: None,
            backend_id: None,
            runner_name: "runner".to_string(),
            server_url: None,
            relay_ws_url: None,
            workspace_roots: Vec::new(),
            executor_enabled: true,
            last_claim_attempt_at: None,
            last_claim_success_at: None,
            last_connected_at: None,
            last_disconnected_at: None,
            last_error_code: None,
            last_error_message: None,
            log_path: "runner.log".to_string(),
            status_path: "runner-status.json".to_string(),
            updated_at: Utc::now(),
        };
        let now = snapshot.updated_at + chrono::Duration::seconds(121);

        assert!(is_stale(&snapshot, now));
        snapshot.updated_at = now;
        assert!(!is_stale(&snapshot, now));
    }

    #[tokio::test]
    async fn reporter_writes_relay_transition_status() {
        let temp = tempfile::tempdir().expect("tempdir");
        let status_file = temp.path().join(RUNNER_STATUS_FILENAME);
        let snapshot = RunnerStatusSnapshot {
            version: "test".to_string(),
            pid: 1,
            service_name: SERVICE_NAME.to_string(),
            service_state: "foreground".to_string(),
            status_stale: false,
            config_path: "runner.toml".to_string(),
            config_sources: BTreeMap::new(),
            config_ready: true,
            credential_state: "ready".to_string(),
            credential_ready: true,
            claim_ready: true,
            relay_ready: true,
            relay_state: "configured".to_string(),
            registration_source: Some("runner_registration_token".to_string()),
            backend_id: Some("backend-1".to_string()),
            runner_name: "runner".to_string(),
            server_url: Some("https://example.test".to_string()),
            relay_ws_url: Some("wss://example.test/ws/backend".to_string()),
            workspace_roots: Vec::new(),
            executor_enabled: true,
            last_claim_attempt_at: None,
            last_claim_success_at: None,
            last_connected_at: None,
            last_disconnected_at: None,
            last_error_code: None,
            last_error_message: None,
            log_path: "runner.log".to_string(),
            status_path: status_file.to_string_lossy().to_string(),
            updated_at: Utc::now(),
        };
        let reporter = RunnerStatusReporter::new(snapshot);

        reporter.mark_connecting().await.expect("connecting");
        reporter.mark_registered().await.expect("registered");
        let loaded = read_status(&status_file)
            .expect("read status")
            .expect("status exists");

        assert_eq!(loaded.relay_state, "registered");
        assert!(loaded.last_connected_at.is_some());
        assert!(loaded.last_error_message.is_none());
    }
}
