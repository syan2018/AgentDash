use std::sync::Arc;

use agent_client_protocol::{
    EnvVariable, HttpHeader, McpServer, McpServerHttp, McpServerSse, McpServerStdio,
};
use agentdash_relay::*;
use tokio::sync::mpsc;

use crate::local_backend_config::WorkspaceContractRuntimeConfig;
use crate::mcp_client_manager::McpClientManager;
use crate::tool_executor::ToolExecutor;
use agentdash_application::session::{PromptSessionRequest, SessionHub, UserPromptInput};
use agentdash_spi::AgentConnector;

/// 命令处理器，路由云端命令到本地执行组件
#[derive(Clone)]
pub struct CommandHandler {
    tool_executor: ToolExecutor,
    session_hub: Option<SessionHub>,
    connector: Option<Arc<dyn AgentConnector>>,
    mcp_manager: Option<Arc<McpClientManager>>,
    workspace_contract_config: WorkspaceContractRuntimeConfig,
    /// 异步事件发送通道（用于 SessionNotification 等流式推送）
    event_tx: mpsc::UnboundedSender<RelayMessage>,
}

impl CommandHandler {
    pub fn new(
        tool_executor: ToolExecutor,
        session_hub: Option<SessionHub>,
        connector: Option<Arc<dyn AgentConnector>>,
        mcp_manager: Option<Arc<McpClientManager>>,
        workspace_contract_config: WorkspaceContractRuntimeConfig,
        event_tx: mpsc::UnboundedSender<RelayMessage>,
    ) -> Self {
        Self {
            tool_executor,
            session_hub,
            connector,
            mcp_manager,
            workspace_contract_config,
            event_tx,
        }
    }

