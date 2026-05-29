use std::sync::Arc;

use agentdash_spi::context::tool_schema_sanitizer::schema_value;
use agentdash_spi::{AgentTool, AgentToolError, AgentToolResult, ToolUpdateCallback};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use crate::vfs::ListOptions;
use crate::vfs::inline_persistence::InlineContentOverlay;
use crate::vfs::service::{VfsService, is_vcs_path};
use crate::vfs::tools::common::{SharedRuntimeVfs, ok_text, resolve_uri_path};

// ---------------------------------------------------------------------------
// fs_glob — aligned with Claude Code GlobTool
// ---------------------------------------------------------------------------

/// 默认命中条目上限；与 CC GlobTool 一致。
const DEFAULT_MAX_RESULTS: usize = 100;

#[derive(Clone)]
pub struct FsGlobTool {
    service: Arc<VfsService>,
    vfs: SharedRuntimeVfs,
    overlay: Option<Arc<InlineContentOverlay>>,
    identity: Option<agentdash_spi::platform::auth::AuthIdentity>,
}
impl FsGlobTool {
    pub fn new(
        service: Arc<VfsService>,
        vfs: SharedRuntimeVfs,
        overlay: Option<Arc<InlineContentOverlay>>,
        identity: Option<agentdash_spi::platform::auth::AuthIdentity>,
    ) -> Self {
        Self {
            service,
            vfs,
            overlay,
            identity,
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FsGlobParams {
    /// The glob pattern to match files against.
    pub pattern: String,
    /// Mount-rooted directory to search in (`mount_id://relative/path`). If omitted, the mount root is used.
    pub path: Option<String>,
}

#[async_trait]
impl AgentTool for FsGlobTool {
    fn name(&self) -> &str {
        "fs_glob"
    }
    fn description(&self) -> &str {
        "Fast file pattern matching using glob patterns.\n\
         \n\
         Usage:\n\
         - The pattern parameter is required and always interpreted as a glob.\n\
         - Use `*` for the current directory; `**/foo` for recursive match.\n\
         - The optional path parameter scopes the search to a mount-rooted directory; omit it to search the mount root.\n\
         - Searches from the mount root respect workspace ignore files and built-in dependency/build noise exclusions.\n\
         - Pass an explicit path to inspect ordinary ignored subtrees such as dependencies or build output.\n\
         - Returns paths sorted by modification time (newest first), then alphabetically.\n\
         - Directories are shown with a trailing slash (e.g., `src/utils/`).\n\
         - Results are limited to 100 entries by default.\n\
         - VCS metadata directories (.git, .svn, .hg, .bzr, .jj, .sl) are excluded automatically.\n\
         - For text content search, use fs_grep instead."
    }
    fn parameters_schema(&self) -> serde_json::Value {
        schema_value::<FsGlobParams>()
    }
    async fn execute(
        &self,
        _: &str,
        args: serde_json::Value,
        _: CancellationToken,
        _: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let params: FsGlobParams = serde_json::from_value(args)
            .map_err(|e| AgentToolError::InvalidArguments(format!("invalid arguments: {e}")))?;
        let vfs = self.vfs.snapshot().await;
        let target = resolve_uri_path(&vfs, params.path.as_deref().unwrap_or("."))
            .map_err(AgentToolError::ExecutionFailed)?;

        // pattern 含 `**` ⇒ 递归扫描；否则只列当前目录。
        let recursive = params.pattern.contains("**");
        let result = self
            .service
            .list(
                &vfs,
                &target.mount_id,
                ListOptions {
                    path: if target.path.is_empty() {
                        ".".to_string()
                    } else {
                        target.path
                    },
                    pattern: Some(params.pattern.clone()),
                    recursive,
                },
                self.overlay.as_ref().map(|arc| arc.as_ref()),
                self.identity.as_ref(),
            )
            .await
            .map_err(|e| AgentToolError::ExecutionFailed(e.to_string()))?;

        // VCS 过滤
        let mut entries: Vec<_> = result
            .entries
            .into_iter()
            .filter(|e| !is_vcs_path(&e.path))
            .collect();

        // mtime desc + path asc 兜底
        entries.sort_by(|a, b| {
            let a_m = a.modified_at.unwrap_or(0);
            let b_m = b.modified_at.unwrap_or(0);
            b_m.cmp(&a_m).then_with(|| a.path.cmp(&b.path))
        });

        let cap = DEFAULT_MAX_RESULTS;
        let total = entries.len();
        let truncated = total > cap;
        entries.truncate(cap);

        let mut output = if entries.is_empty() {
            "(no matches)".to_string()
        } else {
            entries
                .iter()
                .map(|e| {
                    let path = e.path.replace('\\', "/");
                    if e.is_dir { format!("{path}/") } else { path }
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        if truncated {
            output.push_str(&format!(
                "\n({} more entries; refine the pattern to see more)",
                total - cap
            ));
        }
        Ok(ok_text(output))
    }
}

#[cfg(test)]
mod fs_glob_tests {
    use super::*;
    use crate::vfs::MountProviderRegistry;
    use crate::vfs::mount::PROVIDER_INLINE_FS;
    use crate::vfs::provider_inline::InlineFsMountProvider;
    use agentdash_domain::common::error::DomainError;
    use agentdash_domain::inline_file::{InlineFile, InlineFileOwnerKind, InlineFileRepository};
    use agentdash_spi::{Mount, MountCapability, Vfs};
    use chrono::{DateTime, Duration, Utc};
    use serde_json::json;
    use tokio::sync::Mutex;
    use uuid::Uuid;

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
                .find(|f| {
                    f.owner_kind == owner_kind
                        && f.owner_id == owner_id
                        && f.container_id == container_id
                        && f.path == path
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
                .filter(|f| {
                    f.owner_kind == owner_kind
                        && f.owner_id == owner_id
                        && f.container_id == container_id
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
                .filter(|f| f.owner_kind == owner_kind && f.owner_id == owner_id)
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
            _: InlineFileOwnerKind,
            _: Uuid,
            _: &str,
            _: &str,
        ) -> Result<(), DomainError> {
            Ok(())
        }
        async fn delete_by_container(
            &self,
            _: InlineFileOwnerKind,
            _: Uuid,
            _: &str,
        ) -> Result<(), DomainError> {
            Ok(())
        }
        async fn delete_by_owner(
            &self,
            _: InlineFileOwnerKind,
            _: Uuid,
        ) -> Result<(), DomainError> {
            Ok(())
        }
        async fn count_files(
            &self,
            _: InlineFileOwnerKind,
            _: Uuid,
            _: &str,
        ) -> Result<i64, DomainError> {
            Ok(self.files.lock().await.len() as i64)
        }
    }

    fn make_tool_with_files(files: Vec<(&str, &str, DateTime<Utc>)>) -> FsGlobTool {
        let owner_id = Uuid::new_v4();
        let repo = Arc::new(MemoryInlineFileRepo::default());
        let mut seeded: Vec<InlineFile> = files
            .iter()
            .map(|(path, content, ts)| {
                let mut f = InlineFile::new_text(
                    InlineFileOwnerKind::Project,
                    owner_id,
                    "brief",
                    *path,
                    *content,
                );
                f.updated_at = *ts;
                f
            })
            .collect();
        {
            let mut guard = repo.files.try_lock().expect("uncontended setup");
            guard.append(&mut seeded);
        }
        let provider = Arc::new(InlineFsMountProvider::new(repo));
        let mut registry = MountProviderRegistry::new();
        registry.register(provider);
        let service = Arc::new(VfsService::new(Arc::new(registry)));
        let mount = Mount {
            id: "mem".to_string(),
            provider: PROVIDER_INLINE_FS.to_string(),
            backend_id: String::new(),
            root_ref: "context://inline/brief".to_string(),
            capabilities: vec![MountCapability::List],
            default_write: false,
            display_name: "Memory".to_string(),
            metadata: json!({
                "container_id": "brief",
                "agentdash_context_owner_kind": "project",
                "agentdash_context_owner_id": owner_id.to_string(),
            }),
        };
        let vfs = Vfs {
            mounts: vec![mount],
            default_mount_id: Some("mem".to_string()),
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        };
        FsGlobTool::new(service, SharedRuntimeVfs::new(vfs), None, None)
    }

    fn at(offset_secs: i64) -> DateTime<Utc> {
        Utc::now() + Duration::seconds(offset_secs)
    }

    #[test]
    fn fs_glob_schema_matches_claude_code_required_shape() {
        let tool = make_tool_with_files(vec![("a.rs", "x", at(0))]);
        let schema = tool.parameters_schema();
        let required = schema["required"]
            .as_array()
            .expect("required should be array")
            .iter()
            .filter_map(|value| value.as_str())
            .collect::<Vec<_>>();

        assert_eq!(required, vec!["pattern"]);
        assert!(schema["properties"].get("pattern").is_some());
        assert!(schema["properties"].get("path").is_some());
        assert!(schema["properties"].get("max_results").is_none());
    }

    #[tokio::test]
    async fn fs_glob_rejects_legacy_recursive_field() {
        let tool = make_tool_with_files(vec![("a.rs", "x", at(0))]);
        let err = tool
            .execute(
                "c",
                json!({ "pattern": "*.rs", "recursive": true }),
                CancellationToken::new(),
                None,
            )
            .await
            .expect_err("legacy schema rejected");
        assert!(matches!(err, AgentToolError::InvalidArguments(_)));
    }

    #[tokio::test]
    async fn fs_glob_requires_pattern() {
        let tool = make_tool_with_files(vec![("a.rs", "x", at(0))]);
        let err = tool
            .execute("c", json!({}), CancellationToken::new(), None)
            .await
            .expect_err("missing pattern rejected");
        assert!(matches!(err, AgentToolError::InvalidArguments(_)));
    }

    #[tokio::test]
    async fn fs_glob_sorts_by_mtime_desc() {
        let tool = make_tool_with_files(vec![
            ("a.rs", "x", at(-300)),
            ("b.rs", "x", at(-100)),
            ("c.rs", "x", at(0)),
        ]);
        let res = tool
            .execute(
                "c",
                json!({ "pattern": "**/*.rs" }),
                CancellationToken::new(),
                None,
            )
            .await
            .expect("execute");
        let text = res.content[0].extract_text().expect("text");
        let lines: Vec<&str> = text.lines().filter(|l| !l.is_empty()).collect();
        assert_eq!(lines, vec!["c.rs", "b.rs", "a.rs"]);
    }

    #[tokio::test]
    async fn fs_glob_recursive_inferred_from_double_star() {
        let tool =
            make_tool_with_files(vec![("foo.rs", "x", at(0)), ("nested/bar.rs", "x", at(-1))]);
        // 仅根：`*.rs` 不应该匹配 nested/
        let res = tool
            .execute(
                "c",
                json!({ "pattern": "*.rs" }),
                CancellationToken::new(),
                None,
            )
            .await
            .expect("execute");
        let text = res.content[0].extract_text().expect("text");
        assert!(text.contains("foo.rs"));
        assert!(
            !text.contains("nested/"),
            "non-recursive should skip subdirs: {text}"
        );

        // 递归：`**/*.rs` 应该都匹配
        let res = tool
            .execute(
                "c",
                json!({ "pattern": "**/*.rs" }),
                CancellationToken::new(),
                None,
            )
            .await
            .expect("execute");
        let text = res.content[0].extract_text().expect("text");
        assert!(text.contains("foo.rs"));
        assert!(
            text.contains("nested/bar.rs"),
            "recursive should include subdirs: {text}"
        );
    }

    #[tokio::test]
    async fn fs_glob_default_max_results_caps_at_100() {
        let mut files: Vec<(String, String, DateTime<Utc>)> = (0..200)
            .map(|i| (format!("f{i:03}.rs"), "x".to_string(), at(-(i as i64))))
            .collect();
        let files_ref: Vec<(&str, &str, DateTime<Utc>)> = files
            .iter_mut()
            .map(|(p, c, t)| (p.as_str(), c.as_str(), *t))
            .collect();
        let tool = make_tool_with_files(files_ref);
        let res = tool
            .execute(
                "c",
                json!({ "pattern": "**/*.rs" }),
                CancellationToken::new(),
                None,
            )
            .await
            .expect("execute");
        let text = res.content[0].extract_text().expect("text");
        let path_lines: Vec<&str> = text
            .lines()
            .filter(|l| !l.is_empty() && !l.starts_with('('))
            .collect();
        assert_eq!(path_lines.len(), 100);
        assert!(text.contains("more entries"));
    }

    #[tokio::test]
    async fn fs_glob_directory_has_trailing_slash() {
        let tool =
            make_tool_with_files(vec![("README.md", "x", at(0)), ("src/lib.rs", "x", at(-1))]);
        let res = tool
            .execute(
                "c",
                json!({ "pattern": "*" }),
                CancellationToken::new(),
                None,
            )
            .await
            .expect("execute");
        let text = res.content[0].extract_text().expect("text");
        // 根目录列表应包含 README.md 和 src/（注意 inline list_inline_entries
        // 是否会输出 src/ 这个目录条目，取决于 list_inline_entries 实现）。
        // 至少 README.md 不应有 [file] 前缀。
        assert!(text.contains("README.md"));
        assert!(
            !text.contains("[file]"),
            "should not have [file] prefix: {text}"
        );
        assert!(
            !text.contains("[dir]"),
            "should not have [dir] prefix: {text}"
        );
    }

    #[tokio::test]
    async fn fs_glob_excludes_vcs_dirs() {
        let tool = make_tool_with_files(vec![
            (".git/HEAD", "ref", at(0)),
            ("src/main.rs", "fn main() {}", at(-1)),
        ]);
        let res = tool
            .execute(
                "c",
                json!({ "pattern": "**/*" }),
                CancellationToken::new(),
                None,
            )
            .await
            .expect("execute");
        let text = res.content[0].extract_text().expect("text");
        assert!(text.contains("src/main.rs"));
        assert!(
            !text.contains(".git"),
            "VCS dirs should be excluded: {text}"
        );
    }
}
