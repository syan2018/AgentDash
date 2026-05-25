use std::sync::Arc;

use agentdash_application::runtime::Mount;
use agentdash_application::vfs::{
    ApplyPatchRequest, ApplyPatchResult, BinaryReadResult, ExecRequest, ExecResult, GrepQuery,
    ListOptions, ListResult, MountEditCapabilities, MountError, MountOperationContext,
    MountProvider, PROVIDER_RELAY_FS, ReadResult, SearchMatch, SearchQuery, SearchResult,
    normalize_mount_relative_path,
};
use agentdash_relay::{
    RelayMessage, ToolApplyPatchPayload, ToolFileDeletePayload, ToolFileListPayload,
    ToolFileReadPayload, ToolFileRenamePayload, ToolFileWritePayload, ToolSearchPayload,
    ToolShellExecPayload,
};
use async_trait::async_trait;
use base64::Engine;

use crate::relay::registry::{BackendCommandError, BackendRegistry};
use crate::runtime_bridge::relay_file_entries_to_runtime;

fn map_relay_err(e: BackendCommandError) -> MountError {
    MountError::OperationFailed(e.to_string())
}

/// 通过 `BackendRegistry` 将文件与 shell 操作转发到本机后端。
pub struct RelayFsMountProvider {
    backends: Arc<BackendRegistry>,
}

impl RelayFsMountProvider {
    pub fn new(backends: Arc<BackendRegistry>) -> Self {
        Self { backends }
    }
}

#[async_trait]
impl MountProvider for RelayFsMountProvider {
    fn provider_id(&self) -> &str {
        PROVIDER_RELAY_FS
    }

    fn edit_capabilities(&self, _mount: &Mount) -> MountEditCapabilities {
        MountEditCapabilities {
            create: true,
            delete: true,
            rename: true,
        }
    }

    async fn is_available(&self, mount: &Mount) -> bool {
        if mount.backend_id.is_empty() {
            return true;
        }
        self.backends.is_online(&mount.backend_id).await
    }

    async fn read_text(
        &self,
        mount: &Mount,
        path: &str,
        _ctx: &MountOperationContext,
    ) -> Result<ReadResult, MountError> {
        let path =
            normalize_mount_relative_path(path, false).map_err(MountError::OperationFailed)?;
        let response = self
            .backends
            .send_command(
                &mount.backend_id,
                RelayMessage::CommandToolFileRead {
                    id: RelayMessage::new_id("mp-read"),
                    payload: ToolFileReadPayload {
                        call_id: RelayMessage::new_id("call"),
                        path: path.clone(),
                        mount_root_ref: mount.root_ref.clone(),
                        offset: None,
                        limit: None,
                    },
                },
            )
            .await
            .map_err(map_relay_err)?;

        match response {
            RelayMessage::ResponseToolFileRead {
                payload: Some(payload),
                error: None,
                ..
            } => Ok(ReadResult {
                path,
                content: payload.content,
                attributes: None,
                version_token: None,
                modified_at: None,
            }),
            RelayMessage::ResponseToolFileRead {
                error: Some(error), ..
            } => Err(MountError::OperationFailed(error.message)),
            other => Err(MountError::OperationFailed(format!(
                "file_read 返回意外响应: {}",
                other.id()
            ))),
        }
    }

    async fn read_text_range(
        &self,
        mount: &Mount,
        path: &str,
        offset: usize,
        limit: Option<usize>,
        _ctx: &MountOperationContext,
    ) -> Result<ReadResult, MountError> {
        let path =
            normalize_mount_relative_path(path, false).map_err(MountError::OperationFailed)?;
        let response = self
            .backends
            .send_command(
                &mount.backend_id,
                RelayMessage::CommandToolFileRead {
                    id: RelayMessage::new_id("mp-read-range"),
                    payload: ToolFileReadPayload {
                        call_id: RelayMessage::new_id("call"),
                        path: path.clone(),
                        mount_root_ref: mount.root_ref.clone(),
                        offset: Some(offset as u64),
                        limit: limit.map(|n| n as u64),
                    },
                },
            )
            .await
            .map_err(map_relay_err)?;

        // 远端 backend 是否真按 offset/limit 切片由远端实现决定。如果远端忽略
        // 这两个字段返回全文，本地按行号切片兜底，行为退化为 SPI 默认实现。
        match response {
            RelayMessage::ResponseToolFileRead {
                payload: Some(payload),
                error: None,
                ..
            } => {
                let content = payload.content;
                // 启发式：如果远端返回的内容行数 > offset + limit，说明远端没切，
                // 本地兜底切一刀；否则信任远端结果。
                let line_count = content.lines().count();
                let needs_local_slice = line_count > offset + limit.unwrap_or(0);
                let sliced = if needs_local_slice {
                    let take = limit.unwrap_or(usize::MAX);
                    content
                        .lines()
                        .skip(offset)
                        .take(take)
                        .collect::<Vec<_>>()
                        .join("\n")
                } else {
                    content
                };
                Ok(ReadResult {
                    path,
                    content: sliced,
                    attributes: None,
                    version_token: None,
                    modified_at: None,
                })
            }
            RelayMessage::ResponseToolFileRead {
                error: Some(error), ..
            } => Err(MountError::OperationFailed(error.message)),
            other => Err(MountError::OperationFailed(format!(
                "file_read 返回意外响应: {}",
                other.id()
            ))),
        }
    }

