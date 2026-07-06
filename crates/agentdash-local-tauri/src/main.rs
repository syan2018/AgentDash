#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod codex_oauth;

use agentdash_diagnostics::{DiagnosticErrorContext, Subsystem, diag, diag_error};
use agentdash_process::{ProcessDomain, background_std_command};
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};

use agentdash_contracts::desktop_release::{DesktopUpdateCheckResponse, DesktopUpdateStatus};
use agentdash_local::local_backend_config::McpLocalServerEntry;
use agentdash_local::{
    DesktopAppSettings, DesktopEnsureRetryEvent, DesktopEnsureRetryPolicy, DesktopRunnerHost,
    DesktopRuntimeStartRequest as RuntimeStartRequest, LocalLogEvent, LocalRuntimeProfile,
    LocalRuntimeSnapshot, McpProbeResult, StopReason, browse_directory,
    delete_desktop_runtime_profile, ensure_desktop_runtime_config, load_desktop_app_settings,
    load_desktop_runtime_profile_with_server_origin, local_mcp_servers_path,
    normalize_desktop_app_settings, normalize_desktop_runtime_start_request_with_server_origin,
    probe_mcp_server, redact_secret, save_desktop_app_settings,
    save_desktop_runtime_profile_with_server_origin,
};
use agentdash_relay::BrowseDirectoryEntry;
use codex_oauth::{codex_oauth_cancel, codex_oauth_start};
use serde::Serialize;
use tauri::menu::MenuBuilder;
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, RunEvent, State, WindowEvent};
use tauri_plugin_updater::UpdaterExt;
use tokio::sync::Mutex as AsyncMutex;
use tracing_subscriber::EnvFilter;

const DESKTOP_API_PORT: u16 = 17301;
const DESKTOP_API_MODE_ENV: &str = "AGENTDASH_DESKTOP_API_MODE";
const DESKTOP_API_ORIGIN_ENV: &str = "AGENTDASH_DESKTOP_API_ORIGIN";
const DESKTOP_API_SIDECAR_ENV: &str = "AGENTDASH_DESKTOP_API_SIDECAR";
const DESKTOP_AUTOSTART_ENTRY_NAME: &str = "AgentDash";
#[cfg(target_os = "windows")]
const WINDOWS_AUTOSTART_RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const MAIN_WINDOW_LABEL: &str = "main";
const TRAY_MENU_OPEN: &str = "open_agentdash";
const TRAY_MENU_RUNTIME_START: &str = "start_local_runtime";
const TRAY_MENU_RUNTIME_STOP: &str = "stop_local_runtime";
const TRAY_MENU_STATUS: &str = "show_status";
const TRAY_MENU_QUIT: &str = "quit_agentdash";

#[derive(Clone)]
struct DesktopState {
    runtime: DesktopRunnerHost,
    api: DesktopApiManager,
    updates: DesktopUpdateGate,
    lifecycle: Arc<DesktopLifecycleState>,
}

impl Default for DesktopState {
    fn default() -> Self {
        Self {
            runtime: DesktopRunnerHost::new(),
            api: DesktopApiManager::from_snapshot(default_desktop_api_snapshot()),
            updates: DesktopUpdateGate::default(),
            lifecycle: Arc::new(DesktopLifecycleState::default()),
        }
    }
}

#[derive(Default)]
struct DesktopLifecycleState {
    explicit_quit: AtomicBool,
}

impl DesktopState {
    fn request_explicit_quit(&self) {
        self.lifecycle.explicit_quit.store(true, Ordering::SeqCst);
    }

    fn is_explicit_quit_requested(&self) -> bool {
        self.lifecycle.explicit_quit.load(Ordering::SeqCst)
    }
}

#[derive(Clone, Default)]
struct DesktopApiManager {
    snapshot: Arc<AsyncMutex<DesktopApiSnapshot>>,
    sidecar: Arc<StdMutex<Option<Child>>>,
}

