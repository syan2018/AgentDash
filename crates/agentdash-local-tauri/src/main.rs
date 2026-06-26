use agentdash_diagnostics::{diag, Subsystem};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex as StdMutex};

use agentdash_api::{ApiServerOptions, ApiServerReady};
use agentdash_local::local_backend_config::McpLocalServerEntry;
use agentdash_local::{
    LocalLogEvent, LocalRuntimeConfig, LocalRuntimeManager, LocalRuntimeSnapshot, McpProbeResult,
    StopReason, browse_directory, load_or_create_machine_identity, local_mcp_servers_path,
    local_runtime_profile_path, probe_mcp_server,
};
use agentdash_relay::BrowseDirectoryEntry;
use serde::{Deserialize, Serialize};
use tauri::{Manager, RunEvent, State};
use tokio::sync::Mutex as AsyncMutex;
use tracing_subscriber::EnvFilter;

const DESKTOP_API_PORT: u16 = 3001;
const DESKTOP_API_MODE_ENV: &str = "AGENTDASH_DESKTOP_API_MODE";
const DESKTOP_API_ORIGIN_ENV: &str = "AGENTDASH_DESKTOP_API_ORIGIN";
const DESKTOP_API_SIDECAR_ENV: &str = "AGENTDASH_DESKTOP_API_SIDECAR";
const DEFAULT_PROFILE_ID: &str = "default";

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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct RuntimeStartRequest {
    server_url: String,
    #[serde(default)]
    access_token: String,
    profile_id: String,
    #[serde(default)]
    machine_id: String,
    #[serde(default)]
    machine_label: Option<String>,
    name: Option<String>,
    #[serde(default)]
    workspace_roots: Vec<PathBuf>,
    executor_enabled: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
struct LocalRuntimeProfile {
    server_url: String,
    #[serde(default)]
    access_token: String,
    #[serde(default = "default_profile_id")]
    profile_id: String,
    #[serde(default)]
    machine_id: String,
    #[serde(default)]
    machine_label: Option<String>,
    #[serde(default)]
    backend_id: Option<String>,
    #[serde(default)]
    relay_ws_url: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    workspace_roots: Vec<PathBuf>,
    #[serde(default = "default_executor_enabled")]
    executor_enabled: bool,
    #[serde(default)]
    auto_start: bool,
}

fn default_profile_id() -> String {
    DEFAULT_PROFILE_ID.to_string()
}

fn default_executor_enabled() -> bool {
    true
}

impl From<LocalRuntimeProfile> for RuntimeStartRequest {
    fn from(profile: LocalRuntimeProfile) -> Self {
        Self {
            server_url: profile.server_url,
            access_token: profile.access_token,
            profile_id: profile.profile_id,
            machine_id: profile.machine_id,
            machine_label: profile.machine_label,
            name: profile.name,
            workspace_roots: profile.workspace_roots,
            executor_enabled: profile.executor_enabled,
        }
    }
}

#[tauri::command]
async fn profile_load() -> Result<Option<LocalRuntimeProfile>, String> {
    let path = profile_path()?;
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&path).map_err(|error| error.to_string())?;
    let profile = serde_json::from_str(&content)
        .map_err(|error| format!("读取桌面端 profile 失败: {error}"))?;
    Ok(Some(normalize_profile(profile)?))
}

#[tauri::command]
async fn profile_save(profile: LocalRuntimeProfile) -> Result<LocalRuntimeProfile, String> {
    let path = profile_path()?;
    let profile = normalize_profile(profile)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let content = serde_json::to_string_pretty(&profile).map_err(|error| error.to_string())?;
    std::fs::write(&path, content).map_err(|error| error.to_string())?;
    Ok(profile)
}

