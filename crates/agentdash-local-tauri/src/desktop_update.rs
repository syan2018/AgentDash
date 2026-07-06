use std::sync::Arc;

use agentdash_contracts::desktop_release::{DesktopUpdateCheckResponse, DesktopUpdateStatus};
use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_updater::UpdaterExt;
use tokio::sync::Mutex as AsyncMutex;

use crate::desktop_api::desktop_runtime_server_origin;
use crate::settings::{env_trimmed, normalize_optional_env_text};
use crate::state::DesktopState;

#[derive(Clone, Default)]
pub(crate) struct DesktopUpdateGate {
    snapshot: Arc<AsyncMutex<DesktopUpdatePolicySnapshot>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct DesktopUpdatePolicySnapshot {
    current_version: String,
    status: DesktopUpdateGateStatus,
    force_update_required: bool,
    checked_at: Option<String>,
    latest_version: Option<String>,
    min_desktop_version: Option<String>,
    recommended_desktop_version: Option<String>,
    update_available: Option<bool>,
    manifest_url_configured: Option<bool>,
    diagnostics_code: Option<String>,
    diagnostics_message: Option<String>,
    last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum DesktopUpdateGateStatus {
    Unchecked,
    Ready,
    ForceUpdateRequired,
    Unavailable,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct DesktopUpdateInstallResult {
    installed: bool,
    version: Option<String>,
    message: String,
}

impl Default for DesktopUpdatePolicySnapshot {
    fn default() -> Self {
        Self {
            current_version: env!("CARGO_PKG_VERSION").to_string(),
            status: DesktopUpdateGateStatus::Unchecked,
            force_update_required: false,
            checked_at: None,
            latest_version: None,
            min_desktop_version: None,
            recommended_desktop_version: None,
            update_available: None,
            manifest_url_configured: None,
            diagnostics_code: None,
            diagnostics_message: None,
            last_error: None,
        }
    }
}

pub(crate) async fn refresh_desktop_update_policy(
    state: &DesktopState,
) -> DesktopUpdatePolicySnapshot {
    let current_version = env!("CARGO_PKG_VERSION").to_string();
    let endpoint = match desktop_product_update_endpoint(&current_version) {
        Ok(endpoint) => endpoint,
        Err(error) => {
            let snapshot = DesktopUpdatePolicySnapshot::unavailable(
                current_version,
                format!("构造桌面更新策略 endpoint 失败: {error}"),
            );
            state.updates.store(snapshot.clone()).await;
            return snapshot;
        }
    };

    let response = reqwest::Client::new().get(endpoint).send().await;
    let response = match response {
        Ok(response) => response,
        Err(error) => {
            let snapshot = DesktopUpdatePolicySnapshot::unavailable(
                current_version,
                format!("读取桌面更新策略失败: {error}"),
            );
            state.updates.store(snapshot.clone()).await;
            return snapshot;
        }
    };

    if !response.status().is_success() {
        let snapshot = DesktopUpdatePolicySnapshot::unavailable(
            current_version,
            format!("读取桌面更新策略返回 HTTP {}", response.status().as_u16()),
        );
        state.updates.store(snapshot.clone()).await;
        return snapshot;
    }

    let body = response.json::<DesktopUpdateCheckResponse>().await;
    let snapshot = match body {
        Ok(body) => DesktopUpdatePolicySnapshot::from_update_response(current_version, body),
        Err(error) => DesktopUpdatePolicySnapshot::unavailable(
            current_version,
            format!("解析桌面更新策略失败: {error}"),
        ),
    };
    if snapshot.force_update_required {
        state
            .runtime
            .mark_disabled("当前桌面端版本低于云端最低要求，请先完成更新")
            .await;
    }
    state.updates.store(snapshot.clone()).await;
    snapshot
}

pub(crate) async fn install_desktop_update(
    app: AppHandle,
) -> Result<DesktopUpdateInstallResult, String> {
    let pubkey = desktop_updater_pubkey()
        .ok_or_else(|| "未配置桌面更新签名公钥，无法安装自动更新".to_string())?;
    let endpoint = desktop_tauri_update_endpoint()
        .map_err(|error| format!("构造桌面更新 endpoint 失败: {error}"))?;

    let updater = app
        .updater_builder()
        .pubkey(pubkey)
        .endpoints(vec![endpoint])
        .map_err(|error| format!("桌面更新 endpoint 无效: {error}"))?
        .build()
        .map_err(|error| format!("初始化桌面更新器失败: {error}"))?;

    let Some(update) = updater
        .check()
        .await
        .map_err(|error| format!("检查桌面更新失败: {error}"))?
    else {
        return Ok(DesktopUpdateInstallResult {
            installed: false,
            version: None,
            message: "当前没有可安装的桌面更新".to_string(),
        });
    };

    let version = update.version.clone();
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|error| format!("下载或安装桌面更新失败: {error}"))?;

    Ok(DesktopUpdateInstallResult {
        installed: true,
        version: Some(version.clone()),
        message: format!("桌面更新 {version} 已安装，重启后生效"),
    })
}

pub(crate) async fn ensure_desktop_update_allows_runtime(
    state: &DesktopState,
) -> anyhow::Result<()> {
    let snapshot = current_desktop_update_policy_snapshot(state).await;
    ensure_desktop_update_snapshot_allows_mutation(&snapshot)
}

pub(crate) async fn ensure_desktop_update_allows_mutation(
    state: &DesktopState,
) -> anyhow::Result<()> {
    let snapshot = current_desktop_update_policy_snapshot(state).await;
    ensure_desktop_update_snapshot_allows_mutation(&snapshot)
}

async fn current_desktop_update_policy_snapshot(
    state: &DesktopState,
) -> DesktopUpdatePolicySnapshot {
    let mut snapshot = state.updates.snapshot().await;
    if snapshot.status == DesktopUpdateGateStatus::Unchecked {
        snapshot = refresh_desktop_update_policy(state).await;
    }
    snapshot
}

fn ensure_desktop_update_snapshot_allows_mutation(
    snapshot: &DesktopUpdatePolicySnapshot,
) -> anyhow::Result<()> {
    if snapshot.force_update_required {
        anyhow::bail!("{}", force_update_required_message(snapshot));
    }
    Ok(())
}

fn desktop_product_update_endpoint(current_version: &str) -> anyhow::Result<reqwest::Url> {
    let origin = desktop_runtime_server_origin();
    let endpoint = format!(
        "{origin}/api/desktop/update?platform={platform}&arch={arch}&current_version={current_version}",
        platform = desktop_update_platform(),
        arch = desktop_update_arch(),
    );
    reqwest::Url::parse(&endpoint).map_err(anyhow::Error::from)
}

fn desktop_tauri_update_endpoint() -> anyhow::Result<reqwest::Url> {
    let origin = desktop_runtime_server_origin();
    let endpoint = format!(
        "{origin}/api/desktop/update/tauri?platform={platform}&arch={arch}&current_version=%7B%7Bcurrent_version%7D%7D",
        platform = desktop_update_platform(),
        arch = desktop_update_arch(),
    );
    reqwest::Url::parse(&endpoint).map_err(anyhow::Error::from)
}

fn desktop_updater_pubkey() -> Option<String> {
    env_trimmed("AGENTDASH_DESKTOP_UPDATER_PUBKEY").or_else(|| {
        option_env!("AGENTDASH_DESKTOP_UPDATER_PUBKEY")
            .and_then(|value| normalize_optional_env_text(value.to_string()))
    })
}

fn desktop_update_platform() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "darwin"
    } else {
        "linux"
    }
}

