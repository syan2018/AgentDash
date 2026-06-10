use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use agentdash_relay::ShellOutputStream;
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::tool_executor::{
    ToolError, canonicalize_workspace_roots, is_absolute_like, resolve_existing_path_with_root,
};

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
    workspace_roots_configured: bool,
    canonical_workspace_roots: Vec<PathBuf>,
}

impl ProcessExecutor {
    pub fn new(workspace_roots: Vec<PathBuf>) -> Self {
        let workspace_roots_configured = !workspace_roots.is_empty();
        let canonical_workspace_roots = canonicalize_workspace_roots(workspace_roots);
        Self {
            workspace_roots_configured,
            canonical_workspace_roots,
        }
    }

    pub fn validate_workspace_root(&self, workspace_root: &str) -> Result<PathBuf, ToolError> {
        let trimmed = workspace_root.trim();
        if trimmed.is_empty() {
            return Err(ToolError::InvalidPath(
                "workspace root 不能为空".to_string(),
            ));
        }

        let ws_path = PathBuf::from(trimmed);
        let canonical = std::fs::canonicalize(&ws_path)
            .map_err(|_| ToolError::InvalidPath(workspace_root.to_string()))?;

        if !canonical.is_dir() {
            return Err(ToolError::InvalidPath(format!(
                "workspace root 不是目录: {workspace_root}"
            )));
        }

        if !self.workspace_roots_configured {
            return Ok(canonical);
        }

        for root in &self.canonical_workspace_roots {
            if canonical.starts_with(root) {
                return Ok(canonical);
            }
        }

        Err(ToolError::PathNotAccessible(format!(
            "workspace root 未登记: {workspace_root}"
        )))
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
        tracing::debug!(
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
        tracing::debug!(
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

    /// 流式 shell 执行 — 逐行推送 stdout/stderr 到回调，完成后返回最终结果。
    pub async fn shell_exec_streaming<F>(
        &self,
        command: &str,
        workspace_root: &str,
        cwd: Option<&str>,
        timeout_ms: Option<u64>,
        env: &[(String, String)],
        mut on_output: F,
    ) -> Result<ProcessOutput, ToolError>
    where
        F: FnMut(&str, ShellOutputStream) + Send,
    {
        let cwd = self.resolve_cwd(workspace_root, cwd)?;
        let timeout_value = timeout_ms.unwrap_or(DEFAULT_PROCESS_TIMEOUT_MS);
        let timeout = Duration::from_millis(timeout_value);

        tracing::debug!(
            command = %command,
            cwd = %cwd.display(),
            "process_shell_exec_streaming"
        );

        let mut command = shell_command(command, &cwd);
        apply_env_overlay(&mut command, env);
        let mut child = command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let stdout = child.stdout.take().expect("stdout piped");
        let stderr = child.stderr.take().expect("stderr piped");

        let mut stdout_reader = BufReader::new(stdout);
        let mut stderr_reader = BufReader::new(stderr);
        let mut stdout_buf = String::new();
        let mut stderr_buf = String::new();

        let read_loop = async {
            let mut stdout_done = false;
            let mut stderr_done = false;
            let mut stdout_line = Vec::new();
            let mut stderr_line = Vec::new();

            while !stdout_done || !stderr_done {
                tokio::select! {
                    read = stdout_reader.read_until(b'\n', &mut stdout_line), if !stdout_done => {
                        match read {
                            Ok(0) => {
                                stdout_done = true;
                            }
                            Ok(_) => {
                                let chunk = decode_output_chunk(&stdout_line);
                                stdout_line.clear();
                                on_output(&chunk, ShellOutputStream::Stdout);
                                stdout_buf.push_str(&chunk);
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, "stdout read error");
                                return Err(e);
                            }
                        }
                    }
                    read = stderr_reader.read_until(b'\n', &mut stderr_line), if !stderr_done => {
                        match read {
                            Ok(0) => {
                                stderr_done = true;
                            }
                            Ok(_) => {
                                let chunk = decode_output_chunk(&stderr_line);
                                stderr_line.clear();
                                on_output(&chunk, ShellOutputStream::Stderr);
                                stderr_buf.push_str(&chunk);
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, "stderr read error");
                                return Err(e);
                            }
                        }
                    }
                }
            }

            Ok::<(), std::io::Error>(())
        };

        match tokio::time::timeout(timeout, read_loop).await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return Err(ToolError::Io(e)),
            Err(_) => {
                let _ = child.kill().await;
                return Err(ToolError::Timeout(timeout_value));
            }
        }

        let status = child.wait().await.map_err(ToolError::Io)?;

        Ok(ProcessOutput {
            exit_code: status.code().unwrap_or(-1),
            stdout: stdout_buf,
            stderr: stderr_buf,
        })
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
