use std::path::PathBuf;

use agentdash_diagnostics::{Subsystem, diag};
use agentdash_local::runner_claim::{claim_runner, direct_credentials};
use agentdash_local::runner_config::{
    ResolvedRunnerConfig, RunnerCliOverrides, persist_credentials, resolve_runner_config,
};
use agentdash_local::runner_service::{current_exe_path, service_action_plan, service_status};
use agentdash_local::runner_status::{
    RunnerStatusSnapshot, is_stale, read_status, render_human, status_path, write_status,
};
use agentdash_local::{load_or_create_machine_identity, run_standalone};
use clap::{Args, Parser};
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(name = "agentdash-local", about = "AgentDash 本机后端 / Local Runner")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(clap::Subcommand, Debug)]
enum Command {
    /// 前台运行 headless Local Runner；只建立出站 WebSocket relay，不启动 HTTP API。
    Run(RunArgs),
    /// 输出 runner 最近状态。
    Status(StatusArgs),
    /// 生成或查询 OS service 管理计划；当前切片不执行系统服务注册。
    Service {
        #[command(subcommand)]
        action: ServiceCommand,
    },
    /// 输出本机 runtime 识别到的机器身份。
    MachineIdentity,
}

#[derive(Args, Debug, Clone, Default)]
struct RunArgs {
    #[command(flatten)]
    config: ConfigArgs,
}

#[derive(Args, Debug, Clone, Default)]
struct StatusArgs {
    #[command(flatten)]
    config: ConfigArgs,
    /// 输出 JSON。
    #[arg(long)]
    json: bool,
}

#[derive(clap::Subcommand, Debug, Clone)]
enum ServiceCommand {
    Install(ServiceArgs),
    Uninstall(ServiceArgs),
    Start(ServiceArgs),
    Stop(ServiceArgs),
    Status(ServiceArgs),
}

#[derive(Args, Debug, Clone, Default)]
struct ServiceArgs {
    #[command(flatten)]
    config: ConfigArgs,
    /// 输出 JSON。
    #[arg(long)]
    json: bool,
}

#[derive(Args, Debug, Clone, Default)]
struct ConfigArgs {
    /// Runner TOML 配置路径。
    #[arg(long)]
    config: Option<PathBuf>,
    /// HTTP(S) server origin，用于 registration token claim。
    #[arg(long)]
    server_url: Option<String>,
    /// Runner registration token，仅用于 claim，不用于 WebSocket relay。
    #[arg(long)]
    registration_token: Option<String>,
    /// Server-issued backend id。
    #[arg(long)]
    backend_id: Option<String>,
    /// Server-issued relay WebSocket URL；兼容旧参数名 --cloud-url。
    #[arg(long, alias = "cloud-url")]
    relay_ws_url: Option<String>,
    /// Server-issued relay auth token；兼容旧参数名 --token。
    #[arg(long, alias = "token")]
    auth_token: Option<String>,
    /// Runner 展示名称。
    #[arg(long = "runner-name", alias = "name")]
    name: Option<String>,
    /// 已确认的工作空间目录（逗号分隔；不指定时按 session mount root 执行）。
    #[arg(
        long = "workspace-root",
        alias = "workspace-roots",
        value_delimiter = ','
    )]
    workspace_roots: Vec<PathBuf>,
    /// 启用 session executor。
    #[arg(long, conflicts_with = "no_executor")]
    executor_enabled: bool,
    /// 禁用 session executor。
    #[arg(long)]
    no_executor: bool,
    /// Runner log path。
    #[arg(long)]
    log_path: Option<PathBuf>,
    /// Runner state dir，runner-status.json 写入此目录。
    #[arg(long)]
    state_dir: Option<PathBuf>,
}