    async fn write_text(
        &self,
        mount: &Mount,
        path: &str,
        content: &str,
        _ctx: &MountOperationContext,
    ) -> Result<(), MountError> {
        let path =
            normalize_mount_relative_path(path, false).map_err(MountError::OperationFailed)?;
        let response = self
            .backends
            .send_command(
                &mount.backend_id,
                RelayMessage::CommandToolFileWrite {
                    id: RelayMessage::new_id("mp-write"),
                    payload: ToolFileWritePayload {
                        call_id: RelayMessage::new_id("call"),
                        path,
                        content: content.to_string(),
                        mount_root_ref: mount.root_ref.clone(),
                    },
                },
            )
            .await
            .map_err(map_relay_err)?;

        match response {
            RelayMessage::ResponseToolFileWrite { error: None, .. } => Ok(()),
            RelayMessage::ResponseToolFileWrite {
                error: Some(error), ..
            } => Err(MountError::OperationFailed(error.message)),
            other => Err(MountError::OperationFailed(format!(
                "file_write 返回意外响应: {}",
                other.id()
            ))),
        }
    }

    async fn read_binary(
        &self,
        mount: &Mount,
        path: &str,
        _ctx: &MountOperationContext,
    ) -> Result<BinaryReadResult, MountError> {
        let path =
            normalize_mount_relative_path(path, false).map_err(MountError::OperationFailed)?;
        let response = self
            .backends
            .send_command(
                &mount.backend_id,
                RelayMessage::CommandToolFileReadBinary {
                    id: RelayMessage::new_id("mp-read-bin"),
                    payload: ToolFileReadPayload {
                        call_id: RelayMessage::new_id("call"),
                        path: path.clone(),
                        mount_root_ref: mount.root_ref.clone(),
                        offset: None,
                        limit: None,
                    },
                },
            )
            .await
            .map_err(map_relay_err)?;

        match response {
            RelayMessage::ResponseToolFileReadBinary {
                payload: Some(payload),
                error: None,
                ..
            } => {
                let data = base64::engine::general_purpose::STANDARD
                    .decode(payload.data_base64)
                    .map_err(|error| {
                        MountError::OperationFailed(format!(
                            "file_read_binary 返回无效 base64: {error}"
                        ))
                    })?;
                Ok(BinaryReadResult::new(path, data, payload.mime_type))
            }
            RelayMessage::ResponseToolFileReadBinary {
                error: Some(error), ..
            } => Err(MountError::OperationFailed(error.message)),
            other => Err(MountError::OperationFailed(format!(
                "file_read_binary 返回意外响应: {}",
                other.id()
            ))),
        }
    }

    async fn delete_text(
        &self,
        mount: &Mount,
        path: &str,
        _ctx: &MountOperationContext,
    ) -> Result<(), MountError> {
        let path =
            normalize_mount_relative_path(path, false).map_err(MountError::OperationFailed)?;
        let response = self
            .backends
            .send_command(
                &mount.backend_id,
                RelayMessage::CommandToolFileDelete {
                    id: RelayMessage::new_id("mp-delete"),
                    payload: ToolFileDeletePayload {
                        call_id: RelayMessage::new_id("call"),
                        path,
                        mount_root_ref: mount.root_ref.clone(),
                    },
                },
            )
            .await
            .map_err(map_relay_err)?;

        match response {
            RelayMessage::ResponseToolFileDelete { error: None, .. } => Ok(()),
            RelayMessage::ResponseToolFileDelete {
                error: Some(error), ..
            } => Err(MountError::OperationFailed(error.message)),
            other => Err(MountError::OperationFailed(format!(
                "file_delete 返回意外响应: {}",
                other.id()
            ))),
        }
    }

