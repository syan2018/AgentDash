//! `inline_fs` mount：从 inline_fs_files 表读取文件，不含 session overlay。

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;

use super::mount::{PROVIDER_INLINE_FS, list_inline_entries, parse_inline_mount_owner};
use super::path::normalize_mount_relative_path;
use super::provider::{
    MountError, MountOperationContext, MountProvider, SearchMatch, SearchQuery, SearchResult,
};
use super::types::{ExecRequest, ExecResult, ListOptions, ListResult, ReadResult};
use crate::runtime::Mount;
use agentdash_domain::inline_file::InlineFileRepository;

fn map_mount_err(e: String) -> MountError {
    MountError::OperationFailed(e)
}

fn map_domain_err(e: agentdash_domain::common::error::DomainError) -> MountError {
    MountError::OperationFailed(e.to_string())
}

/// 从 DB 读取 inline 文件内容并转为 BTreeMap
async fn load_inline_files_from_db(
    repo: &dyn InlineFileRepository,
    mount: &Mount,
) -> Result<BTreeMap<String, String>, MountError> {
    let (owner_kind, owner_id, container_id) =
        parse_inline_mount_owner(mount).map_err(map_mount_err)?;
    let files = repo
        .list_files(owner_kind, owner_id, &container_id)
        .await
        .map_err(map_domain_err)?;
    let mut map = BTreeMap::new();
    for file in files {
        map.insert(file.path, file.content);
    }
    Ok(map)
}

/// 基于 InlineFileRepository 的内联文件提供者（overlay 由上层服务处理）。
pub struct InlineFsMountProvider {
    inline_file_repo: Arc<dyn InlineFileRepository>,
}

impl InlineFsMountProvider {
    pub fn new(inline_file_repo: Arc<dyn InlineFileRepository>) -> Self {
        Self { inline_file_repo }
    }
}

#[async_trait]
impl MountProvider for InlineFsMountProvider {
    fn provider_id(&self) -> &str {
        PROVIDER_INLINE_FS
    }

    async fn read_text(
        &self,
        mount: &Mount,
        path: &str,
        _ctx: &MountOperationContext,
    ) -> Result<ReadResult, MountError> {
        let path = normalize_mount_relative_path(path, false).map_err(map_mount_err)?;
        let (owner_kind, owner_id, container_id) =
            parse_inline_mount_owner(mount).map_err(map_mount_err)?;
        let file = self
            .inline_file_repo
            .get_file(owner_kind, owner_id, &container_id, &path)
            .await
            .map_err(map_domain_err)?;
        let content = file
            .map(|f| f.content)
            .ok_or_else(|| MountError::NotFound(format!("文件不存在: {path}")))?;
        Ok(ReadResult { path, content })
    }

    async fn write_text(
        &self,
        mount: &Mount,
        path: &str,
        _content: &str,
        _ctx: &MountOperationContext,
    ) -> Result<(), MountError> {
        let _ = (mount, path);
        Err(MountError::NotSupported(
            "inline_fs 写入由 RelayVfsService 与 InlineContentOverlay 处理".into(),
        ))
    }

    async fn list(
        &self,
        mount: &Mount,
        options: &ListOptions,
        _ctx: &MountOperationContext,
    ) -> Result<ListResult, MountError> {
        let path = normalize_mount_relative_path(&options.path, true).map_err(map_mount_err)?;
        let files = load_inline_files_from_db(self.inline_file_repo.as_ref(), mount).await?;
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
        mount: &Mount,
        query: &SearchQuery,
        _ctx: &MountOperationContext,
    ) -> Result<SearchResult, MountError> {
        let files = load_inline_files_from_db(self.inline_file_repo.as_ref(), mount).await?;
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
                    line.to_lowercase().contains(&query.pattern.to_lowercase())
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
        _mount: &Mount,
        _request: &ExecRequest,
        _ctx: &MountOperationContext,
    ) -> Result<ExecResult, MountError> {
        Err(MountError::NotSupported(
            "inline_fs 不支持 exec".to_string(),
        ))
    }
}