impl ConfigArgs {
    fn overrides(&self) -> RunnerCliOverrides {
        RunnerCliOverrides {
            config_path: self.config.clone(),
            server_url: self.server_url.clone(),
            registration_token: self.registration_token.clone(),
            backend_id: self.backend_id.clone(),
            relay_ws_url: self.relay_ws_url.clone(),
            auth_token: self.auth_token.clone(),
            runner_name: self.name.clone(),
            workspace_roots: if self.workspace_roots.is_empty() {
                None
            } else {
                Some(self.workspace_roots.clone())
            },
            executor_enabled: if self.no_executor {
                Some(false)
            } else if self.executor_enabled {
                Some(true)
            } else {
                None
            },
            log_path: self.log_path.clone(),
            state_dir: self.state_dir.clone(),
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    match Cli::parse().command {
        Command::Run(args) => run_runner(args).await,
        Command::Status(args) => print_status(args),
        Command::Service { action } => run_service_command(action),
        Command::MachineIdentity => {
            let identity = load_or_create_machine_identity()?;
            println!("{}", serde_json::to_string_pretty(&identity)?);
            Ok(())
        }
    }
}

async fn run_runner(args: RunArgs) -> anyhow::Result<()> {
    let mut config = resolve_runner_config(args.config.overrides())?;
    let mut snapshot = RunnerStatusSnapshot::from_config(&config, "foreground");
    write_status(&snapshot)?;

    if !config.credentials.is_complete() {
        if config.registration_token.is_none() {
            let snapshot = snapshot.with_error(
                "missing_credentials",
                "缺少完整 credentials，且未提供 registration token",
            );
            write_status(&snapshot)?;
            anyhow::bail!("缺少完整 credentials，且未提供 registration token");
        }

        snapshot = snapshot.mark_claim_attempt();
        write_status(&snapshot)?;
        diag!(Info, Subsystem::Infra, "开始领取 Local Runner credentials");

        let identity = load_or_create_machine_identity()?;
        match claim_runner(&config, &identity).await {
            Ok(credentials) => {
                persist_credentials(&config.config_path, credentials.clone())?;
                config.apply_credentials(credentials);
                snapshot =
                    RunnerStatusSnapshot::from_config(&config, "foreground").mark_claim_success();
                write_status(&snapshot)?;
                diag!(
                    Info,
                    Subsystem::Infra,
                    "Local Runner credentials 已领取并写回配置"
                );
            }
            Err(error) => {
                let snapshot = snapshot.with_error(error.code(), error.message());
                write_status(&snapshot)?;
                return Err(error.into());
            }
        }
    } else if direct_credential_sources(&config) {
        let direct = direct_credentials(
            config
                .credentials
                .backend_id
                .clone()
                .expect("checked by is_complete"),
            config
                .credentials
                .relay_ws_url
                .clone()
                .expect("checked by is_complete"),
            config
                .credentials
                .auth_token
                .clone()
                .expect("checked by is_complete"),
        );
        config.credentials.token_source = direct.token_source;
    }

    let runtime_config = config.runtime_config()?;
    write_status(&RunnerStatusSnapshot::from_config(&config, "foreground").mark_connecting())?;
    run_standalone(runtime_config).await
}

fn print_status(args: StatusArgs) -> anyhow::Result<()> {
    let config = resolve_runner_config(args.config.overrides())?;
    let service = service_status();
    let status_file = status_path(&config.state_dir);
    let mut snapshot = read_status(&status_file)?
        .unwrap_or_else(|| RunnerStatusSnapshot::from_config(&config, service.state.clone()));
    snapshot = snapshot.merge_service(&service);
    snapshot.status_stale = is_stale(&snapshot, chrono::Utc::now());

    if args.json {
        println!("{}", serde_json::to_string_pretty(&snapshot)?);
    } else {
        println!("{}", render_human(&snapshot));
    }
    Ok(())
}

fn run_service_command(action: ServiceCommand) -> anyhow::Result<()> {
    let (name, args, json) = match action {
        ServiceCommand::Install(args) => ("install", args.config, args.json),
        ServiceCommand::Uninstall(args) => ("uninstall", args.config, args.json),
        ServiceCommand::Start(args) => ("start", args.config, args.json),
        ServiceCommand::Stop(args) => ("stop", args.config, args.json),
        ServiceCommand::Status(args) => ("status", args.config, args.json),
    };
    let config = resolve_runner_config(args.overrides())?;
    let result = service_action_plan(name, &config.config_path, &current_exe_path());
    if json {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        println!("service: {}", result.service_name);
        println!("state: {}", result.state);
        println!("supported: {}", result.supported);
        println!("{}", result.message);
        if let Some(unit) = result.unit {
            println!("\n{unit}");
        }
        if !result.commands.is_empty() {
            println!("commands:");
            for command in result.commands {
                println!("- {command}");
            }
        }
    }
    Ok(())
}

fn direct_credential_sources(config: &ResolvedRunnerConfig) -> bool {
    config
        .sources
        .get("backend_id")
        .is_some_and(|source| source == "cli" || source == "env")
        || config
            .sources
            .get("auth_token")
            .is_some_and(|source| source == "cli" || source == "env")
        || config
            .sources
            .get("relay_ws_url")
            .is_some_and(|source| source == "cli" || source == "env")
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_shape_supports_run_status_service_and_machine_identity() {
        Cli::command().debug_assert();

        Cli::try_parse_from([
            "agentdash-local",
            "run",
            "--server-url",
            "https://example.test",
            "--registration-token",
            "adrt_token_secret",
            "--runner-name",
            "runner-1",
            "--workspace-root",
            "C:/work/a,C:/work/b",
        ])
        .expect("run command parses");
        Cli::try_parse_from(["agentdash-local", "status", "--json"]).expect("status parses");
        Cli::try_parse_from(["agentdash-local", "service", "install"])
            .expect("service install parses");
        Cli::try_parse_from(["agentdash-local", "machine-identity"])
            .expect("machine-identity parses");
    }

    #[test]
    fn run_command_accepts_server_issued_direct_credentials() {
        let cli = Cli::try_parse_from([
            "agentdash-local",
            "run",
            "--relay-ws-url",
            "wss://example/ws/backend",
            "--auth-token",
            "relay-token",
            "--backend-id",
            "backend-1",
            "--name",
            "runner-1",
        ])
        .expect("direct credential shape parses");

        let Command::Run(args) = cli.command else {
            panic!("expected run command");
        };

        let overrides = args.config.overrides();
        assert_eq!(overrides.backend_id.as_deref(), Some("backend-1"));
        assert_eq!(overrides.auth_token.as_deref(), Some("relay-token"));
    }

    #[test]
    fn machine_identity_command_has_no_config_requirements() {
        Cli::try_parse_from(["agentdash-local", "machine-identity"])
            .expect("machine-identity 命令应可解析");
    }
}
