use std::path::{Path, PathBuf};

use agentdash_process::{ProcessDomain, background_std_command};
use anyhow::Context;
use serde::{Deserialize, Serialize};

use crate::runner_redaction::redact_secret;

#[cfg(windows)]
pub const SERVICE_NAME: &str = "AgentDashLocalRunner";
#[cfg(not(windows))]
pub const SERVICE_NAME: &str = "agentdash-local-runner";

pub const SERVICE_DISPLAY_NAME: &str = "AgentDash Local Runner";

const SYSTEMD_UNIT_DIR: &str = "/etc/systemd/system";
const SYSTEMCTL: &str = "systemctl";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceAction {
    Install,
    Uninstall,
    Start,
    Stop,
    Status,
}

impl ServiceAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Install => "install",
            Self::Uninstall => "uninstall",
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Status => "status",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ServiceContext {
    pub config_path: PathBuf,
    pub state_dir: PathBuf,
    pub log_path: PathBuf,
    pub exe_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ServiceCommandResult {
    pub service_name: String,
    pub supported: bool,
    pub state: String,
    pub message: String,
    pub unit_path: Option<String>,
    pub unit: Option<String>,
    pub commands: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl CommandOutput {
    pub fn success(&self) -> bool {
        self.code == Some(0)
    }
}

pub trait ServiceExecutor {
    fn create_dir_all(&mut self, path: &Path) -> anyhow::Result<()>;
    fn write_file(&mut self, path: &Path, content: &str) -> anyhow::Result<()>;
    fn remove_file_if_exists(&mut self, path: &Path) -> anyhow::Result<()>;
    fn path_exists(&mut self, path: &Path) -> anyhow::Result<bool>;
    fn run(&mut self, program: &str, args: &[&str]) -> anyhow::Result<CommandOutput>;
}

#[derive(Debug, Default)]
pub struct RealServiceExecutor;

impl ServiceExecutor for RealServiceExecutor {
    fn create_dir_all(&mut self, path: &Path) -> anyhow::Result<()> {
        std::fs::create_dir_all(path).with_context(|| format!("创建目录失败: {}", path.display()))
    }

    fn write_file(&mut self, path: &Path, content: &str) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("创建目录失败: {}", parent.display()))?;
        }
        std::fs::write(path, content).with_context(|| format!("写入文件失败: {}", path.display()))
    }

    fn remove_file_if_exists(&mut self, path: &Path) -> anyhow::Result<()> {
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error).with_context(|| format!("删除文件失败: {}", path.display())),
        }
    }

    fn path_exists(&mut self, path: &Path) -> anyhow::Result<bool> {
        Ok(path.exists())
    }

    fn run(&mut self, program: &str, args: &[&str]) -> anyhow::Result<CommandOutput> {
        let mut command = background_std_command(ProcessDomain::RunnerService, program);
        let output = command
            .args(args)
            .output()
            .with_context(|| format!("执行命令失败: {}", redact_command(program, args)))?;
        Ok(CommandOutput {
            code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    }
}

pub fn execute_service_action(
    action: ServiceAction,
    context: &ServiceContext,
    executor: &mut dyn ServiceExecutor,
    dry_run: bool,
) -> anyhow::Result<ServiceCommandResult> {
    if cfg!(target_os = "linux") {
        linux_service_action(action, context, executor, dry_run)
    } else if cfg!(windows) {
        windows_service_action(action, context, executor, dry_run)
    } else {
        Ok(platform_unsupported(
            action,
            "当前平台尚未支持 OS service 管理",
        ))
    }
}

pub fn service_status() -> ServiceCommandResult {
    let context = ServiceContext {
        config_path: PathBuf::new(),
        state_dir: PathBuf::new(),
        log_path: PathBuf::new(),
        exe_path: current_exe_path(),
    };
    let mut executor = RealServiceExecutor;
    execute_service_action(ServiceAction::Status, &context, &mut executor, false).unwrap_or_else(
        |error| ServiceCommandResult {
            service_name: SERVICE_NAME.to_string(),
            supported: cfg!(any(target_os = "linux", windows)),
            state: "unknown".to_string(),
            message: redact_secret(&error.to_string()),
            unit_path: service_unit_path().map(|path| redacted_path(&path)),
            unit: None,
            commands: service_action_commands(ServiceAction::Status),
        },
    )
}

pub fn service_action_plan(
    action: ServiceAction,
    context: &ServiceContext,
) -> anyhow::Result<ServiceCommandResult> {
    let mut executor = PlanServiceExecutor;
    execute_service_action(action, context, &mut executor, true)
}

fn linux_service_action(
    action: ServiceAction,
    context: &ServiceContext,
    executor: &mut dyn ServiceExecutor,
    dry_run: bool,
) -> anyhow::Result<ServiceCommandResult> {
    let unit_path = linux_unit_path();
    let unit = systemd_unit(&context.config_path, &context.exe_path);
    let commands = service_action_commands(action);

    if dry_run {
        return Ok(ServiceCommandResult {
            service_name: SERVICE_NAME.to_string(),
            supported: true,
            state: dry_run_state(action),
            message: format!("service {} dry-run 计划已生成", action.as_str()),
            unit_path: Some(redacted_path(&unit_path)),
            unit: Some(redact_unit(&unit)),
            commands,
        });
    }

    match action {
        ServiceAction::Install => {
            ensure_service_directories(context, executor)?;
            executor.write_file(&unit_path, &unit)?;
            run_required(executor, SYSTEMCTL, &["daemon-reload"])?;
            run_required(executor, SYSTEMCTL, &["enable", SERVICE_NAME])?;
            Ok(ServiceCommandResult {
                service_name: SERVICE_NAME.to_string(),
                supported: true,
                state: "installed".to_string(),
                message: "systemd service 已安装；配置、状态和日志目录已确认存在".to_string(),
                unit_path: Some(redacted_path(&unit_path)),
                unit: Some(redact_unit(&unit)),
                commands,
            })
        }
        ServiceAction::Uninstall => {
            let _ = executor.run(SYSTEMCTL, &["stop", SERVICE_NAME]);
            let _ = executor.run(SYSTEMCTL, &["disable", SERVICE_NAME]);
            executor.remove_file_if_exists(&unit_path)?;
            run_required(executor, SYSTEMCTL, &["daemon-reload"])?;
            Ok(ServiceCommandResult {
                service_name: SERVICE_NAME.to_string(),
                supported: true,
                state: "not_installed".to_string(),
                message: "systemd service 已卸载；配置、凭据、状态和日志文件已保留".to_string(),
                unit_path: Some(redacted_path(&unit_path)),
                unit: None,
                commands,
            })
        }
        ServiceAction::Start => {
            run_required(executor, SYSTEMCTL, &["start", SERVICE_NAME])?;
            Ok(ServiceCommandResult {
                service_name: SERVICE_NAME.to_string(),
                supported: true,
                state: "running".to_string(),
                message: "systemd service 已启动".to_string(),
                unit_path: Some(redacted_path(&unit_path)),
                unit: None,
                commands,
            })
        }
        ServiceAction::Stop => {
            run_required(executor, SYSTEMCTL, &["stop", SERVICE_NAME])?;
            Ok(ServiceCommandResult {
                service_name: SERVICE_NAME.to_string(),
                supported: true,
                state: "stopped".to_string(),
                message: "systemd service 已停止".to_string(),
                unit_path: Some(redacted_path(&unit_path)),
                unit: None,
                commands,
            })
        }
        ServiceAction::Status => linux_status(executor, &unit_path, commands),
    }
}

fn linux_status(
    executor: &mut dyn ServiceExecutor,
    unit_path: &Path,
    commands: Vec<String>,
) -> anyhow::Result<ServiceCommandResult> {
    if !executor.path_exists(unit_path)? {
        return Ok(ServiceCommandResult {
            service_name: SERVICE_NAME.to_string(),
            supported: true,
            state: "not_installed".to_string(),
            message: "systemd unit 不存在".to_string(),
            unit_path: Some(redacted_path(unit_path)),
            unit: None,
            commands,
        });
    }

    let active = executor.run(SYSTEMCTL, &["is-active", SERVICE_NAME])?;
    let state = if active.success() && active.stdout.trim() == "active" {
        "running"
    } else {
        "stopped"
    };
    let status = executor.run(SYSTEMCTL, &["status", "--no-pager", SERVICE_NAME])?;
    let message = if status.success() {
        "systemd service 状态已查询".to_string()
    } else {
        let detail = first_non_empty_line(&status.stderr)
            .or_else(|| first_non_empty_line(&status.stdout))
            .unwrap_or("systemctl status 返回非零状态");
        redact_secret(detail)
    };

    Ok(ServiceCommandResult {
        service_name: SERVICE_NAME.to_string(),
        supported: true,
        state: state.to_string(),
        message,
        unit_path: Some(redacted_path(unit_path)),
        unit: None,
        commands,
    })
}

fn windows_service_action(
    action: ServiceAction,
    context: &ServiceContext,
    executor: &mut dyn ServiceExecutor,
    dry_run: bool,
) -> anyhow::Result<ServiceCommandResult> {
    let commands = windows_action_commands(action, context);

    if dry_run {
        return Ok(ServiceCommandResult {
            service_name: SERVICE_NAME.to_string(),
            supported: true,
            state: dry_run_state(action),
            message: format!("Windows service {} dry-run 计划已生成", action.as_str()),
            unit_path: None,
            unit: None,
            commands,
        });
    }

    match action {
        ServiceAction::Install => {
            ensure_service_directories(context, executor)?;
            run_required(
                executor,
                "sc.exe",
                &[
                    "create",
                    SERVICE_NAME,
                    "binPath=",
                    &windows_service_bin_path(&context.exe_path, &context.config_path),
                    "DisplayName=",
                    SERVICE_DISPLAY_NAME,
                    "start=",
                    "auto",
                ],
            )?;
            Ok(ServiceCommandResult {
                service_name: SERVICE_NAME.to_string(),
                supported: true,
                state: "installed".to_string(),
                message: "Windows Service 已安装；配置、状态和日志目录已确认存在".to_string(),
                unit_path: None,
                unit: None,
                commands,
            })
        }
        ServiceAction::Uninstall => {
            let _ = executor.run("sc.exe", &["stop", SERVICE_NAME]);
            run_required(executor, "sc.exe", &["delete", SERVICE_NAME])?;
            Ok(ServiceCommandResult {
                service_name: SERVICE_NAME.to_string(),
                supported: true,
                state: "not_installed".to_string(),
                message: "Windows Service 已卸载；配置、凭据、状态和日志文件已保留".to_string(),
                unit_path: None,
                unit: None,
                commands,
            })
        }
        ServiceAction::Start => {
            run_required(executor, "sc.exe", &["start", SERVICE_NAME])?;
            Ok(ServiceCommandResult {
                service_name: SERVICE_NAME.to_string(),
                supported: true,
                state: "running".to_string(),
                message: "Windows Service 已启动".to_string(),
                unit_path: None,
                unit: None,
                commands,
            })
        }
        ServiceAction::Stop => {
            run_required(executor, "sc.exe", &["stop", SERVICE_NAME])?;
            Ok(ServiceCommandResult {
                service_name: SERVICE_NAME.to_string(),
                supported: true,
                state: "stopped".to_string(),
                message: "Windows Service 已停止".to_string(),
                unit_path: None,
                unit: None,
                commands,
            })
        }
        ServiceAction::Status => windows_status(executor, commands),
    }
}

fn windows_status(
    executor: &mut dyn ServiceExecutor,
    commands: Vec<String>,
) -> anyhow::Result<ServiceCommandResult> {
    let output = executor.run("sc.exe", &["query", SERVICE_NAME])?;
    if !output.success() {
        let detail = first_non_empty_line(&output.stderr)
            .or_else(|| first_non_empty_line(&output.stdout))
            .unwrap_or("Windows Service 不存在或不可查询");
        return Ok(ServiceCommandResult {
            service_name: SERVICE_NAME.to_string(),
            supported: true,
            state: "not_installed".to_string(),
            message: redact_secret(detail),
            unit_path: None,
            unit: None,
            commands,
        });
    }

    let combined = format!("{}\n{}", output.stdout, output.stderr);
    let state = if combined.contains("RUNNING") {
        "running"
    } else if combined.contains("STOP_PENDING") || combined.contains("START_PENDING") {
        "pending"
    } else {
        "stopped"
    };
    Ok(ServiceCommandResult {
        service_name: SERVICE_NAME.to_string(),
        supported: true,
        state: state.to_string(),
        message: "Windows Service 状态已查询".to_string(),
        unit_path: None,
        unit: None,
        commands,
    })
}

fn ensure_service_directories(
    context: &ServiceContext,
    executor: &mut dyn ServiceExecutor,
) -> anyhow::Result<()> {
    if let Some(parent) = context.config_path.parent() {
        executor.create_dir_all(parent)?;
    }
    executor.create_dir_all(&context.state_dir)?;
    if let Some(parent) = context.log_path.parent() {
        executor.create_dir_all(parent)?;
    }
    Ok(())
}

fn run_required(
    executor: &mut dyn ServiceExecutor,
    program: &str,
    args: &[&str],
) -> anyhow::Result<CommandOutput> {
    let output = executor.run(program, args)?;
    if output.success() {
        Ok(output)
    } else {
        let message = first_non_empty_line(&output.stderr)
            .or_else(|| first_non_empty_line(&output.stdout))
            .unwrap_or("命令返回非零状态");
        anyhow::bail!(
            "{} 失败: {}",
            redact_command(program, args),
            redact_secret(message)
        );
    }
}

fn platform_unsupported(action: ServiceAction, message: &str) -> ServiceCommandResult {
    ServiceCommandResult {
        service_name: SERVICE_NAME.to_string(),
        supported: false,
        state: "unsupported".to_string(),
        message: format!("service {} unsupported: {message}", action.as_str()),
        unit_path: None,
        unit: None,
        commands: Vec::new(),
    }
}

fn systemd_unit(config_path: &Path, exe_path: &Path) -> String {
    format!(
        "[Unit]\nDescription=AgentDash Local Runner\nAfter=network-online.target\nWants=network-online.target\n\n[Service]\nType=simple\nExecStart={} run --config {}\nRestart=always\nRestartSec=5s\n\n[Install]\nWantedBy=multi-user.target\n",
        systemd_arg(exe_path),
        systemd_arg(config_path)
    )
}

fn service_action_commands(action: ServiceAction) -> Vec<String> {
    if cfg!(target_os = "linux") {
        linux_action_commands(action)
    } else if cfg!(windows) {
        windows_action_commands(
            action,
            &ServiceContext {
                config_path: PathBuf::new(),
                state_dir: PathBuf::new(),
                log_path: PathBuf::new(),
                exe_path: PathBuf::new(),
            },
        )
    } else {
        Vec::new()
    }
}

fn linux_action_commands(action: ServiceAction) -> Vec<String> {
    match action {
        ServiceAction::Install => vec![
            "write <systemd-unit>".to_string(),
            "systemctl daemon-reload".to_string(),
            format!("systemctl enable {SERVICE_NAME}"),
        ],
        ServiceAction::Uninstall => vec![
            format!("systemctl stop {SERVICE_NAME}"),
            format!("systemctl disable {SERVICE_NAME}"),
            "remove <systemd-unit>".to_string(),
            "systemctl daemon-reload".to_string(),
        ],
        ServiceAction::Start => vec![format!("systemctl start {SERVICE_NAME}")],
        ServiceAction::Stop => vec![format!("systemctl stop {SERVICE_NAME}")],
        ServiceAction::Status => vec![
            format!("systemctl is-active {SERVICE_NAME}"),
            format!("systemctl status --no-pager {SERVICE_NAME}"),
        ],
    }
}

fn windows_action_commands(action: ServiceAction, context: &ServiceContext) -> Vec<String> {
    match action {
        ServiceAction::Install => vec![format!(
            "sc.exe create {SERVICE_NAME} binPath= {} DisplayName= \"{SERVICE_DISPLAY_NAME}\" start= auto",
            redacted_windows_service_bin_path(context)
        )],
        ServiceAction::Uninstall => vec![
            format!("sc.exe stop {SERVICE_NAME}"),
            format!("sc.exe delete {SERVICE_NAME}"),
        ],
        ServiceAction::Start => vec![format!("sc.exe start {SERVICE_NAME}")],
        ServiceAction::Stop => vec![format!("sc.exe stop {SERVICE_NAME}")],
        ServiceAction::Status => vec![format!("sc.exe query {SERVICE_NAME}")],
    }
}

pub fn windows_service_bin_path(exe_path: &Path, config_path: &Path) -> String {
    format!(
        "\"{}\" service run --config \"{}\"",
        exe_path.to_string_lossy().replace('"', "\\\""),
        config_path.to_string_lossy().replace('"', "\\\"")
    )
}

fn redacted_windows_service_bin_path(context: &ServiceContext) -> String {
    if context.exe_path.as_os_str().is_empty() || context.config_path.as_os_str().is_empty() {
        "\"<agentdash-local>\" service run --config \"<runner-config>\"".to_string()
    } else {
        redact_secret(&windows_service_bin_path(
            &context.exe_path,
            &context.config_path,
        ))
        .replace(
            &context.exe_path.to_string_lossy().to_string(),
            "<agentdash-local>",
        )
        .replace(
            &context.config_path.to_string_lossy().to_string(),
            "<runner-config>",
        )
    }
}

fn dry_run_state(action: ServiceAction) -> String {
    match action {
        ServiceAction::Install => "installed".to_string(),
        ServiceAction::Uninstall => "not_installed".to_string(),
        ServiceAction::Start => "running".to_string(),
        ServiceAction::Stop => "stopped".to_string(),
        ServiceAction::Status => "unknown".to_string(),
    }
}

fn service_unit_path() -> Option<PathBuf> {
    if cfg!(target_os = "linux") {
        Some(linux_unit_path())
    } else {
        None
    }
}

fn linux_unit_path() -> PathBuf {
    PathBuf::from(SYSTEMD_UNIT_DIR).join(format!("{SERVICE_NAME}.service"))
}

fn systemd_arg(path: &Path) -> String {
    let raw = path.to_string_lossy();
    if raw
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '/' | '.' | '-' | '_' | ':'))
    {
        raw.to_string()
    } else {
        format!("\"{}\"", raw.replace('\\', "\\\\").replace('"', "\\\""))
    }
}

