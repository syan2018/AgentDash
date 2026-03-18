use std::path::Path;
use std::sync::Arc;

use agentdash_relay::*;
use tokio::sync::mpsc;

use crate::tool_executor::ToolExecutor;
use agentdash_executor::{AgentConnector, ExecutorHub, hub::PromptSessionRequest};

/// 命令处理器，路由云端命令到本地执行组件
#[derive(Clone)]
pub struct CommandHandler {
    tool_executor: ToolExecutor,
    executor_hub: Option<ExecutorHub>,
    connector: Option<Arc<dyn AgentConnector>>,
    /// 异步事件发送通道（用于 SessionNotification 等流式推送）
    event_tx: mpsc::UnboundedSender<RelayMessage>,
}

impl CommandHandler {
    pub fn new(
        tool_executor: ToolExecutor,
        executor_hub: Option<ExecutorHub>,
        connector: Option<Arc<dyn AgentConnector>>,
        event_tx: mpsc::UnboundedSender<RelayMessage>,
    ) -> Self {
        Self {
            tool_executor,
            executor_hub,
            connector,
            event_tx,
        }
    }

    pub fn list_executors(&self) -> Vec<ExecutorInfoRelay> {
        match &self.connector {
            Some(connector) => connector
                .list_executors()
                .into_iter()
                .map(|info| ExecutorInfoRelay {
                    id: info.id,
                    name: info.name,
                    variants: info.variants,
                    available: info.available,
                })
                .collect(),
            None => vec![],
        }
    }

    /// 处理一条云端消息，返回零或多条同步响应
    /// 异步事件（如 SessionNotification）通过 event_tx 推送
    pub async fn handle(&self, msg: RelayMessage) -> Vec<RelayMessage> {
        match msg {
            RelayMessage::Ping { id, payload } => {
                vec![RelayMessage::Pong {
                    id,
                    payload: PongPayload {
                        client_time: payload.server_time,
                    },
                }]
            }

            RelayMessage::CommandPrompt { id, payload } => {
                vec![self.handle_prompt(id, payload).await]
            }

            RelayMessage::CommandCancel { id, payload } => {
                vec![self.handle_cancel(id, payload).await]
            }

            RelayMessage::CommandDiscover { id, .. } => {
                vec![self.handle_discover(id).await]
            }

            RelayMessage::CommandToolFileRead { id, payload } => {
                vec![self.handle_tool_file_read(id, payload).await]
            }

            RelayMessage::CommandToolFileWrite { id, payload } => {
                vec![self.handle_tool_file_write(id, payload).await]
            }

            RelayMessage::CommandToolShellExec { id, payload } => {
                vec![self.handle_tool_shell_exec(id, payload).await]
            }

            RelayMessage::CommandToolFileList { id, payload } => {
                vec![self.handle_tool_file_list(id, payload).await]
            }

            RelayMessage::CommandWorkspaceFilesList { id, payload } => {
                vec![self.handle_workspace_files_list(id, payload).await]
            }

            RelayMessage::CommandWorkspaceFilesRead { id, payload } => {
                vec![self.handle_workspace_files_read(id, payload).await]
            }

            RelayMessage::CommandWorkspaceDetectGit { id, payload } => {
                vec![self.handle_workspace_detect_git(id, payload).await]
            }

            other => {
                tracing::debug!(msg_id = %other.id(), "忽略非命令消息");
                vec![]
            }
        }
    }

    // ─── 第三方 Agent 命令处理 ──────────────────────────────