#[derive(Clone, Default)]
struct DesktopUpdateGate {
    snapshot: Arc<AsyncMutex<DesktopUpdatePolicySnapshot>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
struct DesktopApiSnapshot {
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
struct DesktopUpdatePolicySnapshot {
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
struct DesktopUpdateInstallResult {
    installed: bool,
    version: Option<String>,
    message: String,
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
struct DesktopAutostartStatus {
    supported: bool,
    enabled: bool,
    message: Option<String>,
}

#[tauri::command]
async fn desktop_settings_load() -> Result<DesktopAppSettings, String> {
    load_desktop_app_settings().map_err(|error| error.to_string())
}

#[tauri::command]
async fn desktop_settings_save(
    state: State<'_, DesktopState>,
    settings: DesktopAppSettings,
) -> Result<DesktopAppSettings, String> {
    ensure_desktop_update_allows_mutation(state.inner())
        .await
        .map_err(|error| error.to_string())?;
    let mut settings = normalize_desktop_app_settings(settings);
    let autostart = desktop_autostart_set_enabled_internal(settings.launch_at_login)?;
    settings.launch_at_login = autostart.enabled;
    save_desktop_app_settings(settings).map_err(|error| error.to_string())
}

#[tauri::command]
async fn desktop_autostart_is_enabled() -> Result<DesktopAutostartStatus, String> {
    desktop_autostart_is_enabled_internal()
}

#[tauri::command]
async fn desktop_autostart_set_enabled(
    state: State<'_, DesktopState>,
    enabled: bool,
) -> Result<DesktopAutostartStatus, String> {
    ensure_desktop_update_allows_mutation(state.inner())
        .await
        .map_err(|error| error.to_string())?;
    let mut settings = load_desktop_app_settings().map_err(|error| error.to_string())?;
    let status = desktop_autostart_set_enabled_internal(enabled)?;
    settings.launch_at_login = status.enabled;
    save_desktop_app_settings(settings).map_err(|error| error.to_string())?;
    Ok(status)
}

#[tauri::command]
async fn desktop_quit_request(
    app: AppHandle,
    state: State<'_, DesktopState>,
) -> Result<(), String> {
    request_desktop_quit(app, state.inner().clone()).await;
    Ok(())
}

#[tauri::command]
async fn profile_load() -> Result<Option<LocalRuntimeProfile>, String> {
    load_desktop_runtime_profile_with_server_origin(&desktop_runtime_server_origin())
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn profile_save(
    state: State<'_, DesktopState>,
    profile: LocalRuntimeProfile,
) -> Result<LocalRuntimeProfile, String> {
    ensure_desktop_update_allows_mutation(state.inner())
        .await
        .map_err(|error| error.to_string())?;
    save_desktop_runtime_profile_with_server_origin(profile, &desktop_runtime_server_origin())
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn profile_delete(state: State<'_, DesktopState>) -> Result<(), String> {
    ensure_desktop_update_allows_mutation(state.inner())
        .await
        .map_err(|error| error.to_string())?;
    delete_desktop_runtime_profile().map_err(|error| error.to_string())
}

#[tauri::command]
async fn runtime_start(
    state: State<'_, DesktopState>,
    request: RuntimeStartRequest,
) -> Result<LocalRuntimeSnapshot, String> {
    start_runtime_from_request(state.inner(), request, false)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn runtime_stop(state: State<'_, DesktopState>) -> Result<(), String> {
    state
        .runtime
        .stop(StopReason::UserRequested)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn runtime_restart(state: State<'_, DesktopState>) -> Result<LocalRuntimeSnapshot, String> {
    ensure_desktop_update_allows_runtime(state.inner())
        .await
        .map_err(|error| error.to_string())?;
    state
        .runtime
        .restart()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn runtime_snapshot(
    state: State<'_, DesktopState>,
) -> Result<Option<LocalRuntimeSnapshot>, String> {
    Ok(state.runtime.snapshot().await)
}

#[tauri::command]
async fn mcp_servers_load(
    state: State<'_, DesktopState>,
) -> Result<Vec<McpLocalServerEntry>, String> {
    let path = mcp_servers_path()?;
    state
        .runtime
        .record_log(
            "info",
            "mcp",
            format!("加载 MCP servers: {}", path.display()),
        )
        .await;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str::<Vec<McpLocalServerEntry>>(&content)
        .map_err(|e| format!("解析 MCP servers 配置失败: {e}"))
}

#[tauri::command]
async fn mcp_servers_save(
    state: State<'_, DesktopState>,
    servers: Vec<McpLocalServerEntry>,
) -> Result<(), String> {
    ensure_desktop_update_allows_mutation(state.inner())
        .await
        .map_err(|error| error.to_string())?;
    let path = mcp_servers_path()?;
    state
        .runtime
        .record_log(
            "info",
            "mcp",
            format!("保存 MCP servers: count={}", servers.len()),
        )
        .await;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let content = serde_json::to_string_pretty(&servers).map_err(|e| e.to_string())?;
    std::fs::write(&path, content).map_err(|e| e.to_string())
}

#[tauri::command]
async fn mcp_server_probe(
    state: State<'_, DesktopState>,
    server: McpLocalServerEntry,
) -> Result<McpProbeResult, String> {
    ensure_desktop_update_allows_mutation(state.inner())
        .await
        .map_err(|error| error.to_string())?;
    let result = probe_mcp_server(server.clone()).await;
    state
        .runtime
        .record_log(
            if result.ok { "info" } else { "warn" },
            "mcp",
            format!("探测 MCP server: name={}, {}", server.name, result.message),
        )
        .await;
    Ok(result)
}

#[derive(Serialize)]
struct BrowseDirectoryResponse {
    current_path: String,
    entries: Vec<BrowseDirectoryEntry>,
}

#[tauri::command]
async fn desktop_browse_directory(path: Option<String>) -> Result<BrowseDirectoryResponse, String> {
    let (current_path, entries) =
        tokio::task::spawn_blocking(move || browse_directory(path.as_deref()))
            .await
            .map_err(|e| format!("目录浏览任务失败: {e}"))??;
    Ok(BrowseDirectoryResponse {
        current_path,
        entries,
    })
}

#[tauri::command]
async fn logs_tail(
    state: State<'_, DesktopState>,
    limit: Option<usize>,
) -> Result<Vec<LocalLogEvent>, String> {
    Ok(state.runtime.logs_tail(limit.unwrap_or(200)).await)
}

#[tauri::command]
async fn logs_clear(state: State<'_, DesktopState>) -> Result<(), String> {
    ensure_desktop_update_allows_mutation(state.inner())
        .await
        .map_err(|error| error.to_string())?;
    state.runtime.logs_clear().await;
    state
        .runtime
        .record_log("info", "runtime", "已清空本机日志")
        .await;
    Ok(())
}

#[tauri::command]
async fn desktop_api_snapshot(
    state: State<'_, DesktopState>,
) -> Result<DesktopApiSnapshot, String> {
    Ok(state.api.snapshot().await)
}

#[tauri::command]
async fn desktop_update_policy_snapshot(
    state: State<'_, DesktopState>,
) -> Result<DesktopUpdatePolicySnapshot, String> {
    Ok(state.updates.snapshot().await)
}

#[tauri::command]
async fn desktop_update_policy_refresh(
    state: State<'_, DesktopState>,
) -> Result<DesktopUpdatePolicySnapshot, String> {
    Ok(refresh_desktop_update_policy(state.inner()).await)
}

#[tauri::command]
async fn desktop_update_install(app: AppHandle) -> Result<DesktopUpdateInstallResult, String> {
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

#[tauri::command]
async fn open_external_url(url: String) -> Result<(), String> {
    let parsed = reqwest::Url::parse(&url).map_err(|error| format!("外部链接无效: {error}"))?;
    match parsed.scheme() {
        "http" | "https" => {}
        scheme => return Err(format!("不支持打开 {scheme}:// 外部链接")),
    }
    open::that(parsed.as_str()).map_err(|error| format!("打开系统浏览器失败: {error}"))
}

async fn refresh_desktop_update_policy(state: &DesktopState) -> DesktopUpdatePolicySnapshot {
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

async fn ensure_desktop_update_allows_runtime(state: &DesktopState) -> anyhow::Result<()> {
    let snapshot = current_desktop_update_policy_snapshot(state).await;
    ensure_desktop_update_snapshot_allows_mutation(&snapshot)
}

async fn ensure_desktop_update_allows_mutation(state: &DesktopState) -> anyhow::Result<()> {
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
        anyhow::bail!("{}", force_update_required_message(&snapshot));
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
    async fn snapshot(&self) -> DesktopUpdatePolicySnapshot {
        self.snapshot.lock().await.clone()
    }

    async fn store(&self, snapshot: DesktopUpdatePolicySnapshot) {
        let mut guard = self.snapshot.lock().await;
        *guard = snapshot;
    }
}

fn desktop_autostart_is_enabled_internal() -> Result<DesktopAutostartStatus, String> {
    #[cfg(target_os = "windows")]
    {
        let app_exe = current_app_exe_path()?;
        let expected = build_windows_autostart_command(&app_exe)?;
        let stored = windows_autostart_read_value()?;
        let enabled = stored.as_deref() == Some(expected.as_str());
        let message = match stored {
            Some(value) if value != expected => Some(
                "检测到 AgentDash 登录项，但它不指向当前应用可执行文件；请重新启用登录自启动"
                    .to_string(),
            ),
            Some(_) => Some("Windows 登录自启动已启用".to_string()),
            None => Some("Windows 登录自启动未启用".to_string()),
        };
        Ok(DesktopAutostartStatus {
            supported: true,
            enabled,
            message,
        })
    }

    #[cfg(not(target_os = "windows"))]
    {
        Ok(DesktopAutostartStatus {
            supported: false,
            enabled: false,
            message: Some("当前平台不支持 AgentDash 桌面登录自启动".to_string()),
        })
    }
}

fn desktop_autostart_set_enabled_internal(enabled: bool) -> Result<DesktopAutostartStatus, String> {
    #[cfg(target_os = "windows")]
    {
        if enabled {
            let app_exe = current_app_exe_path()?;
            let command = build_windows_autostart_command(&app_exe)?;
            windows_autostart_write_value(&command)?;
            Ok(DesktopAutostartStatus {
                supported: true,
                enabled: true,
                message: Some(format!("Windows 登录自启动已指向 {}", app_exe.display())),
            })
        } else {
            windows_autostart_delete_value()?;
            Ok(DesktopAutostartStatus {
                supported: true,
                enabled: false,
                message: Some("Windows 登录自启动已关闭".to_string()),
            })
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = enabled;
        Ok(DesktopAutostartStatus {
            supported: false,
            enabled: false,
            message: Some("当前平台不支持 AgentDash 桌面登录自启动".to_string()),
        })
    }
}

fn current_app_exe_path() -> Result<PathBuf, String> {
    std::env::current_exe().map_err(|error| format!("读取当前应用可执行文件路径失败: {error}"))
}

fn build_windows_autostart_command(app_exe: &Path) -> Result<String, String> {
    let file_name = app_exe
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "应用可执行文件路径缺少文件名".to_string())?;
    if is_setup_exe_name(file_name) {
        return Err(format!(
            "拒绝将安装器写入登录自启动项: {}",
            app_exe.display()
        ));
    }
    let raw = app_exe
        .to_str()
        .ok_or_else(|| "应用可执行文件路径不是有效 Unicode".to_string())?;
    if raw.contains('"') {
        return Err("应用可执行文件路径不能包含双引号".to_string());
    }
    Ok(format!("\"{raw}\""))
}

fn is_setup_exe_name(file_name: &str) -> bool {
    let normalized = file_name.to_ascii_lowercase();
    normalized.ends_with(".exe")
        && (normalized.contains("setup") || normalized.contains("installer"))
}

#[cfg(target_os = "windows")]
fn windows_autostart_read_value() -> Result<Option<String>, String> {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let run_key = match hkcu.open_subkey(WINDOWS_AUTOSTART_RUN_KEY) {
        Ok(key) => key,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("读取 Windows 登录自启动注册表失败: {error}")),
    };
    match run_key.get_value(DESKTOP_AUTOSTART_ENTRY_NAME) {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("读取 AgentDash 登录自启动项失败: {error}")),
    }
}

#[cfg(target_os = "windows")]
fn windows_autostart_write_value(command: &str) -> Result<(), String> {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (run_key, _) = hkcu
        .create_subkey(WINDOWS_AUTOSTART_RUN_KEY)
        .map_err(|error| format!("打开 Windows 登录自启动注册表失败: {error}"))?;
    run_key
        .set_value(DESKTOP_AUTOSTART_ENTRY_NAME, &command)
        .map_err(|error| format!("写入 AgentDash 登录自启动项失败: {error}"))
}

#[cfg(target_os = "windows")]
fn windows_autostart_delete_value() -> Result<(), String> {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let run_key = match hkcu
        .open_subkey_with_flags(WINDOWS_AUTOSTART_RUN_KEY, winreg::enums::KEY_SET_VALUE)
    {
        Ok(key) => key,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(format!("打开 Windows 登录自启动注册表失败: {error}")),
    };
    match run_key.delete_value(DESKTOP_AUTOSTART_ENTRY_NAME) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("删除 AgentDash 登录自启动项失败: {error}")),
    }
}

async fn start_runtime_from_request(
    state: &DesktopState,
    request: RuntimeStartRequest,
    retry_until_server_ready: bool,
) -> anyhow::Result<LocalRuntimeSnapshot> {
    ensure_desktop_update_allows_runtime(state).await?;
    let request = normalize_desktop_runtime_start_request_with_server_origin(
        request,
        &desktop_runtime_server_origin(),
    )?;
    let runtime_for_claim = state.runtime.clone();
    state
        .runtime
        .ensure_started_with(|| async move {
            let policy = if retry_until_server_ready {
                DesktopEnsureRetryPolicy::wait_for_server_ready()
            } else {
                DesktopEnsureRetryPolicy::single_attempt()
            };
            ensure_desktop_runtime_config(request, policy, |event| {
                let runtime = runtime_for_claim.clone();
                async move {
                    record_desktop_ensure_retry(&runtime, event).await;
                }
            })
            .await
            .map_err(anyhow::Error::from)
        })
        .await
}

fn desktop_runtime_server_origin() -> String {
    desktop_api_config()
        .map(|config| config.origin)
        .unwrap_or_else(|_| desktop_api_origin(DESKTOP_API_PORT))
}

async fn record_desktop_ensure_retry(runtime: &DesktopRunnerHost, event: DesktopEnsureRetryEvent) {
    runtime
        .mark_waiting_for_api(
            "Dashboard API 暂不可用，等待后继续领取本机 runtime",
            Some(event.error.clone()),
            Some(event.attempt),
            Some(event.next_retry_at.clone()),
        )
        .await;
    let error = redact_secret(&event.error);
    let context = DiagnosticErrorContext::new("desktop.runtime.ensure", "wait_for_api_ready");
    diag_error!(
        Warn,
        Subsystem::Api,
        context = &context,
        error = &error,
        attempt = event.attempt,
        retry_count = event.attempt.saturating_sub(1),
        "领取本机 runtime 失败，等待 server 就绪后重试"
    );
}

fn initialize_desktop_runner_host(state: DesktopState) {
    tauri::async_runtime::spawn(async move {
        let update_policy = refresh_desktop_update_policy(&state).await;
        if update_policy.force_update_required {
            return;
        }

        let settings = match load_desktop_app_settings() {
            Ok(settings) => settings,
            Err(error) => {
                let context =
                    DiagnosticErrorContext::new("desktop.runtime.initialize", "load_settings");
                diag_error!(
                    Error,
                    Subsystem::Infra,
                    context = &context,
                    error = &error,
                    "读取桌面设置失败，无法判断 runtime 自动连接策略"
                );
                state
                    .runtime
                    .mark_error(
                        "读取桌面设置失败，无法判断 runtime 自动连接策略",
                        error.to_string(),
                    )
                    .await;
                return;
            }
        };

        if !settings.auto_connect_local_runtime {
            state
                .runtime
                .mark_disabled("桌面设置已关闭启动后自动连接 runtime")
                .await;
            return;
        }

        let profile = match profile_load().await {
            Ok(Some(profile)) => profile,
            Ok(None) => {
                state
                    .runtime
                    .mark_idle("等待登录后创建桌面本机 runtime profile")
                    .await;
                return;
            }
            Err(error) => {
                let context =
                    DiagnosticErrorContext::new("desktop.runtime.initialize", "load_profile");
                diag_error!(
                    Error,
                    Subsystem::Infra,
                    context = &context,
                    error = &error,
                    "读取桌面本机 runtime profile 失败"
                );
                state
                    .runtime
                    .mark_error("读取桌面本机 runtime profile 失败", error)
                    .await;
                return;
            }
        };

        if !profile.auto_start {
            state
                .runtime
                .mark_idle("profile 未开启自动启动，等待登录桥接或手动启动")
                .await;
            return;
        }

        state
            .runtime
            .mark_waiting_for_auth("profile 已允许自动启动，等待 Web bridge 提供当前 access token")
            .await;
    });
}

fn main() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .try_init();

    let state = DesktopState::default();
    let state_for_exit = state.clone();
    let state_for_window_events = state.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            restore_main_window(app);
        }))
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(state)
        .on_window_event(move |window, event| {
            if window.label() != MAIN_WINDOW_LABEL {
                return;
            }
            if let WindowEvent::CloseRequested { api, .. } = event {
                if state_for_window_events.is_explicit_quit_requested() {
                    return;
                }
                api.prevent_close();
                if let Err(error) = window.hide() {
                    let context =
                        DiagnosticErrorContext::new("desktop.window.lifecycle", "hide_to_tray");
                    diag_error!(
                        Warn,
                        Subsystem::Infra,
                        context = &context,
                        error = &error,
                        window_label = MAIN_WINDOW_LABEL,
                        "关闭窗口时隐藏到托盘失败"
                    );
                }
            }
        })
        .setup(|app| {
            let state = app.state::<DesktopState>().inner().clone();
            configure_tray(app.handle(), state.clone())?;
            let state_for_api = state.clone();
            match desktop_api_config() {
                Ok(api_config) => match api_config.mode {
                    DesktopApiMode::External => {
                        diag!(Info, Subsystem::Api,

                            origin = %api_config.origin,
                            "Tauri 桌面端复用外部 Dashboard API"
                        );
                    }
                    DesktopApiMode::Sidecar => start_desktop_api_sidecar(state_for_api, api_config),
                },
                Err(message) => {
                    diag!(
                        Error,
                        Subsystem::Api,
                        operation = "desktop.api.configure",
                        stage = "parse_config",
                        error_kind = "invalid_desktop_api_config",
                        "桌面端 API 配置无效"
                    );
                    let state_for_error = state.clone();
                    tauri::async_runtime::spawn(async move {
                        state_for_error
                            .api
                            .mark_error_origin(desktop_api_origin(DESKTOP_API_PORT), message)
                            .await;
                    });
                }
            }
            initialize_desktop_runner_host(state);
            apply_startup_window_visibility(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            codex_oauth_cancel,
            codex_oauth_start,
            desktop_autostart_is_enabled,
            desktop_autostart_set_enabled,
            desktop_api_snapshot,
            desktop_browse_directory,
            desktop_quit_request,
            desktop_settings_load,
            desktop_settings_save,
            desktop_update_install,
            desktop_update_policy_refresh,
            desktop_update_policy_snapshot,
            profile_delete,
            profile_load,
            profile_save,
            mcp_server_probe,
            mcp_servers_load,
            mcp_servers_save,
            logs_clear,
            logs_tail,
            runtime_restart,
            runtime_start,
            runtime_stop,
            runtime_snapshot,
            open_external_url
        ])
        .build(tauri::generate_context!())
        .expect("启动 AgentDash 桌面端失败")
        .run(move |_app, event| match event {
            RunEvent::ExitRequested { api, .. } if !state_for_exit.is_explicit_quit_requested() => {
                api.prevent_exit();
            }
            RunEvent::Exit => {
                state_for_exit.api.stop_sidecar();
            }
            _ => {}
        });
}

fn configure_tray(app: &AppHandle, state: DesktopState) -> tauri::Result<()> {
    let menu = MenuBuilder::new(app)
        .text(TRAY_MENU_OPEN, "打开 AgentDash")
        .separator()
        .text(TRAY_MENU_RUNTIME_START, "启动本机 runtime")
        .text(TRAY_MENU_RUNTIME_STOP, "停止本机 runtime")
        .text(TRAY_MENU_STATUS, "查看状态")
        .separator()
        .text(TRAY_MENU_QUIT, "退出 AgentDash")
        .build()?;

    let mut tray = TrayIconBuilder::with_id("agentdash-main")
        .tooltip("AgentDash")
        .menu(&menu)
        .show_menu_on_left_click(false);

    if let Some(icon) = app.default_window_icon().cloned() {
        tray = tray.icon(icon);
    }

    let state_for_menu = state.clone();
    tray.on_menu_event(move |app, event| {
        handle_tray_menu_event(app.clone(), state_for_menu.clone(), event.id().as_ref());
    })
    .on_tray_icon_event(|tray, event| {
        if let TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        } = event
        {
            restore_main_window(tray.app_handle());
        }
    })
    .build(app)?;

