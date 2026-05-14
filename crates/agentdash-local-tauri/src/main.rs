use std::path::PathBuf;
use std::sync::Arc;

use agentdash_api::{ApiServerOptions, ApiServerReady};
use agentdash_local::local_backend_config::McpLocalServerEntry;
use agentdash_local::{
    LocalLogEvent, LocalRuntimeConfig, LocalRuntimeManager, LocalRuntimeSnapshot, McpProbeResult,
    StopReason, load_mcp_servers_for_root, probe_mcp_server, save_mcp_servers_for_root,
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use tokio::sync::Mutex;
use tracing_subscriber::EnvFilter;

const DESKTOP_PROFILE_FILE: &str = "desktop-runtime-profile.json";
const DESKTOP_API_PORT: u16 = 3001;
const DESKTOP_API_MODE_ENV: &str = "AGENTDASH_DESKTOP_API_MODE";
const DESKTOP_API_ORIGIN_ENV: &str = "AGENTDASH_DESKTOP_API_ORIGIN";

#[derive(Clone)]
struct DesktopState {
    runtime: LocalRuntimeManager,
    api: DesktopApiManager,
}

impl Default for DesktopState {
    fn default() -> Self {
        Self {
            runtime: LocalRuntimeManager::new(),
            api: DesktopApiManager::from_snapshot(default_desktop_api_snapshot()),
        }
    }
}

#[derive(Clone, Default)]
struct DesktopApiManager {
    snapshot: Arc<Mutex<DesktopApiSnapshot>>,
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

#[tauri::command]
async fn desktop_api_snapshot(
    state: State<'_, DesktopState>,
) -> Result<DesktopApiSnapshot, String> {
    Ok(state.api.snapshot().await)
}

fn main() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .try_init();

    tauri::Builder::default()
        .manage(DesktopState::default())
        .setup(|app| {
            let state = app.state::<DesktopState>().inner().clone();
            if let Some(origin) = external_desktop_api_origin() {
                tracing::info!(
                    origin = %origin,
                    "Tauri 桌面端复用外部 Dashboard API"
                );
            } else {
                start_desktop_api(state);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            desktop_api_snapshot,
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
                    tracing::error!(error = %error, "创建桌面端 API runtime 失败");
                }
            }
        })
        .expect("启动桌面端 API 线程失败");
}

async fn run_desktop_api(state: DesktopState) {
    state.api.mark_starting(DESKTOP_API_PORT).await;
    let options = ApiServerOptions::desktop_localhost(DESKTOP_API_PORT);
    match agentdash_api::build_server(agentdash_api::builtin_plugins(), options).await {
        Ok(server) => {
            let ready = server.ready().clone();
            state.api.mark_running(&ready).await;
            if let Err(error) = server.serve().await {
                tracing::error!(error = %error, "桌面端 API 服务退出");
                state
                    .api
                    .mark_error(DESKTOP_API_PORT, error.to_string())
                    .await;
            } else {
                state.api.mark_stopped(DESKTOP_API_PORT).await;
            }
        }
        Err(error) => {
            tracing::error!(error = %error, "桌面端 API 启动失败");
            state
                .api
                .mark_error(DESKTOP_API_PORT, error.to_string())
                .await;
        }
    }
}

impl DesktopApiManager {
    fn from_snapshot(snapshot: DesktopApiSnapshot) -> Self {
        Self {
            snapshot: Arc::new(Mutex::new(snapshot)),
        }
    }

    async fn snapshot(&self) -> DesktopApiSnapshot {
        self.snapshot.lock().await.clone()
    }

    async fn mark_starting(&self, port: u16) {
        let mut guard = self.snapshot.lock().await;
        *guard = DesktopApiSnapshot {
            state: DesktopApiState::Starting,
            origin: desktop_api_origin(port),
            message: Some("桌面端 API 正在启动".to_string()),
            database_url: None,
        };
    }

    async fn mark_running(&self, ready: &ApiServerReady) {
        let mut guard = self.snapshot.lock().await;
        *guard = DesktopApiSnapshot {
            state: DesktopApiState::Running,
            origin: ready.origin.clone(),
            message: Some(format!("桌面端 API 已启动: {}", ready.addr)),
            database_url: Some(ready.database_url.clone()),
        };
    }

    async fn mark_error(&self, port: u16, message: String) {
        let mut guard = self.snapshot.lock().await;
        *guard = DesktopApiSnapshot {
            state: DesktopApiState::Error,
            origin: desktop_api_origin(port),
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
}

fn desktop_api_origin(port: u16) -> String {
    format!("http://127.0.0.1:{port}")
}

fn default_desktop_api_snapshot() -> DesktopApiSnapshot {
    if let Some(origin) = external_desktop_api_origin() {
        return DesktopApiSnapshot {
            state: DesktopApiState::Running,
            origin: origin.clone(),
            message: Some(format!("复用外部 Dashboard API: {origin}")),
            database_url: None,
        };
    }
    DesktopApiSnapshot::default()
}

fn external_desktop_api_origin() -> Option<String> {
    let explicit_origin = std::env::var(DESKTOP_API_ORIGIN_ENV)
        .ok()
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty());
    if explicit_origin.is_some() {
        return explicit_origin;
    }

    let mode = std::env::var(DESKTOP_API_MODE_ENV)
        .ok()
        .map(|value| value.trim().to_ascii_lowercase())
        .unwrap_or_default();
    if mode == "external" {
        return Some(desktop_api_origin(DESKTOP_API_PORT));
    }

    None
}

fn profile_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|dir| dir.join(DESKTOP_PROFILE_FILE))
        .map_err(|error| format!("无法定位桌面端配置目录: {error}"))
}
