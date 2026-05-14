use std::path::PathBuf;
use std::sync::Arc;

use agentdash_api::{ApiServerOptions, ApiServerReady};
use agentdash_local::local_backend_config::McpLocalServerEntry;
use agentdash_local::{
    LocalLogEvent, LocalRuntimeConfig, LocalRuntimeManager, LocalRuntimeSnapshot, McpProbeResult,
    StopReason, load_mcp_servers_for_root, load_or_create_machine_identity, probe_mcp_server,
    save_mcp_servers_for_root,
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use tokio::sync::Mutex;
use tracing_subscriber::EnvFilter;

const DESKTOP_PROFILE_FILE: &str = "desktop-runtime-profile.json";
const DESKTOP_API_PORT: u16 = 3001;
const DESKTOP_API_MODE_ENV: &str = "AGENTDASH_DESKTOP_API_MODE";
const DESKTOP_API_ORIGIN_ENV: &str = "AGENTDASH_DESKTOP_API_ORIGIN";
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
    server_url: String,
    #[serde(default)]
    access_token: String,
    profile_id: String,
    #[serde(default)]
    machine_id: String,
    #[serde(default)]
    machine_label: Option<String>,
    #[serde(default)]
    legacy_machine_ids: Vec<String>,
    name: Option<String>,
    accessible_roots: Vec<PathBuf>,
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
    legacy_machine_ids: Vec<String>,
    #[serde(default, alias = "device_id", skip_serializing)]
    legacy_device_id: String,
    #[serde(default)]
    backend_id: Option<String>,
    #[serde(default)]
    relay_ws_url: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    accessible_roots: Vec<PathBuf>,
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
            legacy_machine_ids: profile.legacy_machine_ids,
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
    let profile = serde_json::from_str(&content)
        .map_err(|error| format!("读取桌面端 profile 失败: {error}"))?;
    Ok(Some(normalize_profile(profile)?))
}

#[tauri::command]
async fn profile_save(
    app: AppHandle,
    profile: LocalRuntimeProfile,
) -> Result<LocalRuntimeProfile, String> {
    let path = profile_path(&app)?;
    let profile = normalize_profile(profile)?;
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
    legacy_machine_ids: Vec<String>,
    profile_id: String,
    scope: LocalRuntimeScopePayload,
    capability_slot: String,
    name: Option<String>,
    accessible_roots: Vec<String>,
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

async fn auto_start_profile(app: AppHandle, state: DesktopState) {
    let profile = match profile_load(app.clone()).await {
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
        request.accessible_roots,
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
        legacy_machine_ids: request.legacy_machine_ids.clone(),
        profile_id: request.profile_id.clone(),
        scope: LocalRuntimeScopePayload {
            kind: "user".to_string(),
            id: None,
        },
        capability_slot: "default".to_string(),
        name: request.name.clone(),
        accessible_roots: request
            .accessible_roots
            .iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect(),
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
                tracing::warn!(
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
    let mut legacy_machine_ids = profile.legacy_machine_ids;
    if let Some(profile_machine_id) = normalize_optional_text(profile.machine_id) {
        if profile_machine_id != identity.machine_id {
            legacy_machine_ids.push(profile_machine_id);
        }
    }
    if !profile.legacy_device_id.trim().is_empty() {
        legacy_machine_ids.push(profile.legacy_device_id);
    }
    legacy_machine_ids.extend(identity.legacy_machine_ids.clone());
    let machine_id = identity.machine_id;
    let machine_label = profile
        .machine_label
        .and_then(normalize_optional_text)
        .unwrap_or(identity.machine_label);
    let legacy_machine_ids = normalize_legacy_machine_ids(legacy_machine_ids, &machine_id);

    Ok(LocalRuntimeProfile {
        server_url: normalize_server_origin(&profile.server_url),
        access_token: profile.access_token.trim().to_string(),
        profile_id: normalize_profile_id(profile.profile_id),
        machine_id,
        machine_label: Some(machine_label),
        legacy_machine_ids,
        legacy_device_id: String::new(),
        name: profile.name.and_then(normalize_optional_text),
        accessible_roots: profile.accessible_roots,
        executor_enabled: profile.executor_enabled,
        auto_start: profile.auto_start,
        backend_id: profile.backend_id.and_then(normalize_optional_text),
        relay_ws_url: profile.relay_ws_url.and_then(normalize_optional_text),
    })
}

fn normalize_start_request(request: RuntimeStartRequest) -> Result<RuntimeStartRequest, String> {
    let identity = load_or_create_machine_identity().map_err(|error| error.to_string())?;
    let mut legacy_machine_ids = request.legacy_machine_ids;
    if let Some(request_machine_id) = normalize_optional_text(request.machine_id) {
        if request_machine_id != identity.machine_id {
            legacy_machine_ids.push(request_machine_id);
        }
    }
    legacy_machine_ids.extend(identity.legacy_machine_ids);
    let machine_id = identity.machine_id;
    let machine_label = request
        .machine_label
        .and_then(normalize_optional_text)
        .unwrap_or(identity.machine_label);
    let legacy_machine_ids = normalize_legacy_machine_ids(legacy_machine_ids, &machine_id);

    Ok(RuntimeStartRequest {
        server_url: normalize_server_origin(&request.server_url),
        access_token: request.access_token.trim().to_string(),
        profile_id: normalize_profile_id(request.profile_id),
        machine_id,
        machine_label: Some(machine_label),
        legacy_machine_ids,
        name: request.name.and_then(normalize_optional_text),
        accessible_roots: request.accessible_roots,
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

fn normalize_legacy_machine_ids(values: Vec<String>, machine_id: &str) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty() && value != machine_id)
        .filter(|value| seen.insert(value.clone()))
        .collect()
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
            let app_handle = app.handle().clone();
            let state = app.state::<DesktopState>().inner().clone();
            tauri::async_runtime::spawn(async move {
                auto_start_profile(app_handle, state).await;
            });
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
            runtime_snapshot,
            open_external_url
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
