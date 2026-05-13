use std::path::PathBuf;

use agentdash_local::local_backend_config::McpLocalServerEntry;
use agentdash_local::{
    LocalLogEvent, LocalRuntimeConfig, LocalRuntimeManager, LocalRuntimeSnapshot, McpProbeResult,
    StopReason, load_mcp_servers_for_root, probe_mcp_server, save_mcp_servers_for_root,
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

const DESKTOP_PROFILE_FILE: &str = "desktop-runtime-profile.json";

#[derive(Clone)]
struct DesktopState {
    runtime: LocalRuntimeManager,
}

impl Default for DesktopState {
    fn default() -> Self {
        Self {
            runtime: LocalRuntimeManager::new(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct RuntimeStartRequest {
    cloud_url: String,
    token: String,
    backend_id: Option<String>,
    name: Option<String>,
    accessible_roots: Vec<PathBuf>,
    executor_enabled: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
struct LocalRuntimeProfile {
    cloud_url: String,
    token: String,
    #[serde(default)]
    backend_id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    accessible_roots: Vec<PathBuf>,
    #[serde(default = "default_executor_enabled")]
    executor_enabled: bool,
    #[serde(default)]
    auto_start: bool,
}

fn default_executor_enabled() -> bool {
    true
}

impl From<LocalRuntimeProfile> for RuntimeStartRequest {
    fn from(profile: LocalRuntimeProfile) -> Self {
        Self {
            cloud_url: profile.cloud_url,
            token: profile.token,
            backend_id: profile.backend_id,
            name: profile.name,
            accessible_roots: profile.accessible_roots,
            executor_enabled: profile.executor_enabled,
        }
    }
}

#[tauri::command]
async fn profile_load(app: AppHandle) -> Result<Option<LocalRuntimeProfile>, String> {
    let path = profile_path(&app)?;
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&path).map_err(|error| error.to_string())?;
    serde_json::from_str(&content)
        .map(Some)
        .map_err(|error| format!("读取桌面端 profile 失败: {error}"))
}

#[tauri::command]
async fn profile_save(
    app: AppHandle,
    profile: LocalRuntimeProfile,
) -> Result<LocalRuntimeProfile, String> {
    let path = profile_path(&app)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let content = serde_json::to_string_pretty(&profile).map_err(|error| error.to_string())?;
    std::fs::write(&path, content).map_err(|error| error.to_string())?;
    Ok(profile)
}

#[tauri::command]
async fn profile_delete(app: AppHandle) -> Result<(), String> {
    let path = profile_path(&app)?;
    if !path.exists() {
        return Ok(());
    }
    std::fs::remove_file(path).map_err(|error| error.to_string())
}

#[tauri::command]
async fn runtime_start(
    state: State<'_, DesktopState>,
    request: RuntimeStartRequest,
) -> Result<LocalRuntimeSnapshot, String> {
    let backend_id = request
        .backend_id
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let config = LocalRuntimeConfig::new(
        request.cloud_url,
        request.token,
        backend_id,
        request
            .name
            .unwrap_or_else(|| "desktop-local-backend".to_string()),
        request.accessible_roots,
        request.executor_enabled,
    );

    let handle = state
        .runtime
        .start(config)
        .await
        .map_err(|error| error.to_string())?;

    Ok(handle.status_rx.borrow().clone())
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
    root: PathBuf,
) -> Result<Vec<McpLocalServerEntry>, String> {
    state
        .runtime
        .record_log(
            "info",
            "mcp",
            format!("加载 MCP servers: root={}", root.display()),
        )
        .await;
    load_mcp_servers_for_root(root).map_err(|error| error.to_string())
}

#[tauri::command]
async fn mcp_servers_save(
    state: State<'_, DesktopState>,
    root: PathBuf,
    servers: Vec<McpLocalServerEntry>,
) -> Result<(), String> {
    state
        .runtime
        .record_log(
            "info",
            "mcp",
            format!(
                "保存 MCP servers: root={}, count={}",
                root.display(),
                servers.len()
            ),
        )
        .await;
    save_mcp_servers_for_root(root, servers).map_err(|error| error.to_string())
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

fn main() {
    tauri::Builder::default()
        .manage(DesktopState::default())
        .invoke_handler(tauri::generate_handler![
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
            runtime_snapshot
        ])
        .run(tauri::generate_context!())
        .expect("启动 AgentDash 桌面端失败");
}

fn profile_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|dir| dir.join(DESKTOP_PROFILE_FILE))
        .map_err(|error| format!("无法定位桌面端配置目录: {error}"))
}