fn desktop_update_arch() -> &'static str {
    if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else if cfg!(target_arch = "x86") {
        "i686"
    } else {
        "unknown"
    }
}

fn force_update_required_message(snapshot: &DesktopUpdatePolicySnapshot) -> String {
    match snapshot.min_desktop_version.as_deref() {
        Some(min_version) => format!(
            "当前桌面端版本 {} 低于云端最低要求 {min_version}，请先完成更新",
            snapshot.current_version
        ),
        None => "当前桌面端版本低于云端最低要求，请先完成更新".to_string(),
    }
}

fn desktop_update_status_code(status: DesktopUpdateStatus) -> &'static str {
    match status {
        DesktopUpdateStatus::UpdateAvailable => "update_available",
        DesktopUpdateStatus::UpToDate => "up_to_date",
        DesktopUpdateStatus::LatestAvailable => "latest_available",
        DesktopUpdateStatus::UnsupportedTarget => "unsupported_target",
        DesktopUpdateStatus::Unconfigured => "unconfigured",
        DesktopUpdateStatus::FetchFailed => "fetch_failed",
        DesktopUpdateStatus::InvalidManifest => "invalid_manifest",
    }
}

fn is_force_update_required(current_version: &str, min_desktop_version: Option<&str>) -> bool {
    min_desktop_version
        .map(|min_version| version_cmp(current_version, min_version) == std::cmp::Ordering::Less)
        .unwrap_or(false)
}

