use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use agentdash_diagnostics::{Subsystem, diag};
use agentdash_local::runner_claim::{claim_runner, direct_credentials};
use agentdash_local::runner_config::{
    ResolvedRunnerConfig, RunnerCliOverrides, persist_credentials, persist_runner_setup_config,
    resolve_runner_config,
};
use agentdash_local::runner_service::{
    RealServiceExecutor, SERVICE_NAME, ServiceAction, ServiceContext, current_exe_path,
    execute_service_action, service_action_plan, service_status,
};
use agentdash_local::runner_status::{
    RunnerStatusReporter, RunnerStatusSnapshot, is_stale, read_status, render_human, status_path,
    write_status,
};
use agentdash_local::{
    load_or_create_machine_identity, redact_optional, run_standalone_with_status_and_shutdown,
};
use clap::{Args, Parser};
use serde::Serialize;
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
    /// 一键配置、claim、可选安装并启动 Local Runner service。
    Setup(SetupArgs),
    /// 前台运行 headless Local Runner；只建立出站 WebSocket relay，不启动 HTTP API。
    Run(RunArgs),
    /// 只读诊断 runner 配置、凭据、service、状态和 server health。
    Doctor(DoctorArgs),
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

#[derive(Args, Debug, Clone, Default)]
struct DoctorArgs {
    #[command(flatten)]
    config: ConfigArgs,
    /// 输出 JSON。
    #[arg(long)]
    json: bool,
}