#[tauri::command]
async fn profile_delete() -> Result<(), String> {
    let path = profile_path()?;
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

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
struct EnsureLocalRuntimePayload {
    machine_id: String,
    machine_label: Option<String>,
    profile_id: String,
    scope: LocalRuntimeScopePayload,
    capability_slot: String,
    name: Option<String>,
    executor_enabled: bool,
    client_version: Option<String>,
    device: serde_json::Value,
    rotate_token: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
struct LocalRuntimeScopePayload {
    kind: String,
    id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct EnsureLocalRuntimeResponse {
    backend_id: String,
    name: String,
    relay_ws_url: String,
    auth_token: String,
    machine_id: String,
    machine_label: String,
    share_scope_kind: String,
    share_scope_id: Option<String>,
    capability_slot: String,
}

async fn auto_start_profile(state: DesktopState) {
    let profile = match profile_load().await {
        Ok(Some(profile)) if profile.auto_start => profile,
        Ok(_) => return,
        Err(error) => {
            state
                .runtime
                .record_log(
                    "warn",
                    "profile",
                    format!("加载 auto-start profile 失败: {error}"),
                )
                .await;
            return;
        }
    };

    let request = RuntimeStartRequest::from(profile);
    state
        .runtime
        .record_log(
            "info",
            "profile",
            "检测到 auto-start profile，准备连接 server",
        )
        .await;

    match start_runtime_from_request(&state, request, true).await {
        Ok(snapshot) => {
            state
                .runtime
                .record_log(
                    "info",
                    "profile",
                    format!("auto-start runtime 已启动: backend={}", snapshot.backend_id),
                )
                .await;
        }
        Err(error) => {
            state
                .runtime
                .record_log(
                    "error",
                    "profile",
                    format!("auto-start runtime 失败: {error}"),
                )
                .await;
        }
    }
}

async fn start_runtime_from_request(
    state: &DesktopState,
    request: RuntimeStartRequest,
    retry_until_server_ready: bool,
) -> anyhow::Result<LocalRuntimeSnapshot> {
    let request = normalize_start_request(request).map_err(anyhow::Error::msg)?;
    let claim = claim_local_runtime(&request, retry_until_server_ready).await?;
    let config = LocalRuntimeConfig::new(
        claim.relay_ws_url,
        claim.auth_token,
        claim.backend_id,
        claim.name,
        request.workspace_roots,
        request.executor_enabled,
    );

    let handle = state.runtime.start(config).await?;
    Ok(handle.status_rx.borrow().clone())
}

async fn claim_local_runtime(
    request: &RuntimeStartRequest,
    retry_until_server_ready: bool,
) -> anyhow::Result<EnsureLocalRuntimeResponse> {
    let server_url = normalize_server_origin(&request.server_url);
    let payload = EnsureLocalRuntimePayload {
        machine_id: request.machine_id.clone(),
        machine_label: request.machine_label.clone(),
        profile_id: request.profile_id.clone(),
        scope: LocalRuntimeScopePayload {
            kind: "user".to_string(),
            id: None,
        },
        capability_slot: "default".to_string(),
        name: request.name.clone(),
        executor_enabled: request.executor_enabled,
        client_version: Some(env!("CARGO_PKG_VERSION").to_string()),
        device: local_device_payload(),
        rotate_token: false,
    };

    let mut attempts = 0;
    loop {
        attempts += 1;
        match post_local_runtime_claim(&server_url, &request.access_token, &payload).await {
            Ok(response) => {
                validate_claim_response(&response, request)?;
                return Ok(response);
            }
            Err(error) if retry_until_server_ready && attempts < 30 => {
                diag!(Warn, Subsystem::Api,
        
                    attempt = attempts,
                    error = %error,
                    "领取本机 runtime 失败，等待 server 就绪后重试"
                );
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
            Err(error) => return Err(error),
        }
    }
}

fn validate_claim_response(
    response: &EnsureLocalRuntimeResponse,
    request: &RuntimeStartRequest,
) -> anyhow::Result<()> {
    if response.machine_id != request.machine_id {
        anyhow::bail!(
            "server 返回的 machine_id 与本机 profile 不一致: expected={}, actual={}",
            request.machine_id,
            response.machine_id
        );
    }
    if response.machine_label.trim().is_empty() {
        anyhow::bail!("server 返回的 machine_label 为空");
    }
    if response.share_scope_kind != "user" {
        anyhow::bail!(
            "当前桌面端只支持 personal runtime scope，server 返回: {}",
            response.share_scope_kind
        );
    }
    if response.capability_slot != "default" {
        anyhow::bail!(
            "当前桌面端只支持 default capability slot，server 返回: {}",
            response.capability_slot
        );
    }
    let _personal_scope = response.share_scope_id.as_deref().unwrap_or("current-user");
    Ok(())
}

async fn post_local_runtime_claim(
    server_url: &str,
    access_token: &str,
    payload: &EnsureLocalRuntimePayload,
) -> anyhow::Result<EnsureLocalRuntimeResponse> {
    let endpoint = format!("{server_url}/api/local-runtime/ensure");
    let client = reqwest::Client::new();
    let mut request = client.post(&endpoint);
    let access_token = access_token.trim();
    if !access_token.is_empty() {
        request = request.bearer_auth(access_token);
    }

    let response = request
        .json(payload)
        .send()
        .await
        .map_err(|error| anyhow::anyhow!("请求本机 runtime 领取接口失败: {error}"))?;

    let status = response.status();
    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        anyhow::bail!("本机 runtime 领取失败: HTTP {status} {text}");
    }

    response
        .json::<EnsureLocalRuntimeResponse>()
        .await
        .map_err(|error| anyhow::anyhow!("解析本机 runtime 领取响应失败: {error}"))
}

fn normalize_profile(profile: LocalRuntimeProfile) -> Result<LocalRuntimeProfile, String> {
    let identity = load_or_create_machine_identity().map_err(|error| error.to_string())?;
    let machine_id = identity.machine_id;
    let machine_label = profile
        .machine_label
        .and_then(normalize_optional_text)
        .unwrap_or(identity.machine_label);

    Ok(LocalRuntimeProfile {
        server_url: normalize_server_origin(&profile.server_url),
        access_token: profile.access_token.trim().to_string(),
        profile_id: normalize_profile_id(profile.profile_id),
        machine_id,
        machine_label: Some(machine_label),
        name: profile.name.and_then(normalize_optional_text),
        workspace_roots: profile.workspace_roots,
        executor_enabled: profile.executor_enabled,
        auto_start: profile.auto_start,
        backend_id: profile.backend_id.and_then(normalize_optional_text),
        relay_ws_url: profile.relay_ws_url.and_then(normalize_optional_text),
    })
}

fn normalize_start_request(request: RuntimeStartRequest) -> Result<RuntimeStartRequest, String> {
    let identity = load_or_create_machine_identity().map_err(|error| error.to_string())?;
    let machine_id = identity.machine_id;
    let machine_label = request
        .machine_label
        .and_then(normalize_optional_text)
        .unwrap_or(identity.machine_label);

    Ok(RuntimeStartRequest {
        server_url: normalize_server_origin(&request.server_url),
        access_token: request.access_token.trim().to_string(),
        profile_id: normalize_profile_id(request.profile_id),
        machine_id,
        machine_label: Some(machine_label),
        name: request.name.and_then(normalize_optional_text),
        workspace_roots: request.workspace_roots,
        executor_enabled: request.executor_enabled,
    })
}

fn normalize_server_origin(value: &str) -> String {
    let trimmed = value.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        desktop_api_origin(DESKTOP_API_PORT)
    } else {
        trimmed.to_string()
    }
}

fn normalize_profile_id(value: String) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        DEFAULT_PROFILE_ID.to_string()
    } else {
        trimmed.to_string()
    }
}

