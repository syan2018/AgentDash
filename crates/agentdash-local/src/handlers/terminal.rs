//! 交互式终端命令处理——spawn / input / resize / kill

use std::sync::Arc;

use agentdash_relay::*;

use super::CommandDispatchPlan;
use crate::shell_session_manager::ShellSessionManager;
use crate::tool_executor::ToolExecutor;

fn terminal_kill_status_name(status: ToolShellTerminateStatus) -> &'static str {
    match status {
        ToolShellTerminateStatus::Killed => "killed",
        ToolShellTerminateStatus::AlreadyExited => "already_exited",
        ToolShellTerminateStatus::UnknownSession => "unknown_session",
    }
}

#[derive(Clone)]
pub(super) struct TerminalCommandHandler {
    tool_executor: ToolExecutor,
    shell_sessions: Arc<ShellSessionManager>,
}

impl TerminalCommandHandler {
    pub(super) fn new(
        tool_executor: ToolExecutor,
        shell_sessions: Arc<ShellSessionManager>,
    ) -> Self {
        Self {
            tool_executor,
            shell_sessions,
        }
    }

    pub(super) fn dispatch_plan(msg: &RelayMessage) -> Option<CommandDispatchPlan> {
        match msg {
            RelayMessage::CommandTerminalSpawn { .. }
            | RelayMessage::CommandTerminalInput { .. }
            | RelayMessage::CommandTerminalResize { .. }
            | RelayMessage::CommandTerminalKill { .. }
            | RelayMessage::CommandTerminalInventory { .. } => Some(CommandDispatchPlan::INLINE),
            _ => None,
        }
    }

    pub(super) async fn handle_terminal_spawn(
        &self,
        id: String,
        payload: TerminalSpawnPayload,
    ) -> RelayMessage {
        let workspace_root = match self
            .tool_executor
            .validate_workspace_root(&payload.mount_root_ref)
        {
            Ok(path) => path,
            Err(error) => {
                return RelayMessage::ResponseTerminalSpawn {
                    id,
                    payload: None,
                    error: Some(RelayError::runtime_error(format!(
                        "mount_root_ref 校验失败: {error}"
                    ))),
                };
            }
        };

        match self
            .shell_sessions
            .spawn_terminal(&payload, &workspace_root)
            .await
        {
            Ok(resp) => RelayMessage::ResponseTerminalSpawn {
                id,
                payload: Some(resp),
                error: None,
            },
            Err(e) => RelayMessage::ResponseTerminalSpawn {
                id,
                payload: None,
                error: Some(RelayError::runtime_error(e)),
            },
        }
    }

    pub(super) async fn handle_terminal_input(
        &self,
        id: String,
        payload: TerminalInputPayload,
    ) -> RelayMessage {
        match self
            .shell_sessions
            .input_shell(ToolShellInputPayload {
                session_id: payload.terminal_id.clone(),
                data: payload.data,
                close_stdin: false,
                wait_ms: Some(0),
                max_bytes: None,
            })
            .await
        {
            Ok(_) => RelayMessage::ResponseTerminalInput {
                id,
                payload: Some(TerminalInputResponse {
                    terminal_id: payload.terminal_id,
                }),
                error: None,
            },
            Err(e) => RelayMessage::ResponseTerminalInput {
                id,
                payload: None,
                error: Some(RelayError::runtime_error(e)),
            },
        }
    }

    pub(super) async fn handle_terminal_resize(
        &self,
        id: String,
        payload: TerminalResizePayload,
    ) -> RelayMessage {
        match self.shell_sessions.resize_terminal(&payload).await {
            Ok(()) => RelayMessage::ResponseTerminalResize {
                id,
                payload: Some(TerminalResizeResponse {
                    terminal_id: payload.terminal_id,
                }),
                error: None,
            },
            Err(e) => RelayMessage::ResponseTerminalResize {
                id,
                payload: None,
                error: Some(RelayError::runtime_error(e)),
            },
        }
    }

    pub(super) async fn handle_terminal_kill(
        &self,
        id: String,
        payload: TerminalKillPayload,
    ) -> RelayMessage {
        match self
            .shell_sessions
            .terminate_shell(ToolShellTerminatePayload {
                session_id: payload.terminal_id.clone(),
            })
            .await
        {
            Ok(resp) => RelayMessage::ResponseTerminalKill {
                id,
                payload: Some(TerminalKillResponse {
                    terminal_id: payload.terminal_id,
                    status: terminal_kill_status_name(resp.status).to_string(),
                }),
                error: None,
            },
            Err(e) => RelayMessage::ResponseTerminalKill {
                id,
                payload: None,
                error: Some(RelayError::runtime_error(e)),
            },
        }
    }

    pub(super) async fn handle_terminal_inventory(
        &self,
        id: String,
        payload: TerminalInventoryRequest,
    ) -> RelayMessage {
        RelayMessage::ResponseTerminalInventory {
            id,
            payload: Some(self.shell_sessions.terminal_inventory(&payload).await),
            error: None,
        }
    }
}