    async fn rename_text(
        &self,
        mount: &Mount,
        from_path: &str,
        to_path: &str,
        _ctx: &MountOperationContext,
    ) -> Result<(), MountError> {
        let from_path =
            normalize_mount_relative_path(from_path, false).map_err(MountError::OperationFailed)?;
        let to_path =
            normalize_mount_relative_path(to_path, false).map_err(MountError::OperationFailed)?;
        let response = self
            .backends
            .send_command(
                &mount.backend_id,
                RelayMessage::CommandToolFileRename {
                    id: RelayMessage::new_id("mp-rename"),
                    payload: ToolFileRenamePayload {
                        call_id: RelayMessage::new_id("call"),
                        from_path,
                        to_path,
                        mount_root_ref: mount.root_ref.clone(),
                    },
                },
            )
            .await
            .map_err(map_relay_err)?;

        match response {
            RelayMessage::ResponseToolFileRename { error: None, .. } => Ok(()),
            RelayMessage::ResponseToolFileRename {
                error: Some(error), ..
            } => Err(MountError::OperationFailed(error.message)),
            other => Err(MountError::OperationFailed(format!(
                "file_rename 返回意外响应: {}",
                other.id()
            ))),
        }
    }

    async fn apply_patch(
        &self,
        mount: &Mount,
        request: &ApplyPatchRequest,
        _ctx: &MountOperationContext,
    ) -> Result<ApplyPatchResult, MountError> {
        let response = self
            .backends
            .send_command(
                &mount.backend_id,
                RelayMessage::CommandToolApplyPatch {
                    id: RelayMessage::new_id("mp-apply-patch"),
                    payload: ToolApplyPatchPayload {
                        call_id: RelayMessage::new_id("call"),
                        patch: request.patch.clone(),
                        mount_root_ref: mount.root_ref.clone(),
                    },
                },
            )
            .await
            .map_err(map_relay_err)?;

        match response {
            RelayMessage::ResponseToolApplyPatch {
                payload: Some(payload),
                error: None,
                ..
            } => Ok(ApplyPatchResult {
                added: payload.added,
                modified: payload.modified,
                deleted: payload.deleted,
            }),
            RelayMessage::ResponseToolApplyPatch {
                error: Some(error), ..
            } => Err(MountError::OperationFailed(error.message)),
            other => Err(MountError::OperationFailed(format!(
                "apply_patch 返回意外响应: {}",
                other.id()
            ))),
        }
    }

    async fn list(
        &self,
        mount: &Mount,
        options: &ListOptions,
        _ctx: &MountOperationContext,
    ) -> Result<ListResult, MountError> {
        let path = normalize_mount_relative_path(&options.path, true)
            .map_err(MountError::OperationFailed)?;
        let response = self
            .backends
            .send_command(
                &mount.backend_id,
                RelayMessage::CommandToolFileList {
                    id: RelayMessage::new_id("mp-list"),
                    payload: ToolFileListPayload {
                        call_id: RelayMessage::new_id("call"),
                        path,
                        mount_root_ref: mount.root_ref.clone(),
                        pattern: options.pattern.clone(),
                        recursive: options.recursive,
                    },
                },
            )
            .await
            .map_err(map_relay_err)?;

        match response {
            RelayMessage::ResponseToolFileList {
                payload: Some(payload),
                error: None,
                ..
            } => Ok(ListResult {
                entries: relay_file_entries_to_runtime(&payload.entries),
            }),
            RelayMessage::ResponseToolFileList {
                error: Some(error), ..
            } => Err(MountError::OperationFailed(error.message)),
            other => Err(MountError::OperationFailed(format!(
                "file_list 返回意外响应: {}",
                other.id()
            ))),
        }
    }

    async fn search_text(
        &self,
        mount: &Mount,
        query: &SearchQuery,
        _ctx: &MountOperationContext,
    ) -> Result<SearchResult, MountError> {
        let base_path = match &query.path {
            Some(p) => {
                normalize_mount_relative_path(p, true).map_err(MountError::OperationFailed)?
            }
            None => String::new(),
        };
        let max_results = query.max_results.unwrap_or(50);
        let response = self
            .backends
            .send_command(
                &mount.backend_id,
                RelayMessage::CommandToolSearch {
                    id: RelayMessage::new_id("mp-search"),
                    payload: ToolSearchPayload {
                        call_id: RelayMessage::new_id("call"),
                        mount_root_ref: mount.root_ref.clone(),
                        query: query.pattern.clone(),
                        path: if base_path.is_empty() {
                            None
                        } else {
                            Some(base_path)
                        },
                        is_regex: false,
                        include_glob: None,
                        max_results,
                        context_lines: 0,
                        case_sensitive: query.case_sensitive,
                        multiline: false,
                        before_lines: 0,
                        after_lines: 0,
                    },
                },
            )
            .await
            .map_err(map_relay_err)?;

        match response {
            RelayMessage::ResponseToolSearch {
                payload: Some(payload),
                error: None,
                ..
            } => {
                let matches = payload
                    .hits
                    .into_iter()
                    .map(|hit| SearchMatch {
                        path: hit.path,
                        line: u32::try_from(hit.line_number).ok(),
                        content: hit.content,
                    })
                    .collect();
                Ok(SearchResult {
                    matches,
                    truncated: false,
                })
            }
            RelayMessage::ResponseToolSearch {
                error: Some(error), ..
            } => Err(MountError::OperationFailed(error.message)),
            other => Err(MountError::OperationFailed(format!(
                "search 返回意外响应: {}",
                other.id()
            ))),
        }
    }