fn normalize_optional_text(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn local_device_payload() -> serde_json::Value {
    serde_json::json!({
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "family": std::env::consts::FAMILY,
        "hostname": local_hostname(),
    })
}

fn local_hostname() -> Option<String> {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn main() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .try_init();

    let state = DesktopState::default();
    let state_for_exit = state.clone();

    tauri::Builder::default()
        .manage(state)
        .setup(|app| {
            let state = app.state::<DesktopState>().inner().clone();
            let api_config = desktop_api_config();
            match api_config.mode {
                DesktopApiMode::Builtin => start_desktop_api(state),
                DesktopApiMode::External => {
                    diag!(Info, Subsystem::Api,
        
                        origin = %api_config.origin,
                        "Tauri 桌面端复用外部 Dashboard API"
                    );
                }
                DesktopApiMode::Sidecar => start_desktop_api_sidecar(state, api_config),
            }
            let state = app.state::<DesktopState>().inner().clone();
            tauri::async_runtime::spawn(async move {
                auto_start_profile(state).await;
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            desktop_api_snapshot,
            desktop_browse_directory,
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
            RunEvent::Exit | RunEvent::ExitRequested { .. } => {
                state_for_exit.api.stop_sidecar();
            }
            _ => {}
        });
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
    match agentdash_api::build_server(
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
    let config = desktop_api_config();
    match config.mode {
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

fn desktop_api_config() -> DesktopApiConfig {
    let explicit_origin = env_trimmed(DESKTOP_API_ORIGIN_ENV).map(normalize_origin);
    let build_default_origin = option_env!("AGENTDASH_DESKTOP_DEFAULT_API_ORIGIN")
        .and_then(|value| normalize_optional_text(value.to_string()))
        .map(normalize_origin);
    let origin = explicit_origin
        .or(build_default_origin)
        .unwrap_or_else(|| desktop_api_origin(DESKTOP_API_PORT));

    let explicit_sidecar = env_trimmed(DESKTOP_API_SIDECAR_ENV);
    let build_default_sidecar = option_env!("AGENTDASH_DESKTOP_DEFAULT_API_SIDECAR")
        .and_then(|value| normalize_optional_text(value.to_string()));
    let sidecar = explicit_sidecar.or(build_default_sidecar);

    let explicit_mode = env_trimmed(DESKTOP_API_MODE_ENV).and_then(|value| {
        parse_desktop_api_mode(&value).or_else(|| {
            diag!(Warn, Subsystem::Api,
        mode = %value, "忽略未知桌面端 API mode");
            None
        })
    });
    let build_default_mode = option_env!("AGENTDASH_DESKTOP_DEFAULT_API_MODE")
        .and_then(|value| normalize_optional_text(value.to_string()))
        .and_then(|value| {
            parse_desktop_api_mode(&value).or_else(|| {
                diag!(Warn, Subsystem::Api,
        mode = %value, "忽略未知桌面端默认 API mode");
                None
            })
        });

    let mode = explicit_mode
        .or(build_default_mode)
        .unwrap_or(DesktopApiMode::Builtin);

    DesktopApiConfig {
        mode,
        origin,
        sidecar,
    }
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
    std::env::var(key).ok().and_then(normalize_optional_text)
}

fn normalize_origin(value: String) -> String {
    let trimmed = value.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        desktop_api_origin(DESKTOP_API_PORT)
    } else {
        trimmed.to_string()
    }
}

fn profile_path() -> Result<PathBuf, String> {
    local_runtime_profile_path().map_err(|error| error.to_string())
}

fn mcp_servers_path() -> Result<PathBuf, String> {
    local_mcp_servers_path().map_err(|error| error.to_string())
}
