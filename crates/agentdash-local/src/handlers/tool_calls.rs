//! PiAgent Tool Call 命令处理——file_read/write/delete/rename/apply_patch/shell_exec/file_list/search

use agentdash_relay::*;
use base64::Engine;
use tokio::sync::mpsc;

use crate::tool_executor::{ToolError, ToolExecutor};

#[derive(Clone)]
pub(super) struct ToolCommandHandler {
    tool_executor: ToolExecutor,
    event_tx: mpsc::UnboundedSender<RelayMessage>,
}

impl ToolCommandHandler {
    pub(super) fn new(
        tool_executor: ToolExecutor,
        event_tx: mpsc::UnboundedSender<RelayMessage>,
    ) -> Self {
        Self {
            tool_executor,
            event_tx,
        }
    }
}

fn tool_error_to_relay_error(error: ToolError) -> RelayError {
    let message = error.to_string();
    match error {
        ToolError::PathNotAccessible(_) => RelayError::new(RelayErrorCode::Forbidden, message),
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
        let call_id = payload.call_id.clone();
        let event_tx = self.event_tx.clone();

        tracing::info!(
            call_id = %payload.call_id,
            cwd = ?payload.cwd,
            command = %payload.command,
            "本机收到 shell_exec"
        );

        match self
            .tool_executor
            .shell_exec_streaming(
                &payload.command,
                &payload.mount_root_ref,
                payload.cwd.as_deref(),
                payload.timeout_ms,
                |delta, stream| {
                    let _ = event_tx.send(RelayMessage::EventToolShellOutput {
                        id: RelayMessage::new_id("shell-out"),
                        payload: ToolShellOutputPayload {
                            call_id: call_id.clone(),
                            delta: delta.to_string(),
                            stream,
                        },
                    });
                },
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
                error: Some(tool_error_to_relay_error(e)),
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
    fn tool_error_to_relay_error_maps_timeout_code() {
        assert_tool_error_code(ToolError::Timeout(1_000), RelayErrorCode::Timeout);
    }
}
