use std::path::PathBuf;

use agentdash_local::local_backend_config::McpLocalServerEntry;
use agentdash_local::{
    LocalLogEvent, LocalRuntimeConfig, LocalRuntimeManager, LocalRuntimeSnapshot, McpProbeResult,
    StopReason, load_mcp_servers_for_root, probe_mcp_server, save_mcp_servers_for_root,
};
use serde::Deserialize;
use tauri::State;

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
