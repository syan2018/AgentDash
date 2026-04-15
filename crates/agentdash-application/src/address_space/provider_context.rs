//! `context_vfs` mount: 通过 mount metadata 中预填充的 entries 暴露运行时上下文。
//!
//! session 创建时将 execution_context、story_prd 等运行时数据写入 mount metadata，
//! provider 按 path 查表返回。这样 context:// locator 和其他 VFS locator 走完全相同的链路。

use async_trait::async_trait;
use serde_json::Value;

use super::mount::PROVIDER_CONTEXT_VFS;
use super::path::normalize_mount_relative_path;
use super::provider::{
    MountError, MountOperationContext, MountProvider, SearchMatch, SearchQuery, SearchResult,
};
use super::types::{ExecRequest, ExecResult, ListOptions, ListResult, ReadResult};
use crate::runtime::{Mount, RuntimeFileEntry};

/// 从 mount metadata 中读取预填充的 context entries。
///
/// metadata 格式：
/// ```json
/// { "entries": { "execution_context": "...", "story_prd": "..." } }
/// ```
fn read_entries(mount: &Mount) -> Result<&serde_json::Map<String, Value>, MountError> {
    mount
        .metadata
        .get("entries")
        .and_then(|v| v.as_object())
        .ok_or_else(|| {
            MountError::OperationFailed("context_vfs mount metadata 缺少 entries".to_string())
        })
}

pub struct ContextMountProvider;

#[async_trait]
impl MountProvider for ContextMountProvider {
    fn provider_id(&self) -> &str {
        PROVIDER_CONTEXT_VFS
    }

    async fn read_text(
        &self,
        mount: &Mount,
        path: &str,
        _ctx: &MountOperationContext,
    ) -> Result<ReadResult, MountError> {
        let path_norm =
            normalize_mount_relative_path(path, false).map_err(MountError::OperationFailed)?;
        let entries = read_entries(mount)?;

        let content = entries
            .get(&path_norm)
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                MountError::NotFound(format!("context_vfs 不存在的路径: `{path_norm}`"))
            })?;

        Ok(ReadResult {
            path: path_norm,
            content: content.to_string(),
        })
    }

    async fn write_text(
        &self,
        _mount: &Mount,
        _path: &str,
        _content: &str,
        _ctx: &MountOperationContext,
    ) -> Result<(), MountError> {
        Err(MountError::NotSupported(
            "context_vfs 不支持写入".to_string(),
        ))
    }

    async fn list(
        &self,
        mount: &Mount,
        _options: &ListOptions,
        _ctx: &MountOperationContext,
    ) -> Result<ListResult, MountError> {
        let entries = read_entries(mount)?;

        let file_entries: Vec<RuntimeFileEntry> = entries
            .keys()
            .map(|key| RuntimeFileEntry {
                path: key.clone(),
                size: entries
                    .get(key)
                    .and_then(|v| v.as_str())
                    .map(|s| s.len() as u64),
                modified_at: None,
                is_dir: false,
            })
            .collect();

        Ok(ListResult {
            entries: file_entries,
        })
    }

    async fn search_text(
        &self,
        mount: &Mount,
        query: &SearchQuery,
        _ctx: &MountOperationContext,
    ) -> Result<SearchResult, MountError> {
        let entries = read_entries(mount)?;
        let pattern = &query.pattern;
        if pattern.is_empty() {
            return Ok(SearchResult { matches: vec![] });
        }

        let max = query.max_results.unwrap_or(50);
        let mut matches = Vec::new();

        for (key, value) in entries {
            if matches.len() >= max {
                break;
            }
            let Some(content) = value.as_str() else {
                continue;
            };
            let found = if query.case_sensitive {
                content.contains(pattern.as_str())
            } else {
                content.to_lowercase().contains(&pattern.to_lowercase())
            };
            if found {
                matches.push(SearchMatch {
                    path: key.clone(),
                    line: None,
                    content: content.chars().take(500).collect(),
                });
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
            "context_vfs 不支持 exec".to_string(),
        ))
    }
}
