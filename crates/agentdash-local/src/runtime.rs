//! 本机 runtime 组装与生命周期管理。

use agentdash_diagnostics::{Subsystem, diag};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;

use agentdash_application_runtime_session::session::{
    SessionExecutionState, SessionRuntimeServices,
};
use agentdash_executor::connectors::codex_bridge::CodexBridgeConnector;
use agentdash_executor::connectors::composite::CompositeConnector;
use agentdash_infrastructure::PostgresSessionRepository;
use agentdash_infrastructure::postgres_runtime::PostgresRuntime;
use agentdash_spi::AgentConnector;
use anyhow::Context;
use serde::Serialize;
use tokio::sync::{Mutex, watch};

use crate::LocalExtensionHostManager;
use crate::local_backend_config::{self, McpLocalServerEntry};
use crate::mcp_client_manager::{McpClientManager, local_server_to_relay_mcp_server};
use crate::runner_redaction::redact_secret;
use crate::runner_status::RunnerStatusReporter;
use crate::runtime_paths::local_runtime_data_dir;
use crate::tool_executor::ToolExecutor;
use crate::ws_client;

/// 本机 runtime 启动配置。
#[derive(Debug, Clone)]
pub struct LocalRuntimeConfig {
    pub cloud_url: String,
    pub token: String,
    pub backend_id: String,
    pub name: String,
    pub workspace_roots: Vec<PathBuf>,
    pub executor_enabled: bool,
}

impl LocalRuntimeConfig {
    pub fn new(
        cloud_url: String,
        token: String,
        backend_id: String,
        name: String,
        workspace_roots: Vec<PathBuf>,
        executor_enabled: bool,
    ) -> Self {
        Self {
            cloud_url,
            token,
            backend_id,
            name,
            workspace_roots: canonicalize_workspace_roots(workspace_roots),
            executor_enabled,
        }
    }
}

/// 本机 runtime 生命周期状态。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocalRuntimeState {
    Starting,
    Running,
    Stopping,
    Stopped,
    Error,
}

/// Runtime 状态快照，用于 desktop command 或诊断 UI。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct LocalRuntimeStatus {
    pub state: LocalRuntimeState,
    pub backend_id: String,
    pub name: String,
    pub workspace_roots: Vec<String>,
    pub executor_enabled: bool,
    pub mcp_server_count: usize,
    pub message: Option<String>,
    pub relay_connection: Option<ws_client::RelayConnectionStatus>,
}

pub type LocalRuntimeSnapshot = LocalRuntimeStatus;

/// Desktop local 设置页展示的结构化日志事件。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LocalLogEvent {
    pub sequence: u64,
    pub timestamp: String,
    pub level: String,
    pub target: String,
    pub message: String,
}

/// 对已启动 runtime 的只读句柄。
pub struct LocalRuntimeHandle {
    pub backend_id: String,
    pub status_rx: watch::Receiver<LocalRuntimeStatus>,
}

/// 停止 runtime 的原因。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    UserRequested,
    Restart,
    Shutdown,
}

/// MCP server 探测结果，用于桌面端设置页即时反馈。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct McpProbeResult {
    pub ok: bool,
    pub tool_count: usize,
    pub message: String,
}

#[derive(Default, Clone)]
pub struct LocalRuntimeManager {
    inner: Arc<Mutex<Option<RunningRuntime>>>,
    logs: Arc<Mutex<LocalLogBuffer>>,
}

struct RunningRuntime {
    config: LocalRuntimeConfig,
    session_runtime: Option<SessionRuntimeServices>,
    shutdown_tx: watch::Sender<bool>,
    status_tx: watch::Sender<LocalRuntimeStatus>,
    status_rx: watch::Receiver<LocalRuntimeStatus>,
    join: tokio::task::JoinHandle<anyhow::Result<()>>,
}

#[derive(Default)]
struct LocalLogBuffer {
    next_sequence: u64,
    events: VecDeque<LocalLogEvent>,
}

const LOCAL_LOG_CAPACITY: usize = 500;

impl LocalRuntimeManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn start(&self, config: LocalRuntimeConfig) -> anyhow::Result<LocalRuntimeHandle> {
        let mut guard = self.inner.lock().await;
        if guard.is_some() {
            self.record_log("warn", "runtime", "本机 runtime 已经在运行")
                .await;
            anyhow::bail!("本机 runtime 已经在运行");
        }

