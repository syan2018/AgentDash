use std::path::PathBuf;

use agentdash_diagnostics::{Subsystem, diag};
use agentdash_local::runner_claim::{claim_runner, direct_credentials};
use agentdash_local::runner_config::{
    ResolvedRunnerConfig, RunnerCliOverrides, persist_credentials, resolve_runner_config,
};
use agentdash_local::runner_service::{
    RealServiceExecutor, SERVICE_NAME, ServiceAction, ServiceContext, current_exe_path,
    execute_service_action, service_action_plan, service_status,
};
use agentdash_local::runner_status::{
    RunnerStatusReporter, RunnerStatusSnapshot, is_stale, read_status, render_human, status_path,
    write_status,
};
use agentdash_local::{load_or_create_machine_identity, run_standalone_with_status_and_shutdown};
use clap::{Args, Parser};
#[cfg(windows)]
use std::sync::OnceLock;
use tokio::sync::watch;
use tracing_subscriber::{EnvFilter, Registry, fmt, prelude::*};

#[cfg(windows)]
static WINDOWS_SERVICE_RUN_ARGS: OnceLock<RunArgs> = OnceLock::new();

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
    /// 管理 OS service；Linux 使用 systemd，Windows 使用 SCM。
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
    /// Windows SCM entrypoint used by installed services.
    #[command(hide = true)]
    Run(RunArgs),
}

#[derive(Args, Debug, Clone, Default)]
struct ServiceArgs {
    #[command(flatten)]
    config: ConfigArgs,
    /// 输出 JSON。
    #[arg(long)]
    json: bool,
    /// 只输出计划，不写 systemd unit、不调用 systemctl。
    #[arg(long)]
    dry_run: bool,
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
    match Cli::parse().command {
        Command::Run(args) => run_runner(args).await,
        Command::Status(args) => {
            let _guard = init_stdout_logging();
            print_status(args)
        }
        Command::Service {
            action: ServiceCommand::Run(args),
        } => run_service_entrypoint(args),
        Command::Service { action } => {
            let _guard = init_stdout_logging();
            run_service_command(action)
        }
        Command::MachineIdentity => {
            let _guard = init_stdout_logging();
            let identity = load_or_create_machine_identity()?;
            println!("{}", serde_json::to_string_pretty(&identity)?);
            Ok(())
        }
    }
}

async fn run_runner(args: RunArgs) -> anyhow::Result<()> {
    let (_shutdown_tx, shutdown_rx) = watch::channel(false);
    run_runner_until_shutdown(args, shutdown_rx).await
}

async fn run_runner_until_shutdown(
    args: RunArgs,
    shutdown_rx: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let mut config = resolve_runner_config(args.config.overrides())?;
    let _log_guard = init_runner_logging(&config.log_path)?;
    let mut snapshot = RunnerStatusSnapshot::from_config(&config, "foreground");
    write_status(&snapshot)?;
    diag!(
        Info,
        Subsystem::Infra,
        service_name = %snapshot.service_name,
        relay_state = %snapshot.relay_state,
        "Local Runner 启动"
    );

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
    let snapshot = RunnerStatusSnapshot::from_config(&config, "foreground").mark_connecting();
    write_status(&snapshot)?;
    let reporter = RunnerStatusReporter::new(snapshot);
    run_standalone_with_status_and_shutdown(runtime_config, Some(reporter), shutdown_rx).await
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
    let (action, args, json, dry_run) = match action {
        ServiceCommand::Install(args) => {
            (ServiceAction::Install, args.config, args.json, args.dry_run)
        }
        ServiceCommand::Uninstall(args) => (
            ServiceAction::Uninstall,
            args.config,
            args.json,
            args.dry_run,
        ),
        ServiceCommand::Start(args) => (ServiceAction::Start, args.config, args.json, args.dry_run),
        ServiceCommand::Stop(args) => (ServiceAction::Stop, args.config, args.json, args.dry_run),
        ServiceCommand::Status(args) => {
            (ServiceAction::Status, args.config, args.json, args.dry_run)
        }
        ServiceCommand::Run(args) => return run_service_entrypoint(args),
    };
    let config = resolve_runner_config(args.overrides())?;
    let context = ServiceContext {
        config_path: config.config_path.clone(),
        state_dir: config.state_dir.clone(),
        log_path: config.log_path.clone(),
        exe_path: current_exe_path(),
    };
    let result = if dry_run {
        service_action_plan(action, &context)?
    } else {
        let mut executor = RealServiceExecutor;
        execute_service_action(action, &context, &mut executor, false)?
    };
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

fn run_service_entrypoint(args: RunArgs) -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        WINDOWS_SERVICE_RUN_ARGS
            .set(args)
            .map_err(|_| anyhow::anyhow!("Windows Service 参数已初始化"))?;
        windows_service_dispatcher()
    }

    #[cfg(not(windows))]
    {
        let _ = args;
        anyhow::bail!("service run 仅用于 Windows Service Control Manager");
    }
}