#[derive(Args, Debug, Clone, Default)]
struct SetupArgs {
    #[command(flatten)]
    config: ConfigArgs,
    /// 安装 OS service。
    #[arg(long)]
    install_service: bool,
    /// 安装后或针对现有 service 立即启动。
    #[arg(long)]
    start: bool,
    /// 只输出计划，不写 config、不 claim、不安装/启动 service。
    #[arg(long)]
    dry_run: bool,
    /// 输出 JSON。
    #[arg(long)]
    json: bool,
    /// 禁用交互式提示；缺少关键字段时返回错误。
    #[arg(long)]
    non_interactive: bool,
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
        Command::Setup(args) => {
            let _guard = init_cli_logging();
            run_setup(args).await
        }
        Command::Run(args) => run_runner(args).await,
        Command::Doctor(args) => {
            let _guard = init_cli_logging();
            run_doctor(args).await
        }
        Command::Status(args) => {
            let _guard = init_cli_logging();
            print_status(args)
        }
        Command::Service {
            action: ServiceCommand::Run(args),
        } => run_service_entrypoint(args),
        Command::Service { action } => {
            let _guard = init_cli_logging();
            run_service_command(action)
        }
        Command::MachineIdentity => {
            let _guard = init_cli_logging();
            let identity = load_or_create_machine_identity()?;
            println!("{}", serde_json::to_string_pretty(&identity)?);
            Ok(())
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
struct SetupSummary {
    ok: bool,
    dry_run: bool,
    config_path: String,
    server_url: Option<String>,
    runner_name: String,
    backend_id: Option<String>,
    service_state: String,
    claim_state: String,
    relay_state: String,
    log_path: String,
    status_path: String,
    missing_fields: Vec<String>,
    planned_actions: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
struct DoctorReport {
    ok: bool,
    config_path: String,
    config_readable: bool,
    config_ready: bool,
    credential_state: String,
    registration_token_present: bool,
    service_state: String,
    status_path: String,
    status_state: String,
    status_stale: bool,
    log_path: String,
    log_path_state: String,
    server_url: Option<String>,
    server_health: String,
    checks: Vec<DoctorCheck>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
struct DoctorCheck {
    name: String,
    ok: bool,
    state: String,
    message: String,
}

async fn run_setup(mut args: SetupArgs) -> anyhow::Result<()> {
    let mut config = resolve_runner_config(args.config.overrides())?;
    let mut missing_fields = setup_missing_fields(&config);

    if !args.dry_run && !missing_fields.is_empty() {
        if args.non_interactive || !io::stdin().is_terminal() || !io::stdout().is_terminal() {
            anyhow::bail!(
                "setup 缺少关键字段: {}。请通过 --server-url / --registration-token 提供，或在 TTY 中运行交互式 setup。",
                missing_fields.join(", ")
            );
        }
        collect_setup_prompts(&mut args, &config)?;
        config = resolve_runner_config(args.config.overrides())?;
        missing_fields = setup_missing_fields(&config);
        if !missing_fields.is_empty() {
            anyhow::bail!("setup 缺少关键字段: {}", missing_fields.join(", "));
        }
    }

    if args.dry_run {
        let summary = setup_summary(
            &config,
            true,
            missing_fields.is_empty(),
            if missing_fields.is_empty() {
                "would_claim"
            } else {
                "missing_input"
            },
            "not_started",
            setup_planned_actions(&args),
            missing_fields,
        );
        print_setup_summary(&summary, args.json);
        return Ok(());
    }

    persist_runner_setup_config(&config)?;

    let mut claim_state = if config.credentials.is_complete() {
        "skipped_existing_credentials".to_string()
    } else {
        "success".to_string()
    };
    if !config.credentials.is_complete() {
        let identity = load_or_create_machine_identity()?;
        match claim_runner(&config, &identity).await {
            Ok(credentials) => {
                persist_credentials(&config.config_path, credentials.clone())?;
                config.apply_credentials(credentials);
            }
            Err(error) => {
                claim_state = error.code().to_string();
                let summary = setup_summary(
                    &config,
                    false,
                    false,
                    &claim_state,
                    "not_started",
                    setup_planned_actions(&args),
                    Vec::new(),
                );
                print_setup_summary(&summary, args.json);
                return Err(error.into());
            }
        }
    }

    let mut service_state = "not_requested".to_string();
    let context = service_context_from_config(&config);
    if args.install_service {
        let mut executor = RealServiceExecutor;
        let result =
            execute_service_action(ServiceAction::Install, &context, &mut executor, false)?;
        service_state = result.state;
    }
    if args.start {
        let mut executor = RealServiceExecutor;
        let result = execute_service_action(ServiceAction::Start, &context, &mut executor, false)?;
        service_state = result.state;
    }

    let mut snapshot = RunnerStatusSnapshot::from_config(&config, service_state.clone());
    if config.credentials.is_complete() {
        snapshot = snapshot.mark_claim_success();
    }
    write_status(&snapshot)?;

    let summary = setup_summary(
        &config,
        false,
        true,
        &claim_state,
        &service_state,
        setup_planned_actions(&args),
        Vec::new(),
    );
    print_setup_summary(&summary, args.json);
    Ok(())
}

async fn run_doctor(args: DoctorArgs) -> anyhow::Result<()> {
    let config_result = resolve_runner_config(args.config.overrides());
    let report = build_doctor_report(config_result, args.config.config.as_deref()).await;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("{}", render_doctor_human(&report));
    }
    Ok(())
}

fn setup_missing_fields(config: &ResolvedRunnerConfig) -> Vec<String> {
    let mut missing = Vec::new();
    if config
        .server_url
        .as_deref()
        .is_none_or(|value| value.trim().is_empty())
    {
        missing.push("server_url".to_string());
    }
    if !config.credentials.is_complete()
        && config
            .registration_token
            .as_deref()
            .is_none_or(|value| value.trim().is_empty())
    {
        missing.push("registration_token".to_string());
    }
    missing
}

fn collect_setup_prompts(
    args: &mut SetupArgs,
    config: &ResolvedRunnerConfig,
) -> anyhow::Result<()> {
    if config.server_url.is_none() {
        args.config.server_url = prompt_string("Cloud server URL", None)?;
    }
    if !config.credentials.is_complete() && config.registration_token.is_none() {
        args.config.registration_token = prompt_string(
            "Runner registration token (input will be redacted from output)",
            None,
        )?;
    }
    if args.config.name.is_none() {
        args.config.name = prompt_string("Runner name", Some(&config.runner_name))?;
    }
    if args.config.workspace_roots.is_empty()
        && config.workspace_roots.is_empty()
        && let Some(value) = prompt_string("Workspace root (empty allowed)", None)?
    {
        args.config.workspace_roots.push(PathBuf::from(value));
    }
    if args.config.executor_enabled == args.config.no_executor
        && let Some(value) = prompt_bool("Enable executor?", true)?
    {
        args.config.executor_enabled = value;
        args.config.no_executor = !value;
    }
    args.install_service = prompt_bool("Install as service?", true)?.unwrap_or(true);
    args.start =
        prompt_bool("Start service now?", args.install_service)?.unwrap_or(args.install_service);
    Ok(())
}

fn prompt_string(label: &str, default: Option<&str>) -> anyhow::Result<Option<String>> {
    match default {
        Some(default) => print!("{label} [{default}]: "),
        None => print!("{label}: "),
    }
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let trimmed = input.trim();
    if trimmed.is_empty() {
        Ok(default.map(str::to_string))
    } else {
        Ok(Some(trimmed.to_string()))
    }
}

fn prompt_bool(label: &str, default: bool) -> anyhow::Result<Option<bool>> {
    let suffix = if default { "Y/n" } else { "y/N" };
    print!("{label} [{suffix}]: ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let trimmed = input.trim().to_ascii_lowercase();
    if trimmed.is_empty() {
        Ok(Some(default))
    } else if matches!(trimmed.as_str(), "y" | "yes" | "1" | "true") {
        Ok(Some(true))
    } else if matches!(trimmed.as_str(), "n" | "no" | "0" | "false") {
        Ok(Some(false))
    } else {
        anyhow::bail!("{label} 只能输入 yes/no");
    }
}

fn setup_planned_actions(args: &SetupArgs) -> Vec<String> {
    let mut actions = vec!["write_config".to_string(), "claim_runner".to_string()];
    if args.install_service {
        actions.push("install_service".to_string());
    }
    if args.start {
        actions.push("start_service".to_string());
    }
    actions
}

fn setup_summary(
    config: &ResolvedRunnerConfig,
    dry_run: bool,
    ok: bool,
    claim_state: &str,
    service_state: &str,
    planned_actions: Vec<String>,
    missing_fields: Vec<String>,
) -> SetupSummary {
    SetupSummary {
        ok,
        dry_run,
        config_path: config.config_path.to_string_lossy().to_string(),
        server_url: redact_optional(config.server_url.as_deref()),
        runner_name: config.runner_name.clone(),
        backend_id: config.credentials.backend_id.clone(),
        service_state: service_state.to_string(),
        claim_state: claim_state.to_string(),
        relay_state: if config.credentials.is_complete() {
            "configured".to_string()
        } else {
            "not_configured".to_string()
        },
        log_path: config.log_path.to_string_lossy().to_string(),
        status_path: status_path(&config.state_dir).to_string_lossy().to_string(),
        missing_fields,
        planned_actions,
    }
}

fn print_setup_summary(summary: &SetupSummary, json: bool) {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(summary).expect("setup summary serializes")
        );
        return;
    }

    let title = if summary.dry_run {
        "AgentDash Local Runner setup plan"
    } else if summary.ok {
        "AgentDash Local Runner setup complete"
    } else {
        "AgentDash Local Runner setup failed"
    };
    println!("{title}");
    println!(
        "server:     {}",
        summary.server_url.as_deref().unwrap_or("<missing>")
    );
    println!("runner:     {}", summary.runner_name);
    println!(
        "backend_id: {}",
        summary.backend_id.as_deref().unwrap_or("<missing>")
    );
    println!("config:     {}", summary.config_path);
    println!("log:        {}", summary.log_path);
    println!("service:    {}", summary.service_state);
    println!("claim:      {}", summary.claim_state);
    println!("relay:      {}", summary.relay_state);
    if !summary.missing_fields.is_empty() {
        println!("missing:    {}", summary.missing_fields.join(", "));
    }
}

fn service_context_from_config(config: &ResolvedRunnerConfig) -> ServiceContext {
    ServiceContext {
        config_path: config.config_path.clone(),
        state_dir: config.state_dir.clone(),
        log_path: config.log_path.clone(),
        exe_path: current_exe_path(),
    }
}

async fn build_doctor_report(
    config_result: anyhow::Result<ResolvedRunnerConfig>,
    requested_config_path: Option<&Path>,
) -> DoctorReport {
    let service = service_status();
    match config_result {
        Ok(config) => {
            let status_file = status_path(&config.state_dir);
            let status_snapshot = read_status(&status_file).ok().flatten();
            let status_stale = status_snapshot
                .as_ref()
                .map(|snapshot| is_stale(snapshot, chrono::Utc::now()))
                .unwrap_or(false);
            let status_state = status_snapshot
                .as_ref()
                .map(|snapshot| snapshot.relay_state.clone())
                .unwrap_or_else(|| "missing".to_string());
            let log_path_state = log_path_state(&config.log_path);
            let server_health = check_server_health(config.server_url.as_deref()).await;

            let checks = vec![
                DoctorCheck {
                    name: "config".to_string(),
                    ok: true,
                    state: "readable".to_string(),
                    message: "runner config 可读取".to_string(),
                },
                DoctorCheck {
                    name: "credentials".to_string(),
                    ok: config.credentials.is_complete() || config.registration_token.is_some(),
                    state: config.credential_state().to_string(),
                    message: "credentials 或 registration token 状态已检查".to_string(),
                },
                DoctorCheck {
                    name: "service".to_string(),
                    ok: service.supported,
                    state: service.state.clone(),
                    message: service.message.clone(),
                },
                DoctorCheck {
                    name: "status".to_string(),
                    ok: status_snapshot.is_some(),
                    state: status_state.clone(),
                    message: if status_snapshot.is_some() {
                        "status snapshot 可读取".to_string()
                    } else {
                        "status snapshot 不存在；runner 可能尚未启动".to_string()
                    },
                },
                DoctorCheck {
                    name: "log_path".to_string(),
                    ok: log_path_state != "parent_missing",
                    state: log_path_state.clone(),
                    message: "log path 父目录状态已检查".to_string(),
                },
                DoctorCheck {
                    name: "server_health".to_string(),
                    ok: server_health == "ok" || server_health == "skipped",
                    state: server_health.clone(),
                    message: "server health 为轻量容错检查".to_string(),
                },
            ];
            let ok = checks.iter().all(|check| check.ok);

            DoctorReport {
                ok,
                config_path: config.config_path.to_string_lossy().to_string(),
                config_readable: true,
                config_ready: config.server_url.is_some() || config.credentials.is_complete(),
                credential_state: config.credential_state().to_string(),
                registration_token_present: config.registration_token.is_some(),
                service_state: service.state,
                status_path: status_file.to_string_lossy().to_string(),
                status_state,
                status_stale,
                log_path: config.log_path.to_string_lossy().to_string(),
                log_path_state,
                server_url: redact_optional(config.server_url.as_deref()),
                server_health,
                checks,
            }
        }
        Err(error) => {
            let config_path = requested_config_path
                .map(|path| path.to_string_lossy().to_string())
                .unwrap_or_else(|| "<default>".to_string());
            DoctorReport {
                ok: false,
                config_path,
                config_readable: false,
                config_ready: false,
                credential_state: "unknown".to_string(),
                registration_token_present: false,
                service_state: service.state,
                status_path: "<unknown>".to_string(),
                status_state: "unknown".to_string(),
                status_stale: false,
                log_path: "<unknown>".to_string(),
                log_path_state: "unknown".to_string(),
                server_url: None,
                server_health: "skipped".to_string(),
                checks: vec![DoctorCheck {
                    name: "config".to_string(),
                    ok: false,
                    state: "error".to_string(),
                    message: error.to_string(),
                }],
            }
        }
    }
}

fn log_path_state(log_path: &Path) -> String {
    match log_path.parent() {
        Some(parent) if parent.exists() => "parent_exists".to_string(),
        Some(_) => "parent_missing".to_string(),
        None => "relative".to_string(),
    }
}

async fn check_server_health(server_url: Option<&str>) -> String {
    let Some(server_url) = server_url else {
        return "skipped".to_string();
    };
    let url = format!("{}/api/health", server_url.trim_end_matches('/'));
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
    {
        Ok(client) => client,
        Err(_) => return "error".to_string(),
    };
    match client.get(url).send().await {
        Ok(response) if response.status().is_success() => "ok".to_string(),
        Ok(response) => format!("http_{}", response.status().as_u16()),
        Err(_) => "unreachable".to_string(),
    }
}

fn render_doctor_human(report: &DoctorReport) -> String {
    let mut output = format!(
        "AgentDash Local Runner doctor\nok: {}\nconfig: {}\ncredentials: {}\nservice: {}\nstatus: {}{}\nlog: {}\nserver_health: {}",
        report.ok,
        report.config_path,
        report.credential_state,
        report.service_state,
        report.status_state,
        if report.status_stale { " (stale)" } else { "" },
        report.log_path_state,
        report.server_health
    );
    for check in &report.checks {
        output.push_str(&format!(
            "\n- {}: {} ({})",
            check.name,
            if check.ok { "ok" } else { "error" },
            check.state
        ));
    }
    output
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

fn init_cli_logging() -> Option<tracing_appender::non_blocking::WorkerGuard> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();
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
            "setup",
            "--server-url",
            "https://example.test",
            "--registration-token",
            "adrt_token_secret",
            "--runner-name",
            "runner-1",
            "--workspace-root",
            "C:/work",
            "--install-service",
            "--start",
            "--dry-run",
            "--json",
            "--non-interactive",
        ])
        .expect("setup command parses");
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
        Cli::try_parse_from(["agentdash-local", "doctor", "--json"]).expect("doctor parses");
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

