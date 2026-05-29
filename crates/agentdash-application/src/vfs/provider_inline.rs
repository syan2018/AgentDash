//! `inline_fs` mount：从 inline_fs_files 表读取文件，不含 session overlay。

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;

use super::mount::{PROVIDER_INLINE_FS, list_inline_entries, parse_inline_mount_owner};
use super::path::normalize_mount_relative_path;
use super::provider::{
    GrepQuery, MountError, MountOperationContext, MountProvider, SearchMatch, SearchQuery,
    SearchResult,
};
use super::types::{
    BinaryReadResult, ExecRequest, ExecResult, ListOptions, ListResult, ReadResult,
};
use crate::runtime::Mount;
use agentdash_domain::inline_file::{
    InlineFile, InlineFileContent, InlineFileContentKind, InlineFileRepository,
};

fn map_mount_err(e: String) -> MountError {
    MountError::OperationFailed(e)
}

fn map_domain_err(e: agentdash_domain::common::error::DomainError) -> MountError {
    MountError::OperationFailed(e.to_string())
}

async fn load_inline_files_from_db(
    repo: &dyn InlineFileRepository,
    mount: &Mount,
) -> Result<Vec<InlineFile>, MountError> {
    let (owner_kind, owner_id, container_id) =
        parse_inline_mount_owner(mount).map_err(map_mount_err)?;
    repo.list_files(owner_kind, owner_id, &container_id)
        .await
        .map_err(map_domain_err)
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
            .map_err(map_domain_err)?
            .ok_or_else(|| MountError::NotFound(format!("文件不存在: {path}")))?;
        let updated_at_ms = file.updated_at.timestamp_millis();
        let size_bytes = file.size_bytes;
        let content = match file.content {
            InlineFileContent::Text { content } => content,
            InlineFileContent::Binary { .. } => {
                return Err(MountError::NotSupported(format!(
                    "文件是二进制内容，不能按文本读取: {path}"
                )));
            }
        };
        Ok(ReadResult::new(path, content)
            .with_version_token(format!("ts:{}:{}", updated_at_ms, size_bytes))
            .with_modified_at(updated_at_ms))
    }

    async fn read_binary(
        &self,
        mount: &Mount,
        path: &str,
        _ctx: &MountOperationContext,
    ) -> Result<BinaryReadResult, MountError> {
        let path = normalize_mount_relative_path(path, false).map_err(map_mount_err)?;
        let (owner_kind, owner_id, container_id) =
            parse_inline_mount_owner(mount).map_err(map_mount_err)?;
        let file = self
            .inline_file_repo
            .get_file(owner_kind, owner_id, &container_id, &path)
            .await
            .map_err(map_domain_err)?
            .ok_or_else(|| MountError::NotFound(format!("文件不存在: {path}")))?;
        let attrs = inline_file_attributes(&file);
        let (bytes, mime_type) = match file.content {
            InlineFileContent::Binary { bytes, mime_type } => (bytes, mime_type),
            InlineFileContent::Text { .. } => {
                return Err(MountError::NotSupported(format!(
                    "文件是文本内容，不能按二进制读取: {path}"
                )));
            }
        };
        Ok(BinaryReadResult::new(path, bytes, mime_type).with_attributes(attrs))
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
            "inline_fs 写入由 VfsService 与 InlineContentOverlay 处理".into(),
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
            entries: list_inline_file_entries(
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
        // 通用搜索：substring 匹配（不识别 regex / glob / context / multiline）。
        // grep 风格的搜索请走 grep_text。
        let files = load_inline_files_from_db(self.inline_file_repo.as_ref(), mount).await?;
        let base_path = match &query.path {
            Some(p) => normalize_mount_relative_path(p, true).map_err(map_mount_err)?,
            None => String::new(),
        };
        let max_results = query.max_results.unwrap_or(usize::MAX);
        let mut matches = Vec::new();

        for file in &files {
            let InlineFileContent::Text { content } = &file.content else {
                continue;
            };
            let file_path = &file.path;
            if !file_path.starts_with(base_path.trim_start_matches("./").trim_start_matches('/'))
                && !base_path.is_empty()
                && base_path != "."
            {
                continue;
            }
            for (idx, line) in content.lines().enumerate() {
                let matched = if query.case_sensitive {
                    line.contains(&query.pattern)
                } else {
                    line.to_lowercase().contains(&query.pattern.to_lowercase())
                };
                if !matched {
                    continue;
                }
                matches.push(SearchMatch {
                    path: file_path.clone(),
                    line: Some((idx + 1) as u32),
                    content: line.trim().to_string(),
                });
                if matches.len() >= max_results {
                    return Ok(SearchResult {
                        matches,
                        truncated: true,
                    });
                }
            }
        }
        Ok(SearchResult {
            matches,
            truncated: false,
        })
    }

    async fn grep_text(
        &self,
        mount: &Mount,
        query: &GrepQuery,
        _ctx: &MountOperationContext,
    ) -> Result<SearchResult, MountError> {
        // grep 风格：pattern 始终正则；支持 case_insensitive / include_glob /
        // before/after/context_lines / multiline。inline 是唯一原生实现 grep_text
        // 的内置 provider；其它 provider 走 trait 默认 forward + warn。
        let files = load_inline_files_from_db(self.inline_file_repo.as_ref(), mount).await?;
        let base_path = match &query.base.path {
            Some(p) => normalize_mount_relative_path(p, true).map_err(map_mount_err)?,
            None => String::new(),
        };
        let max_results = query.base.max_results.unwrap_or(usize::MAX);
        let mut matches = Vec::new();

        let mut builder = regex::RegexBuilder::new(&query.base.pattern);
        builder
            .case_insensitive(!query.base.case_sensitive)
            .multi_line(query.multiline)
            .dot_matches_new_line(query.multiline);
        let re = builder
            .build()
            .map_err(|e| MountError::OperationFailed(format!("无效正则: {e}")))?;

        let glob_matcher = match query.include_glob.as_deref() {
            Some(pat) => Some(
                globset::Glob::new(pat)
                    .map_err(|e| MountError::OperationFailed(format!("无效 glob: {e}")))?
                    .compile_matcher(),
            ),
            None => None,
        };

        let before = query.before_lines.max(query.context_lines);
        let after = query.after_lines.max(query.context_lines);

        for file in &files {
            let InlineFileContent::Text { content } = &file.content else {
                continue;
            };
            let file_path = &file.path;
            if !file_path.starts_with(base_path.trim_start_matches("./").trim_start_matches('/'))
                && !base_path.is_empty()
                && base_path != "."
            {
                continue;
            }
            if let Some(matcher) = &glob_matcher
                && !matcher.is_match(file_path.as_str())
            {
                continue;
            }
            let lines: Vec<&str> = content.lines().collect();
            for (idx, line) in lines.iter().enumerate() {
                if !re.is_match(line) {
                    continue;
                }
                let start = idx.saturating_sub(before);
                for (ctx_idx, ctx_line) in lines.iter().enumerate().take(idx).skip(start) {
                    matches.push(SearchMatch {
                        path: file_path.clone(),
                        line: Some((ctx_idx + 1) as u32),
                        content: ctx_line.trim().to_string(),
                    });
                }
                matches.push(SearchMatch {
                    path: file_path.clone(),
                    line: Some((idx + 1) as u32),
                    content: line.trim().to_string(),
                });
                let end = (idx + 1 + after).min(lines.len());
                for (ctx_idx, ctx_line) in lines.iter().enumerate().take(end).skip(idx + 1) {
                    matches.push(SearchMatch {
                        path: file_path.clone(),
                        line: Some((ctx_idx + 1) as u32),
                        content: ctx_line.trim().to_string(),
                    });
                }
                if matches.len() >= max_results {
                    return Ok(SearchResult {
                        matches,
                        truncated: true,
                    });
                }
            }
        }

        Ok(SearchResult {
            matches,
            truncated: false,
        })
    }

    async fn stat(
        &self,
        mount: &Mount,
        path: &str,
        _ctx: &MountOperationContext,
    ) -> Result<crate::runtime::RuntimeFileEntry, MountError> {
        let path = normalize_mount_relative_path(path, false).map_err(map_mount_err)?;
        let (owner_kind, owner_id, container_id) =
            parse_inline_mount_owner(mount).map_err(map_mount_err)?;
        let file = self
            .inline_file_repo
            .get_file(owner_kind, owner_id, &container_id, &path)
            .await
            .map_err(map_domain_err)?
            .ok_or_else(|| MountError::NotFound(format!("文件不存在: {path}")))?;
        Ok(crate::runtime::RuntimeFileEntry::file(path)
            .with_size(file.size_bytes)
            .with_attributes(inline_file_attributes(&file)))
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

fn list_inline_file_entries(
    files: &[InlineFile],
    base_path: &str,
    pattern: Option<&str>,
    recursive: bool,
) -> Vec<crate::runtime::RuntimeFileEntry> {
    let mut text_files = BTreeMap::new();
    for file in files {
        text_files.insert(file.path.clone(), String::new());
    }
    let mut entries = list_inline_entries(&text_files, base_path, pattern, recursive);
    for entry in &mut entries {
        if entry.is_dir {
            continue;
        }
        if let Some(file) = files.iter().find(|file| file.path == entry.path) {
            entry.size = Some(file.size_bytes);
            entry.modified_at = Some(file.updated_at.timestamp_millis());
            entry.attributes = Some(inline_file_attributes(file));
        }
    }
    entries
}

fn inline_file_attributes(file: &InlineFile) -> serde_json::Map<String, serde_json::Value> {
    let mut attrs = serde_json::Map::new();
    attrs.insert(
        "content_kind".to_string(),
        serde_json::Value::String(file.content_kind_str().to_string()),
    );
    if let Some(mime_type) = file.mime_type() {
        attrs.insert(
            "mime_type".to_string(),
            serde_json::Value::String(mime_type.to_string()),
        );
    } else if file.content_kind() == InlineFileContentKind::Text {
        attrs.insert(
            "mime_type".to_string(),
            serde_json::Value::String("text/plain; charset=utf-8".to_string()),
        );
    }
    attrs
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use agentdash_domain::common::error::DomainError;
    use agentdash_domain::inline_file::{InlineFileOwnerKind, InlineFileRepository};
    use tokio::sync::Mutex;
    use uuid::Uuid;

    use super::*;

    #[derive(Default)]
    struct MemoryInlineFileRepo {
        files: Mutex<Vec<InlineFile>>,
    }

    #[async_trait::async_trait]
    impl InlineFileRepository for MemoryInlineFileRepo {
        async fn get_file(
            &self,
            owner_kind: InlineFileOwnerKind,
            owner_id: Uuid,
            container_id: &str,
            path: &str,
        ) -> Result<Option<InlineFile>, DomainError> {
            Ok(self
                .files
                .lock()
                .await
                .iter()
                .find(|file| {
                    file.owner_kind == owner_kind
                        && file.owner_id == owner_id
                        && file.container_id == container_id
                        && file.path == path
                })
                .cloned())
        }

        async fn list_files(
            &self,
            owner_kind: InlineFileOwnerKind,
            owner_id: Uuid,
            container_id: &str,
        ) -> Result<Vec<InlineFile>, DomainError> {
            Ok(self
                .files
                .lock()
                .await
                .iter()
                .filter(|file| {
                    file.owner_kind == owner_kind
                        && file.owner_id == owner_id
                        && file.container_id == container_id
                })
                .cloned()
                .collect())
        }

        async fn list_files_by_owner(
            &self,
            owner_kind: InlineFileOwnerKind,
            owner_id: Uuid,
        ) -> Result<Vec<InlineFile>, DomainError> {
            Ok(self
                .files
                .lock()
                .await
                .iter()
                .filter(|file| file.owner_kind == owner_kind && file.owner_id == owner_id)
                .cloned()
                .collect())
        }

        async fn upsert_file(&self, file: &InlineFile) -> Result<(), DomainError> {
            self.files.lock().await.push(file.clone());
            Ok(())
        }

        async fn upsert_files(&self, files: &[InlineFile]) -> Result<(), DomainError> {
            self.files.lock().await.extend(files.iter().cloned());
            Ok(())
        }

        async fn delete_file(
            &self,
            _owner_kind: InlineFileOwnerKind,
            _owner_id: Uuid,
            _container_id: &str,
            _path: &str,
        ) -> Result<(), DomainError> {
            Ok(())
        }

        async fn delete_by_container(
            &self,
            _owner_kind: InlineFileOwnerKind,
            _owner_id: Uuid,
            _container_id: &str,
        ) -> Result<(), DomainError> {
            Ok(())
        }

        async fn delete_by_owner(
            &self,
            _owner_kind: InlineFileOwnerKind,
            _owner_id: Uuid,
        ) -> Result<(), DomainError> {
            Ok(())
        }

        async fn count_files(
            &self,
            _owner_kind: InlineFileOwnerKind,
            _owner_id: Uuid,
            _container_id: &str,
        ) -> Result<i64, DomainError> {
            Ok(self.files.lock().await.len() as i64)
        }
    }

    fn inline_mount(owner_id: Uuid) -> Mount {
        Mount {
            id: "brief".to_string(),
            provider: PROVIDER_INLINE_FS.to_string(),
            backend_id: String::new(),
            root_ref: "context://inline/brief".to_string(),
            capabilities: vec![],
            default_write: false,
            display_name: "Brief".to_string(),
            metadata: serde_json::json!({
                "container_id": "brief",
                "agentdash_context_owner_kind": "project",
                "agentdash_context_owner_id": owner_id.to_string(),
            }),
        }
    }

    #[tokio::test]
    async fn list_exposes_binary_metadata() {
        let owner_id = Uuid::new_v4();
        let repo = Arc::new(MemoryInlineFileRepo::default());
        repo.upsert_file(&InlineFile::new_binary(
            InlineFileOwnerKind::Project,
            owner_id,
            "brief",
            "assets/logo.png",
            vec![1, 2, 3],
            "image/png",
        ))
        .await
        .expect("seed");
        let provider = InlineFsMountProvider::new(repo);
        let result = provider
            .list(
                &inline_mount(owner_id),
                &ListOptions {
                    path: "assets".to_string(),
                    pattern: None,
                    recursive: false,
                },
                &MountOperationContext::default(),
            )
            .await
            .expect("list");
        let entry = result
            .entries
            .iter()
            .find(|entry| !entry.is_dir)
            .expect("file");
        assert_eq!(entry.path, "assets/logo.png");
        assert_eq!(entry.size, Some(3));
        assert_eq!(
            entry
                .attributes
                .as_ref()
                .and_then(|attrs| attrs.get("content_kind"))
                .and_then(|value| value.as_str()),
            Some("binary")
        );
        assert_eq!(
            entry
                .attributes
                .as_ref()
                .and_then(|attrs| attrs.get("mime_type"))
                .and_then(|value| value.as_str()),
            Some("image/png")
        );
    }

    #[tokio::test]
    async fn read_text_rejects_binary_and_search_skips_it() {
        let owner_id = Uuid::new_v4();
        let repo = Arc::new(MemoryInlineFileRepo::default());
        repo.upsert_files(&[
            InlineFile::new_text(
                InlineFileOwnerKind::Project,
                owner_id,
                "brief",
                "note.md",
                "needle in text",
            ),
            InlineFile::new_binary(
                InlineFileOwnerKind::Project,
                owner_id,
                "brief",
                "assets/needle.png",
                vec![1, 2, 3],
                "image/png",
            ),
        ])
        .await
        .expect("seed");
        let provider = InlineFsMountProvider::new(repo);
        let mount = inline_mount(owner_id);
        let read_error = provider
            .read_text(
                &mount,
                "assets/needle.png",
                &MountOperationContext::default(),
            )
            .await
            .expect_err("binary read_text should fail");
        assert!(matches!(read_error, MountError::NotSupported(_)));

        let search = provider
            .search_text(
                &mount,
                &SearchQuery {
                    pattern: "needle".to_string(),
                    ..Default::default()
                },
                &MountOperationContext::default(),
            )
            .await
            .expect("search");
        assert_eq!(search.matches.len(), 1);
        assert_eq!(search.matches[0].path, "note.md");
    }

    #[tokio::test]
    async fn read_binary_returns_bytes_and_metadata() {
        let owner_id = Uuid::new_v4();
        let repo = Arc::new(MemoryInlineFileRepo::default());
        repo.upsert_file(&InlineFile::new_binary(
            InlineFileOwnerKind::Project,
            owner_id,
            "brief",
            "assets/logo.png",
            vec![0, 1, 2, 3],
            "image/png",
        ))
        .await
        .expect("seed");
        let provider = InlineFsMountProvider::new(repo);
        let result = provider
            .read_binary(
                &inline_mount(owner_id),
                "assets/logo.png",
                &MountOperationContext::default(),
            )
            .await
            .expect("read binary");

        assert_eq!(result.path, "assets/logo.png");
        assert_eq!(result.data, vec![0, 1, 2, 3]);
        assert_eq!(result.mime_type, "image/png");
        assert_eq!(
            result
                .attributes
                .as_ref()
                .and_then(|attrs| attrs.get("content_kind"))
                .and_then(|value| value.as_str()),
            Some("binary")
        );
    }

    // ─── vfs-search-spi-fix 集成测试 ────────────────────────────────────
    // T3 truncated / T4 version_token / T5 read_text_range 默认实现。
    // T1 (is_regex) 与 T2 (include_glob) 在 inline provider 上不直接生效（inline
    // 当前是 substring 实现，新字段 warn-and-degrade）；这两项语义在
    // fs-grep-rebuild 任务里随 inline 升级到 regex 时一并验收。

    #[tokio::test]
    async fn search_truncated_when_max_results_reached() {
        let owner_id = Uuid::new_v4();
        let repo = Arc::new(MemoryInlineFileRepo::default());
        repo.upsert_files(&[InlineFile::new_text(
            InlineFileOwnerKind::Project,
            owner_id,
            "brief",
            "a.md",
            "needle\nneedle\nneedle\n",
        )])
        .await
        .expect("seed");
        let provider = InlineFsMountProvider::new(repo);

        let result = provider
            .search_text(
                &inline_mount(owner_id),
                &SearchQuery {
                    pattern: "needle".to_string(),
                    max_results: Some(2),
                    ..Default::default()
                },
                &MountOperationContext::default(),
            )
            .await
            .expect("search");
        assert_eq!(result.matches.len(), 2);
        assert!(result.truncated, "max_results 命中应触发 truncated=true");
    }

    #[tokio::test]
    async fn read_text_returns_version_token_and_modified_at() {
        let owner_id = Uuid::new_v4();
        let repo = Arc::new(MemoryInlineFileRepo::default());
        repo.upsert_file(&InlineFile::new_text(
            InlineFileOwnerKind::Project,
            owner_id,
            "brief",
            "a.md",
            "hello",
        ))
        .await
        .expect("seed");
        let provider = InlineFsMountProvider::new(repo);

        let result = provider
            .read_text(
                &inline_mount(owner_id),
                "a.md",
                &MountOperationContext::default(),
            )
            .await
            .expect("read");
        assert!(
            result.version_token.is_some(),
            "inline provider 应填充 version_token"
        );
        assert!(
            result.version_token.as_deref().unwrap().starts_with("ts:"),
            "inline token 形如 ts:<ms>:<size>"
        );
        assert!(
            result.modified_at.is_some(),
            "inline provider 应填充 modified_at"
        );
    }

    #[tokio::test]
    async fn read_text_range_default_impl_slices_lines() {
        let owner_id = Uuid::new_v4();
        let repo = Arc::new(MemoryInlineFileRepo::default());
        repo.upsert_file(&InlineFile::new_text(
            InlineFileOwnerKind::Project,
            owner_id,
            "brief",
            "a.md",
            "line1\nline2\nline3\nline4\nline5\n",
        ))
        .await
        .expect("seed");
        let provider = InlineFsMountProvider::new(repo);

        // offset=2, limit=Some(2) ⇒ 取 line3 + line4
        let result = provider
            .read_text_range(
                &inline_mount(owner_id),
                "a.md",
                2,
                Some(2),
                &MountOperationContext::default(),
            )
            .await
            .expect("read_text_range");
        assert_eq!(result.content, "line3\nline4");
        // 默认实现沿用全文 token
        assert!(result.version_token.is_some());
    }

    #[tokio::test]
    async fn read_text_range_default_impl_limit_none_reads_to_eof() {
        let owner_id = Uuid::new_v4();
        let repo = Arc::new(MemoryInlineFileRepo::default());
        repo.upsert_file(&InlineFile::new_text(
            InlineFileOwnerKind::Project,
            owner_id,
            "brief",
            "a.md",
            "line1\nline2\nline3\n",
        ))
        .await
        .expect("seed");
        let provider = InlineFsMountProvider::new(repo);

        let result = provider
            .read_text_range(
                &inline_mount(owner_id),
                "a.md",
                1,
                None,
                &MountOperationContext::default(),
            )
            .await
            .expect("read_text_range");
        assert_eq!(result.content, "line2\nline3");
    }

    // ─── vfs-grep-query-split 拆分后语义验证 ─────────────────────────────
    // T5：search_text 退化为 substring，不再识别 regex 元字符。

    #[tokio::test]
    async fn search_text_substring_does_not_treat_pattern_as_regex() {
        let owner_id = Uuid::new_v4();
        let repo = Arc::new(MemoryInlineFileRepo::default());
        repo.upsert_file(&InlineFile::new_text(
            InlineFileOwnerKind::Project,
            owner_id,
            "brief",
            "a.md",
            "funcXfoo\nfunc.*foo literal",
        ))
        .await
        .expect("seed");
        let provider = InlineFsMountProvider::new(repo);

        let result = provider
            .search_text(
                &inline_mount(owner_id),
                &SearchQuery {
                    pattern: "func.*foo".to_string(),
                    ..Default::default()
                },
                &MountOperationContext::default(),
            )
            .await
            .expect("search");
        // substring 仅匹配字面 "func.*foo"，不应该匹配 "funcXfoo"。
        assert_eq!(result.matches.len(), 1);
        assert!(result.matches[0].content.contains("func.*foo literal"));
    }

    // T2：grep_text 直接调（含 regex / context 字段）的真实路径。
    #[tokio::test]
    async fn grep_text_supports_regex_and_context_lines() {
        let owner_id = Uuid::new_v4();
        let repo = Arc::new(MemoryInlineFileRepo::default());
        repo.upsert_file(&InlineFile::new_text(
            InlineFileOwnerKind::Project,
            owner_id,
            "brief",
            "a.md",
            "L1\nL2\nfunc_foo()\nL4\nL5",
        ))
        .await
        .expect("seed");
        let provider = InlineFsMountProvider::new(repo);

        let result = provider
            .grep_text(
                &inline_mount(owner_id),
                &GrepQuery {
                    base: SearchQuery {
                        pattern: "func.*foo".to_string(),
                        ..Default::default()
                    },
                    before_lines: 1,
                    after_lines: 1,
                    ..Default::default()
                },
                &MountOperationContext::default(),
            )
            .await
            .expect("grep");
        // regex 匹配 "func_foo()"，context = before/after 1 行 ⇒ 输出 L2 + 命中 + L4。
        let contents: Vec<&str> = result.matches.iter().map(|m| m.content.as_str()).collect();
        assert!(contents.iter().any(|c| c.contains("func_foo")));
        assert!(contents.iter().any(|c| c == &"L2"));
        assert!(contents.iter().any(|c| c == &"L4"));
        // 不应该有 L1 / L5（超出 context 范围）
        assert!(!contents.iter().any(|c| c == &"L1"));
        assert!(!contents.iter().any(|c| c == &"L5"));
    }
}
