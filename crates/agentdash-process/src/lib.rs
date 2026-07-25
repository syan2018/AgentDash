//! 本机进程启动 substrate。
//!
//! 后台执行面通过这里创建子进程，Windows GUI 宿主下统一应用无窗口策略。
//! 诊断只记录进程类别、程序、工作目录和可见性，不记录 args、env 或 token-bearing 值。

use std::ffi::OsStr;
use std::path::Path;
use std::process::Command as StdCommand;

use agentdash_diagnostics::{Subsystem, diag};
use tokio::process::Command as TokioCommand;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessVisibility {
    Background,
    UserVisible,
}

impl ProcessVisibility {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Background => "background",
            Self::UserVisible => "user_visible",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessDomain {
    McpStdio,
    ToolShell,
    TerminalPty,
    WorkspaceProbe,
    FunctionRunner,
    DesktopSidecar,
    CodexAppServer,
    PostgresRuntime,
    ExtensionHost,
    RunnerService,
}

impl ProcessDomain {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::McpStdio => "mcp_stdio",
            Self::ToolShell => "tool_shell",
            Self::TerminalPty => "terminal_pty",
            Self::WorkspaceProbe => "workspace_probe",
            Self::FunctionRunner => "function_runner",
            Self::DesktopSidecar => "desktop_sidecar",
            Self::CodexAppServer => "codex_app_server",
            Self::PostgresRuntime => "postgres_runtime",
            Self::ExtensionHost => "extension_host",
            Self::RunnerService => "runner_service",
        }
    }

    pub const fn subsystem(self) -> Subsystem {
        match self {
            Self::McpStdio => Subsystem::Mcp,
            Self::PostgresRuntime => Subsystem::Infra,
            _ => Subsystem::AgentRun,
        }
    }
}

pub fn background_std_command(domain: ProcessDomain, program: impl AsRef<OsStr>) -> StdCommand {
    std_command(
        domain,
        ProcessVisibility::Background,
        program,
        None::<&Path>,
    )
}

pub fn background_std_command_with_cwd(
    domain: ProcessDomain,
    program: impl AsRef<OsStr>,
    cwd: impl AsRef<Path>,
) -> StdCommand {
    std_command(
        domain,
        ProcessVisibility::Background,
        program,
        Some(cwd.as_ref()),
    )
}

pub fn user_visible_std_command(domain: ProcessDomain, program: impl AsRef<OsStr>) -> StdCommand {
    std_command(
        domain,
        ProcessVisibility::UserVisible,
        program,
        None::<&Path>,
    )
}

pub fn background_tokio_command(domain: ProcessDomain, program: impl AsRef<OsStr>) -> TokioCommand {
    tokio_command(
        domain,
        ProcessVisibility::Background,
        program,
        None::<&Path>,
    )
}

pub fn background_tokio_command_with_cwd(
    domain: ProcessDomain,
    program: impl AsRef<OsStr>,
    cwd: impl AsRef<Path>,
) -> TokioCommand {
    tokio_command(
        domain,
        ProcessVisibility::Background,
        program,
        Some(cwd.as_ref()),
    )
}

pub fn user_visible_tokio_command(
    domain: ProcessDomain,
    program: impl AsRef<OsStr>,
) -> TokioCommand {
    tokio_command(
        domain,
        ProcessVisibility::UserVisible,
        program,
        None::<&Path>,
    )
}

pub fn apply_background_window_policy(command: &mut StdCommand) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;

        command.creation_flags(CREATE_NO_WINDOW);
    }

    #[cfg(not(windows))]
    {
        let _ = command;
    }
}

pub fn apply_background_window_policy_tokio(command: &mut TokioCommand) {
    #[cfg(windows)]
    {
        command.creation_flags(CREATE_NO_WINDOW);
    }

    #[cfg(not(windows))]
    {
        let _ = command;
    }
}

fn std_command(
    domain: ProcessDomain,
    visibility: ProcessVisibility,
    program: impl AsRef<OsStr>,
    cwd: Option<&Path>,
) -> StdCommand {
    let program_ref = program.as_ref();
    let mut command = StdCommand::new(program_ref);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    apply_window_policy_for_visibility(&mut command, visibility);
    diagnose_spawn(domain, visibility, program_ref, cwd);
    command
}

fn tokio_command(
    domain: ProcessDomain,
    visibility: ProcessVisibility,
    program: impl AsRef<OsStr>,
    cwd: Option<&Path>,
) -> TokioCommand {
    let program_ref = program.as_ref();
    let mut command = TokioCommand::new(program_ref);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    apply_window_policy_for_visibility_tokio(&mut command, visibility);
    diagnose_spawn(domain, visibility, program_ref, cwd);
    command
}

fn apply_window_policy_for_visibility(command: &mut StdCommand, visibility: ProcessVisibility) {
    if visibility == ProcessVisibility::Background {
        apply_background_window_policy(command);
    }
}

fn apply_window_policy_for_visibility_tokio(
    command: &mut TokioCommand,
    visibility: ProcessVisibility,
) {
    if visibility == ProcessVisibility::Background {
        apply_background_window_policy_tokio(command);
    }
}

fn diagnose_spawn(
    domain: ProcessDomain,
    visibility: ProcessVisibility,
    program: &OsStr,
    cwd: Option<&Path>,
) {
    let program = program.to_string_lossy();
    let cwd = cwd
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "<inherit>".to_string());
    let hidden_window = visibility == ProcessVisibility::Background && cfg!(windows);

    diag!(
        Debug,
        domain.subsystem(),
        domain = domain.as_str(),
        program = %program,
        cwd = %cwd,
        visibility = visibility.as_str(),
        hidden_window = hidden_window,
        "process_spawn"
    );
}

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;