    Ok(())
}

fn handle_tray_menu_event(app: AppHandle, state: DesktopState, id: &str) {
    match id {
        TRAY_MENU_OPEN => restore_main_window(&app),
        TRAY_MENU_RUNTIME_START => {
            tauri::async_runtime::spawn(async move {
                start_runtime_from_profile(state).await;
            });
        }
        TRAY_MENU_RUNTIME_STOP => {
            tauri::async_runtime::spawn(async move {
                if let Err(error) = state.runtime.stop(StopReason::UserRequested).await {
                    state
                        .runtime
                        .record_log(
                            "error",
                            "runtime",
                            format!("托盘停止本机 runtime 失败: {error}"),
                        )
                        .await;
                }
            });
        }
        TRAY_MENU_STATUS => {
            restore_main_window(&app);
            tauri::async_runtime::spawn(async move {
                record_tray_status(state).await;
            });
        }
        TRAY_MENU_QUIT => {
            tauri::async_runtime::spawn(async move {
                request_desktop_quit(app, state).await;
            });
        }
        _ => {}
    }
}

async fn start_runtime_from_profile(state: DesktopState) {
    let profile = match profile_load().await {
        Ok(Some(profile)) => profile,
        Ok(None) => {
            state
                .runtime
                .record_log(
                    "warn",
                    "profile",
                    "未配置本机 runtime profile，无法从托盘启动 runtime",
                )
                .await;
            return;
        }
        Err(error) => {
            state
                .runtime
                .record_log(
                    "error",
                    "profile",
                    format!("托盘加载本机 runtime profile 失败: {error}"),
                )
                .await;
            return;
        }
    };

    match start_runtime_from_request(&state, RuntimeStartRequest::from(profile), false).await {
        Ok(snapshot) => {
            state
                .runtime
                .record_log(
                    "info",
                    "runtime",
                    format!("托盘已启动本机 runtime: backend={}", snapshot.backend_id),
                )
                .await;
        }
        Err(error) => {
            state
                .runtime
                .record_log(
                    "error",
                    "runtime",
                    format!("托盘启动本机 runtime 失败: {error}"),
                )
                .await;
        }
    }
}

async fn record_tray_status(state: DesktopState) {
    let api = state.api.snapshot().await;
    let runtime = state.runtime.snapshot().await;
    let runtime_message = runtime
        .map(|snapshot| format!("{:?}", snapshot.state))
        .unwrap_or_else(|| "未启动".to_string());
    state
        .runtime
        .record_log(
            "info",
            "desktop",
            format!(
                "托盘状态查看: desktop_api={:?}, runtime={}",
                api.state, runtime_message
            ),
        )
        .await;
}

async fn request_desktop_quit(app: AppHandle, state: DesktopState) {
    state.request_explicit_quit();
    if let Err(error) = state.runtime.stop(StopReason::UserRequested).await {
        let context = DiagnosticErrorContext::new("desktop.lifecycle.quit", "stop_runtime");
        diag_error!(
            Warn,
            Subsystem::Infra,
            context = &context,
            error = &error,
            stop_reason = "user_requested",
            "显式退出前停止本机 runtime 失败"
        );
        state
            .runtime
            .record_log(
                "warn",
                "runtime",
                format!("显式退出前停止本机 runtime 失败: {error}"),
            )
            .await;
    }
    state.api.stop_sidecar();
    app.exit(0);
}

fn apply_startup_window_visibility(app: &AppHandle) {
    let settings = match load_desktop_app_settings() {
        Ok(settings) => settings,
        Err(error) => {
            let context =
                DiagnosticErrorContext::new("desktop.window.startup_visibility", "load_settings");
            diag_error!(
                Warn,
                Subsystem::Infra,
                context = &context,
                error = &error,
                "读取桌面端启动窗口设置失败，使用默认显示行为"
            );
            DesktopAppSettings::default()
        }
    };

    if settings.start_minimized_to_tray {
        if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL)
            && let Err(error) = window.hide()
        {
            let context =
                DiagnosticErrorContext::new("desktop.window.startup_visibility", "hide_window");
            diag_error!(
                Warn,
                Subsystem::Infra,
                context = &context,
                error = &error,
                window_label = MAIN_WINDOW_LABEL,
                "按启动到托盘设置隐藏主窗口失败"
            );
        }
    } else {
        restore_main_window(app);
    }
}

fn restore_main_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) else {
        return;
    };
    if let Err(error) = window.show() {
        let context = DiagnosticErrorContext::new("desktop.window.restore", "show_window");
        diag_error!(
            Warn,
            Subsystem::Infra,
            context = &context,
            error = &error,
            window_label = MAIN_WINDOW_LABEL,
            "显示 AgentDash 主窗口失败"
        );
    }
    if let Err(error) = window.unminimize() {
        let context = DiagnosticErrorContext::new("desktop.window.restore", "unminimize_window");
        diag_error!(
            Warn,
            Subsystem::Infra,
            context = &context,
            error = &error,
            window_label = MAIN_WINDOW_LABEL,
            "还原 AgentDash 主窗口失败"
        );
    }
    if let Err(error) = window.set_focus() {
        let context = DiagnosticErrorContext::new("desktop.window.restore", "focus_window");
        diag_error!(
            Warn,
            Subsystem::Infra,
            context = &context,
            error = &error,
            window_label = MAIN_WINDOW_LABEL,
            "聚焦 AgentDash 主窗口失败"
        );
    }
}