        self.record_log(
            "info",
            "runtime",
            format!(
                "准备启动 runtime: backend={}, roots={}",
                config.backend_id,
                config.workspace_roots.len()
            ),
        )
        .await;

        let mut ws_config = build_ws_config(&config).await?;
        let initial_relay_status = ws_client::RelayConnectionStatus::not_configured(Some(
            redact_secret(&config.cloud_url),
        ));
        let (relay_status_tx, mut relay_status_rx) = watch::channel(initial_relay_status.clone());
        ws_config.relay_status_tx = Some(relay_status_tx);
        let initial_status = status_from_config(
            &config,
            LocalRuntimeState::Starting,
            None,
            Some(initial_relay_status),
        );
        let (status_tx, status_rx) = watch::channel(initial_status);
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let backend_id = config.backend_id.clone();
        let logs = Arc::clone(&self.logs);
        let session_runtime = ws_config.session_runtime.clone();

        tokio::spawn({
            let status_tx = status_tx.clone();
            async move {
                loop {
                    if relay_status_rx.changed().await.is_err() {
                        break;
                    }
                    let relay_status = relay_status_rx.borrow().clone();
                    let next = status_with_relay(&status_tx.borrow(), Some(relay_status));
                    if status_tx.send(next).is_err() {
                        break;
                    }
                }
            }
        });

        let join = tokio::spawn({
            let status_tx = status_tx.clone();
            async move {
                let running_status =
                    status_from_ws_config(&ws_config, LocalRuntimeState::Running, None)
                        .with_relay(status_tx.borrow().relay_connection.clone());
                let _ = status_tx.send(running_status);
                push_log(
                    &logs,
                    "info",
                    "runtime",
                    format!("runtime 已启动: backend={}", ws_config.backend_id),
                )
                .await;

                let result = ws_client::run_until_shutdown(ws_config, shutdown_rx).await;
                let final_status = match &result {
                    Ok(()) => status_with_state(
                        &status_tx.borrow(),
                        LocalRuntimeState::Stopped,
                        Some("runtime 已停止".to_string()),
                    ),
                    Err(error) => status_with_state(
                        &status_tx.borrow(),
                        LocalRuntimeState::Error,
                        Some(error.to_string()),
                    ),
                };
                let _ = status_tx.send(final_status);
                match &result {
                    Ok(()) => {
                        push_log(&logs, "info", "runtime", "runtime 已停止").await;
                    }
                    Err(error) => {
                        push_log(&logs, "error", "runtime", error.to_string()).await;
                    }
                }
                result
            }
        });

        let handle = LocalRuntimeHandle {
            backend_id: backend_id.clone(),
            status_rx: status_rx.clone(),
        };

        *guard = Some(RunningRuntime {
            config,
            session_runtime,
            shutdown_tx,
            status_tx,
            status_rx,
            join,
        });

        Ok(handle)
    }

    pub async fn stop(&self, _reason: StopReason) -> anyhow::Result<()> {
        let running = {
            let mut guard = self.inner.lock().await;
            guard.take()
        };

        let Some(running) = running else {
            self.record_log("info", "runtime", "runtime 未运行，忽略停止请求")
                .await;
            return Ok(());
        };

        self.record_log("info", "runtime", "收到 runtime 停止请求")
            .await;
        let stopping_status = status_with_state(
            &running.status_rx.borrow(),
            LocalRuntimeState::Stopping,
            Some("正在停止 runtime".to_string()),
        );
        let _ = running.status_tx.send(stopping_status);
        let _ = running.shutdown_tx.send(true);

        running
            .join
            .await
            .map_err(|error| anyhow::anyhow!("runtime task join 失败: {error}"))??;

        Ok(())
    }

    pub async fn restart(&self) -> anyhow::Result<LocalRuntimeSnapshot> {
        let config = {
            let guard = self.inner.lock().await;
            let Some(running) = guard.as_ref() else {
                anyhow::bail!("本机 runtime 未运行");
            };

            let active_sessions = count_active_sessions(running.session_runtime.as_ref()).await?;
            if active_sessions > 0 {
                self.record_log(
                    "warn",
                    "runtime",
                    format!("存在 {active_sessions} 个运行中的 session，已阻止 runtime 重启"),
                )
                .await;
                anyhow::bail!("存在 {active_sessions} 个运行中的 session，暂不重启 runtime");
            }

            running.config.clone()
        };

        self.record_log("info", "runtime", "准备重启 runtime").await;
        self.stop(StopReason::Restart).await?;
        let handle = self.start(config).await?;
        Ok(handle.status_rx.borrow().clone())
    }

    pub async fn record_log(
        &self,
        level: impl Into<String>,
        target: impl Into<String>,
        message: impl Into<String>,
    ) {
        push_log(&self.logs, level, target, message).await;
    }

    pub async fn logs_tail(&self, limit: usize) -> Vec<LocalLogEvent> {
        let limit = limit.clamp(1, LOCAL_LOG_CAPACITY);
        let guard = self.logs.lock().await;
        guard
            .events
            .iter()
            .rev()
            .take(limit)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }

    pub async fn logs_clear(&self) {
        let mut guard = self.logs.lock().await;
        guard.events.clear();
    }

    pub async fn snapshot(&self) -> Option<LocalRuntimeSnapshot> {
        let guard = self.inner.lock().await;
        guard
            .as_ref()
            .map(|running| running.status_rx.borrow().clone())
    }
}

