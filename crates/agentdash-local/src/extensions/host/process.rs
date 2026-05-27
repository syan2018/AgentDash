use std::path::PathBuf;
use std::process::Stdio;

use agentdash_domain::shared_library::ExtensionTemplatePayload;
use serde::Serialize;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command};

use super::manager::LocalTsExtensionHostConfig;
use super::permissions::resolve_host_api;
use super::protocol::{RunnerMessage, RunnerRequest};
use super::runner::EXTENSION_HOST_RUNNER;
use super::{LocalExtensionHostError, LocalExtensionHostProfile};

pub(super) struct ExtensionHostProcess {
    pub child: Child,
    stdin: ChildStdin,
    stdout: Lines<BufReader<ChildStdout>>,
    next_id: u64,
    pub active: Option<ActiveExtension>,
}

#[derive(Debug, Clone)]
pub(super) struct ActiveExtension {
    pub extension_key: String,
    pub manifest: ExtensionTemplatePayload,
    pub profile: LocalExtensionHostProfile,
    pub workspace_roots: Vec<PathBuf>,
}

impl ExtensionHostProcess {
    pub async fn spawn(
        config: &LocalTsExtensionHostConfig,
    ) -> Result<Self, LocalExtensionHostError> {
        tokio::fs::create_dir_all(&config.runner_dir).await?;
        let runner_path = config
            .runner_dir
            .join("agentdash-extension-host-runner.mjs");
        tokio::fs::write(&runner_path, EXTENSION_HOST_RUNNER).await?;
        let mut child = Command::new(&config.node_command)
            .arg("--experimental-vm-modules")
            .arg(&runner_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| {
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
            active: None,
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
                    tracing::debug!(level = %level, message = %text, "extension host log");
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
        let response = match resolve_host_api(self.active.as_ref(), &method, &params).await {
            Ok(result) => json!({ "kind": "host_api_response", "id": id, "result": result }),
            Err(error) => {
                json!({ "kind": "host_api_response", "id": id, "error": error.to_string() })
            }
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

fn spawn_stderr_drain(stderr: ChildStderr) {
    tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            tracing::debug!(message = %line, "extension host stderr");
        }
    });
}
