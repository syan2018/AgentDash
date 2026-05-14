use std::path::PathBuf;

use clap::Parser;
use tracing_subscriber::EnvFilter;

use agentdash_local::{LocalRuntimeConfig, run_standalone};

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

    let config = LocalRuntimeConfig::new(
        cli.cloud_url,
        cli.token,
        backend_id,
        cli.name,
        cli.accessible_roots,
        !cli.no_executor,
    );

    run_standalone(config).await
}