async fn push_log(
    logs: &Arc<Mutex<LocalLogBuffer>>,
    level: impl Into<String>,
    target: impl Into<String>,
    message: impl Into<String>,
) {
    let mut guard = logs.lock().await;
    let event = LocalLogEvent {
        sequence: guard.next_sequence,
        timestamp: chrono::Utc::now().to_rfc3339(),
        level: level.into(),
        target: target.into(),
        message: redact_log_message(&message.into()),
    };
    guard.next_sequence = guard.next_sequence.saturating_add(1);
    guard.events.push_back(event);
    while guard.events.len() > LOCAL_LOG_CAPACITY {
        guard.events.pop_front();
    }
}

async fn count_active_sessions(
    session_runtime: Option<&SessionRuntimeServices>,
) -> anyhow::Result<usize> {
    let Some(session_runtime) = session_runtime else {
        return Ok(0);
    };
    let session_core = &session_runtime.core;

    let sessions = session_core.list_sessions().await?;
    if sessions.is_empty() {
        return Ok(0);
    }

    let ids = sessions
        .into_iter()
        .map(|session| session.id)
        .collect::<Vec<_>>();
    let states = session_core.inspect_execution_states_bulk(&ids).await?;
    Ok(states
        .values()
        .filter(|state| {
            matches!(
                state,
                SessionExecutionState::Running { .. } | SessionExecutionState::Cancelling { .. }
            )
        })
        .count())
}

/// standalone CLI 入口：保持原有无限重连行为。
pub async fn run_standalone(config: LocalRuntimeConfig) -> anyhow::Result<()> {
    run_standalone_with_status(config, None).await
}

