use std::process::{Child, Stdio};
use std::sync::{Arc, Mutex as StdMutex};

use agentdash_diagnostics::{DiagnosticErrorContext, Subsystem, diag, diag_error};
use agentdash_process::{ProcessDomain, background_std_command};
use serde::Serialize;
use tokio::sync::Mutex as AsyncMutex;

use crate::settings::{env_trimmed, normalize_optional_env_text};
use crate::state::DesktopState;

pub(crate) const DESKTOP_API_PORT: u16 = 17301;
const DESKTOP_API_MODE_ENV: &str = "AGENTDASH_DESKTOP_API_MODE";
const DESKTOP_API_ORIGIN_ENV: &str = "AGENTDASH_DESKTOP_API_ORIGIN";
const DESKTOP_API_SIDECAR_ENV: &str = "AGENTDASH_DESKTOP_API_SIDECAR";

#[derive(Clone, Default)]
pub(crate) struct DesktopApiManager {
    snapshot: Arc<AsyncMutex<DesktopApiSnapshot>>,
    sidecar: Arc<StdMutex<Option<Child>>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct DesktopApiSnapshot {
    state: DesktopApiState,
    origin: String,
    message: Option<String>,
    database_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
enum DesktopApiState {
    Starting,
    Running,
    Error,
    Stopped,
}

impl DesktopApiSnapshot {
    pub(crate) fn state_label(&self) -> &'static str {
        match self.state {
            DesktopApiState::Starting => "starting",
            DesktopApiState::Running => "running",
            DesktopApiState::Error => "error",
            DesktopApiState::Stopped => "stopped",
        }
    }
}

impl Default for DesktopApiSnapshot {
    fn default() -> Self {
        Self {
            state: DesktopApiState::Starting,
            origin: desktop_api_origin(DESKTOP_API_PORT),
            message: Some("桌面端 API 正在启动".to_string()),
            database_url: None,
        }
    }
}

pub(crate) fn start_desktop_api_sidecar(state: DesktopState, config: DesktopApiConfig) {
    if config.sidecar.is_none() {
        diag!(Error, Subsystem::Api,
            operation = "desktop.api.sidecar",
            stage = "config_missing",
            process_domain = %ProcessDomain::DesktopSidecar.as_str(),
            program_kind = "desktop_api_sidecar",
            sidecar_configured = false,
            "未配置桌面端 API sidecar 命令"
        );
        let origin = config.origin.clone();
        tauri::async_runtime::spawn(async move {
            state
                .api
                .mark_error_origin(origin, "未配置桌面端 API sidecar 命令".to_string())
                .await;
        });
        return;
    }

    diag!(Info, Subsystem::Api,
        origin = %config.origin,
        process_domain = %ProcessDomain::DesktopSidecar.as_str(),
        program_kind = "desktop_api_sidecar",
        sidecar_configured = true,
        "Tauri 桌面端启动 API sidecar"
    );

    match spawn_desktop_api_sidecar(&config) {
        Ok(child) => {
            state.api.store_sidecar(child);
            tauri::async_runtime::spawn(async move {
                wait_for_sidecar_api_ready(state.api, config.origin).await;
            });
        }
        Err(error) => {
            let context = DiagnosticErrorContext::new("desktop.api.sidecar", "spawn_process");
            diag_error!(
                Error,
                Subsystem::Api,
                context = &context,
                error = &error,
                origin = %config.origin,
                process_domain = %ProcessDomain::DesktopSidecar.as_str(),
                program_kind = "desktop_api_sidecar",
                sidecar_configured = true,
                "启动桌面端 API sidecar 失败"
            );
            let origin = config.origin.clone();
            tauri::async_runtime::spawn(async move {
                state.api.mark_error_origin(origin, error.to_string()).await;
            });
        }
    }
}

fn spawn_desktop_api_sidecar(config: &DesktopApiConfig) -> anyhow::Result<Child> {
    let sidecar = config
        .sidecar
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("未配置桌面端 API sidecar 命令"))?;
    validate_sidecar_desktop_api_origin(&config.origin)?;
    let origin = reqwest::Url::parse(&config.origin)
        .map_err(|error| anyhow::anyhow!("桌面端 API origin 无效: {error}"))?;
    let host = origin.host_str().unwrap_or("127.0.0.1");
    let port = origin
        .port_or_known_default()
        .ok_or_else(|| anyhow::anyhow!("桌面端 API origin 缺少端口: {}", config.origin))?;