    async fn grep_text(
        &self,
        mount: &Mount,
        query: &GrepQuery,
        _ctx: &MountOperationContext,
    ) -> Result<SearchResult, MountError> {
        let base_path = match &query.base.path {
            Some(p) => {
                normalize_mount_relative_path(p, true).map_err(MountError::OperationFailed)?
            }
            None => String::new(),
        };
        let max_results = query.base.max_results.unwrap_or(50);
        let response = self
            .backends
            .send_command(
                &mount.backend_id,
                RelayMessage::CommandToolSearch {
                    id: RelayMessage::new_id("mp-grep"),
                    payload: ToolSearchPayload {
                        call_id: RelayMessage::new_id("call"),
                        mount_root_ref: mount.root_ref.clone(),
                        query: query.base.pattern.clone(),
                        path: if base_path.is_empty() {
                            None
                        } else {
                            Some(base_path)
                        },
                        // A7 决议：grep_text 的 pattern 始终视为正则。
                        is_regex: true,
                        include_glob: query.include_glob.clone(),
                        max_results,
                        context_lines: query.context_lines,
                        case_sensitive: query.base.case_sensitive,
                        multiline: query.multiline,
                        before_lines: query.before_lines,
                        after_lines: query.after_lines,
                    },
                },
            )
            .await
            .map_err(map_relay_err)?;

        match response {
            RelayMessage::ResponseToolSearch {
                payload: Some(payload),
                error: None,
                ..
            } => {
                let matches = payload
                    .hits
                    .into_iter()
                    .map(|hit| SearchMatch {
                        path: hit.path,
                        line: u32::try_from(hit.line_number).ok(),
                        content: hit.content,
                    })
                    .collect();
                Ok(SearchResult {
                    matches,
                    truncated: false,
                })
            }
            RelayMessage::ResponseToolSearch {
                error: Some(error), ..
            } => Err(MountError::OperationFailed(error.message)),
            other => Err(MountError::OperationFailed(format!(
                "grep 返回意外响应: {}",
                other.id()
            ))),
        }
    }

    async fn exec(
        &self,
        mount: &Mount,
        request: &ExecRequest,
        _ctx: &MountOperationContext,
    ) -> Result<ExecResult, MountError> {
        let cwd = normalize_mount_relative_path(&request.cwd, true)
            .map_err(MountError::OperationFailed)?;
        let call_id = request
            .streaming_call_id
            .clone()
            .unwrap_or_else(|| RelayMessage::new_id("call"));
        tracing::info!(
            backend_id = %mount.backend_id,
            mount_id = %mount.id,
            cwd = %cwd,
            command = %request.command,
            "relay_fs 下发 shell_exec"
        );
        let response = self
            .backends
            .send_command(
                &mount.backend_id,
                RelayMessage::CommandToolShellExec {
                    id: RelayMessage::new_id("mp-exec"),
                    payload: ToolShellExecPayload {
                        call_id,
                        command: request.command.clone(),
                        mount_root_ref: mount.root_ref.clone(),
                        cwd: if cwd.is_empty() { None } else { Some(cwd) },
                        timeout_ms: request.timeout_ms,
                    },
                },
            )
            .await
            .map_err(map_relay_err)?;

        match response {
            RelayMessage::ResponseToolShellExec {
                payload: Some(payload),
                error: None,
                ..
            } => Ok(ExecResult {
                exit_code: payload.exit_code,
                stdout: payload.stdout,
                stderr: payload.stderr,
            }),
            RelayMessage::ResponseToolShellExec {
                error: Some(error), ..
            } => Err(MountError::OperationFailed(error.message)),
            other => Err(MountError::OperationFailed(format!(
                "shell_exec 返回意外响应: {}",
                other.id()
            ))),
        }
    }
}