pub async fn run_standalone_with_status(
    config: LocalRuntimeConfig,
    runner_status: Option<RunnerStatusReporter>,
) -> anyhow::Result<()> {
    let (_shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    run_standalone_with_status_and_shutdown(config, runner_status, shutdown_rx).await
}

pub async fn run_standalone_with_status_and_shutdown(
    config: LocalRuntimeConfig,
    runner_status: Option<RunnerStatusReporter>,
    shutdown_rx: tokio::sync::watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let mut ws_config = build_ws_config(&config).await?;
    ws_config.runner_status = runner_status;
    tokio::spawn(async move { ws_client::run_until_shutdown(ws_config, shutdown_rx).await })
        .await
        .map_err(|error| anyhow::anyhow!("standalone runtime task join 失败: {error}"))?
}

pub fn canonicalize_workspace_roots(roots: Vec<PathBuf>) -> Vec<PathBuf> {
    roots
        .into_iter()
        .map(|path| {
            std::fs::canonicalize(&path).unwrap_or_else(|_| {
                diag!(Warn, Subsystem::Relay,
        path = %path.display(), "无法规范化路径");
                path
            })
        })
        .collect()
}

pub fn load_mcp_servers_for_root(root: PathBuf) -> anyhow::Result<Vec<McpLocalServerEntry>> {
    let root = canonicalize_existing_root(root)?;
    Ok(local_backend_config::load_local_backend_config_for_root(&root).mcp_servers)
}

pub fn save_mcp_servers_for_root(
    root: PathBuf,
    servers: Vec<McpLocalServerEntry>,
) -> anyhow::Result<()> {
    let root = canonicalize_existing_root(root)?;
    let mut config = local_backend_config::load_local_backend_config_for_root(&root);
    config.mcp_servers = servers;
    local_backend_config::save_local_backend_config_for_root(&root, &config)
}

pub async fn probe_mcp_server(server: McpLocalServerEntry) -> McpProbeResult {
    if server.name.trim().is_empty() {
        return McpProbeResult {
            ok: false,
            tool_count: 0,
            message: "MCP server name 不能为空".to_string(),
        };
    }

    let manager = McpClientManager::new(vec![server.clone()], true);
    let relay_server = local_server_to_relay_mcp_server(&server);
    match manager.list_tools(&relay_server).await {
        Ok(tools) => {
            let tool_count = tools.len();
            let _ = manager.close(&server.name).await;
            McpProbeResult {
                ok: true,
                tool_count,
                message: format!("连接成功，发现 {tool_count} 个工具"),
            }
        }
        Err(error) => {
            let _ = manager.close(&server.name).await;
            McpProbeResult {
                ok: false,
                tool_count: 0,
                message: error.to_string(),
            }
        }
    }
}

async fn build_ws_config(config: &LocalRuntimeConfig) -> anyhow::Result<ws_client::Config> {
    diag!(Info, Subsystem::Relay,

        backend_id = %config.backend_id,
        name = %config.name,
        cloud_url = %redact_secret(&config.cloud_url),
        workspace_roots = ?config.workspace_roots,
        executor_enabled = config.executor_enabled,
        "启动 AgentDash 本机 runtime"
    );

    let tool_executor = ToolExecutor::new(config.workspace_roots.clone());
    let local_backend_config =
        local_backend_config::load_local_backend_config(&config.workspace_roots);

    let mcp_manager = Some(Arc::new(McpClientManager::new(
        local_backend_config.mcp_servers.clone(),
        local_backend_config.mcp_protect_mode,
    )));

    let (session_runtime, connector, session_db_runtime) = if config.executor_enabled {
        let sub_connectors: Vec<Arc<dyn AgentConnector>> =
            vec![Arc::new(CodexBridgeConnector::new())];
        let connector: Arc<dyn AgentConnector> = Arc::new(CompositeConnector::new(sub_connectors));
        let db_runtime = Arc::new(
            PostgresRuntime::resolve_embedded_at_data_root(
                &format!(
                    "agentdash-local-{}",
                    local_runtime_backend_key(&config.backend_id)
                ),
                10,
                local_runtime_data_dir()?,
            )
            .await?,
        );
        agentdash_infrastructure::migration::run_postgres_migrations(&db_runtime.pool).await?;
        let session_repo = Arc::new(PostgresSessionRepository::new(db_runtime.pool.clone()));
        session_repo.initialize().await?;
        let session_runtime = SessionRuntimeServices::new_with_hooks_and_persistence(
            connector.clone(),
            None,
            session_repo,
        );

        if let Err(error) = session_runtime.runtime.recover_interrupted_sessions().await {
            diag!(Warn, Subsystem::Relay,
        error = %error, "启动恢复 session 状态失败（非致命）");
        }

        diag!(Info, Subsystem::Relay, "Session runtime 已初始化");
        (Some(session_runtime), Some(connector), Some(db_runtime))
    } else {
        diag!(Info, Subsystem::Relay, "Session runtime 已禁用");
        (None, None, None)
    };

    Ok(ws_client::Config {
        cloud_url: config.cloud_url.clone(),
        token: config.token.clone(),
        api_base_url: api_base_url_from_cloud_url(&config.cloud_url)?,
        backend_id: config.backend_id.clone(),
        name: config.name.clone(),
        workspace_roots: config.workspace_roots.clone(),
        tool_executor,
        session_runtime,
        connector,
        _session_db_runtime: session_db_runtime,
        mcp_manager,
        workspace_contract_config: local_backend_config.workspace_contract,
        extension_host: LocalExtensionHostManager::with_default_config(),
        extension_artifact_cache_root: local_runtime_data_dir()?
            .join("extension-artifact-cache")
            .join(local_runtime_backend_key(&config.backend_id)),
        runner_status: None,
        relay_status_tx: None,
    })
}

fn canonicalize_existing_root(root: PathBuf) -> anyhow::Result<PathBuf> {
    let root = std::fs::canonicalize(&root)
        .with_context(|| format!("workspace root 不存在或不可访问: {}", root.display()))?;
    if !root.is_dir() {
        anyhow::bail!("workspace root 不是目录: {}", root.display());
    }
    Ok(root)
}

fn local_runtime_backend_key(backend_id: &str) -> String {
    backend_id
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>()
}

fn api_base_url_from_cloud_url(cloud_url: &str) -> anyhow::Result<String> {
    let trimmed = cloud_url.trim();
    let (scheme, rest) = if let Some(rest) = trimmed.strip_prefix("ws://") {
        ("http", rest)
    } else if let Some(rest) = trimmed.strip_prefix("wss://") {
        ("https", rest)
    } else {
        anyhow::bail!("cloud_url 必须使用 ws:// 或 wss://: {cloud_url}");
    };
    let authority = rest
        .split('/')
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("cloud_url 缺少 host: {cloud_url}"))?;
    Ok(format!("{scheme}://{authority}"))
}

