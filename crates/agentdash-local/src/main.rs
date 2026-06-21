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

    /// 后端 ID（来自 server ensure/claim 响应；standalone 必须显式传入）
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

    match cli_action(Cli::parse())? {
        CliAction::PrintMachineIdentity => {
            let identity = load_or_create_machine_identity()?;
            println!("{}", serde_json::to_string_pretty(&identity)?);
            Ok(())
        }
        CliAction::Run(config) => run_standalone(config).await,
    }
}

#[derive(Debug)]
enum CliAction {
    PrintMachineIdentity,
    Run(LocalRuntimeConfig),
}

fn cli_action(cli: Cli) -> anyhow::Result<CliAction> {
    if matches!(cli.command, Some(Command::MachineIdentity)) {
        return Ok(CliAction::PrintMachineIdentity);
    }
    let cloud_url = cli
        .cloud_url
        .ok_or_else(|| anyhow::anyhow!("缺少 --cloud-url"))?;
    let token = cli.token.ok_or_else(|| anyhow::anyhow!("缺少 --token"))?;
    let backend_id = required_backend_id(cli.backend_id)?;

    let config = LocalRuntimeConfig::new(
        cloud_url,
        token,
        backend_id,
        cli.name,
        cli.workspace_roots,
        !cli.no_executor,
    );
    Ok(CliAction::Run(config))
}

fn required_backend_id(value: Option<String>) -> anyhow::Result<String> {
    let Some(value) = value.map(|value| value.trim().to_string()) else {
        anyhow::bail!(
            "缺少 --backend-id；backend_id 必须来自 server ensure/claim 响应或显式 token-bound input"
        );
    };
    if value.is_empty() {
        anyhow::bail!(
            "--backend-id 不能为空；backend_id 必须来自 server ensure/claim 响应或显式 token-bound input"
        );
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standalone_cli_requires_explicit_backend_id() {
        let cli = Cli::try_parse_from([
            "agentdash-local",
            "--cloud-url",
            "ws://127.0.0.1:3001/api/relay",
            "--token",
            "token-1",
        ])
        .expect("参数形态应可解析");

        let error = cli_action(cli).expect_err("standalone run 必须显式传入 backend id");

        assert!(error.to_string().contains("缺少 --backend-id"));
    }

    #[test]
    fn standalone_cli_accepts_claimed_backend_id() {
        let cli = Cli::try_parse_from([
            "agentdash-local",
            "--cloud-url",
            "ws://127.0.0.1:3001/api/relay",
            "--token",
            "token-1",
            "--backend-id",
            " local_claimed_1 ",
            "--name",
            "claimed-local",
        ])
        .expect("参数形态应可解析");

        let CliAction::Run(config) = cli_action(cli).expect("显式 backend id 应可启动")
        else {
            panic!("应进入 standalone run 路径");
        };

        assert_eq!(config.backend_id, "local_claimed_1");
        assert_eq!(config.name, "claimed-local");
    }

    #[test]
    fn machine_identity_command_does_not_require_backend_id() {
        let cli = Cli::try_parse_from(["agentdash-local", "machine-identity"])
            .expect("machine-identity 命令应可解析");

        assert!(matches!(
            cli_action(cli).expect("machine-identity 不需要 runtime backend id"),
            CliAction::PrintMachineIdentity
        ));
    }
}