    let mut command = background_std_command(ProcessDomain::DesktopSidecar, sidecar);
    command
        .env("AGENTDASH_BIND_HOST", host)
        .env("AGENTDASH_PORT", port.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
        .spawn()
        .map_err(|error| anyhow::anyhow!("启动桌面端 API sidecar 失败: {error}"))
}

async fn wait_for_sidecar_api_ready(api: DesktopApiManager, origin: String) {
    api.mark_starting_origin(
        origin.clone(),
        format!("桌面端 API sidecar 正在启动: {origin}"),
        None,
    )
    .await;

    let endpoint = format!("{origin}/api/health");
    let client = reqwest::Client::new();
    for attempt in 1..=240 {
        match client.get(&endpoint).send().await {
            Ok(response) if response.status().is_success() => {
                api.mark_running_origin(origin, "桌面端 API sidecar 已就绪".to_string(), None)
                    .await;
                return;
            }
            Ok(response) => {
                if attempt % 20 == 0 {
                    diag!(Warn, Subsystem::Api,
                        operation = "desktop.api.sidecar_readiness",
                        stage = "health_status",
                        attempt,
                        retry_count = attempt - 1,
                        status = %response.status(),
                        origin = %origin,
                        process_domain = %ProcessDomain::DesktopSidecar.as_str(),
                        "等待桌面端 API sidecar 就绪"
                    );
                }
            }
            Err(error) => {
                if attempt % 20 == 0 {
                    let context = DiagnosticErrorContext::new(
                        "desktop.api.sidecar_readiness",
                        "health_request",
                    );
                    diag_error!(
                        Warn,
                        Subsystem::Api,
                        context = &context,
                        error = &error,
                        attempt = attempt,
                        retry_count = attempt - 1,
                        origin = %origin,
                        process_domain = %ProcessDomain::DesktopSidecar.as_str(),
                        "等待桌面端 API sidecar 就绪"
                    );
                }
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }

    diag!(Error, Subsystem::Api,
        operation = "desktop.api.sidecar_readiness",
        stage = "timeout",
        attempt = 240,
        retry_count = 239,
        origin = %origin,
        process_domain = %ProcessDomain::DesktopSidecar.as_str(),
        "桌面端 API sidecar 未在 120s 内就绪"
    );
    api.mark_error_origin(origin, "桌面端 API sidecar 未在 120s 内就绪".to_string())
        .await;
}

impl DesktopApiManager {
    pub(crate) fn from_snapshot(snapshot: DesktopApiSnapshot) -> Self {
        Self {
            snapshot: Arc::new(AsyncMutex::new(snapshot)),
            sidecar: Arc::new(StdMutex::new(None)),
        }
    }

    pub(crate) async fn snapshot(&self) -> DesktopApiSnapshot {
        self.snapshot.lock().await.clone()
    }

    async fn mark_starting_origin(
        &self,
        origin: String,
        message: String,
        database_url: Option<String>,
    ) {
        let mut guard = self.snapshot.lock().await;
        *guard = DesktopApiSnapshot {
            state: DesktopApiState::Starting,
            origin,
            message: Some(message),
            database_url,
        };
    }

    async fn mark_running_origin(
        &self,
        origin: String,
        message: String,
        database_url: Option<String>,
    ) {
        let mut guard = self.snapshot.lock().await;
        *guard = DesktopApiSnapshot {
            state: DesktopApiState::Running,
            origin,
            message: Some(message),
            database_url,
        };
    }

    pub(crate) async fn mark_error_origin(&self, origin: String, message: String) {
        let mut guard = self.snapshot.lock().await;
        *guard = DesktopApiSnapshot {
            state: DesktopApiState::Error,
            origin,
            message: Some(message),
            database_url: None,
        };
    }

    async fn mark_stopped_origin(&self, origin: String) {
        let mut guard = self.snapshot.lock().await;
        *guard = DesktopApiSnapshot {
            state: DesktopApiState::Stopped,
            origin,
            message: Some("桌面端 API sidecar 已停止".to_string()),
            database_url: None,
        };
    }

    fn store_sidecar(&self, child: Child) {
        match self.sidecar.lock() {
            Ok(mut guard) => {
                *guard = Some(child);
            }
            Err(error) => {
                let context = DiagnosticErrorContext::new("desktop.api.sidecar", "store_handle");
                diag_error!(
                    Error,
                    Subsystem::Api,
                    context = &context,
                    error = &error,
                    process_domain = %ProcessDomain::DesktopSidecar.as_str(),
                    program_kind = "desktop_api_sidecar",
                    "记录桌面端 API sidecar 句柄失败"
                );
            }
        }
    }

    pub(crate) fn stop_sidecar(&self) {
        let child = match self.sidecar.lock() {
            Ok(mut guard) => guard.take(),
            Err(error) => {
                let context = DiagnosticErrorContext::new("desktop.api.sidecar", "take_handle");
                diag_error!(
                    Error,
                    Subsystem::Api,
                    context = &context,
                    error = &error,
                    process_domain = %ProcessDomain::DesktopSidecar.as_str(),
                    program_kind = "desktop_api_sidecar",
                    "停止桌面端 API sidecar 时锁已污染"
                );
                None
            }
        };
        if let Some(mut child) = child {
            if let Err(error) = child.kill() {
                let context = DiagnosticErrorContext::new("desktop.api.sidecar", "kill_process");
                diag_error!(
                    Warn,
                    Subsystem::Api,
                    context = &context,
                    error = &error,
                    process_domain = %ProcessDomain::DesktopSidecar.as_str(),
                    program_kind = "desktop_api_sidecar",
                    "终止桌面端 API sidecar 失败"
                );
            }
            let _ = child.wait();
            let api = self.clone();
            tauri::async_runtime::spawn(async move {
                api.mark_stopped_origin(desktop_api_origin(DESKTOP_API_PORT))
                    .await;
            });
        }
    }
}

pub(crate) fn desktop_api_origin(port: u16) -> String {
    format!("http://127.0.0.1:{port}")
}

pub(crate) fn default_desktop_api_snapshot() -> DesktopApiSnapshot {
    match desktop_api_config() {
        Ok(config) => match config.mode {
            DesktopApiMode::External => DesktopApiSnapshot {
                state: DesktopApiState::Running,
                origin: config.origin.clone(),
                message: Some(format!("复用外部 Dashboard API: {}", config.origin)),
                database_url: None,
            },
            DesktopApiMode::Sidecar => DesktopApiSnapshot {
                state: DesktopApiState::Starting,
                origin: config.origin.clone(),
                message: Some(format!("桌面端 API sidecar 正在启动: {}", config.origin)),
                database_url: None,
            },
        },
        Err(message) => DesktopApiSnapshot {
            state: DesktopApiState::Error,
            origin: desktop_api_origin(DESKTOP_API_PORT),
            message: Some(message),
            database_url: None,
        },
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DesktopApiMode {
    External,
    Sidecar,
}

#[derive(Debug, Clone)]
pub(crate) struct DesktopApiConfig {
    pub(crate) mode: DesktopApiMode,
    pub(crate) origin: String,
    sidecar: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DesktopApiBuildProfile {
    Debug,
    Release,
}

fn active_desktop_api_build_profile() -> DesktopApiBuildProfile {
    if cfg!(debug_assertions) {
        DesktopApiBuildProfile::Debug
    } else {
        DesktopApiBuildProfile::Release
    }
}

pub(crate) fn desktop_api_config() -> Result<DesktopApiConfig, String> {
    desktop_api_config_from_values(
        env_trimmed(DESKTOP_API_ORIGIN_ENV),
        option_env!("AGENTDASH_DESKTOP_DEFAULT_API_ORIGIN")
            .and_then(|value| normalize_optional_env_text(value.to_string())),
        env_trimmed(DESKTOP_API_SIDECAR_ENV),
        option_env!("AGENTDASH_DESKTOP_DEFAULT_API_SIDECAR")
            .and_then(|value| normalize_optional_env_text(value.to_string())),
        env_trimmed(DESKTOP_API_MODE_ENV),
        option_env!("AGENTDASH_DESKTOP_DEFAULT_API_MODE")
            .and_then(|value| normalize_optional_env_text(value.to_string())),
        active_desktop_api_build_profile(),
    )
}

pub(crate) fn desktop_runtime_server_origin() -> String {
    desktop_api_config()
        .map(|config| config.origin)
        .unwrap_or_else(|_| desktop_api_origin(DESKTOP_API_PORT))
}

pub(crate) fn desktop_api_config_from_values(
    explicit_origin: Option<String>,
    build_default_origin: Option<String>,
    explicit_sidecar: Option<String>,
    build_default_sidecar: Option<String>,
    explicit_mode: Option<String>,
    build_default_mode: Option<String>,
    _build_profile: DesktopApiBuildProfile,
) -> Result<DesktopApiConfig, String> {
    let configured_origin = explicit_origin
        .map(normalize_origin)
        .or_else(|| build_default_origin.map(normalize_origin));

    let sidecar = explicit_sidecar.or(build_default_sidecar);

    let explicit_mode = explicit_mode
        .as_deref()
        .map(parse_desktop_api_mode)
        .transpose()?;
    let build_default_mode = build_default_mode
        .as_deref()
        .map(parse_desktop_api_mode)
        .transpose()?;

    let mode = explicit_mode
        .or(build_default_mode)
        .unwrap_or(DesktopApiMode::External);
    let origin = match mode {
        DesktopApiMode::External => {
            let origin = configured_origin
                .ok_or_else(|| "桌面端 external API mode 需要配置远端 server origin".to_string())?;
            validate_external_desktop_api_origin(&origin).map_err(|error| error.to_string())?;
            origin
        }
        DesktopApiMode::Sidecar => {
            let origin = configured_origin.unwrap_or_else(|| desktop_api_origin(DESKTOP_API_PORT));
            validate_sidecar_desktop_api_origin(&origin).map_err(|error| error.to_string())?;
            origin
        }
    };

    Ok(DesktopApiConfig {
        mode,
        origin,
        sidecar,
    })
}

fn validate_external_desktop_api_origin(origin: &str) -> anyhow::Result<()> {
    let url = reqwest::Url::parse(origin)?;
    if !matches!(url.scheme(), "http" | "https") {
        anyhow::bail!("桌面端 external API origin 只支持 http/https: {origin}");
    }
    if !url.username().is_empty() || url.password().is_some() {
        anyhow::bail!("桌面端 external API origin 不应包含认证信息: {origin}");
    }
    if url.path() != "/" || url.query().is_some() || url.fragment().is_some() {
        anyhow::bail!(
            "桌面端 external API origin 必须是 origin，不应包含 path/query/fragment: {origin}"
        );
    }
    Ok(())
}

fn validate_sidecar_desktop_api_origin(origin: &str) -> anyhow::Result<()> {
    if !is_127_loopback_origin(origin) {
        anyhow::bail!("桌面端 API sidecar 只允许绑定 127.0.0.1 origin: {origin}");
    }
    Ok(())
}

#[cfg(test)]
fn is_default_desktop_api_origin(origin: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(origin) else {
        return false;
    };
    url.scheme() == "http"
        && url.host_str() == Some("127.0.0.1")
        && url.port_or_known_default() == Some(DESKTOP_API_PORT)
        && url.path() == "/"
        && url.query().is_none()
        && url.fragment().is_none()
}

fn is_127_loopback_origin(origin: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(origin) else {
        return false;
    };
    matches!(url.scheme(), "http" | "https")
        && url.host_str() == Some("127.0.0.1")
        && url.path() == "/"
        && url.query().is_none()
        && url.fragment().is_none()
}

fn parse_desktop_api_mode(value: &str) -> Result<DesktopApiMode, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "external" => Ok(DesktopApiMode::External),
        "sidecar" => Ok(DesktopApiMode::Sidecar),
        "builtin" => {
            Err("桌面端 API mode 不再支持 builtin；请使用 external 或 sidecar".to_string())
        }
        other => Err(format!(
            "未知桌面端 API mode: {other}；仅支持 external 或 sidecar"
        )),
    }
}

fn normalize_origin(value: String) -> String {
    let trimmed = value.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        desktop_api_origin(DESKTOP_API_PORT)
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_api_default_origin_uses_dedicated_port() {
        assert_eq!(
            desktop_api_origin(DESKTOP_API_PORT),
            "http://127.0.0.1:17301"
        );
        assert!(is_default_desktop_api_origin("http://127.0.0.1:17301"));
        assert!(!is_default_desktop_api_origin("http://127.0.0.1:3001"));
    }

    #[test]
    fn builtin_config_is_rejected() {
        let error = desktop_api_config_from_values(
            Some("http://10.0.0.5:3001".to_string()),
            None,
            None,
            None,
            Some("builtin".to_string()),
            None,
            DesktopApiBuildProfile::Release,
        )
        .expect_err("builtin Desktop API mode is no longer supported");

        assert!(error.contains("builtin"));
        assert!(error.contains("external"));
        assert!(error.contains("sidecar"));
    }

    #[test]
    fn default_config_requires_external_origin() {
        let error = desktop_api_config_from_values(
            None,
            None,
            None,
            None,
            None,
            None,
            DesktopApiBuildProfile::Release,
        )
        .expect_err("default external mode requires a configured origin");

        assert!(error.contains("external API mode"));
    }

    #[test]
    fn release_external_origin_may_use_remote_cloud_origin() {
        let config = desktop_api_config_from_values(
            Some("https://agentdash.example.com".to_string()),
            None,
            None,
            None,
            Some("external".to_string()),
            None,
            DesktopApiBuildProfile::Release,
        )
        .expect("release desktop app may connect to the configured remote server");

        assert_eq!(config.mode, DesktopApiMode::External);
        assert_eq!(config.origin, "https://agentdash.example.com");
    }

    #[test]
    fn debug_external_origin_may_use_dev_server_port() {
        let config = desktop_api_config_from_values(
            Some("http://127.0.0.1:3001".to_string()),
            None,
            None,
            None,
            Some("external".to_string()),
            None,
            DesktopApiBuildProfile::Debug,
        )
        .expect("desktop dev runtime may reuse the ordinary backend dev server");

        assert_eq!(config.mode, DesktopApiMode::External);
        assert_eq!(config.origin, "http://127.0.0.1:3001");
    }

    #[test]
    fn release_sidecar_origin_must_match_desktop_api_origin() {
        let config = desktop_api_config_from_values(
            Some("http://127.0.0.1:17301".to_string()),
            None,
            Some("agentdash-server".to_string()),
            None,
            Some("sidecar".to_string()),
            None,
            DesktopApiBuildProfile::Release,
        )
        .expect("release sidecar may use the fixed Desktop API origin");

        assert_eq!(config.mode, DesktopApiMode::Sidecar);
        assert_eq!(config.origin, "http://127.0.0.1:17301");
        assert_eq!(config.sidecar.as_deref(), Some("agentdash-server"));
    }

    #[test]
    fn sidecar_origin_never_binds_non_loopback_host() {
        let error = desktop_api_config_from_values(
            Some("http://0.0.0.0:17301".to_string()),
            None,
            Some("agentdash-server".to_string()),
            None,
            Some("sidecar".to_string()),
            None,
            DesktopApiBuildProfile::Debug,
        )
        .expect_err("sidecar must not bind a non-loopback host");

        assert!(error.contains("127.0.0.1"));
    }
}
