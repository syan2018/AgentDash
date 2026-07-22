//! PiAgent Tool Call 命令处理——file_read/write/delete/rename/apply_patch/shell_exec/file_list/search

use agentdash_diagnostics::{Subsystem, diag};
use agentdash_relay::*;
use base64::Engine;
use std::sync::Arc;

use crate::shell_session_manager::ShellSessionManager;
use crate::tool_executor::{ToolError, ToolExecutor};

use super::CommandDispatchPlan;

#[derive(Clone)]
pub(super) struct ToolCommandHandler {
    tool_executor: ToolExecutor,
    shell_sessions: Arc<ShellSessionManager>,
}

impl ToolCommandHandler {
    pub(super) fn new(
        tool_executor: ToolExecutor,
        _event_tx: tokio::sync::mpsc::UnboundedSender<RelayMessage>,
        shell_sessions: Arc<ShellSessionManager>,
    ) -> Self {
        Self {
            tool_executor,
            shell_sessions,
        }
    }

    pub(super) fn dispatch_plan(msg: &RelayMessage) -> Option<CommandDispatchPlan> {
        match msg {
            RelayMessage::CommandToolShellExec { .. }
            | RelayMessage::CommandToolShellRead { .. }
            | RelayMessage::CommandToolShellInput { .. }
            | RelayMessage::CommandToolShellTerminate { .. } => {
                Some(CommandDispatchPlan::BACKGROUND)
            }
            RelayMessage::CommandToolFileRead { .. }
            | RelayMessage::CommandToolFileReadBinary { .. }
            | RelayMessage::CommandToolFileWrite { .. }
            | RelayMessage::CommandToolFileDelete { .. }
            | RelayMessage::CommandToolFileRename { .. }
            | RelayMessage::CommandToolApplyPatch { .. }
            | RelayMessage::CommandToolFileList { .. }
            | RelayMessage::CommandToolSearch { .. } => Some(CommandDispatchPlan::INLINE),
            _ => None,
        }
    }
}

fn tool_error_to_relay_error(error: ToolError) -> RelayError {
    let message = error.to_string();
    match error {
        ToolError::PathNotAccessible(_) => RelayError::new(RelayErrorCode::Forbidden, message),
        ToolError::NotFound(_) => RelayError::not_found(message),
        ToolError::InvalidPath(_) => RelayError::invalid_message(message),
        ToolError::Timeout(_) => RelayError::timeout(message),
        ToolError::Io(_) => RelayError::io_error(message),
        ToolError::PatchApply(_) => RelayError::runtime_error(message),
    }
}

