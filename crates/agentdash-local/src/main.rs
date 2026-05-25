use std::path::PathBuf;

use clap::Parser;
use tracing_subscriber::EnvFilter;

use agentdash_local::{LocalRuntimeConfig, load_or_create_machine_identity, run_standalone};

#[derive(Parser, Debug)]
#[command(name = "agentdash-local", about = "AgentDash 本机后端")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// 云端 WebSocket 地址
    #[arg(long)]
    cloud_url: Option<String>,

    /// 鉴权 token
    #[arg(long)]
    token: Option<String>,

    /// 已确认的工作空间目录（逗号分隔；不指定时按 session mount root 执行）
    #[arg(long, value_delimiter = ',')]
    workspace_roots: Vec<PathBuf>,

    /// 后端显示名称
    #[arg(long, default_value = "local-backend")]
    name: String,

    /// 后端 ID（不指定则自动生成）
    #[arg(long)]
    backend_id: Option<String>,

    /// 禁用 session runtime（仅保留 ToolExecutor 能力）
    #[arg(long, default_value_t = false)]
    no_executor: bool,
}

#[derive(clap::Subcommand, Debug)]
enum Command {
    /// 输出本机 runtime 识别到的机器身份
    MachineIdentity,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();
    if matches!(cli.command, Some(Command::MachineIdentity)) {
        let identity = load_or_create_machine_identity()?;
        println!("{}", serde_json::to_string_pretty(&identity)?);
        return Ok(());
    }

    let cloud_url = cli
        .cloud_url
        .ok_or_else(|| anyhow::anyhow!("缺少 --cloud-url"))?;
    let token = cli.token.ok_or_else(|| anyhow::anyhow!("缺少 --token"))?;

    let backend_id = cli
        .backend_id
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let config = LocalRuntimeConfig::new(
        cloud_url,
        token,
        backend_id,
        cli.name,
        cli.workspace_roots,
        !cli.no_executor,
    );

    run_standalone(config).await
}