fn version_cmp(left: &str, right: &str) -> std::cmp::Ordering {
    let left_parts = version_parts(left);
    let right_parts = version_parts(right);
    let max_len = left_parts.len().max(right_parts.len());
    for index in 0..max_len {
        let left = left_parts.get(index).map(String::as_str).unwrap_or("0");
        let right = right_parts.get(index).map(String::as_str).unwrap_or("0");
        let ordering = match (left.parse::<u64>(), right.parse::<u64>()) {
            (Ok(left), Ok(right)) => left.cmp(&right),
            _ => left.cmp(right),
        };
        if ordering != std::cmp::Ordering::Equal {
            return ordering;
        }
    }
    std::cmp::Ordering::Equal
}

fn version_parts(value: &str) -> Vec<String> {
    value
        .trim()
        .trim_start_matches('v')
        .split(['.', '-', '+', '_'])
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

impl DesktopUpdatePolicySnapshot {
    pub(crate) fn force_update_required(&self) -> bool {
        self.force_update_required
    }

    fn unavailable(current_version: String, message: String) -> Self {
        Self {
            current_version,
            status: DesktopUpdateGateStatus::Unavailable,
            force_update_required: false,
            checked_at: Some(chrono::Utc::now().to_rfc3339()),
            latest_version: None,
            min_desktop_version: None,
            recommended_desktop_version: None,
            update_available: None,
            manifest_url_configured: None,
            diagnostics_code: Some("desktop_update_policy_unavailable".to_string()),
            diagnostics_message: None,
            last_error: Some(message),
        }
    }

    fn from_update_response(current_version: String, response: DesktopUpdateCheckResponse) -> Self {
        let force_update_required = response.policy.min_desktop_version_configured
            && is_force_update_required(
                &current_version,
                response.policy.min_desktop_version.as_deref(),
            );
        let status = if force_update_required {
            DesktopUpdateGateStatus::ForceUpdateRequired
        } else {
            DesktopUpdateGateStatus::Ready
        };
        Self {
            current_version,
            status,
            force_update_required,
            checked_at: Some(chrono::Utc::now().to_rfc3339()),
            latest_version: response.latest.map(|latest| latest.version),
            min_desktop_version: response.policy.min_desktop_version,
            recommended_desktop_version: response.policy.recommended_desktop_version,
            update_available: response.update_available,
            manifest_url_configured: Some(response.diagnostics.manifest_url_configured),
            diagnostics_code: response
                .diagnostics
                .code
                .or_else(|| Some(desktop_update_status_code(response.status).to_string())),
            diagnostics_message: response.diagnostics.message,
            last_error: None,
        }
    }
}

impl DesktopUpdateGate {
    pub(crate) async fn snapshot(&self) -> DesktopUpdatePolicySnapshot {
        self.snapshot.lock().await.clone()
    }

    async fn store(&self, snapshot: DesktopUpdatePolicySnapshot) {
        let mut guard = self.snapshot.lock().await;
        *guard = snapshot;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_contracts::desktop_release::{
        DesktopUpdateDiagnostics, DesktopUpdatePolicy, DesktopUpdateRecommendedVersionSource,
    };

    #[test]
    fn desktop_update_gate_does_not_force_without_explicit_minimum_version() {
        let snapshot = DesktopUpdatePolicySnapshot::from_update_response(
            "0.1.0".to_string(),
            update_response(None, false),
        );

        assert_eq!(snapshot.status, DesktopUpdateGateStatus::Ready);
        assert!(!snapshot.force_update_required);
        assert_eq!(snapshot.min_desktop_version, None);
    }

    #[test]
    fn desktop_update_gate_forces_when_current_version_is_below_explicit_minimum() {
        let snapshot = DesktopUpdatePolicySnapshot::from_update_response(
            "0.1.0".to_string(),
            update_response(Some("0.2.0"), true),
        );

        assert_eq!(
            snapshot.status,
            DesktopUpdateGateStatus::ForceUpdateRequired
        );
        assert!(snapshot.force_update_required);
        assert_eq!(snapshot.min_desktop_version.as_deref(), Some("0.2.0"));
    }

    #[test]
    fn desktop_update_gate_blocks_mutation_when_force_update_is_required() {
        let snapshot = DesktopUpdatePolicySnapshot::from_update_response(
            "0.1.0".to_string(),
            update_response(Some("0.2.0"), true),
        );

        let error = ensure_desktop_update_snapshot_allows_mutation(&snapshot)
            .expect_err("force update should block mutation commands");
        assert!(error.to_string().contains("低于云端最低要求 0.2.0"));
    }

    #[test]
    fn desktop_update_gate_allows_mutation_when_minimum_version_is_absent() {
        let snapshot = DesktopUpdatePolicySnapshot::from_update_response(
            "0.1.0".to_string(),
            update_response(None, false),
        );

        ensure_desktop_update_snapshot_allows_mutation(&snapshot)
            .expect("unconfigured minimum version should not block local development");
    }

    #[test]
    fn desktop_update_version_comparison_handles_numeric_components() {
        assert_eq!(version_cmp("0.10.0", "0.2.0"), std::cmp::Ordering::Greater);
        assert_eq!(version_cmp("v0.1.0", "0.1.0"), std::cmp::Ordering::Equal);
        assert_eq!(version_cmp("0.1.0", "0.1.1"), std::cmp::Ordering::Less);
    }

    fn update_response(
        min_desktop_version: Option<&str>,
        min_desktop_version_configured: bool,
    ) -> DesktopUpdateCheckResponse {
        DesktopUpdateCheckResponse {
            status: DesktopUpdateStatus::Unconfigured,
            channel: "stable".to_string(),
            platform: "windows".to_string(),
            arch: "x86_64".to_string(),
            current_version: Some("0.1.0".to_string()),
            latest: None,
            update_available: None,
            policy: DesktopUpdatePolicy {
                min_desktop_version: min_desktop_version.map(str::to_string),
                recommended_desktop_version: None,
                min_desktop_version_configured,
                recommended_desktop_version_source: DesktopUpdateRecommendedVersionSource::None,
            },
            diagnostics: DesktopUpdateDiagnostics {
                manifest_url_configured: false,
                code: Some("desktop_manifest_url_unconfigured".to_string()),
                message: None,
                fetched_at: None,
                cache_hit: false,
                cache_ttl_seconds: 60,
            },
        }
    }
}