impl ToolCommandHandler {
    pub(super) async fn handle_tool_file_read(
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
                error: Some(tool_error_to_relay_error(e)),
            },
        }
    }

    pub(super) async fn handle_tool_file_read_binary(
        &self,
        id: String,
        payload: ToolFileReadPayload,
    ) -> RelayMessage {
        match self
            .tool_executor
            .file_read_binary(&payload.path, &payload.mount_root_ref)
            .await
        {
            Ok(result) => {
                let data_base64 = base64::engine::general_purpose::STANDARD.encode(&result.data);
                RelayMessage::ResponseToolFileReadBinary {
                    id,
                    payload: Some(ToolFileReadBinaryResponse {
                        call_id: payload.call_id,
                        data_base64,
                        mime_type: result.mime_type,
                        size: result.data.len() as u64,
                    }),
                    error: None,
                }
            }
            Err(e) => RelayMessage::ResponseToolFileReadBinary {
                id,
                payload: None,
                error: Some(tool_error_to_relay_error(e)),
            },
        }
    }

    pub(super) async fn handle_tool_file_write(
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
                error: Some(tool_error_to_relay_error(e)),
            },
        }
    }

    pub(super) async fn handle_tool_file_delete(
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
                error: Some(tool_error_to_relay_error(e)),
            },
        }
    }

    pub(super) async fn handle_tool_file_rename(
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
                error: Some(tool_error_to_relay_error(e)),
            },
        }
    }

    pub(super) async fn handle_tool_apply_patch(
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
                error: Some(tool_error_to_relay_error(e)),
            },
        }
    }

    pub(super) async fn handle_tool_shell_exec(
        &self,
        id: String,
        payload: ToolShellExecPayload,
    ) -> RelayMessage {
        diag!(Info, Subsystem::AgentRun,

            call_id = %payload.call_id,
            cwd = ?payload.cwd,
            command = %payload.command,
            "本机收到 shell_exec"
        );

        match self.shell_sessions.start_shell(payload).await {
            Ok(result) => RelayMessage::ResponseToolShellExec {
                id,
                payload: Some(result),
                error: None,
            },
            Err(e) => RelayMessage::ResponseToolShellExec {
                id,
                payload: None,
                error: Some(tool_error_to_relay_error(e)),
            },
        }
    }

    pub(super) async fn handle_tool_shell_read(
        &self,
        id: String,
        payload: ToolShellReadPayload,
    ) -> RelayMessage {
        match self
            .shell_sessions
            .read_session(
                &payload.session_id,
                payload.after_seq,
                payload.wait_ms,
                payload.max_bytes,
            )
            .await
        {
            Ok(result) => RelayMessage::ResponseToolShellRead {
                id,
                payload: Some(result),
                error: None,
            },
            Err(e) => RelayMessage::ResponseToolShellRead {
                id,
                payload: None,
                error: Some(RelayError::runtime_error(e)),
            },
        }
    }

    pub(super) async fn handle_tool_shell_input(
        &self,
        id: String,
        payload: ToolShellInputPayload,
    ) -> RelayMessage {
        match self.shell_sessions.input_shell(payload).await {
            Ok(result) => RelayMessage::ResponseToolShellInput {
                id,
                payload: Some(result),
                error: None,
            },
            Err(e) => RelayMessage::ResponseToolShellInput {
                id,
                payload: None,
                error: Some(RelayError::runtime_error(e)),
            },
        }
    }

    pub(super) async fn handle_tool_shell_terminate(
        &self,
        id: String,
        payload: ToolShellTerminatePayload,
    ) -> RelayMessage {
        match self.shell_sessions.terminate_shell(payload).await {
            Ok(result) => RelayMessage::ResponseToolShellTerminate {
                id,
                payload: Some(result),
                error: None,
            },
            Err(e) => RelayMessage::ResponseToolShellTerminate {
                id,
                payload: None,
                error: Some(RelayError::runtime_error(e)),
            },
        }
    }

    pub(super) async fn handle_tool_file_list(
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
                error: Some(tool_error_to_relay_error(e)),
            },
        }
    }

    pub(super) async fn handle_tool_search(
        &self,
        id: String,
        payload: ToolSearchPayload,
    ) -> RelayMessage {
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
                error: Some(tool_error_to_relay_error(e)),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_tool_error_code(error: ToolError, expected_code: RelayErrorCode) {
        let relay_error = tool_error_to_relay_error(error);
        assert_eq!(relay_error.code, expected_code);
    }

    #[test]
    fn tool_error_to_relay_error_maps_path_not_accessible_code() {
        assert_tool_error_code(
            ToolError::PathNotAccessible("outside-workspace".to_string()),
            RelayErrorCode::Forbidden,
        );
    }

    #[test]
    fn tool_error_to_relay_error_maps_invalid_path_code() {
        assert_tool_error_code(
            ToolError::InvalidPath("bad-path".to_string()),
            RelayErrorCode::InvalidMessage,
        );
    }

    #[test]
    fn tool_error_to_relay_error_maps_missing_path_code() {
        assert_tool_error_code(
            ToolError::NotFound("missing-path".to_string()),
            RelayErrorCode::NotFound,
        );
    }

    #[test]
    fn tool_error_to_relay_error_maps_timeout_code() {
        assert_tool_error_code(ToolError::Timeout(1_000), RelayErrorCode::Timeout);
    }
}
