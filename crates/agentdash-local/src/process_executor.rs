use agentdash_diagnostics::{Subsystem, diag};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use crate::tool_executor::{ToolError, is_absolute_like, resolve_existing_path_with_root};
use crate::workspace_root_guard::WorkspaceRootGuard;

const DEFAULT_PROCESS_TIMEOUT_MS: u64 = 30_000;

pub type ProcessEnvOverlay = Vec<(String, String)>;

/// 原始进程输出结果。调用方在 API 边界负责按各自协议截断。
#[derive(Debug)]
pub struct ProcessOutput {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, Clone)]
pub struct ProcessExecutor {
    workspace_guard: WorkspaceRootGuard,
}

impl ProcessExecutor {
    pub(crate) fn new(workspace_guard: WorkspaceRootGuard) -> Self {
        Self { workspace_guard }
    }

    pub fn validate_workspace_root(&self, workspace_root: &str) -> Result<PathBuf, ToolError> {
        self.workspace_guard.validate_workspace_root(workspace_root)
    }

    pub fn resolve_cwd(
        &self,
        workspace_root: &str,
        cwd: Option<&str>,
    ) -> Result<PathBuf, ToolError> {
        let ws = self.validate_workspace_root(workspace_root)?;
        let requested = cwd.unwrap_or_default().trim();
        if requested.is_empty() || requested == "." {
            return Ok(ws);
        }

        if is_absolute_like(requested) {
            return Err(ToolError::InvalidPath(
                "shell cwd 必须是相对于 workspace root 的路径".to_string(),
            ));
        }

        resolve_existing_path_with_root(&ws, requested)
    }

    pub async fn shell_exec(
        &self,
        command: &str,
        workspace_root: &str,
        cwd: Option<&str>,
        timeout_ms: Option<u64>,
        env: &[(String, String)],
    ) -> Result<ProcessOutput, ToolError> {
        let cwd = self.resolve_cwd(workspace_root, cwd)?;
        diag!(Debug, Subsystem::AgentRun,

            command = %command,
            workspace_root = workspace_root,
            cwd = %cwd.display(),
            "process_shell_exec"
        );

        let mut command = shell_command(command, &cwd);
        apply_env_overlay(&mut command, env);
        run_output_command(command, timeout_ms).await
    }

    pub async fn exec(
        &self,
        command: &str,
        args: &[String],
        workspace_root: &str,
        cwd: Option<&str>,
        timeout_ms: Option<u64>,
        env: &[(String, String)],
    ) -> Result<ProcessOutput, ToolError> {
        let cwd = self.resolve_cwd(workspace_root, cwd)?;
        diag!(Debug, Subsystem::AgentRun,

            command = %command,
            args = ?args,
            workspace_root = workspace_root,
            cwd = %cwd.display(),
            "process_argv_exec"
        );

        let mut child = tokio::process::Command::new(command);
        child.args(args).current_dir(cwd);
        apply_env_overlay(&mut child, env);
        run_output_command(child, timeout_ms).await
    }
}

async fn run_output_command(
    mut command: tokio::process::Command,
    timeout_ms: Option<u64>,
) -> Result<ProcessOutput, ToolError> {
    let timeout_value = timeout_ms.unwrap_or(DEFAULT_PROCESS_TIMEOUT_MS);
    let timeout = Duration::from_millis(timeout_value);
    command.stdout(Stdio::piped()).stderr(Stdio::piped());

    match tokio::time::timeout(timeout, command.output()).await {
        Ok(Ok(output)) => Ok(ProcessOutput {
            exit_code: output.status.code().unwrap_or(-1),
            stdout: decode_output_chunk(&output.stdout),
            stderr: decode_output_chunk(&output.stderr),
        }),
        Ok(Err(error)) => Err(ToolError::Io(error)),
        Err(_) => Err(ToolError::Timeout(timeout_value)),
    }
}

fn apply_env_overlay(command: &mut tokio::process::Command, env: &[(String, String)]) {
    for (key, value) in env {
        command.env(key, value);
    }
}

fn shell_command(command: &str, cwd: &Path) -> tokio::process::Command {
    #[cfg(windows)]
    {
        let mut shell = tokio::process::Command::new("powershell.exe");
        let command = format!(
            "$OutputEncoding = [System.Text.UTF8Encoding]::new($false); [Console]::OutputEncoding = $OutputEncoding; {command}"
        );
        shell
            .arg("-NoLogo")
            .arg("-NoProfile")
            .arg("-NonInteractive")
            .arg("-ExecutionPolicy")
            .arg("Bypass")
            .arg("-Command")
            .arg(command)
            .current_dir(cwd);
        shell
    }

    #[cfg(not(windows))]
    {
        let mut shell = tokio::process::Command::new("sh");
        shell.arg("-c").arg(command).current_dir(cwd);
        shell
    }
}

fn decode_output_chunk(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).to_string()
}
