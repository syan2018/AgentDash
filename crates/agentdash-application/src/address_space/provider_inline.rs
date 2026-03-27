//! `inline_fs` mount：仅从 mount metadata 的 `files` 映射读写，不含 session overlay。

use async_trait::async_trait;

use super::mount::{PROVIDER_INLINE_FS, inline_files_from_mount, list_inline_entries};
use super::path::normalize_mount_relative_path;
use super::provider::{
    MountError, MountOperationContext, MountProvider, SearchMatch, SearchQuery, SearchResult,
};
use super::types::{ExecRequest, ExecResult, ListOptions, ListResult, ReadResult};
use crate::runtime::RuntimeMount;

fn map_mount_err(e: String) -> MountError {
    MountError::OperationFailed(e)
}

/// 仅基于 mount metadata 的内联文件提供者（overlay 由上层服务处理）。
pub struct InlineFsMountProvider;

#[async_trait]
impl MountProvider for InlineFsMountProvider {
    fn provider_id(&self) -> &str {
        PROVIDER_INLINE_FS
    }

    async fn read_text(
        &self,
        mount: &RuntimeMount,
        path: &str,
        _ctx: &MountOperationContext,
    ) -> Result<ReadResult, MountError> {
        let path = normalize_mount_relative_path(path, false).map_err(map_mount_err)?;
        let files = inline_files_from_mount(mount).map_err(map_mount_err)?;
        let content = files
            .get(&path)
            .cloned()
            .ok_or_else(|| MountError::NotFound(format!("文件不存在: {path}")))?;
        Ok(ReadResult { path, content })
    }

    async fn write_text(
        &self,
        mount: &RuntimeMount,
        path: &str,
        _content: &str,
        _ctx: &MountOperationContext,
    ) -> Result<(), MountError> {
        let _ = (mount, path);
        Err(MountError::NotSupported(
            "inline_fs 写入由 RelayAddressSpaceService 与 InlineContentOverlay 处理".into(),
        ))
    }

    async fn list(
        &self,
        mount: &RuntimeMount,
        options: &ListOptions,
        _ctx: &MountOperationContext,
    ) -> Result<ListResult, MountError> {
        let path = normalize_mount_relative_path(&options.path, true).map_err(map_mount_err)?;
        let files = inline_files_from_mount(mount).map_err(map_mount_err)?;
        Ok(ListResult {
            entries: list_inline_entries(
                &files,
                &path,
                options.pattern.as_deref(),
                options.recursive,
            ),
        })
    }

    async fn search_text(
        &self,
        mount: &RuntimeMount,
        query: &SearchQuery,
        _ctx: &MountOperationContext,
    ) -> Result<SearchResult, MountError> {
        let files = inline_files_from_mount(mount).map_err(map_mount_err)?;
        let base_path = match &query.path {
            Some(p) => normalize_mount_relative_path(p, true).map_err(map_mount_err)?,
            None => String::new(),
        };
        let max_results = query.max_results.unwrap_or(usize::MAX);
        let mut matches = Vec::new();

        for (file_path, content) in &files {
            if !file_path.starts_with(base_path.trim_start_matches("./").trim_start_matches('/'))
                && !base_path.is_empty()
                && base_path != "."
            {
                continue;
            }
            let lines: Vec<&str> = content.lines().collect();
            for (idx, line) in lines.iter().enumerate() {
                let matched = if query.case_sensitive {
                    line.contains(&query.pattern)
                } else {
                    line
                        .to_lowercase()
                        .contains(&query.pattern.to_lowercase())
                };
                if matched {
                    matches.push(SearchMatch {
                        path: file_path.clone(),
                        line: Some((idx + 1) as u32),
                        content: line.trim().to_string(),
                    });
                    if matches.len() >= max_results {
                        return Ok(SearchResult { matches });
                    }
                }
            }
        }

        Ok(SearchResult { matches })
    }

    async fn exec(
        &self,
        _mount: &RuntimeMount,
        _request: &ExecRequest,
        _ctx: &MountOperationContext,
    ) -> Result<ExecResult, MountError> {
        Err(MountError::NotSupported(
            "inline_fs 不支持 exec".to_string(),
        ))
    }
}
