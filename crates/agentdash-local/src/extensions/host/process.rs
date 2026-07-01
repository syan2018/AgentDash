use agentdash_diagnostics::{Subsystem, diag};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Stdio;

use agentdash_domain::shared_library::ExtensionTemplatePayload;
use serde::Serialize;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command};

use crate::process_window::hide_window_for_tokio_command;
use crate::tool_executor::ToolExecutor;

use super::host_api::resolve_host_api;
use super::manager::LocalTsExtensionHostConfig;
use super::protocol::{RunnerHostApiResponse, RunnerMessage, RunnerRequest};
use super::runner::{EXTENSION_HOST_RUNNER_ENTRY, EXTENSION_HOST_RUNNER_FILES};
use super::{LocalExtensionHostError, LocalExtensionHostProfile};

pub(super) struct ExtensionHostProcess {
    pub child: Child,
    stdin: ChildStdin,
    stdout: Lines<BufReader<ChildStdout>>,
    next_id: u64,
    pub active_extensions: BTreeMap<String, ActiveExtension>,
}

#[derive(Debug, Clone)]
pub(super) struct ActiveExtension {
    pub extension_key: String,
    pub manifest: ExtensionTemplatePayload,
    pub profile: LocalExtensionHostProfile,
    pub default_workspace_root: Option<PathBuf>,
    pub tool_executor: ToolExecutor,
}

impl ExtensionHostProcess {
    pub async fn spawn(
        config: &LocalTsExtensionHostConfig,
    ) -> Result<Self, LocalExtensionHostError> {
        tokio::fs::create_dir_all(&config.runner_dir).await?;
        for (file_name, source) in EXTENSION_HOST_RUNNER_FILES {
            tokio::fs::write(config.runner_dir.join(file_name), source).await?;
        }
        let runner_path = config.runner_dir.join(EXTENSION_HOST_RUNNER_ENTRY);
        let mut command = Command::new(&config.node_command);
        command
            .arg("--experimental-vm-modules")
            .arg(&runner_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        hide_window_for_tokio_command(&mut command);
        let mut child = command.spawn().map_err(|error| {
            LocalExtensionHostError::Process(format!("启动 node extension host 失败: {error}"))
        })?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| LocalExtensionHostError::Process("无法打开 host stdin".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| LocalExtensionHostError::Process("无法打开 host stdout".into()))?;
        if let Some(stderr) = child.stderr.take() {
            spawn_stderr_drain(stderr);
        }
        Ok(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout).lines(),
            next_id: 1,
            active_extensions: BTreeMap::new(),
        })
    }

    pub async fn call(
        &mut self,
        method: &str,
        params: Value,
    ) -> Result<Value, LocalExtensionHostError> {
        let id = format!("local-{}", self.next_id);
        self.next_id += 1;
        let request = RunnerRequest {
            kind: "request",
            id: &id,
            method,
            params,
        };
        self.write_json(&request).await?;
        loop {
            let Some(line) = self.stdout.next_line().await? else {
                return Err(self.exit_error()?);
            };
            let message: RunnerMessage = serde_json::from_str(&line)?;
            match message.kind.as_str() {
                "response" => {
                    if message.id.as_deref() != Some(id.as_str()) {
                        return Err(LocalExtensionHostError::Protocol(format!(
                            "收到不匹配响应 id: {:?}",
                            message.id
                        )));
                    }
                    if let Some(error) = message.error {
                        return Err(LocalExtensionHostError::Host(error));
                    }
                    return Ok(message.result.unwrap_or(Value::Null));
                }
                "host_api_request" => {
                    self.handle_host_api_request(message).await?;
                }
                "log" => {
                    let level = message.level.unwrap_or_else(|| "info".to_string());
                    let text = message.message.unwrap_or_default();
                    diag!(Debug, Subsystem::AgentRun,
        level = %level, message = %text, "extension host log");
                }
                other => {
                    return Err(LocalExtensionHostError::Protocol(format!(
                        "未知 host 消息类型: {other}"
                    )));
                }
            }
        }
    }

    async fn handle_host_api_request(
        &mut self,
        message: RunnerMessage,
    ) -> Result<(), LocalExtensionHostError> {
        let id = message
            .id
            .ok_or_else(|| LocalExtensionHostError::Protocol("host api request 缺少 id".into()))?;
        let method = message.method.unwrap_or_default();
        let params = message.params.unwrap_or(Value::Null);
        let active = active_extension_for_request(&self.active_extensions, &params);
        let response = match resolve_host_api(active, &method, &params).await {
            Ok(result) => RunnerHostApiResponse::result(&id, result),
            Err(error) => RunnerHostApiResponse::error(&id, error.to_string()),
        };
        self.write_json(&response).await
    }

    async fn write_json<T: Serialize>(
        &mut self,
        message: &T,
    ) -> Result<(), LocalExtensionHostError> {
        let mut bytes = serde_json::to_vec(message)?;
        bytes.push(b'\n');
        self.stdin.write_all(&bytes).await?;
        self.stdin.flush().await?;
        Ok(())
    }

    fn exit_error(&mut self) -> Result<LocalExtensionHostError, LocalExtensionHostError> {
        let status = self.child.try_wait()?;
        Ok(match status {
            Some(status) => {
                LocalExtensionHostError::Process(format!("extension host 已退出: {status}"))
            }
            None => LocalExtensionHostError::Process("extension host stdout 已关闭".into()),
        })
    }
}

fn active_extension_for_request<'a>(
    active_extensions: &'a BTreeMap<String, ActiveExtension>,
    params: &Value,
) -> Option<&'a ActiveExtension> {
    let extension_key = params
        .get("extension_key")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let Some(extension_key) = extension_key {
        return active_extensions.get(extension_key);
    }
    if active_extensions.len() == 1 {
        return active_extensions.values().next();
    }
    None
}

fn spawn_stderr_drain(stderr: ChildStderr) {
    tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            diag!(Debug, Subsystem::AgentRun,
        message = %line, "extension host stderr");
        }
    });
}
