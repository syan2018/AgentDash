//! 交互式终端命令处理——spawn / input / resize / kill

use agentdash_relay::*;

use super::CommandHandler;

impl CommandHandler {
    pub(super) fn handle_terminal_spawn(
        &self,
        id: String,
        payload: TerminalSpawnPayload,
    ) -> RelayMessage {
        let workspace_root = &payload.mount_root_ref;
        match self.terminal_manager.spawn(&payload, workspace_root) {
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

    pub(super) fn handle_terminal_input(
        &self,
        id: String,
        payload: TerminalInputPayload,
    ) -> RelayMessage {
        match self.terminal_manager.input(&payload) {
            Ok(resp) => RelayMessage::ResponseTerminalInput {
                id,
                payload: Some(resp),
                error: None,
            },
            Err(e) => RelayMessage::ResponseTerminalInput {
                id,
                payload: None,
                error: Some(RelayError::runtime_error(e)),
            },
        }
    }

    pub(super) fn handle_terminal_resize(
        &self,
        id: String,
        payload: TerminalResizePayload,
    ) -> RelayMessage {
        match self.terminal_manager.resize(&payload) {
            Ok(resp) => RelayMessage::ResponseTerminalResize {
                id,
                payload: Some(resp),
                error: None,
            },
            Err(e) => RelayMessage::ResponseTerminalResize {
                id,
                payload: None,
                error: Some(RelayError::runtime_error(e)),
            },
        }
    }

    pub(super) fn handle_terminal_kill(
        &self,
        id: String,
        payload: TerminalKillPayload,
    ) -> RelayMessage {
        match self.terminal_manager.kill(&payload) {
            Ok(resp) => RelayMessage::ResponseTerminalKill {
                id,
                payload: Some(resp),
                error: None,
            },
            Err(e) => RelayMessage::ResponseTerminalKill {
                id,
                payload: None,
                error: Some(RelayError::runtime_error(e)),
            },
        }
    }
}
