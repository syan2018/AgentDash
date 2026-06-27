//! 本机 relay 命令 router。
//!
//! 按职责域拆分为子模块：
//! - `prompt`：Agent prompt / cancel / discover
//! - `workspace`：workspace 探测 + 目录浏览
//! - `tool_calls`：PiAgent tool call（文件/Shell/搜索）
//! - `mcp_relay`：MCP probe / list_tools / call_tool / close
//! - `materialization`：VFS 资源物化到本机 cache / working copy
//! - `terminal`：交互式终端 spawn / input / resize / kill
//! - `relay_mcp_servers`：relay MCP Server typed DTO 转换

mod extension;
mod materialization;
mod mcp_relay;
mod prompt;
pub(crate) mod relay_mcp_servers;
mod terminal;
mod tool_calls;
mod workspace;
pub use workspace::browse_directory;

use agentdash_diagnostics::{Subsystem, diag};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use agentdash_relay::*;
use tokio::sync::{Mutex, mpsc};

use extension::{ExtensionCommandHandler, ExtensionCommandHandlerConfig};
use materialization::MaterializationCommandHandler;
use mcp_relay::McpCommandHandler;
use prompt::{PromptCommandHandler, PromptCommandHandlerConfig};
use terminal::TerminalCommandHandler;
use tool_calls::ToolCommandHandler;
use workspace::WorkspaceCommandHandler;

use crate::LocalExtensionHostManager;
use crate::local_backend_config::WorkspaceContractRuntimeConfig;
use crate::materialization::MaterializationStore;
use crate::mcp_client_manager::McpClientManager;
use crate::shell_session_manager::ShellSessionManager;
use crate::tool_executor::ToolExecutor;
use agentdash_application_runtime_session::session::SessionRuntimeServices;
use agentdash_spi::AgentConnector;

/// 本机命令 router，只负责 relay envelope 分发。
#[derive(Clone)]
pub struct LocalCommandRouter {
    prompt: PromptCommandHandler,
    workspace: WorkspaceCommandHandler,
    tool: ToolCommandHandler,
    materialization: MaterializationCommandHandler,
    mcp: McpCommandHandler,
    extension: ExtensionCommandHandler,
    terminal: TerminalCommandHandler,
}

pub struct LocalCommandRouterConfig {
    pub backend_id: String,
    pub workspace_roots: Vec<PathBuf>,
    pub tool_executor: ToolExecutor,
    pub session_runtime: Option<SessionRuntimeServices>,
    pub connector: Option<Arc<dyn AgentConnector>>,
    pub mcp_manager: Option<Arc<McpClientManager>>,
    pub workspace_contract_config: WorkspaceContractRuntimeConfig,
    pub extension_host: LocalExtensionHostManager,
    pub extension_artifact_api_base_url: String,
    pub extension_artifact_access_token: String,
    pub extension_artifact_cache_root: PathBuf,
    pub event_tx: mpsc::UnboundedSender<RelayMessage>,
}

impl LocalCommandRouter {
    pub fn new(config: LocalCommandRouterConfig) -> Self {
        let shell_session_manager = Arc::new(ShellSessionManager::new(
            config.tool_executor.clone(),
            config.event_tx.clone(),
        ));
        let materialization_store = Arc::new(MaterializationStore::new(config.backend_id.clone()));
        let session_forwarders = Arc::new(Mutex::new(HashSet::new()));

        Self {
            prompt: PromptCommandHandler::new(PromptCommandHandlerConfig {
                session_runtime: config.session_runtime,
                connector: config.connector,
                tool_executor: config.tool_executor.clone(),
                workspace_contract_config: config.workspace_contract_config,
                event_tx: config.event_tx.clone(),
                session_forwarders,
            }),
            workspace: WorkspaceCommandHandler,
            tool: ToolCommandHandler::new(
                config.tool_executor.clone(),
                config.event_tx.clone(),
                Arc::clone(&shell_session_manager),
            ),
            materialization: MaterializationCommandHandler::new(materialization_store),
            mcp: McpCommandHandler::new(config.mcp_manager),
            extension: ExtensionCommandHandler::new(ExtensionCommandHandlerConfig {
                backend_id: config.backend_id,
                workspace_roots: config.workspace_roots,
                extension_host: config.extension_host,
                artifact_api_base_url: config.extension_artifact_api_base_url,
                artifact_access_token: config.extension_artifact_access_token,
                artifact_cache_root: config.extension_artifact_cache_root,
            }),
            terminal: TerminalCommandHandler::new(config.tool_executor, shell_session_manager),
        }
    }