    async fn handle_prompt(&self, id: String, payload: CommandPromptPayload) -> RelayMessage {
        let hub = match &self.executor_hub {
            Some(hub) => hub.clone(),
            None => {
                return RelayMessage::ResponsePrompt {
                    id,
                    payload: None,
                    error: Some(RelayError::runtime_error("ExecutorHub 未初始化")),
                };
            }
        };

        let session_id = payload.session_id.clone();
        let follow_up = payload.follow_up_session_id.clone();

        let executor_config = payload.executor_config.map(|c| {
            let mut cfg = agentdash_executor::connector::AgentDashExecutorConfig::new(c.executor);
            cfg.variant = c.variant;
            cfg.model_id = c.model_id;
            cfg.permission_policy = c.permission_policy;
            cfg
        });

        let workspace_root = match self
            .tool_executor
            .validate_workspace_root(&payload.workspace_root)
        {
            Ok(path) => path,
            Err(error) => {
                return RelayMessage::ResponsePrompt {
                    id,
                    payload: None,
                    error: Some(RelayError::runtime_error(format!(
                        "workspace_root 校验失败: {error}"
                    ))),
                };
            }
        };

        let req = PromptSessionRequest {
            prompt: payload.prompt,
            prompt_blocks: payload.prompt_blocks.map(|v| {
                if let serde_json::Value::Array(arr) = v {
                    arr
                } else {
                    vec![v]
                }
            }),
            working_dir: payload.working_dir.clone(),
            env: payload.env,
            executor_config,
            mcp_servers: vec![],
            workspace_root: Some(workspace_root),
            address_space: None,
        };

        tracing::info!(
            session_id = %session_id,
            workspace_root = %payload.workspace_root,
            "收到 command.prompt，启动 Agent 执行"
        );

        let event_tx = self.event_tx.clone();

        match hub
            .start_prompt_with_follow_up(&session_id, follow_up.as_deref(), req)
            .await
        {
            Ok(turn_id) => {
                let hub_clone = hub.clone();
                let sid = session_id.clone();
                let tid = turn_id.clone();

                tokio::spawn(async move {
                    forward_session_notifications(hub_clone, &sid, &tid, event_tx).await;
                });

                RelayMessage::ResponsePrompt {
                    id,
                    payload: Some(ResponsePromptPayload {
                        turn_id,
                        status: "started".to_string(),
                    }),
                    error: None,
                }
            }
            Err(e) => {
                tracing::error!(session_id = %session_id, error = %e, "Agent 启动失败");
                RelayMessage::ResponsePrompt {
                    id,
                    payload: None,
                    error: Some(RelayError::runtime_error(e.to_string())),
                }
            }
        }
    }

    async fn handle_cancel(&self, id: String, payload: CommandCancelPayload) -> RelayMessage {
        let hub = match &self.executor_hub {
            Some(hub) => hub,
            None => {
                return RelayMessage::ResponseCancel {
                    id,
                    payload: None,
                    error: Some(RelayError::runtime_error("ExecutorHub 未初始化")),
                };
            }
        };

        tracing::info!(session_id = %payload.session_id, "收到 command.cancel");
        match hub.cancel(&payload.session_id).await {
            Ok(()) => RelayMessage::ResponseCancel {
                id,
                payload: Some(ResponseCancelPayload {
                    status: "cancelled".to_string(),
                }),
                error: None,
            },
            Err(e) => RelayMessage::ResponseCancel {
                id,
                payload: None,
                error: Some(RelayError::runtime_error(e.to_string())),
            },
        }
    }

    async fn handle_discover(&self, id: String) -> RelayMessage {
        let executors = self.list_executors();
        RelayMessage::ResponseDiscover {
            id,
            payload: Some(ResponseDiscoverPayload { executors }),
            error: None,
        }
    }

    // ─── PiAgent Tool Call 处理 ─────────────────────────────

    async fn handle_tool_file_read(
        &self,
        id: String,
        payload: ToolFileReadPayload,
    ) -> RelayMessage {
        match self
            .tool_executor
            .file_read(&payload.path, &payload.workspace_root)
            .await
        {
            Ok(content) => RelayMessage::ResponseToolFileRead {
                id,
                payload: Some(ToolFileReadResponse {
                    call_id: payload.call_id,
                    content,
                    encoding: "utf-8".to_string(),
                }),
                error: None,
            },
            Err(e) => RelayMessage::ResponseToolFileRead {
                id,
                payload: None,
                error: Some(RelayError::io_error(e.to_string())),
            },
        }
    }

    async fn handle_tool_file_write(
        &self,
        id: String,
        payload: ToolFileWritePayload,
    ) -> RelayMessage {
        match self
            .tool_executor
            .file_write(&payload.path, &payload.content, &payload.workspace_root)
            .await
        {
            Ok(()) => RelayMessage::ResponseToolFileWrite {
                id,
                payload: Some(ToolFileWriteResponse {
                    call_id: payload.call_id,
                    status: "ok".to_string(),
                }),
                error: None,
            },
            Err(e) => RelayMessage::ResponseToolFileWrite {
                id,
                payload: None,
                error: Some(RelayError::io_error(e.to_string())),
            },
        }
    }

    async fn handle_tool_shell_exec(
        &self,
        id: String,
        payload: ToolShellExecPayload,
    ) -> RelayMessage {
        match self
            .tool_executor
            .shell_exec(
                &payload.command,
                &payload.workspace_root,
                payload.timeout_ms,
            )
            .await
        {
            Ok(result) => RelayMessage::ResponseToolShellExec {
                id,
                payload: Some(ToolShellExecResponse {
                    call_id: payload.call_id,
                    exit_code: result.exit_code,
                    stdout: result.stdout,
                    stderr: result.stderr,
                }),
                error: None,
            },
            Err(e) => RelayMessage::ResponseToolShellExec {
                id,
                payload: None,
                error: Some(RelayError::runtime_error(e.to_string())),
            },
        }
    }

