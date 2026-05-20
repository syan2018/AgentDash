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
            .map_err(map_domain_err)?;
        let content = file
            .map(|f| match f.content {
                InlineFileContent::Text { content } => Ok(content),
                InlineFileContent::Binary { .. } => Err(MountError::NotSupported(format!(
                    "文件是二进制内容，不能按文本读取: {path}"
                ))),
            })
            .transpose()?
            .ok_or_else(|| MountError::NotFound(format!("文件不存在: {path}")))?;
        Ok(ReadResult::new(path, content))
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
                    path: None,
                    case_sensitive: true,
                    max_results: None,
                },
                &MountOperationContext::default(),
            )
            .await
            .expect("search");
        assert_eq!(search.matches.len(), 1);
        assert_eq!(search.matches[0].path, "note.md");
    }
}