    pub fn list_executors(&self) -> Vec<AgentInfoRelay> {
        self.prompt.list_executors()
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
                vec![self.prompt.handle_prompt(id, *payload).await]
            }
            RelayMessage::CommandCancel { id, payload } => {
                vec![self.prompt.handle_cancel(id, payload).await]
            }
            RelayMessage::CommandSteer { id, payload } => {
                vec![self.prompt.handle_steer(id, payload).await]
            }
            RelayMessage::CommandDiscover { id, .. } => {
                vec![self.prompt.handle_discover(id).await]
            }
            RelayMessage::CommandDiscoverOptions { id, payload } => {
                vec![self.prompt.handle_discover_options(id, payload).await]
            }

            // ── Workspace 探测 + 目录浏览 ──
            RelayMessage::CommandWorkspaceDetect { id, payload } => {
                vec![self.workspace.handle_workspace_detect(id, payload).await]
            }
            RelayMessage::CommandWorkspaceDetectGit { id, payload } => {
                vec![
                    self.workspace
                        .handle_workspace_detect_git(id, payload)
                        .await,
                ]
            }
            RelayMessage::CommandWorkspaceDiscoverByIdentity { id, payload } => {
                vec![
                    self.workspace
                        .handle_workspace_discover_by_identity(id, payload)
                        .await,
                ]
            }
            RelayMessage::CommandBrowseDirectory { id, payload } => {
                vec![self.workspace.handle_browse_directory(id, payload).await]
            }

            // ── PiAgent tool calls ──
            RelayMessage::CommandToolFileRead { id, payload } => {
                vec![self.tool.handle_tool_file_read(id, payload).await]
            }
            RelayMessage::CommandToolFileReadBinary { id, payload } => {
                vec![self.tool.handle_tool_file_read_binary(id, payload).await]
            }
            RelayMessage::CommandToolFileWrite { id, payload } => {
                vec![self.tool.handle_tool_file_write(id, payload).await]
            }
            RelayMessage::CommandToolFileDelete { id, payload } => {
                vec![self.tool.handle_tool_file_delete(id, payload).await]
            }
            RelayMessage::CommandToolFileRename { id, payload } => {
                vec![self.tool.handle_tool_file_rename(id, payload).await]
            }
            RelayMessage::CommandToolApplyPatch { id, payload } => {
                vec![self.tool.handle_tool_apply_patch(id, payload).await]
            }
            RelayMessage::CommandToolShellExec { id, payload } => {
                vec![self.tool.handle_tool_shell_exec(id, payload).await]
            }
            RelayMessage::CommandToolShellRead { id, payload } => {
                vec![self.tool.handle_tool_shell_read(id, payload).await]
            }
            RelayMessage::CommandToolShellInput { id, payload } => {
                vec![self.tool.handle_tool_shell_input(id, payload).await]
            }
            RelayMessage::CommandToolShellTerminate { id, payload } => {
                vec![self.tool.handle_tool_shell_terminate(id, payload).await]
            }
            RelayMessage::CommandVfsMaterialize { id, payload } => {
                vec![
                    self.materialization
                        .handle_vfs_materialize(id, *payload)
                        .await,
                ]
            }
            RelayMessage::CommandToolFileList { id, payload } => {
                vec![self.tool.handle_tool_file_list(id, payload).await]
            }
            RelayMessage::CommandToolSearch { id, payload } => {
                vec![self.tool.handle_tool_search(id, payload).await]
            }

            // ── MCP relay ──
            RelayMessage::CommandMcpProbeTransport { id, payload } => {
                vec![self.mcp.handle_mcp_probe_transport(id, payload).await]
            }
            RelayMessage::CommandMcpListTools { id, payload } => {
                vec![self.mcp.handle_mcp_list_tools(id, payload).await]
            }
            RelayMessage::CommandMcpCallTool { id, payload } => {
                vec![self.mcp.handle_mcp_call_tool(id, payload).await]
            }
            RelayMessage::CommandMcpClose { id, payload } => {
                vec![self.mcp.handle_mcp_close(id, payload).await]
            }

            RelayMessage::CommandExtensionActionInvoke { id, payload } => {
                vec![
                    self.extension
                        .handle_extension_action_invoke(id, payload)
                        .await,
                ]
            }
            RelayMessage::CommandExtensionChannelInvoke { id, payload } => {
                vec![
                    self.extension
                        .handle_extension_channel_invoke(id, payload)
                        .await,
                ]
            }

            // ── 交互式终端 ──
            RelayMessage::CommandTerminalSpawn { id, payload } => {
                vec![self.terminal.handle_terminal_spawn(id, payload).await]
            }
            RelayMessage::CommandTerminalInput { id, payload } => {
                vec![self.terminal.handle_terminal_input(id, payload).await]
            }
            RelayMessage::CommandTerminalResize { id, payload } => {
                vec![self.terminal.handle_terminal_resize(id, payload).await]
            }
            RelayMessage::CommandTerminalKill { id, payload } => {
                vec![self.terminal.handle_terminal_kill(id, payload).await]
            }

            other => {
                diag!(Debug, Subsystem::AgentRun,
        msg_id = %other.id(), "忽略非命令消息");
                vec![]
            }
        }
    }
}