fn start_desktop_api_sidecar(state: DesktopState, config: DesktopApiConfig) {
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
    fn from_snapshot(snapshot: DesktopApiSnapshot) -> Self {
        Self {
            snapshot: Arc::new(AsyncMutex::new(snapshot)),
            sidecar: Arc::new(StdMutex::new(None)),
        }
    }

    async fn snapshot(&self) -> DesktopApiSnapshot {
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

    async fn mark_error_origin(&self, origin: String, message: String) {
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

    fn stop_sidecar(&self) {
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

fn desktop_api_origin(port: u16) -> String {
    format!("http://127.0.0.1:{port}")
}

fn default_desktop_api_snapshot() -> DesktopApiSnapshot {
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
enum DesktopApiMode {
    External,
    Sidecar,
}

#[derive(Debug, Clone)]
struct DesktopApiConfig {
    mode: DesktopApiMode,
    origin: String,
    sidecar: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DesktopApiBuildProfile {
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

fn desktop_api_config() -> Result<DesktopApiConfig, String> {
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

fn desktop_api_config_from_values(
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

fn env_trimmed(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .and_then(normalize_optional_env_text)
}

fn normalize_optional_env_text(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
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

fn mcp_servers_path() -> Result<PathBuf, String> {
    local_mcp_servers_path().map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_contracts::desktop_release::{
        DesktopUpdateDiagnostics, DesktopUpdatePolicy, DesktopUpdateRecommendedVersionSource,
    };

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

    #[test]
    fn windows_autostart_command_quotes_app_exe_path() {
        let command =
            build_windows_autostart_command(Path::new(r"C:\Program Files\AgentDash\AgentDash.exe"))
                .expect("installed app exe path should form a Run key command");

        assert_eq!(command, r#""C:\Program Files\AgentDash\AgentDash.exe""#);
    }

    #[test]
    fn windows_autostart_command_rejects_setup_exe() {
        let error = build_windows_autostart_command(Path::new(
            r"C:\Users\me\Downloads\AgentDash_0.1.0_x64-setup.exe",
        ))
        .expect_err("login autostart must not point at the installer");

        assert!(error.contains("安装器"));
    }

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
        assert!(
            error
                .to_string()
                .contains("低于云端最低要求 0.2.0")
        );
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