fn redact_unit(unit: &str) -> String {
    let mut lines = unit
        .lines()
        .map(|line| {
            if line.starts_with("ExecStart=") {
                "ExecStart=<agentdash-local> run --config <runner-config>".to_string()
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    lines.push('\n');
    lines
}

fn redacted_path(path: &Path) -> String {
    if path.ends_with(format!("{SERVICE_NAME}.service")) {
        "<systemd-unit>".to_string()
    } else {
        "<path>".to_string()
    }
}

fn redact_command(program: &str, args: &[&str]) -> String {
    redact_secret(&format!("{} {}", program, args.join(" ")))
}

fn first_non_empty_line(value: &str) -> Option<&str> {
    value.lines().map(str::trim).find(|line| !line.is_empty())
}

pub fn current_exe_path() -> PathBuf {
    std::env::current_exe().unwrap_or_else(|_| PathBuf::from("agentdash-local"))
}

#[derive(Default)]
struct PlanServiceExecutor;

impl ServiceExecutor for PlanServiceExecutor {
    fn create_dir_all(&mut self, _path: &Path) -> anyhow::Result<()> {
        Ok(())
    }

    fn write_file(&mut self, _path: &Path, _content: &str) -> anyhow::Result<()> {
        Ok(())
    }

    fn remove_file_if_exists(&mut self, _path: &Path) -> anyhow::Result<()> {
        Ok(())
    }

    fn path_exists(&mut self, _path: &Path) -> anyhow::Result<bool> {
        Ok(false)
    }

    fn run(&mut self, _program: &str, _args: &[&str]) -> anyhow::Result<CommandOutput> {
        Ok(CommandOutput {
            code: Some(0),
            stdout: String::new(),
            stderr: String::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct RecordingExecutor {
        dirs: Vec<PathBuf>,
        writes: Vec<(PathBuf, String)>,
        removes: Vec<PathBuf>,
        commands: Vec<String>,
        existing_paths: Vec<PathBuf>,
        active_stdout: String,
        sc_query_code: Option<i32>,
    }

    impl ServiceExecutor for RecordingExecutor {
        fn create_dir_all(&mut self, path: &Path) -> anyhow::Result<()> {
            self.dirs.push(path.to_path_buf());
            Ok(())
        }

        fn write_file(&mut self, path: &Path, content: &str) -> anyhow::Result<()> {
            self.writes.push((path.to_path_buf(), content.to_string()));
            Ok(())
        }

        fn remove_file_if_exists(&mut self, path: &Path) -> anyhow::Result<()> {
            self.removes.push(path.to_path_buf());
            Ok(())
        }

        fn path_exists(&mut self, path: &Path) -> anyhow::Result<bool> {
            Ok(self.existing_paths.iter().any(|existing| existing == path))
        }

        fn run(&mut self, program: &str, args: &[&str]) -> anyhow::Result<CommandOutput> {
            self.commands.push(format!("{program} {}", args.join(" ")));
            let stdout = if args.first() == Some(&"is-active")
                || (program == "sc.exe" && args.first() == Some(&"query"))
            {
                self.active_stdout.clone()
            } else {
                String::new()
            };
            let code = if program == "sc.exe" && args.first() == Some(&"query") {
                self.sc_query_code.or(Some(0))
            } else {
                Some(0)
            };
            Ok(CommandOutput {
                code,
                stdout,
                stderr: String::new(),
            })
        }
    }

    fn test_context() -> ServiceContext {
        ServiceContext {
            config_path: PathBuf::from("/etc/agentdash/runner.toml"),
            state_dir: PathBuf::from("/var/lib/agentdash/runner"),
            log_path: PathBuf::from("/var/log/agentdash/runner.log"),
            exe_path: PathBuf::from("/usr/bin/agentdash-local"),
        }
    }

    #[test]
    fn systemd_unit_uses_run_command_and_config_path() {
        let unit = systemd_unit(
            Path::new("/etc/agentdash/runner.toml"),
            Path::new("/usr/bin/agentdash-local"),
        );

        assert!(unit.contains(
            "ExecStart=/usr/bin/agentdash-local run --config /etc/agentdash/runner.toml"
        ));
        assert!(unit.contains("Restart=always"));
    }

    #[test]
    fn linux_install_creates_dirs_writes_unit_and_runs_systemctl() {
        let mut executor = RecordingExecutor::default();
        let result = linux_service_action(
            ServiceAction::Install,
            &test_context(),
            &mut executor,
            false,
        )
        .expect("install");

        assert!(result.supported);
        assert_eq!(result.state, "installed");
        assert!(executor.dirs.contains(&PathBuf::from("/etc/agentdash")));
        assert!(
            executor
                .dirs
                .contains(&PathBuf::from("/var/lib/agentdash/runner"))
        );
        assert!(executor.dirs.contains(&PathBuf::from("/var/log/agentdash")));
        assert_eq!(executor.writes.len(), 1);
        assert!(executor.writes[0].1.contains(
            "ExecStart=/usr/bin/agentdash-local run --config /etc/agentdash/runner.toml"
        ));
        assert_eq!(
            executor.commands,
            vec![
                "systemctl daemon-reload".to_string(),
                format!("systemctl enable {SERVICE_NAME}")
            ]
        );
    }

    #[test]
    fn linux_status_distinguishes_not_installed_and_running() {
        let unit_path = linux_unit_path();
        let mut missing = RecordingExecutor::default();
        let missing_result = linux_status(
            &mut missing,
            &unit_path,
            linux_action_commands(ServiceAction::Status),
        )
        .expect("missing status");
        assert_eq!(missing_result.state, "not_installed");

        let mut running = RecordingExecutor {
            existing_paths: vec![unit_path.clone()],
            active_stdout: "active\n".to_string(),
            ..Default::default()
        };
        let running_result = linux_status(
            &mut running,
            &unit_path,
            linux_action_commands(ServiceAction::Status),
        )
        .expect("running status");
        assert_eq!(running_result.state, "running");
    }

    #[test]
    fn dry_run_redacts_paths_and_does_not_execute() {
        let mut executor = RecordingExecutor::default();
        let result =
            linux_service_action(ServiceAction::Install, &test_context(), &mut executor, true)
                .expect("dry run");

        assert_eq!(result.unit_path.as_deref(), Some("<systemd-unit>"));
        assert!(
            result
                .commands
                .iter()
                .all(|command| !command.contains("/etc/"))
        );
        assert!(
            result
                .unit
                .as_deref()
                .is_some_and(|unit| unit.contains("<runner-config>") && !unit.contains("/etc/"))
        );
        assert!(executor.commands.is_empty());
        assert!(executor.writes.is_empty());
    }

    #[test]
    fn windows_service_bin_path_uses_native_service_run_entrypoint() {
        let command = windows_service_bin_path(
            Path::new(r"C:\Program Files\AgentDash\agentdash-local.exe"),
            Path::new(r"C:\ProgramData\AgentDash\runner\config.toml"),
        );

        assert!(command.contains(r#""C:\Program Files\AgentDash\agentdash-local.exe""#));
        assert!(command.contains("service run --config"));
        assert!(command.contains(r#""C:\ProgramData\AgentDash\runner\config.toml""#));
    }

    #[test]
    fn windows_install_uses_sc_create_with_service_run_entrypoint() {
        let mut executor = RecordingExecutor::default();
        let context = ServiceContext {
            config_path: PathBuf::from(r"C:\ProgramData\AgentDash\runner\config.toml"),
            state_dir: PathBuf::from(r"C:\ProgramData\AgentDash\runner"),
            log_path: PathBuf::from(r"C:\ProgramData\AgentDash\runner\runner.log"),
            exe_path: PathBuf::from(r"C:\Program Files\AgentDash\agentdash-local.exe"),
        };
        let result = windows_service_action(ServiceAction::Install, &context, &mut executor, false)
            .expect("windows install");

        assert!(result.supported);
        assert_eq!(result.state, "installed");
        assert!(executor.commands.iter().any(|command| {
            command.contains("sc.exe create AgentDashLocalRunner")
                && command.contains("service run --config")
        }));
        assert!(
            executor
                .dirs
                .contains(&PathBuf::from(r"C:\ProgramData\AgentDash\runner"))
        );
    }

    #[test]
    fn windows_status_maps_running_and_missing_service() {
        let mut missing = RecordingExecutor {
            sc_query_code: Some(1060),
            ..Default::default()
        };
        let missing_result = windows_status(
            &mut missing,
            windows_action_commands(ServiceAction::Status, &test_context()),
        )
        .expect("missing status");
        assert_eq!(missing_result.state, "not_installed");

        let mut running = RecordingExecutor {
            active_stdout: "STATE              : 4  RUNNING\n".to_string(),
            ..Default::default()
        };
        let running_result = windows_status(
            &mut running,
            windows_action_commands(ServiceAction::Status, &test_context()),
        )
        .expect("running status");
        assert_eq!(running_result.state, "running");
    }
}
