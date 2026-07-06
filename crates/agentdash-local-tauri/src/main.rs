#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod autostart;
mod codex_oauth;
mod desktop_api;
mod desktop_tray;
mod desktop_update;
mod runtime_host;
mod settings;
mod state;

use std::path::PathBuf;

use agentdash_diagnostics::{DiagnosticErrorContext, Subsystem, diag, diag_error};
use agentdash_local::local_backend_config::McpLocalServerEntry;
use agentdash_local::{
    DesktopAppSettings, DesktopRuntimeStartRequest as RuntimeStartRequest, LocalLogEvent,
    LocalRuntimeProfile, LocalRuntimeSnapshot, McpProbeResult, StopReason, browse_directory,
    delete_desktop_runtime_profile, load_desktop_app_settings,
    load_desktop_runtime_profile_with_server_origin, local_mcp_servers_path,
    normalize_desktop_app_settings, probe_mcp_server, save_desktop_app_settings,
    save_desktop_runtime_profile_with_server_origin,
};
use agentdash_relay::BrowseDirectoryEntry;
use autostart::{
    DesktopAutostartStatus, desktop_autostart_is_enabled_internal,
    desktop_autostart_set_enabled_internal,
};
use codex_oauth::{codex_oauth_cancel, codex_oauth_start};
use desktop_api::{
    DESKTOP_API_PORT, DesktopApiMode, DesktopApiSnapshot, desktop_api_config, desktop_api_origin,
    desktop_runtime_server_origin, start_desktop_api_sidecar,
};
use desktop_tray::{
    MAIN_WINDOW_LABEL, apply_startup_window_visibility, configure_tray, request_desktop_quit,
    restore_main_window,
};
use desktop_update::{
    DesktopUpdateInstallResult, DesktopUpdatePolicySnapshot, ensure_desktop_update_allows_mutation,
    ensure_desktop_update_allows_runtime, install_desktop_update, refresh_desktop_update_policy,
};
use runtime_host::{initialize_desktop_runner_host, start_runtime_from_request};
use serde::Serialize;
use state::DesktopState;
use tauri::{AppHandle, Manager, RunEvent, State, WindowEvent};
use tracing_subscriber::EnvFilter;

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
    settings.launch_at_login = autostart.is_enabled();
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
    settings.launch_at_login = status.is_enabled();
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
    install_desktop_update(app).await
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

fn mcp_servers_path() -> Result<PathBuf, String> {
    local_mcp_servers_path().map_err(|error| error.to_string())
}
