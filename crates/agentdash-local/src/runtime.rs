//! 本机 runtime 组装与生命周期管理。

use std::path::PathBuf;
use std::sync::Arc;

use agentdash_application::session::SessionHub;
use agentdash_executor::connectors::codex_bridge::CodexBridgeConnector;
use agentdash_executor::connectors::composite::CompositeConnector;
use agentdash_executor::connectors::vibe_kanban::VibeKanbanExecutorsConnector;
use agentdash_infrastructure::SqliteSessionRepository;
use agentdash_spi::AgentConnector;
use serde::Serialize;
use sqlx::sqlite::SqliteConnectOptions;
use tokio::sync::{Mutex, watch};

use crate::local_backend_config;
use crate::mcp_client_manager::McpClientManager;
use crate::tool_executor::ToolExecutor;
use crate::ws_client;

/// 本机 runtime 启动配置。
#[derive(Debug, Clone)]
pub struct LocalRuntimeConfig {
    pub cloud_url: String,
    pub token: String,
    pub backend_id: String,
    pub name: String,
    pub accessible_roots: Vec<PathBuf>,
    pub executor_enabled: bool,
}

impl LocalRuntimeConfig {
    pub fn new(
        cloud_url: String,
        token: String,
        backend_id: String,
        name: String,
        accessible_roots: Vec<PathBuf>,
        executor_enabled: bool,
    ) -> Self {
        Self {
            cloud_url,
            token,
            backend_id,
            name,
            accessible_roots: canonicalize_accessible_roots(accessible_roots),
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
    pub accessible_roots: Vec<String>,
    pub executor_enabled: bool,
    pub mcp_server_count: usize,
    pub message: Option<String>,
}

pub type LocalRuntimeSnapshot = LocalRuntimeStatus;

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

#[derive(Default, Clone)]
pub struct LocalRuntimeManager {
    inner: Arc<Mutex<Option<RunningRuntime>>>,
}

struct RunningRuntime {
    shutdown_tx: watch::Sender<bool>,
    status_tx: watch::Sender<LocalRuntimeStatus>,
    status_rx: watch::Receiver<LocalRuntimeStatus>,
    join: tokio::task::JoinHandle<anyhow::Result<()>>,
}

impl LocalRuntimeManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn start(&self, config: LocalRuntimeConfig) -> anyhow::Result<LocalRuntimeHandle> {
        let mut guard = self.inner.lock().await;
        if guard.is_some() {
            anyhow::bail!("本机 runtime 已经在运行");
        }

        let ws_config = build_ws_config(&config).await?;
        let initial_status = status_from_config(&config, LocalRuntimeState::Starting, None);
        let (status_tx, status_rx) = watch::channel(initial_status);
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let backend_id = config.backend_id.clone();

        let join = tokio::spawn({
            let status_tx = status_tx.clone();
            async move {
                let running_status =
                    status_from_ws_config(&ws_config, LocalRuntimeState::Running, None);
                let _ = status_tx.send(running_status);

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
                result
            }
        });

        let handle = LocalRuntimeHandle {
            backend_id: backend_id.clone(),
            status_rx: status_rx.clone(),
        };

        *guard = Some(RunningRuntime {
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
            return Ok(());
        };

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

    pub async fn snapshot(&self) -> Option<LocalRuntimeSnapshot> {
        let guard = self.inner.lock().await;
        guard
            .as_ref()
            .map(|running| running.status_rx.borrow().clone())
    }
}

/// standalone CLI 入口：保持原有无限重连行为。
pub async fn run_standalone(config: LocalRuntimeConfig) -> anyhow::Result<()> {
    let ws_config = build_ws_config(&config).await?;
    ws_client::run(ws_config).await
}

pub fn canonicalize_accessible_roots(roots: Vec<PathBuf>) -> Vec<PathBuf> {
    roots
        .into_iter()
        .map(|path| {
            std::fs::canonicalize(&path).unwrap_or_else(|_| {
                tracing::warn!(path = %path.display(), "无法规范化路径");
                path
            })
        })
        .collect()
}

async fn build_ws_config(config: &LocalRuntimeConfig) -> anyhow::Result<ws_client::Config> {
    if config.accessible_roots.is_empty() {
        tracing::warn!("未指定 accessible_roots，将使用当前目录作为 SessionHub 工作目录兜底");
    }

    tracing::info!(
        backend_id = %config.backend_id,
        name = %config.name,
        cloud_url = %config.cloud_url,
        accessible_roots = ?config.accessible_roots,
        executor_enabled = config.executor_enabled,
        "启动 AgentDash 本机 runtime"
    );

    let tool_executor = ToolExecutor::new(config.accessible_roots.clone());
    let local_backend_config =
        local_backend_config::load_local_backend_config(&config.accessible_roots);

    let mcp_manager = if local_backend_config.mcp_servers.is_empty() {
        None
    } else {
        Some(Arc::new(McpClientManager::new(
            local_backend_config.mcp_servers.clone(),
        )))
    };

    let (session_hub, connector) = if config.executor_enabled {
        let workspace_root = config
            .accessible_roots
            .first()
            .cloned()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

        let sub_connectors: Vec<Arc<dyn AgentConnector>> = vec![
            Arc::new(VibeKanbanExecutorsConnector::new_with_exclusions(
                workspace_root.clone(),
                ["CODEX"],
            )),
            Arc::new(CodexBridgeConnector::new(workspace_root.clone())),
        ];
        let connector: Arc<dyn AgentConnector> = Arc::new(CompositeConnector::new(sub_connectors));
        let db_path = workspace_root.join(".agentdash").join("agentdash-local.db");
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let pool = sqlx::SqlitePool::connect_with(
            SqliteConnectOptions::new()
                .filename(&db_path)
                .create_if_missing(true),
        )
        .await?;
        let session_repo = Arc::new(SqliteSessionRepository::new(pool));
        session_repo.initialize().await?;
        let hub = SessionHub::new_with_hooks_and_persistence(
            Some(agentdash_application::session::local_workspace_vfs(
                &workspace_root,
            )),
            connector.clone(),
            None,
            session_repo,
        );

        if let Err(error) = hub.recover_interrupted_sessions().await {
            tracing::warn!(error = %error, "启动恢复 session 状态失败（非致命）");
        }

        tracing::info!("SessionHub 已初始化");
        (Some(hub), Some(connector))
    } else {
        tracing::info!("SessionHub 已禁用");
        (None, None)
    };

    Ok(ws_client::Config {
        cloud_url: config.cloud_url.clone(),
        token: config.token.clone(),
        backend_id: config.backend_id.clone(),
        name: config.name.clone(),
        accessible_roots: config.accessible_roots.clone(),
        tool_executor,
        session_hub,
        connector,
        mcp_manager,
        workspace_contract_config: local_backend_config.workspace_contract,
    })
}

fn status_from_config(
    config: &LocalRuntimeConfig,
    state: LocalRuntimeState,
    message: Option<String>,
) -> LocalRuntimeStatus {
    LocalRuntimeStatus {
        state,
        backend_id: config.backend_id.clone(),
        name: config.name.clone(),
        accessible_roots: config
            .accessible_roots
            .iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect(),
        executor_enabled: config.executor_enabled,
        mcp_server_count: 0,
        message,
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
        accessible_roots: config
            .accessible_roots
            .iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect(),
        executor_enabled: config.session_hub.is_some(),
        mcp_server_count: config
            .mcp_manager
            .as_ref()
            .map(|manager| manager.capability_entries().len())
            .unwrap_or(0),
        message,
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