fn redact_log_message(message: &str) -> String {
    redact_secret(message)
}

fn status_from_config(
    config: &LocalRuntimeConfig,
    state: LocalRuntimeState,
    message: Option<String>,
    relay_connection: Option<ws_client::RelayConnectionStatus>,
) -> LocalRuntimeStatus {
    LocalRuntimeStatus {
        state,
        backend_id: config.backend_id.clone(),
        name: config.name.clone(),
        workspace_roots: config
            .workspace_roots
            .iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect(),
        executor_enabled: config.executor_enabled,
        mcp_server_count: 0,
        message,
        relay_connection,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn runtime_logs_keep_recent_events_and_redact_tokens() {
        let manager = LocalRuntimeManager::new();
        manager
            .record_log("info", "test", "connecting token=secret123&mode=test")
            .await;

        let logs = manager.logs_tail(10).await;
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].sequence, 0);
        assert_eq!(logs[0].message, "connecting token=***&mode=test");
    }

    #[tokio::test]
    async fn runtime_logs_clear_removes_events() {
        let manager = LocalRuntimeManager::new();
        manager.record_log("info", "test", "one").await;
        manager.logs_clear().await;

        assert!(manager.logs_tail(10).await.is_empty());
    }

    #[test]
    fn runtime_status_state_transition_preserves_relay_snapshot() {
        let config = LocalRuntimeConfig::new(
            "ws://127.0.0.1:17301/ws/backend".to_string(),
            "secret".to_string(),
            "backend-local".to_string(),
            "Desktop Runtime".to_string(),
            Vec::new(),
            true,
        );
        let relay = ws_client::RelayConnectionStatus::not_configured(Some(
            "ws://127.0.0.1:17301/ws/backend".to_string(),
        ));
        let status = status_from_config(
            &config,
            LocalRuntimeState::Starting,
            None,
            Some(relay.clone()),
        );
        let running = status_with_state(&status, LocalRuntimeState::Running, None);

        assert_eq!(running.relay_connection, Some(relay));
    }
}

fn status_from_ws_config(
    config: &ws_client::Config,
    state: LocalRuntimeState,
    message: Option<String>,
) -> LocalRuntimeStatus {
    LocalRuntimeStatus {
        state,
        backend_id: config.backend_id.clone(),
        name: config.name.clone(),
        workspace_roots: config
            .workspace_roots
            .iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect(),
        executor_enabled: config.session_runtime.is_some(),
        mcp_server_count: config
            .mcp_manager
            .as_ref()
            .map(|manager| manager.capability_entries().len())
            .unwrap_or(0),
        message,
        relay_connection: None,
    }
}

fn status_with_state(
    previous: &LocalRuntimeStatus,
    state: LocalRuntimeState,
    message: Option<String>,
) -> LocalRuntimeStatus {
    LocalRuntimeStatus {
        state,
        message,
        ..previous.clone()
    }
}

fn status_with_relay(
    previous: &LocalRuntimeStatus,
    relay_connection: Option<ws_client::RelayConnectionStatus>,
) -> LocalRuntimeStatus {
    LocalRuntimeStatus {
        relay_connection,
        ..previous.clone()
    }
}

trait LocalRuntimeStatusExt {
    fn with_relay(
        self,
        relay_connection: Option<ws_client::RelayConnectionStatus>,
    ) -> LocalRuntimeStatus;
}

impl LocalRuntimeStatusExt for LocalRuntimeStatus {
    fn with_relay(
        self,
        relay_connection: Option<ws_client::RelayConnectionStatus>,
    ) -> LocalRuntimeStatus {
        LocalRuntimeStatus {
            relay_connection,
            ..self
        }
    }
}
