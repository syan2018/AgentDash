use std::path::PathBuf;

use agentdash_local::{LocalRuntimeConfig, LocalRuntimeManager, LocalRuntimeSnapshot, StopReason};
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
async fn runtime_snapshot(
    state: State<'_, DesktopState>,
) -> Result<Option<LocalRuntimeSnapshot>, String> {
    Ok(state.runtime.snapshot().await)
}

fn main() {
    tauri::Builder::default()
        .manage(DesktopState::default())
        .invoke_handler(tauri::generate_handler![
            runtime_start,
            runtime_stop,
            runtime_snapshot
        ])
        .run(tauri::generate_context!())
        .expect("启动 AgentDash 桌面端失败");
}
