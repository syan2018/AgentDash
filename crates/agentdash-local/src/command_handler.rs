use std::path::Path;
use std::sync::Arc;

use agent_client_protocol::{
    EnvVariable, HttpHeader, McpServer, McpServerHttp, McpServerSse, McpServerStdio,
};
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

            RelayMessage::CommandDiscoverOptions { id, payload } => {
                vec![self.handle_discover_options(id, payload).await]
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

            RelayMessage::CommandToolSearch { id, payload } => {
                vec![self.handle_tool_search(id, payload).await]
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

            RelayMessage::CommandBrowseDirectory { id, payload } => {
                vec![self.handle_browse_directory(id, payload).await]
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
            cfg.provider_id = c.provider_id;
            cfg.model_id = c.model_id;
            cfg.agent_id = c.agent_id;
            cfg.thinking_level = c
                .thinking_level
                .and_then(|value| serde_json::from_value(serde_json::Value::String(value)).ok());
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
            mcp_servers: parse_relay_mcp_servers(&payload.mcp_servers),
            workspace_root: Some(workspace_root),
            address_space: None,
            flow_capabilities: None,
            system_context: None,
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

    async fn handle_discover_options(
        &self,
        id: String,
        payload: CommandDiscoverOptionsPayload,
    ) -> RelayMessage {
        tracing::debug!(
            executor = %payload.executor,
            variant = ?payload.variant,
            "收到 command.discover_options，但本机 relay 尚未实现该流式能力"
        );
        RelayMessage::Error {
            id,
            error: RelayError::runtime_error(
                "本机 relay 尚未实现 command.discover_options，请改走云端直连 discovery 管线",
            ),
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
                payload.cwd.as_deref(),
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

    async fn handle_tool_search(&self, id: String, payload: ToolSearchPayload) -> RelayMessage {
        match self
            .tool_executor
            .search(
                &payload.workspace_root,
                &payload.query,
                payload.path.as_deref(),
                payload.is_regex,
                payload.include_glob.as_deref(),
                payload.max_results,
                payload.context_lines,
            )
            .await
        {
            Ok((hits, truncated)) => RelayMessage::ResponseToolSearch {
                id,
                payload: Some(ToolSearchResponse {
                    call_id: payload.call_id,
                    hits,
                    truncated,
                }),
                error: None,
            },
            Err(e) => RelayMessage::ResponseToolSearch {
                id,
                payload: None,
                error: Some(RelayError::runtime_error(e.to_string())),
            },
        }
    }

    // ─── 工作空间文件命令处理 ───────────────────────────────

    async fn handle_workspace_files_list(
        &self,
        id: String,
        payload: CommandWorkspaceFilesListPayload,
    ) -> RelayMessage {
        let root = match self.resolve_workspace_root(&payload.workspace_id, &payload.root_path) {
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
        let root = match self.resolve_workspace_root(&payload.workspace_id, &payload.root_path) {
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
        workspace_id: &str,
        root_path: &Option<String>,
    ) -> Result<std::path::PathBuf, String> {
        if let Some(rp) = root_path {
            self.tool_executor
                .validate_workspace_root(rp)
                .map_err(|e| format!("路径校验失败: {e}"))
        } else {
            Err(format!(
                "workspace_files 缺少 workspace {} 对应的 root_path，拒绝回退到默认 accessible_root",
                workspace_id
            ))
        }
    }

    async fn handle_workspace_detect_git(
        &self,
        id: String,
        payload: CommandWorkspaceDetectGitPayload,
    ) -> RelayMessage {
        let workspace_root = match self.tool_executor.validate_workspace_root(&payload.path) {
            Ok(path) => path,
            Err(error) => {
                return RelayMessage::ResponseWorkspaceDetectGit {
                    id,
                    payload: None,
                    error: Some(RelayError::runtime_error(format!(
                        "workspace_detect_git 路径校验失败: {error}"
                    ))),
                };
            }
        };

        tracing::debug!(path = %workspace_root.display(), "workspace_detect_git");
        RelayMessage::ResponseWorkspaceDetectGit {
            id,
            payload: Some(ResponseWorkspaceDetectGitPayload {
                is_git: workspace_root.join(".git").exists(),
                default_branch: None,
                current_branch: None,
                remote_url: None,
            }),
            error: None,
        }
    }

    // ─── 目录浏览命令处理 ─────────────────────────────────

    async fn handle_browse_directory(
        &self,
        id: String,
        payload: CommandBrowseDirectoryPayload,
    ) -> RelayMessage {
        let result = tokio::task::spawn_blocking(move || {
            browse_directory(payload.path.as_deref())
        })
        .await;

        match result {
            Ok(Ok((current_path, entries))) => RelayMessage::ResponseBrowseDirectory {
                id,
                payload: Some(ResponseBrowseDirectoryPayload {
                    current_path,
                    entries,
                }),
                error: None,
            },
            Ok(Err(e)) => RelayMessage::ResponseBrowseDirectory {
                id,
                payload: None,
                error: Some(RelayError::io_error(e)),
            },
            Err(e) => RelayMessage::ResponseBrowseDirectory {
                id,
                payload: None,
                error: Some(RelayError::runtime_error(format!("目录浏览任务失败: {e}"))),
            },
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

// ─── 目录浏览实现 ─────────────────────────────────────────

fn browse_directory(
    path: Option<&str>,
) -> Result<(String, Vec<BrowseDirectoryEntry>), String> {
    let path = path.map(|p| p.trim()).filter(|p| !p.is_empty());

    match path {
        None | Some("") => list_root_entries(),
        Some(dir_path) => list_directory_children(dir_path),
    }
}

/// 列出根级入口点：Windows 上返回可用盘符，其他平台返回 "/" 下的目录
fn list_root_entries() -> Result<(String, Vec<BrowseDirectoryEntry>), String> {
    #[cfg(windows)]
    {
        let mut entries = Vec::new();
        for letter in b'A'..=b'Z' {
            let drive = format!("{}:\\", letter as char);
            let drive_path = std::path::Path::new(&drive);
            if drive_path.exists() {
                entries.push(BrowseDirectoryEntry {
                    name: format!("{}: 盘", letter as char),
                    path: drive.clone(),
                    is_dir: true,
                });
            }
        }
        Ok(("".to_string(), entries))
    }

    #[cfg(not(windows))]
    {
        list_directory_children("/")
    }
}

/// 列出指定目录下的子目录（仅目录，不递归）
fn list_directory_children(dir_path: &str) -> Result<(String, Vec<BrowseDirectoryEntry>), String> {
    let path = std::path::Path::new(dir_path);
    if !path.exists() {
        return Err(format!("路径不存在: {dir_path}"));
    }
    if !path.is_dir() {
        return Err(format!("不是目录: {dir_path}"));
    }

    let canonical = std::fs::canonicalize(path)
        .map_err(|e| format!("路径规范化失败: {e}"))?;
    let current_path = normalize_display_path(&canonical);

    let read_dir = std::fs::read_dir(&canonical)
        .map_err(|e| format!("无法读取目录: {e}"))?;

    let mut entries: Vec<BrowseDirectoryEntry> = Vec::new();

    for entry in read_dir.flatten() {
        let ft = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };

        if !ft.is_dir() {
            continue;
        }

        let name = entry.file_name().to_string_lossy().to_string();
        let metadata = match entry.metadata() {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };
        // 跳过隐藏目录和系统目录，减少噪声目录
        if should_skip_directory(&name, &metadata) {
            continue;
        }

        let full_path = normalize_display_path(&entry.path());
        entries.push(BrowseDirectoryEntry {
            name,
            path: full_path,
            is_dir: true,
        });
    }

    entries.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok((current_path, entries))
}

fn should_skip_directory(name: &str, metadata: &std::fs::Metadata) -> bool {
    if name.starts_with('.') || name.starts_with('$') {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
        const FILE_ATTRIBUTE_SYSTEM: u32 = 0x4;
        let attrs = metadata.file_attributes();
        if (attrs & FILE_ATTRIBUTE_HIDDEN) != 0 || (attrs & FILE_ATTRIBUTE_SYSTEM) != 0 {
            return true;
        }
    }
    false
}

fn normalize_display_path(path: &std::path::Path) -> String {
    let raw = path.to_string_lossy();
    #[cfg(windows)]
    {
        // canonicalize 在 Windows 上可能返回 `\\?\` 前缀；UI 不需要该前缀。
        if let Some(rest) = raw.strip_prefix(r"\\?\UNC\") {
            return format!(r"\\{}", rest);
        }
        if let Some(rest) = raw.strip_prefix(r"\\?\") {
            return rest.to_string();
        }
    }
    raw.to_string()
}

// ─── MCP Server 解析 ─────────────────────────────────────

/// 从中继 `CommandPromptPayload.mcp_servers` JSON 列表解析出 ACP `McpServer` 列表。
///
/// 支持三种传输类型:
/// - `"type": "http"` → `McpServer::Http`
/// - `"type": "sse"`  → `McpServer::Sse`
/// - `"type": "stdio"` (或无 `type` 字段且有 `command` 字段) → `McpServer::Stdio`
pub fn parse_relay_mcp_servers(raw: &[serde_json::Value]) -> Vec<McpServer> {
    let mut servers = Vec::new();

    for entry in raw {
        let obj = match entry.as_object() {
            Some(o) => o,
            None => continue,
        };

        let name = obj
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let transport = obj.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let has_url = obj.contains_key("url");
        let has_command = obj.contains_key("command");

        let effective_type = if transport == "http" {
            "http"
        } else if transport == "sse" {
            "sse"
        } else if transport == "stdio" {
            "stdio"
        } else if has_url {
            "http"
        } else if has_command {
            "stdio"
        } else {
            tracing::debug!(name = %name, "relay MCP server 缺少 type/url/command，跳过");
            continue;
        };

        match effective_type {
            "http" | "sse" => {
                let url = match obj.get("url").and_then(|v| v.as_str()) {
                    Some(u) => u.to_string(),
                    None => {
                        tracing::warn!(name = %name, "relay MCP Http/SSE server 缺少 url，跳过");
                        continue;
                    }
                };
                let headers: Vec<HttpHeader> = obj
                    .get("headers")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|h| {
                                let ho = h.as_object()?;
                                let hname = ho.get("name")?.as_str()?.to_string();
                                let hvalue = ho.get("value")?.as_str()?.to_string();
                                Some(HttpHeader::new(hname, hvalue))
                            })
                            .collect()
                    })
                    .unwrap_or_default();

                if effective_type == "http" {
                    servers.push(McpServer::Http(
                        McpServerHttp::new(name, url).headers(headers),
                    ));
                } else {
                    servers.push(McpServer::Sse(
                        McpServerSse::new(name, url).headers(headers),
                    ));
                }
            }
            "stdio" => {
                let command = match obj.get("command").and_then(|v| v.as_str()) {
                    Some(c) => c.to_string(),
                    None => {
                        tracing::warn!(name = %name, "relay MCP Stdio server 缺少 command，跳过");
                        continue;
                    }
                };
                let args: Vec<String> = obj
                    .get("args")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|a| a.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                let env: Vec<EnvVariable> = obj
                    .get("env")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|e| {
                                let eo = e.as_object()?;
                                let ename = eo.get("name")?.as_str()?.to_string();
                                let evalue = eo.get("value")?.as_str()?.to_string();
                                Some(EnvVariable::new(ename, evalue))
                            })
                            .collect()
                    })
                    .unwrap_or_default();

                let server = McpServerStdio::new(name, command).args(args).env(env);
                servers.push(McpServer::Stdio(server));
            }
            _ => {}
        }
    }

    servers
}