    async fn handle_tool_file_list(
        &self,
        id: String,
        payload: ToolFileListPayload,
    ) -> RelayMessage {
        match self
            .tool_executor
            .file_list(
                &payload.path,
                &payload.workspace_root,
                payload.pattern.as_deref(),
                payload.recursive,
            )
            .await
        {
            Ok(entries) => RelayMessage::ResponseToolFileList {
                id,
                payload: Some(ToolFileListResponse {
                    call_id: payload.call_id,
                    entries,
                }),
                error: None,
            },
            Err(e) => RelayMessage::ResponseToolFileList {
                id,
                payload: None,
                error: Some(RelayError::io_error(e.to_string())),
            },
        }
    }

    // ─── 工作空间文件命令处理 ───────────────────────────────

    async fn handle_workspace_files_list(
        &self,
        id: String,
        payload: CommandWorkspaceFilesListPayload,
    ) -> RelayMessage {
        let root = match self.resolve_workspace_root(&payload.root_path) {
            Ok(r) => r,
            Err(e) => {
                return RelayMessage::ResponseWorkspaceFilesList {
                    id,
                    payload: None,
                    error: Some(RelayError::runtime_error(e)),
                };
            }
        };

        let sub_path = payload.path.as_deref().unwrap_or("");
        let dir = match self
            .tool_executor
            .resolve_existing_path(sub_path, &root.to_string_lossy())
        {
            Ok(path) => path,
            Err(_) if sub_path.trim().is_empty() || sub_path.trim() == "." => root.clone(),
            Err(error) => {
                return RelayMessage::ResponseWorkspaceFilesList {
                    id,
                    payload: None,
                    error: Some(RelayError::runtime_error(format!("路径校验失败: {error}"))),
                };
            }
        };
        let pattern = payload.pattern.unwrap_or_default().to_lowercase();

        let result =
            tokio::task::spawn_blocking(move || walk_files(&root, &dir, &pattern, 200)).await;

        match result {
            Ok(files) => RelayMessage::ResponseWorkspaceFilesList {
                id,
                payload: Some(ResponseWorkspaceFilesListPayload { files }),
                error: None,
            },
            Err(e) => RelayMessage::ResponseWorkspaceFilesList {
                id,
                payload: None,
                error: Some(RelayError::runtime_error(format!("文件遍历失败: {e}"))),
            },
        }
    }

    async fn handle_workspace_files_read(
        &self,
        id: String,
        payload: CommandWorkspaceFilesReadPayload,
    ) -> RelayMessage {
        let root = match self.resolve_workspace_root(&payload.root_path) {
            Ok(r) => r,
            Err(e) => {
                return RelayMessage::ResponseWorkspaceFilesRead {
                    id,
                    payload: None,
                    error: Some(RelayError::runtime_error(e)),
                };
            }
        };

        let full_path = match self
            .tool_executor
            .resolve_existing_path(&payload.path, &root.to_string_lossy())
        {
            Ok(path) => path,
            Err(error) => {
                return RelayMessage::ResponseWorkspaceFilesRead {
                    id,
                    payload: None,
                    error: Some(RelayError::runtime_error(format!("路径校验失败: {error}"))),
                };
            }
        };
        if !full_path.starts_with(&root) {
            return RelayMessage::ResponseWorkspaceFilesRead {
                id,
                payload: None,
                error: Some(RelayError::runtime_error("路径越界")),
            };
        }

        let metadata = match tokio::fs::metadata(&full_path).await {
            Ok(metadata) => metadata,
            Err(e) => {
                return RelayMessage::ResponseWorkspaceFilesRead {
                    id,
                    payload: None,
                    error: Some(RelayError::io_error(format!("读取失败: {e}"))),
                };
            }
        };

        const MAX_SIZE: u64 = 100 * 1024;
        if metadata.len() > MAX_SIZE {
            return RelayMessage::ResponseWorkspaceFilesRead {
                id,
                payload: None,
                error: Some(RelayError::io_error(format!(
                    "文件过大: {} bytes，最大允许 {} bytes",
                    metadata.len(),
                    MAX_SIZE
                ))),
            };
        }

        match tokio::fs::read_to_string(&full_path).await {
            Ok(content) => RelayMessage::ResponseWorkspaceFilesRead {
                id,
                payload: Some(ResponseWorkspaceFilesReadPayload {
                    path: payload.path,
                    content,
                    encoding: "utf-8".to_string(),
                }),
                error: None,
            },
            Err(e) if e.kind() == std::io::ErrorKind::InvalidData => {
                RelayMessage::ResponseWorkspaceFilesRead {
                    id,
                    payload: None,
                    error: Some(RelayError::io_error("文件不是有效文本")),
                }
            }
            Err(e) => RelayMessage::ResponseWorkspaceFilesRead {
                id,
                payload: None,
                error: Some(RelayError::io_error(format!("读取失败: {e}"))),
            },
        }
    }