    #[tokio::test]
    async fn setup_dry_run_reports_missing_fields_without_mutation() {
        let temp = tempfile::tempdir().expect("tempdir");
        let config_path = temp.path().join("runner.toml");
        let args = SetupArgs {
            config: ConfigArgs {
                config: Some(config_path.clone()),
                name: Some("runner-1".to_string()),
                ..Default::default()
            },
            dry_run: true,
            json: false,
            non_interactive: true,
            ..Default::default()
        };

        run_setup(args).await.expect("dry-run succeeds");

        assert!(
            !config_path.exists(),
            "setup --dry-run 不应创建 runner config"
        );
    }

    #[test]
    fn setup_summary_redacts_token_bearing_fields_by_omission() {
        let temp = tempfile::tempdir().expect("tempdir");
        let config = ResolvedRunnerConfig {
            config_path: temp.path().join("runner.toml"),
            server_url: Some("https://example.test?token=server-secret".to_string()),
            registration_token: Some("adrt_secret".to_string()),
            credentials: agentdash_local::runner_config::RunnerCredentials {
                backend_id: Some("backend-1".to_string()),
                relay_ws_url: Some("wss://example.test/ws/backend".to_string()),
                auth_token: Some("relay-secret".to_string()),
                ..Default::default()
            },
            runner_name: "runner-1".to_string(),
            workspace_roots: Vec::new(),
            executor_enabled: true,
            log_path: temp.path().join("runner.log"),
            state_dir: temp.path().to_path_buf(),
            sources: std::collections::BTreeMap::new(),
        };

        let summary = setup_summary(
            &config,
            false,
            true,
            "success",
            "running",
            Vec::new(),
            Vec::new(),
        );
        let json = serde_json::to_string(&summary).expect("serialize summary");

        assert!(!json.contains("adrt_secret"));
        assert!(!json.contains("relay-secret"));
        assert!(!json.contains("server-secret"));
        assert!(!json.contains("auth_token"));
        assert!(!json.contains("registration_token"));
    }

    #[tokio::test]
    async fn doctor_output_basic_shape_omits_tokens() {
        let temp = tempfile::tempdir().expect("tempdir");
        let config_path = temp.path().join("runner.toml");
        std::fs::write(
            &config_path,
            r#"
[runner]
name = "runner-1"
server_url = "http://127.0.0.1:9"
state_dir = "C:/state"
log_path = "C:/logs/runner.log"

[registration]
token = "adrt_secret"
"#,
        )
        .expect("write config");
        let config = resolve_runner_config(RunnerCliOverrides {
            config_path: Some(config_path.clone()),
            ..Default::default()
        })
        .expect("resolve");

        let report = build_doctor_report(Ok(config), Some(&config_path)).await;
        let json = serde_json::to_string(&report).expect("serialize doctor");

        assert_eq!(report.config_path, config_path.to_string_lossy());
        assert!(report.registration_token_present);
        assert!(json.contains("credential_state"));
        assert!(!json.contains("adrt_secret"));
    }
}
