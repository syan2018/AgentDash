#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use agentdash_diagnostics::{Subsystem, diag};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};

use agentdash_api::{ApiServerOptions, ApiServerReady};
use agentdash_local::local_backend_config::McpLocalServerEntry;
use agentdash_local::{
    DesktopAppSettings, DesktopEnsureRetryEvent, DesktopEnsureRetryPolicy, DesktopRunnerHost,
    DesktopRuntimeStartRequest as RuntimeStartRequest, LocalLogEvent, LocalRuntimeProfile,
    LocalRuntimeSnapshot, McpProbeResult, StopReason, browse_directory,
    delete_desktop_runtime_profile, ensure_desktop_runtime_config, load_desktop_app_settings,
    load_desktop_runtime_profile_with_server_origin, local_mcp_servers_path,
    normalize_desktop_app_settings, normalize_desktop_runtime_start_request_with_server_origin,
    probe_mcp_server, save_desktop_app_settings, save_desktop_runtime_profile_with_server_origin,
};
use agentdash_relay::BrowseDirectoryEntry;
use serde::Serialize;
use tauri::menu::MenuBuilder;
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, RunEvent, State, WindowEvent};
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
    lifecycle: Arc<DesktopLifecycleState>,
}

impl Default for DesktopState {
    fn default() -> Self {
        Self {
            runtime: DesktopRunnerHost::new(),
            api: DesktopApiManager::from_snapshot(default_desktop_api_snapshot()),
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
async fn desktop_settings_save(settings: DesktopAppSettings) -> Result<DesktopAppSettings, String> {
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
async fn desktop_autostart_set_enabled(enabled: bool) -> Result<DesktopAutostartStatus, String> {
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
async fn profile_save(profile: LocalRuntimeProfile) -> Result<LocalRuntimeProfile, String> {
    save_desktop_runtime_profile_with_server_origin(profile, &desktop_runtime_server_origin())
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn profile_delete() -> Result<(), String> {
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
async fn open_external_url(url: String) -> Result<(), String> {
    let parsed = reqwest::Url::parse(&url).map_err(|error| format!("外部链接无效: {error}"))?;
    match parsed.scheme() {
        "http" | "https" => {}
        scheme => return Err(format!("不支持打开 {scheme}:// 外部链接")),
    }
    open::that(parsed.as_str()).map_err(|error| format!("打开系统浏览器失败: {error}"))
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
    diag!(Warn, Subsystem::Api,
        attempt = event.attempt,
        error = %event.error,
        "领取本机 runtime 失败，等待 server 就绪后重试"
    );
}

fn initialize_desktop_runner_host(state: DesktopState) {
    tauri::async_runtime::spawn(async move {
        let settings = match load_desktop_app_settings() {
            Ok(settings) => settings,
            Err(error) => {
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
                    diag!(Warn, Subsystem::Infra,
                        error = %error,
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
                    DesktopApiMode::Builtin => start_desktop_api(state_for_api),
                    DesktopApiMode::External => {
                        diag!(Info, Subsystem::Api,

                            origin = %api_config.origin,
                            "Tauri 桌面端复用外部 Dashboard API"
                        );
                    }
                    DesktopApiMode::Sidecar => start_desktop_api_sidecar(state_for_api, api_config),
                },
                Err(message) => {
                    diag!(Error, Subsystem::Api,
                        error = %message,
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
            desktop_autostart_is_enabled,
            desktop_autostart_set_enabled,
            desktop_api_snapshot,
            desktop_browse_directory,
            desktop_quit_request,
            desktop_settings_load,
            desktop_settings_save,
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
            RunEvent::ExitRequested { api, .. } => {
                if !state_for_exit.is_explicit_quit_requested() {
                    api.prevent_exit();
                }
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
            diag!(Warn, Subsystem::Infra,
                error = %error,
                "读取桌面端启动窗口设置失败，使用默认显示行为"
            );
            DesktopAppSettings::default()
        }
    };

    if settings.start_minimized_to_tray {
        if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL)
            && let Err(error) = window.hide()
        {
            diag!(Warn, Subsystem::Infra,
                error = %error,
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
        diag!(Warn, Subsystem::Infra,
            error = %error,
            "显示 AgentDash 主窗口失败"
        );
    }
    if let Err(error) = window.unminimize() {
        diag!(Warn, Subsystem::Infra,
            error = %error,
            "还原 AgentDash 主窗口失败"
        );
    }
    if let Err(error) = window.set_focus() {
        diag!(Warn, Subsystem::Infra,
            error = %error,
            "聚焦 AgentDash 主窗口失败"
        );
    }
}

fn start_desktop_api(state: DesktopState) {
    std::thread::Builder::new()
        .name("agentdash-desktop-api".to_string())
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .thread_name("agentdash-desktop-api-worker")
                .build();

            match runtime {
                Ok(runtime) => runtime.block_on(run_desktop_api(state)),
                Err(error) => {
                    diag!(Error, Subsystem::Api,
        error = %error, "创建桌面端 API runtime 失败");
                }
            }
        })
        .expect("启动桌面端 API 线程失败");
}

async fn run_desktop_api(state: DesktopState) {
    state.api.mark_starting(DESKTOP_API_PORT).await;
    let options = ApiServerOptions::desktop_localhost(DESKTOP_API_PORT);
    // 桌面宿主保持原 fmt 订阅器，不接 JSON 文件层 / 诊断缓冲层：传入一个未接订阅器的
    // 空缓冲即可，`/api/diagnostics` 在桌面端返回空集（行为与原先一致）。
    match agentdash_api::build_server_with_migrations(
        agentdash_api::builtin_integrations(),
        options,
        agentdash_api::DiagnosticBuffer::new(0),
    )
    .await
    {
        Ok(server) => {
            let ready = server.ready().clone();
            state.api.mark_running(&ready).await;
            if let Err(error) = server.serve().await {
                diag!(Error, Subsystem::Api,
        error = %error, "桌面端 API 服务退出");
                state
                    .api
                    .mark_error(DESKTOP_API_PORT, error.to_string())
                    .await;
            } else {
                state.api.mark_stopped(DESKTOP_API_PORT).await;
            }
        }
        Err(error) => {
            diag!(Error, Subsystem::Api,
        error = %error, "桌面端 API 启动失败");
            state
                .api
                .mark_error(DESKTOP_API_PORT, error.to_string())
                .await;
        }
    }
}

fn start_desktop_api_sidecar(state: DesktopState, config: DesktopApiConfig) {
    let sidecar = match config.sidecar.as_deref() {
        Some(sidecar) => sidecar,
        None => {
            let origin = config.origin.clone();
            tauri::async_runtime::spawn(async move {
                state
                    .api
                    .mark_error_origin(origin, "未配置桌面端 API sidecar 命令".to_string())
                    .await;
            });
            return;
        }
    };

    diag!(Info, Subsystem::Api,

        origin = %config.origin,
        sidecar = %sidecar,
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

    Command::new(sidecar)
        .env("HOST", host)
        .env("PORT", port.to_string())
        .env(DESKTOP_API_MODE_ENV, "builtin")
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
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

                        attempt,
                        status = %response.status(),
                        "等待桌面端 API sidecar 就绪"
                    );
                }
            }
            Err(error) => {
                if attempt % 20 == 0 {
                    diag!(Warn, Subsystem::Api,

                        attempt,
                        error = %error,
                        "等待桌面端 API sidecar 就绪"
                    );
                }
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }

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

    async fn mark_starting(&self, port: u16) {
        self.mark_starting_origin(
            desktop_api_origin(port),
            "桌面端 API 正在启动".to_string(),
            None,
        )
        .await;
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

    async fn mark_running(&self, ready: &ApiServerReady) {
        self.mark_running_origin(
            ready.origin.clone(),
            format!("桌面端 API 已启动: {}", ready.addr),
            Some(ready.database_url.clone()),
        )
        .await;
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

    async fn mark_error(&self, port: u16, message: String) {
        self.mark_error_origin(desktop_api_origin(port), message)
            .await;
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

    async fn mark_stopped(&self, port: u16) {
        let mut guard = self.snapshot.lock().await;
        *guard = DesktopApiSnapshot {
            state: DesktopApiState::Stopped,
            origin: desktop_api_origin(port),
            message: Some("桌面端 API 已停止".to_string()),
            database_url: None,
        };
    }

    fn store_sidecar(&self, child: Child) {
        match self.sidecar.lock() {
            Ok(mut guard) => {
                *guard = Some(child);
            }
            Err(error) => {
                diag!(Error, Subsystem::Api,
        error = %error, "记录桌面端 API sidecar 句柄失败");
            }
        }
    }

    fn stop_sidecar(&self) {
        let child = match self.sidecar.lock() {
            Ok(mut guard) => guard.take(),
            Err(error) => {
                diag!(Error, Subsystem::Api,
        error = %error, "停止桌面端 API sidecar 时锁已污染");
                None
            }
        };
        if let Some(mut child) = child {
            if let Err(error) = child.kill() {
                diag!(Warn, Subsystem::Api,
        error = %error, "终止桌面端 API sidecar 失败");
            }
            let _ = child.wait();
        }
    }
}

fn desktop_api_origin(port: u16) -> String {
    format!("http://127.0.0.1:{port}")
}

fn default_desktop_api_snapshot() -> DesktopApiSnapshot {
    match desktop_api_config() {
        Ok(config) => match config.mode {
            DesktopApiMode::Builtin => DesktopApiSnapshot::default(),
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
    Builtin,
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

    let explicit_mode = explicit_mode.and_then(|value| {
        parse_desktop_api_mode(&value).or_else(|| {
            diag!(Warn, Subsystem::Api,
        mode = %value, "忽略未知桌面端 API mode");
            None
        })
    });
    let build_default_mode = build_default_mode.and_then(|value| {
        parse_desktop_api_mode(&value).or_else(|| {
            diag!(Warn, Subsystem::Api,
        mode = %value, "忽略未知桌面端默认 API mode");
            None
        })
    });

    let mode = explicit_mode
        .or(build_default_mode)
        .unwrap_or(DesktopApiMode::External);
    let origin = match mode {
        DesktopApiMode::Builtin => desktop_api_origin(DESKTOP_API_PORT),
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

fn parse_desktop_api_mode(value: &str) -> Option<DesktopApiMode> {
    match value.trim().to_ascii_lowercase().as_str() {
        "builtin" => Some(DesktopApiMode::Builtin),
        "external" => Some(DesktopApiMode::External),
        "sidecar" => Some(DesktopApiMode::Sidecar),
        _ => None,
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
    fn builtin_config_keeps_release_origin_fixed() {
        let config = desktop_api_config_from_values(
            Some("http://10.0.0.5:3001".to_string()),
            None,
            None,
            None,
            Some("builtin".to_string()),
            None,
            DesktopApiBuildProfile::Release,
        )
        .expect("builtin origin should be fixed by the desktop API contract");

        assert_eq!(config.mode, DesktopApiMode::Builtin);
        assert_eq!(config.origin, "http://127.0.0.1:17301");
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
            Some("agentdash-api".to_string()),
            None,
            Some("sidecar".to_string()),
            None,
            DesktopApiBuildProfile::Release,
        )
        .expect("release sidecar may use the fixed Desktop API origin");

        assert_eq!(config.mode, DesktopApiMode::Sidecar);
        assert_eq!(config.origin, "http://127.0.0.1:17301");
        assert_eq!(config.sidecar.as_deref(), Some("agentdash-api"));
    }

    #[test]
    fn sidecar_origin_never_binds_non_loopback_host() {
        let error = desktop_api_config_from_values(
            Some("http://0.0.0.0:17301".to_string()),
            None,
            Some("agentdash-api".to_string()),
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
}