#[cfg(windows)]
windows_service::define_windows_service!(ffi_service_main, windows_service_main);

#[cfg(windows)]
fn windows_service_dispatcher() -> anyhow::Result<()> {
    windows_service::service_dispatcher::start(SERVICE_NAME, ffi_service_main)
        .map_err(|error| anyhow::anyhow!("启动 Windows Service dispatcher 失败: {error}"))
}

#[cfg(windows)]
fn windows_service_main(_arguments: Vec<std::ffi::OsString>) {
    if let Err(error) = windows_service_main_inner() {
        eprintln!("AgentDash Local Runner Windows Service failed: {error}");
    }
}

#[cfg(windows)]
fn windows_service_main_inner() -> anyhow::Result<()> {
    use std::time::Duration;
    use windows_service::service::{
        ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
        ServiceType,
    };
    use windows_service::service_control_handler::{self, ServiceControlHandlerResult};

    let args = WINDOWS_SERVICE_RUN_ARGS
        .get()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("缺少 Windows Service run 参数"))?;
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let status_handle =
        service_control_handler::register(SERVICE_NAME, move |control_event| match control_event {
            ServiceControl::Stop | ServiceControl::Shutdown => {
                let _ = shutdown_tx.send(true);
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        })
        .map_err(|error| anyhow::anyhow!("注册 Windows Service control handler 失败: {error}"))?;

    status_handle
        .set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: ServiceState::StartPending,
            controls_accepted: ServiceControlAccept::empty(),
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 1,
            wait_hint: Duration::from_secs(10),
            process_id: None,
        })
        .map_err(|error| anyhow::anyhow!("更新 Windows Service start_pending 状态失败: {error}"))?;

    status_handle
        .set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: ServiceState::Running,
            controls_accepted: ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN,
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::default(),
            process_id: None,
        })
        .map_err(|error| anyhow::anyhow!("更新 Windows Service running 状态失败: {error}"))?;

    let result = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("agentdash-local-windows-service")
        .build()
        .map_err(|error| anyhow::anyhow!("创建 Windows Service Tokio runtime 失败: {error}"))?
        .block_on(run_runner_until_shutdown(args, shutdown_rx));

    let exit_code = if result.is_ok() {
        ServiceExitCode::Win32(0)
    } else {
        ServiceExitCode::ServiceSpecific(1)
    };
    status_handle
        .set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: ServiceState::Stopped,
            controls_accepted: ServiceControlAccept::empty(),
            exit_code,
            checkpoint: 0,
            wait_hint: Duration::default(),
            process_id: None,
        })
        .map_err(|error| anyhow::anyhow!("更新 Windows Service stopped 状态失败: {error}"))?;

    result
}

fn init_stdout_logging() -> Option<tracing_appender::non_blocking::WorkerGuard> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
    None
}

fn init_runner_logging(
    log_path: &std::path::Path,
) -> anyhow::Result<Option<tracing_appender::non_blocking::WorkerGuard>> {
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)?;
    let (file_writer, guard) = tracing_appender::non_blocking(file);
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let file_layer = fmt::layer()
        .json()
        .with_writer(file_writer)
        .with_ansi(false);

    let _ = Registry::default()
        .with(filter)
        .with(fmt::layer())
        .with(file_layer)
        .try_init();

    Ok(Some(guard))
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

    #[test]
    fn service_run_entrypoint_parses_for_windows_scm() {
        let cli = Cli::try_parse_from([
            "agentdash-local",
            "service",
            "run",
            "--config",
            "C:/ProgramData/AgentDash/runner/config.toml",
        ])
        .expect("service run entrypoint parses");

        let Command::Service {
            action: ServiceCommand::Run(args),
        } = cli.command
        else {
            panic!("expected service run command");
        };

        assert_eq!(
            args.config.config.as_deref(),
            Some(std::path::Path::new(
                "C:/ProgramData/AgentDash/runner/config.toml"
            ))
        );
    }
}
