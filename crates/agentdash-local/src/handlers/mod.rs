//! 命令处理器——路由云端 relay 命令到各本地执行组件。
//!
//! 按职责域拆分为子模块：
//! - `prompt`：Agent prompt / cancel / discover
//! - `workspace`：workspace 探测 + 目录浏览
//! - `tool_calls`：PiAgent tool call（文件/Shell/搜索）
//! - `mcp_relay`：MCP probe / list_tools / call_tool / close
//! - `materialization`：VFS 资源物化到本机 cache / working copy
//! - `terminal`：交互式终端 spawn / input / resize / kill
//! - `relay_mcp_servers`：relay MCP Server 配置解析

mod materialization;
mod mcp_relay;
mod prompt;
pub(crate) mod relay_mcp_servers;
mod terminal;
mod tool_calls;
mod workspace;
pub use workspace::browse_directory;

use std::collections::HashSet;
use std::sync::Arc;

use agentdash_relay::*;
use tokio::sync::{Mutex, mpsc};

use crate::local_backend_config::WorkspaceContractRuntimeConfig;
use crate::materialization::MaterializationStore;
use crate::mcp_client_manager::McpClientManager;
use crate::terminal_manager::TerminalManager;
use crate::tool_executor::ToolExecutor;
use agentdash_application::session::SessionRuntimeServices;
use agentdash_spi::AgentConnector;

/// 命令处理器，路由云端命令到本地执行组件
#[derive(Clone)]
pub struct CommandHandler {
    pub(crate) tool_executor: ToolExecutor,
    pub(crate) session_runtime: Option<SessionRuntimeServices>,
    pub(crate) connector: Option<Arc<dyn AgentConnector>>,
    pub(crate) mcp_manager: Option<Arc<McpClientManager>>,
    pub(crate) workspace_contract_config: WorkspaceContractRuntimeConfig,
    pub(crate) event_tx: mpsc::UnboundedSender<RelayMessage>,
    pub(crate) terminal_manager: Arc<TerminalManager>,
    pub(crate) materialization_store: Arc<MaterializationStore>,
    pub(crate) session_forwarders: Arc<Mutex<HashSet<String>>>,
}

impl CommandHandler {
    pub fn new(
        backend_id: String,
        tool_executor: ToolExecutor,
        session_runtime: Option<SessionRuntimeServices>,
        connector: Option<Arc<dyn AgentConnector>>,
        mcp_manager: Option<Arc<McpClientManager>>,
        workspace_contract_config: WorkspaceContractRuntimeConfig,
        event_tx: mpsc::UnboundedSender<RelayMessage>,
    ) -> Self {
        let terminal_manager = Arc::new(TerminalManager::new(event_tx.clone()));
        let materialization_store = Arc::new(MaterializationStore::new(backend_id));
        Self {
            tool_executor,
            session_runtime,
            connector,
            mcp_manager,
            workspace_contract_config,
            event_tx,
            terminal_manager,
            materialization_store,
            session_forwarders: Arc::new(Mutex::new(HashSet::new())),
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

    /// 处理一条云端消息，返回零或多条同步响应。
    /// 异步事件（如 SessionNotification）通过 event_tx 推送。
    pub async fn handle(&self, msg: RelayMessage) -> Vec<RelayMessage> {
        match msg {
            // ── 心跳 ──
            RelayMessage::Ping { id, payload } => {
                vec![RelayMessage::Pong {
                    id,
                    payload: PongPayload {
                        client_time: payload.server_time,
                    },
                }]
            }

            // ── Agent prompt / cancel / discover ──
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

            // ── Workspace 探测 + 目录浏览 ──
            RelayMessage::CommandWorkspaceDetect { id, payload } => {
                vec![self.handle_workspace_detect(id, payload).await]
            }
            RelayMessage::CommandWorkspaceDetectGit { id, payload } => {
                vec![self.handle_workspace_detect_git(id, payload).await]
            }
            RelayMessage::CommandBrowseDirectory { id, payload } => {
                vec![self.handle_browse_directory(id, payload).await]
            }

            // ── PiAgent tool calls ──
            RelayMessage::CommandToolFileRead { id, payload } => {
                vec![self.handle_tool_file_read(id, payload).await]
            }
            RelayMessage::CommandToolFileReadBinary { id, payload } => {
                vec![self.handle_tool_file_read_binary(id, payload).await]
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
            RelayMessage::CommandVfsMaterialize { id, payload } => {
                vec![self.handle_vfs_materialize(id, *payload).await]
            }
            RelayMessage::CommandToolFileList { id, payload } => {
                vec![self.handle_tool_file_list(id, payload).await]
            }
            RelayMessage::CommandToolSearch { id, payload } => {
                vec![self.handle_tool_search(id, payload).await]
            }

            // ── MCP relay ──
            RelayMessage::CommandMcpProbeTransport { id, payload } => {
                vec![self.handle_mcp_probe_transport(id, payload).await]
            }
            RelayMessage::CommandMcpListTools { id, payload } => {
                vec![self.handle_mcp_list_tools(id, payload).await]
            }
            RelayMessage::CommandMcpCallTool { id, payload } => {
                vec![self.handle_mcp_call_tool(id, payload).await]
            }
            RelayMessage::CommandMcpClose { id, payload } => {
                vec![self.handle_mcp_close(id, payload).await]
            }

            // ── 交互式终端 ──
            RelayMessage::CommandTerminalSpawn { id, payload } => {
                vec![self.handle_terminal_spawn(id, payload)]
            }
            RelayMessage::CommandTerminalInput { id, payload } => {
                vec![self.handle_terminal_input(id, payload)]
            }
            RelayMessage::CommandTerminalResize { id, payload } => {
                vec![self.handle_terminal_resize(id, payload)]
            }
            RelayMessage::CommandTerminalKill { id, payload } => {
                vec![self.handle_terminal_kill(id, payload)]
            }

            other => {
                tracing::debug!(msg_id = %other.id(), "忽略非命令消息");
                vec![]
            }
        }
    }
}