    fn resolve_workspace_root(
        &self,
        root_path: &Option<String>,
    ) -> Result<std::path::PathBuf, String> {
        if let Some(rp) = root_path {
            self.tool_executor
                .validate_workspace_root(rp)
                .map_err(|e| format!("路径校验失败: {e}"))
        } else {
            let roots = self.tool_executor.accessible_roots();
            roots
                .first()
                .cloned()
                .ok_or_else(|| "无可用的 accessible_roots".to_string())
                .and_then(|path| {
                    std::fs::canonicalize(&path)
                        .map_err(|e| format!("默认 accessible_root 解析失败: {e}"))
                })
        }
    }

    async fn handle_workspace_detect_git(
        &self,
        id: String,
        payload: CommandWorkspaceDetectGitPayload,
    ) -> RelayMessage {
        tracing::debug!(path = %payload.path, "workspace_detect_git");
        RelayMessage::ResponseWorkspaceDetectGit {
            id,
            payload: Some(ResponseWorkspaceDetectGitPayload {
                is_git: std::path::Path::new(&payload.path).join(".git").exists(),
                default_branch: None,
                current_branch: None,
                remote_url: None,
            }),
            error: None,
        }
    }
}

/// 订阅 ExecutorHub 的通知流并通过事件通道转发到 WebSocket
async fn forward_session_notifications(
    hub: ExecutorHub,
    session_id: &str,
    _turn_id: &str,
    event_tx: mpsc::UnboundedSender<RelayMessage>,
) {
    let mut rx = hub.ensure_session(session_id).await;

    loop {
        match rx.recv().await {
            Ok(notification) => {
                let notification_json =
                    serde_json::to_value(&notification).unwrap_or(serde_json::Value::Null);

                let relay_msg = RelayMessage::EventSessionNotification {
                    id: RelayMessage::new_id("evt"),
                    payload: SessionNotificationPayload {
                        session_id: session_id.to_string(),
                        notification: notification_json,
                    },
                };

                if event_tx.send(relay_msg).is_err() {
                    tracing::warn!(
                        session_id = %session_id,
                        "事件通道已关闭，停止通知转发"
                    );
                    break;
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!(
                    session_id = %session_id,
                    skipped = n,
                    "通知流落后，跳过部分消息"
                );
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                tracing::debug!(session_id = %session_id, "通知流关闭");
                break;
            }
        }
    }
}

// ─── workspace_files 文件遍历 ─────────────────────────────

const SKIP_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "__pycache__",
    ".next",
    ".agentdash",
    "dist",
    "build",
    ".trellis",
    ".venv",
    ".cursor",
    ".claude",
    ".agents",
];

fn walk_files(root: &Path, dir: &Path, pattern: &str, max: usize) -> Vec<FileEntryRelay> {
    let mut results = Vec::new();
    walk_dir_recursive(root, dir, pattern, max, &mut results);
    results.sort_by(|a, b| a.path.cmp(&b.path));
    results
}

fn walk_dir_recursive(
    root: &Path,
    dir: &Path,
    pattern: &str,
    max: usize,
    out: &mut Vec<FileEntryRelay>,
) {
    if out.len() >= max {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        if out.len() >= max {
            return;
        }
        let ft = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        if ft.is_dir() {
            if SKIP_DIRS.contains(&name_str.as_ref()) {
                continue;
            }
            walk_dir_recursive(root, &entry.path(), pattern, max, out);
        } else if ft.is_file() {
            let rel = entry
                .path()
                .strip_prefix(root)
                .unwrap_or(&entry.path())
                .to_string_lossy()
                .replace('\\', "/");

            if !pattern.is_empty() && !rel.to_lowercase().contains(pattern) {
                continue;
            }

            let meta = entry.metadata().ok();
            out.push(FileEntryRelay {
                path: rel,
                size: meta.as_ref().map(|m| m.len()),
                modified_at: meta.as_ref().and_then(|m| m.modified().ok()).map(|t| {
                    t.duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64
                }),
                is_dir: false,
            });
        }
    }
}