    pub fn list_executors(&self) -> Vec<AgentInfoRelay> {
        match &self.connector {
            Some(connector) => connector
                .list_executors()
                .into_iter()
                .map(|info| AgentInfoRelay {
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
                vec![self.handle_prompt(id, *payload).await]
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

            RelayMessage::CommandWorkspaceDetect { id, payload } => {
                vec![self.handle_workspace_detect(id, payload).await]
            }

            RelayMessage::CommandToolFileRead { id, payload } => {
                vec![self.handle_tool_file_read(id, payload).await]
            }

            RelayMessage::CommandToolFileWrite { id, payload } => {
                vec![self.handle_tool_file_write(id, payload).await]
            }

            RelayMessage::CommandToolFileDelete { id, payload } => {
                vec![self.handle_tool_file_delete(id, payload).await]
            }

            RelayMessage::CommandToolFileRename { id, payload } => {
                vec![self.handle_tool_file_rename(id, payload).await]
            }

            RelayMessage::CommandToolApplyPatch { id, payload } => {
                vec![self.handle_tool_apply_patch(id, payload).await]
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

            RelayMessage::CommandWorkspaceDetectGit { id, payload } => {
                vec![self.handle_workspace_detect_git(id, payload).await]
            }

            RelayMessage::CommandBrowseDirectory { id, payload } => {
                vec![self.handle_browse_directory(id, payload).await]
            }

            // ── MCP Relay 命令 ──
            RelayMessage::CommandMcpListTools { id, payload } => {
                vec![self.handle_mcp_list_tools(id, payload).await]
            }
            RelayMessage::CommandMcpCallTool { id, payload } => {
                vec![self.handle_mcp_call_tool(id, payload).await]
            }
            RelayMessage::CommandMcpClose { id, payload } => {
                vec![self.handle_mcp_close(id, payload).await]
            }

            other => {
                tracing::debug!(msg_id = %other.id(), "忽略非命令消息");
                vec![]
            }
        }
    }

    // ─── 第三方 Agent 命令处理 ──────────────────────────────

    async fn handle_prompt(&self, id: String, payload: CommandPromptPayload) -> RelayMessage {
        let hub = match &self.session_hub {
            Some(hub) => hub.clone(),
            None => {
                return RelayMessage::ResponsePrompt {
                    id,
                    payload: None,
                    error: Some(RelayError::runtime_error("SessionHub 未初始化")),
                };
            }
        };

        let session_id = payload.session_id.clone();
        let follow_up = payload.follow_up_session_id.clone();
        let mount_root_ref = payload.mount_root_ref.trim();
        if mount_root_ref.is_empty() {
            return RelayMessage::ResponsePrompt {
                id,
                payload: None,
                error: Some(RelayError::runtime_error(
                    "command.prompt 缺少 mount_root_ref",
                )),
            };
        }

        let executor_config = payload.executor_config.map(|c| {
            let mut cfg = agentdash_spi::AgentConfig::new(c.executor);
            cfg.provider_id = c.provider_id;
            cfg.model_id = c.model_id;
            cfg.agent_id = c.agent_id;
            cfg.thinking_level = c
                .thinking_level
                .and_then(|value| serde_json::from_value(serde_json::Value::String(value)).ok());
            cfg.permission_policy = c.permission_policy;
            cfg
        });

        let workspace_root = match self.tool_executor.validate_workspace_root(mount_root_ref) {
            Ok(path) => path,
            Err(error) => {
                return RelayMessage::ResponsePrompt {
                    id,
                    payload: None,
                    error: Some(RelayError::runtime_error(format!(
                        "mount_root_ref 校验失败: {error}"
                    ))),
                };
            }
        };

        if follow_up.is_none() {
            let prepare_result = tokio::task::spawn_blocking({
                let workspace_root = workspace_root.clone();
                let workspace_identity_kind = payload.workspace_identity_kind.clone();
                let workspace_identity_payload = payload.workspace_identity_payload.clone();
                let workspace_contract_config = self.workspace_contract_config.clone();
                move || {
                    crate::workspace_prepare::prepare_workspace(
                        &workspace_root,
                        workspace_identity_kind,
                        workspace_identity_payload.as_ref(),
                        &workspace_contract_config,
                    )
                }
            })
            .await;

            match prepare_result {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    return RelayMessage::ResponsePrompt {
                        id,
                        payload: None,
                        error: Some(RelayError::runtime_error(format!(
                            "workspace prepare 失败: {error}"
                        ))),
                    };
                }
                Err(error) => {
                    return RelayMessage::ResponsePrompt {
                        id,
                        payload: None,
                        error: Some(RelayError::runtime_error(format!(
                            "workspace prepare 任务失败: {error}"
                        ))),
                    };
                }
            }
        }

        let vfs = agentdash_application::session::local_workspace_vfs(&workspace_root);

        // PR 1 Phase 1d：local relay 入口收敛 struct-literal → from_user_input 工厂 +
        // 后端注入字段按 builder 风格逐项赋值。local 路径无 identity（本地命令触发）
        // 亦无 post_turn_handler（直接对接 agent 进程，没有 task/routine 回调）。
        let mut req = PromptSessionRequest::from_user_input(UserPromptInput {
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
        });
        req.mcp_servers = parse_relay_mcp_servers(&payload.mcp_servers);
        req.vfs = Some(vfs);

        tracing::info!(
            session_id = %session_id,
            mount_root_ref = mount_root_ref,
            "收到 command.prompt，启动 Agent 执行"
        );

        let event_tx = self.event_tx.clone();

        match hub
            .launch_local_relay_prompt_with_follow_up(&session_id, follow_up.as_deref(), req)
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
        let hub = match &self.session_hub {
            Some(hub) => hub,
            None => {
                return RelayMessage::ResponseCancel {
                    id,
                    payload: None,
                    error: Some(RelayError::runtime_error("SessionHub 未初始化")),
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
            "收到 command.discover_options，但本机 relay 尚未实现该流式能力"
        );
        RelayMessage::Error {
            id,
            error: RelayError::runtime_error(
                "本机 relay 尚未实现 command.discover_options，请改走云端直连 discovery 管线",
            ),
        }
    }

    async fn handle_workspace_detect(
        &self,
        id: String,
        payload: CommandWorkspaceDetectPayload,
    ) -> RelayMessage {
        let workspace_root = match self.tool_executor.validate_workspace_root(&payload.path) {
            Ok(path) => path,
            Err(error) => {
                return RelayMessage::ResponseWorkspaceDetect {
                    id,
                    payload: None,
                    error: Some(RelayError::runtime_error(format!(
                        "workspace_detect 路径校验失败: {error}"
                    ))),
                };
            }
        };

        tracing::debug!(path = %workspace_root.display(), "workspace_detect");
        let detected = match tokio::task::spawn_blocking(move || {
            crate::workspace_probe::detect_workspace(&workspace_root)
        })
        .await
        {
            Ok(result) => result,
            Err(err) => {
                return RelayMessage::ResponseWorkspaceDetect {
                    id,
                    payload: None,
                    error: Some(RelayError::runtime_error(format!(
                        "workspace_detect 任务失败: {err}"
                    ))),
                };
            }
        };

        RelayMessage::ResponseWorkspaceDetect {
            id,
            payload: Some(detected),
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
            .file_read(&payload.path, &payload.mount_root_ref)
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
            .file_write(&payload.path, &payload.content, &payload.mount_root_ref)
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

    async fn handle_tool_file_delete(
        &self,
        id: String,
        payload: ToolFileDeletePayload,
    ) -> RelayMessage {
        match self
            .tool_executor
            .file_delete(&payload.path, &payload.mount_root_ref)
            .await
        {
            Ok(()) => RelayMessage::ResponseToolFileDelete {
                id,
                payload: Some(ToolFileDeleteResponse {
                    call_id: payload.call_id,
                    status: "ok".to_string(),
                }),
                error: None,
            },
            Err(e) => RelayMessage::ResponseToolFileDelete {
                id,
                payload: None,
                error: Some(RelayError::io_error(e.to_string())),
            },
        }
    }

    async fn handle_tool_file_rename(
        &self,
        id: String,
        payload: ToolFileRenamePayload,
    ) -> RelayMessage {
        match self
            .tool_executor
            .file_rename(
                &payload.from_path,
                &payload.to_path,
                &payload.mount_root_ref,
            )
            .await
        {
            Ok(()) => RelayMessage::ResponseToolFileRename {
                id,
                payload: Some(ToolFileRenameResponse {
                    call_id: payload.call_id,
                    status: "ok".to_string(),
                }),
                error: None,
            },
            Err(e) => RelayMessage::ResponseToolFileRename {
                id,
                payload: None,
                error: Some(RelayError::io_error(e.to_string())),
            },
        }
    }

    async fn handle_tool_apply_patch(
        &self,
        id: String,
        payload: ToolApplyPatchPayload,
    ) -> RelayMessage {
        match self
            .tool_executor
            .apply_patch(&payload.patch, &payload.mount_root_ref)
            .await
        {
            Ok(affected) => RelayMessage::ResponseToolApplyPatch {
                id,
                payload: Some(ToolApplyPatchResponse {
                    call_id: payload.call_id,
                    status: "ok".to_string(),
                    added: affected.added,
                    modified: affected.modified,
                    deleted: affected.deleted,
                }),
                error: None,
            },
            Err(e) => RelayMessage::ResponseToolApplyPatch {
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
                &payload.mount_root_ref,
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
                &payload.mount_root_ref,
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
                &payload.mount_root_ref,
                &crate::tool_executor::SearchParams {
                    query: &payload.query,
                    path: payload.path.as_deref(),
                    is_regex: payload.is_regex,
                    include_glob: payload.include_glob.as_deref(),
                    max_results: payload.max_results,
                    context_lines: payload.context_lines,
                },
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

    async fn handle_workspace_detect_git(
        &self,
        id: String,
        payload: CommandWorkspaceDetectGitPayload,
    ) -> RelayMessage {
        let detected = match self
            .handle_workspace_detect(
                id.clone(),
                CommandWorkspaceDetectPayload { path: payload.path },
            )
            .await
        {
            RelayMessage::ResponseWorkspaceDetect {
                payload: Some(payload),
                error: None,
                ..
            } => payload,
            RelayMessage::ResponseWorkspaceDetect {
                error: Some(err), ..
            } => {
                return RelayMessage::ResponseWorkspaceDetectGit {
                    id,
                    payload: None,
                    error: Some(err),
                };
            }
            _ => {
                return RelayMessage::ResponseWorkspaceDetectGit {
                    id,
                    payload: None,
                    error: Some(RelayError::runtime_error("workspace_detect 未返回可用结果")),
                };
            }
        };

        let git = detected.git;
        RelayMessage::ResponseWorkspaceDetectGit {
            id,
            payload: Some(ResponseWorkspaceDetectGitPayload {
                is_git: git.is_some(),
                default_branch: git.as_ref().and_then(|item| item.default_branch.clone()),
                current_branch: git.as_ref().and_then(|item| item.current_branch.clone()),
                remote_url: git.as_ref().and_then(|item| item.remote_url.clone()),
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
        let result =
            tokio::task::spawn_blocking(move || browse_directory(payload.path.as_deref())).await;

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

/// 订阅 SessionHub 的通知流并通过事件通道转发到 WebSocket
async fn forward_session_notifications(
    hub: SessionHub,
    session_id: &str,
    _turn_id: &str,
    event_tx: mpsc::UnboundedSender<RelayMessage>,
) {
    let mut rx = hub.ensure_session(session_id).await;

    loop {
        match rx.recv().await {
            Ok(persisted_event) => {
                let envelope_json = serde_json::to_value(&persisted_event.notification)
                    .unwrap_or(serde_json::Value::Null);

                let relay_msg = RelayMessage::EventSessionNotification {
                    id: RelayMessage::new_id("evt"),
                    payload: SessionNotificationPayload {
                        session_id: session_id.to_string(),
                        notification: envelope_json,
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

// ─── 目录浏览实现 ─────────────────────────────────────────

fn browse_directory(path: Option<&str>) -> Result<(String, Vec<BrowseDirectoryEntry>), String> {
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

    let canonical = std::fs::canonicalize(path).map_err(|e| format!("路径规范化失败: {e}"))?;
    let current_path = normalize_display_path(&canonical);

    let read_dir = std::fs::read_dir(&canonical).map_err(|e| format!("无法读取目录: {e}"))?;

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

    entries.sort_by_key(|e| e.name.to_lowercase());
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

// ─── MCP Relay 命令处理 ──────────────────────────────────

impl CommandHandler {
    async fn handle_mcp_list_tools(
        &self,
        id: String,
        payload: CommandMcpListToolsPayload,
    ) -> RelayMessage {
        let mgr = match &self.mcp_manager {
            Some(m) => m,
            None => {
                return RelayMessage::ResponseMcpListTools {
                    id,
                    payload: None,
                    error: Some(RelayError::runtime_error("MCP 管理器未初始化")),
                };
            }
        };
        match mgr.list_tools(&payload.server_name).await {
            Ok(tools) => RelayMessage::ResponseMcpListTools {
                id,
                payload: Some(ResponseMcpListToolsPayload {
                    server_name: payload.server_name,
                    tools,
                }),
                error: None,
            },
            Err(e) => RelayMessage::ResponseMcpListTools {
                id,
                payload: None,
                error: Some(RelayError::runtime_error(e.to_string())),
            },
        }
    }

    async fn handle_mcp_call_tool(
        &self,
        id: String,
        payload: CommandMcpCallToolPayload,
    ) -> RelayMessage {
        let mgr = match &self.mcp_manager {
            Some(m) => m,
            None => {
                return RelayMessage::ResponseMcpCallTool {
                    id,
                    payload: None,
                    error: Some(RelayError::runtime_error("MCP 管理器未初始化")),
                };
            }
        };
        match mgr
            .call_tool(&payload.server_name, &payload.tool_name, payload.arguments)
            .await
        {
            Ok(result) => RelayMessage::ResponseMcpCallTool {
                id,
                payload: Some(result),
                error: None,
            },
            Err(e) => RelayMessage::ResponseMcpCallTool {
                id,
                payload: None,
                error: Some(RelayError::runtime_error(e.to_string())),
            },
        }
    }

    async fn handle_mcp_close(&self, id: String, payload: CommandMcpClosePayload) -> RelayMessage {
        let mgr = match &self.mcp_manager {
            Some(m) => m,
            None => {
                return RelayMessage::ResponseMcpClose {
                    id,
                    payload: None,
                    error: Some(RelayError::runtime_error("MCP 管理器未初始化")),
                };
            }
        };
        match mgr.close(&payload.server_name).await {
            Ok(()) => RelayMessage::ResponseMcpClose {
                id,
                payload: Some(ResponseMcpClosePayload {
                    server_name: payload.server_name,
                    status: "closed".to_string(),
                }),
                error: None,
            },
            Err(e) => RelayMessage::ResponseMcpClose {
                id,
                payload: None,
                error: Some(RelayError::runtime_error(e.to_string())),
            },
        }
    }
}

// ─── MCP Server 解析 ─────────────────────────────────────

/// 从中继 `CommandPromptPayload.mcp_servers` JSON 列表解析出 ACP `McpServer` 列表。
///
/// 仅接受三种显式传输类型:
/// - `"type": "http"` → `McpServer::Http`
/// - `"type": "sse"`  → `McpServer::Sse`
/// - `"type": "stdio"` → `McpServer::Stdio`
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

        let transport = match obj.get("type").and_then(|v| v.as_str()) {
            Some("http") => "http",
            Some("sse") => "sse",
            Some("stdio") => "stdio",
            Some(other) => {
                tracing::warn!(name = %name, transport = %other, "relay MCP server type 非法，跳过");
                continue;
            }
            None => {
                tracing::warn!(name = %name, "relay MCP server 缺少显式 type，跳过");
                continue;
            }
        };

        match transport {
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

                if transport == "http" {
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

#[cfg(test)]
mod tests {
    use super::parse_relay_mcp_servers;

    #[test]
    fn relay_mcp_servers_require_explicit_type() {
        let servers = parse_relay_mcp_servers(&[serde_json::json!({
            "name": "missing-type",
            "url": "http://127.0.0.1:8080/mcp"
        })]);

        assert!(servers.is_empty(), "缺少显式 type 的 MCP server 不应被接受");
    }

    #[test]
    fn relay_mcp_servers_reject_unknown_type() {
        let servers = parse_relay_mcp_servers(&[serde_json::json!({
            "name": "bad-type",
            "type": "ws",
            "url": "ws://127.0.0.1:8080/mcp"
        })]);

        assert!(servers.is_empty(), "未知 type 的 MCP server 不应被接受");
    }
}
