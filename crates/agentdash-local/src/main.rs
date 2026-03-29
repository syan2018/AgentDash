mod command_handler;
mod tool_executor;
mod ws_client;

use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use tracing_subscriber::EnvFilter;

use agentdash_application::session::SessionHub;
use agentdash_executor::connectors::composite::CompositeConnector;
use agentdash_executor::connectors::vibe_kanban::VibeKanbanExecutorsConnector;
use agentdash_spi::AgentConnector;

#[derive(Parser, Debug)]
#[command(name = "agentdash-local", about = "AgentDash 本机后端")]
struct Cli {
    /// 云端 WebSocket 地址
    #[arg(long)]
    cloud_url: String,

    /// 鉴权 token
    #[arg(long)]
    token: String,

    /// 本机可访问的工作空间目录（逗号分隔）
    #[arg(long, value_delimiter = ',')]
    accessible_roots: Vec<PathBuf>,

    /// 后端显示名称
    #[arg(long, default_value = "local-backend")]
    name: String,

    /// 后端 ID（不指定则自动生成）
    #[arg(long)]
    backend_id: Option<String>,

    /// 禁用 SessionHub（仅保留 ToolExecutor 能力）
    #[arg(long, default_value_t = false)]
    no_executor: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    let backend_id = cli
        .backend_id
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let accessible_roots: Vec<PathBuf> = cli
        .accessible_roots
        .iter()
        .map(|p| {
            std::fs::canonicalize(p).unwrap_or_else(|_| {
                tracing::warn!("无法规范化路径: {}", p.display());
                p.clone()
            })
        })
        .collect();

    if accessible_roots.is_empty() {
        tracing::warn!("未指定 --accessible-roots，将使用当前目录");
    }

    tracing::info!(
        backend_id = %backend_id,
        name = %cli.name,
        cloud_url = %cli.cloud_url,
        accessible_roots = ?accessible_roots,
        "启动 AgentDash 本机后端"
    );

    let tool_exec = tool_executor::ToolExecutor::new(accessible_roots.clone());

    let (session_hub, connector) = if !cli.no_executor {
        let workspace_root = accessible_roots
            .first()
            .cloned()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

        let sub_connectors: Vec<Arc<dyn AgentConnector>> = vec![Arc::new(
            VibeKanbanExecutorsConnector::new(workspace_root.clone()),
        )];
        let connector: Arc<dyn AgentConnector> = Arc::new(CompositeConnector::new(sub_connectors));
        let hub = SessionHub::new(workspace_root, connector.clone());

        // 启动恢复：将上次进程异常退出时残留的 running 状态修正为 interrupted
        if let Err(e) = hub.recover_interrupted_sessions().await {
            tracing::warn!("启动恢复 session 状态失败（非致命）: {e}");
        }

        tracing::info!("SessionHub 已初始化");
        (Some(hub), Some(connector))
    } else {
        tracing::info!("SessionHub 已禁用（--no-executor）");
        (None, None)
    };

    ws_client::run(ws_client::Config {
        cloud_url: cli.cloud_url,
        token: cli.token,
        backend_id,
        name: cli.name,
        accessible_roots,
        tool_executor: tool_exec,
        session_hub,
        connector,
    })
    .await
}
