use std::sync::Arc;

use agentdash_application::address_space::{
    ExecRequest, ExecResult, ListOptions, ListResult, MountError, MountOperationContext,
    MountProvider, ReadResult, SearchMatch, SearchQuery, SearchResult, join_root_ref,
    normalize_mount_relative_path, PROVIDER_RELAY_FS,
};
use agentdash_application::runtime::Mount;
use agentdash_relay::{
    RelayMessage, ToolFileListPayload, ToolFileReadPayload, ToolFileWritePayload,
    ToolSearchPayload, ToolShellExecPayload,
};
use async_trait::async_trait;

use crate::relay::registry::BackendRegistry;
use crate::runtime_bridge::relay_file_entries_to_runtime;

fn map_relay_err(e: anyhow::Error) -> MountError {
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
        let path = normalize_mount_relative_path(path, false)
            .map_err(|e| MountError::OperationFailed(e))?;
        let response = self
            .backends
            .send_command(
                &mount.backend_id,
                RelayMessage::CommandToolFileRead {
                    id: RelayMessage::new_id("mp-read"),
                    payload: ToolFileReadPayload {
                        call_id: RelayMessage::new_id("call"),
                        path: path.clone(),
                        workspace_root: mount.root_ref.clone(),
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

    async fn write_text(
        &self,
        mount: &Mount,
        path: &str,
        content: &str,
        _ctx: &MountOperationContext,
    ) -> Result<(), MountError> {
        let path = normalize_mount_relative_path(path, false)
            .map_err(|e| MountError::OperationFailed(e))?;
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
                        workspace_root: mount.root_ref.clone(),
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

    async fn list(
        &self,
        mount: &Mount,
        options: &ListOptions,
        _ctx: &MountOperationContext,
    ) -> Result<ListResult, MountError> {
        let path = normalize_mount_relative_path(&options.path, true)
            .map_err(|e| MountError::OperationFailed(e))?;
        let response = self
            .backends
            .send_command(
                &mount.backend_id,
                RelayMessage::CommandToolFileList {
                    id: RelayMessage::new_id("mp-list"),
                    payload: ToolFileListPayload {
                        call_id: RelayMessage::new_id("call"),
                        path,
                        workspace_root: mount.root_ref.clone(),
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
            Some(p) => normalize_mount_relative_path(p, true)
                .map_err(|e| MountError::OperationFailed(e))?,
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
                        workspace_root: join_root_ref(&mount.root_ref, &base_path),
                        query: query.pattern.clone(),
                        path: None,
                        is_regex: false,
                        include_glob: None,
                        max_results,
                        context_lines: 0,
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
                Ok(SearchResult { matches })
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

    async fn exec(
        &self,
        mount: &Mount,
        request: &ExecRequest,
        _ctx: &MountOperationContext,
    ) -> Result<ExecResult, MountError> {
        let cwd = normalize_mount_relative_path(&request.cwd, true)
            .map_err(|e| MountError::OperationFailed(e))?;
        let response = self
            .backends
            .send_command(
                &mount.backend_id,
                RelayMessage::CommandToolShellExec {
                    id: RelayMessage::new_id("mp-exec"),
                    payload: ToolShellExecPayload {
                        call_id: RelayMessage::new_id("call"),
                        command: request.command.clone(),
                        workspace_root: mount.root_ref.clone(),
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
