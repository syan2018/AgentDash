use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use agentdash_spi::context::tool_schema_sanitizer::schema_value;
use agentdash_spi::{AgentTool, AgentToolError, AgentToolResult, ToolUpdateCallback};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use crate::vfs::inline_persistence::InlineContentOverlay;
use crate::vfs::service::VfsService;
use crate::vfs::tools::common::{SharedRuntimeVfs, ok_text, resolve_uri_path};

// ---------------------------------------------------------------------------
// fs_grep — aligned with Claude Code GrepTool
// ---------------------------------------------------------------------------

/// 默认 `head_limit`，与 CC GrepTool 一致；`0` = 无限。
const DEFAULT_HEAD_LIMIT: usize = 250;
/// `head_limit = 0`（无限）时 service 层传入的硬上限。
const UNLIMITED_PAGE_SIZE: usize = 50_000;

/// 语言快捷键 → 扩展名映射（design.md §3）。
const LANG_EXTENSIONS: &[(&str, &[&str])] = &[
    ("js", &["js", "jsx", "mjs", "cjs"]),
    ("ts", &["ts", "tsx", "mts", "cts"]),
    ("py", &["py", "pyi"]),
    ("rust", &["rs"]),
    ("go", &["go"]),
    ("java", &["java"]),
    ("c", &["c", "h"]),
    ("cpp", &["cc", "cpp", "cxx", "hpp", "hxx"]),
    ("cs", &["cs"]),
    ("rb", &["rb"]),
];

#[derive(Clone)]
pub struct FsGrepTool {
    service: Arc<VfsService>,
    vfs: SharedRuntimeVfs,
    overlay: Option<Arc<InlineContentOverlay>>,
    identity: Option<agentdash_spi::platform::auth::AuthIdentity>,
}
impl FsGrepTool {
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

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OutputMode {
    /// 返回 `path:line:content` 命中行（line_numbers=false 时省略 :line:）。
    Content,
    /// 仅返回去重的命中文件路径。**默认**，与 CC GrepTool 一致。
    #[default]
    FilesWithMatches,
    /// 每文件命中计数 `path:N`。
    Count,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FsGrepParams {
    /// The regular expression pattern to search for. Always interpreted as a regex (no fixed-string mode).
    pub pattern: String,
    /// Mount-rooted path (`mount_id://relative/path`). Omit to search the whole mount.
    pub path: Option<String>,
    /// Glob pattern that filters which files are searched (e.g., `*.rs`, `src/**/*.ts`). Combined with `type` as a union.
    pub glob: Option<String>,
    /// Language shortcut: one of js, ts, py, rust, go, java, c, cpp, cs, rb.
    #[serde(rename = "type")]
    pub type_: Option<String>,
    /// `content` / `files_with_matches` (default) / `count`.
    pub output_mode: Option<OutputMode>,
    /// Case-insensitive matching (`-i`). Default false.
    #[serde(rename = "-i")]
    #[serde(default)]
    pub case_insensitive: bool,
    /// Show line numbers in `content` mode (`-n`). Default true.
    #[serde(rename = "-n")]
    #[serde(default = "default_true")]
    pub line_numbers: bool,
    /// Multiline mode: `.` matches newlines and `^/$` match per line (`-U`). Default false.
    #[serde(default)]
    pub multiline: bool,
    /// Lines of context before each match (`-B`).
    #[serde(rename = "-B")]
    pub before_context: Option<usize>,
    /// Lines of context after each match (`-A`).
    #[serde(rename = "-A")]
    pub after_context: Option<usize>,
    /// Lines of context around each match (`-C`); merged with before/after as max.
    #[serde(rename = "-C")]
    pub context_short: Option<usize>,
    /// Alias for `-C`.
    pub context: Option<usize>,
    /// Cap on returned hits (or files in files_with_matches/count modes). Default 250; `0` = unlimited.
    pub head_limit: Option<usize>,
    /// Pagination offset (skip first N hits). Default 0.
    pub offset: Option<usize>,
}

#[async_trait]
impl AgentTool for FsGrepTool {
    fn name(&self) -> &str {
        "fs_grep"
    }
    fn description(&self) -> &str {
        "Search file contents on a mount with a regular expression.\n\
         \n\
         Usage:\n\
         - The pattern is ALWAYS treated as a regular expression. Escape literal regex metacharacters explicitly.\n\
         - Filter files via glob (e.g., `*.rs`) and/or type shortcut (rust, ts, py, go, ...).\n\
         - output_mode controls the result shape:\n\
           - files_with_matches (default): unique file paths.\n\
           - content: matching lines (with optional line numbers and -A/-B/-C context).\n\
           - count: per-file match count.\n\
         - Use -i for case-insensitive search and -n to control line numbers in content mode.\n\
         - head_limit caps results (default 250; 0 = unlimited). Use offset for pagination.\n\
         - Searches from the mount root respect workspace ignore files and built-in dependency/build noise exclusions.\n\
         - Pass an explicit path to search ordinary ignored subtrees such as dependencies or build output.\n\
         - Common VCS metadata directories (.git, .svn, .hg, .bzr, .jj, .sl) are excluded automatically.\n\
         - Lines longer than 500 characters are truncated with a `...(truncated)` suffix."
    }
    fn parameters_schema(&self) -> serde_json::Value {
        schema_value::<FsGrepParams>()
    }
    async fn execute(
        &self,
        _: &str,
        args: serde_json::Value,
        _: CancellationToken,
        _: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let params: FsGrepParams = serde_json::from_value(args)
            .map_err(|e| AgentToolError::InvalidArguments(format!("invalid arguments: {e}")))?;

        let combined_glob = build_combined_glob(params.glob.as_deref(), params.type_.as_deref())
            .map_err(AgentToolError::InvalidArguments)?;

        let vfs = self.vfs.snapshot().await;
        let target = resolve_uri_path(&vfs, params.path.as_deref().unwrap_or("."))
            .map_err(AgentToolError::ExecutionFailed)?;
        let search_path = if target.path.is_empty() {
            ".".to_string()
        } else {
            target.path
        };

        let head_limit = params.head_limit.unwrap_or(DEFAULT_HEAD_LIMIT);
        let offset = params.offset.unwrap_or(0);
        // service 层 max_results = head_limit + offset（buffer 让 tool 能 skip）。
        // head_limit = 0 ⇒ 50000 上限。
        let service_max = if head_limit == 0 {
            UNLIMITED_PAGE_SIZE
        } else {
            head_limit.saturating_add(offset).max(1)
        };

        let before_lines = params.before_context.unwrap_or(0);
        let after_lines = params.after_context.unwrap_or(0);
        let context_lines = params.context.or(params.context_short).unwrap_or(0);

        let (hits, truncated) = self
            .service
            .grep_text_extended(
                &vfs,
                &crate::vfs::TextSearchParams {
                    mount_id: &target.mount_id,
                    path: &search_path,
                    query: &params.pattern,
                    is_regex: true,
                    include_glob: combined_glob.as_deref(),
                    max_results: service_max,
                    context_lines,
                    overlay: self.overlay.as_ref().map(|arc| arc.as_ref()),
                    identity: self.identity.as_ref(),
                    case_sensitive: !params.case_insensitive,
                    before_lines,
                    after_lines,
                    multiline: params.multiline,
                    // service 始终按 Content 收集；output_mode 转换在 tool 层做。
                    output_mode: agentdash_spi::platform::mount::SearchOutputMode::Content,
                },
            )
            .await
            .map_err(|e| AgentToolError::ExecutionFailed(e.to_string()))?;

        // 分页：tool 层 skip(offset).take(head_limit)；head_limit=0 ⇒ 不限制。
        let take = if head_limit == 0 {
            usize::MAX
        } else {
            head_limit
        };
        let paginated: Vec<String> = hits.into_iter().skip(offset).take(take).collect();

        let output_mode = params.output_mode.unwrap_or_default();
        let mut output = match output_mode {
            OutputMode::Content => format_content(&paginated, params.line_numbers),
            OutputMode::FilesWithMatches => {
                let unique: BTreeSet<&str> =
                    paginated.iter().filter_map(|h| extract_path(h)).collect();
                if unique.is_empty() {
                    "no matches found".to_string()
                } else {
                    unique
                        .into_iter()
                        .map(|s| s.to_string())
                        .collect::<Vec<_>>()
                        .join("\n")
                }
            }
            OutputMode::Count => {
                let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
                for hit in &paginated {
                    if let Some(p) = extract_path(hit) {
                        *counts.entry(p).or_insert(0) += 1;
                    }
                }
                if counts.is_empty() {
                    "no matches found".to_string()
                } else {
                    counts
                        .iter()
                        .map(|(p, c)| format!("{p}:{c}"))
                        .collect::<Vec<_>>()
                        .join("\n")
                }
            }
        };

        if truncated {
            output.push_str("\n(results truncated; narrow your search to see more)");
        }
        Ok(ok_text(output))
    }
}

/// 把命中行的 `path:line:content` 转换为 content 模式输出（按 line_numbers 决定是否保留 :line:）。
fn format_content(hits: &[String], line_numbers: bool) -> String {
    if hits.is_empty() {
        return "no matches found".to_string();
    }
    if line_numbers {
        hits.join("\n")
    } else {
        hits.iter()
            .map(|hit| {
                // 形如 path:line: content 或 path:line- content（context 行用 `-`）
                // 去掉中间的 :line: / :line-。
                let parts: Vec<&str> = hit.splitn(3, ':').collect();
                if parts.len() == 3 {
                    format!("{}: {}", parts[0], parts[2].trim_start())
                } else {
                    hit.clone()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// 解析 `path:line:content` 形式取 path（不含 line/content）。
fn extract_path(hit: &str) -> Option<&str> {
    hit.split(':').next()
}

/// 把 user_glob 与 type 翻译合并为最终 glob。
/// - 都为空 ⇒ Ok(None)
/// - 仅 type ⇒ `**/*.{ext1,ext2,...}`
/// - 仅 glob ⇒ 原样
/// - 都有 ⇒ `{user_glob,**/*.{ext1,...}}`
fn build_combined_glob(
    user_glob: Option<&str>,
    type_shortcut: Option<&str>,
) -> Result<Option<String>, String> {
    let type_glob = match type_shortcut {
        Some(name) => Some(translate_type_to_glob(name)?),
        None => None,
    };
    match (user_glob, type_glob) {
        (None, None) => Ok(None),
        (Some(g), None) => Ok(Some(g.to_string())),
        (None, Some(g)) => Ok(Some(g)),
        (Some(u), Some(t)) => Ok(Some(format!("{{{u},{t}}}"))),
    }
}

fn translate_type_to_glob(name: &str) -> Result<String, String> {
    let entry = LANG_EXTENSIONS
        .iter()
        .find(|(key, _)| *key == name)
        .ok_or_else(|| {
            let supported: Vec<&str> = LANG_EXTENSIONS.iter().map(|(k, _)| *k).collect();
            format!("unknown type `{name}`; supported: {}", supported.join(", "))
        })?;
    let exts: Vec<&str> = entry.1.to_vec();
    Ok(if exts.len() == 1 {
        format!("**/*.{}", exts[0])
    } else {
        format!("**/*.{{{}}}", exts.join(","))
    })
}

#[cfg(test)]
mod fs_grep_tests {
    use super::*;
    use crate::vfs::tools::common::SharedRuntimeVfs;
    use crate::vfs::{MountProviderRegistry, ReadResult};
    use agentdash_domain::common::error::DomainError;
    use agentdash_domain::inline_file::{InlineFile, InlineFileOwnerKind, InlineFileRepository};
    use agentdash_spi::platform::mount::{MountError, MountOperationContext, RuntimeFileEntry};
    use agentdash_spi::{Mount, MountCapability, Vfs};
    use serde_json::json;
    use std::sync::Arc;
    use tokio::sync::Mutex;
    use uuid::Uuid;

    use crate::vfs::mount::PROVIDER_INLINE_FS;
    use crate::vfs::provider_inline::InlineFsMountProvider;

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

    fn make_tool(files: Vec<(&str, &str)>) -> FsGrepTool {
        let owner_id = Uuid::new_v4();
        let repo = Arc::new(MemoryInlineFileRepo::default());
        // Block on async upsert at construction
        let runtime = tokio::runtime::Handle::try_current();
        let to_seed: Vec<InlineFile> = files
            .iter()
            .map(|(path, content)| {
                InlineFile::new_text(
                    InlineFileOwnerKind::Project,
                    owner_id,
                    "brief",
                    *path,
                    *content,
                )
            })
            .collect();
        if let Ok(_h) = runtime {
            // 同步 path: 用 tokio::task::block_in_place 不可在 Mutex 上下文用，
            // 但测试 mutex 是 tokio 异步 mutex；改为在 setup 之外 seed。这里我们用
            // 一个 sync helper：直接 push 到 inner Mutex 的 try_lock。
            // 简化：我们在 test 里用 async setup helper。
        }
        // 同步种子（绕开 async lock）
        {
            let mut guard = repo.files.try_lock().expect("uncontended at test setup");
            guard.extend(to_seed);
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
            capabilities: vec![
                MountCapability::Read,
                MountCapability::List,
                MountCapability::Search,
            ],
            default_write: false,
            display_name: "Memory".to_string(),
            metadata: serde_json::json!({
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
        // 验证：消除 unused 警告
        let _ = ReadResult::default();
        let _ = MountError::NotFound("".into());
        let _ = MountOperationContext::default();
        let _ = RuntimeFileEntry::file("");
        FsGrepTool::new(service, SharedRuntimeVfs::new(vfs), None, None)
    }

    #[test]
    fn fs_grep_schema_matches_claude_code_required_shape() {
        let tool = make_tool(vec![("a.rs", "fn main() {}")]);
        let schema = tool.parameters_schema();
        let required = schema["required"]
            .as_array()
            .expect("required should be array")
            .iter()
            .filter_map(|value| value.as_str())
            .collect::<Vec<_>>();
        let properties = schema["properties"]
            .as_object()
            .expect("properties should be object");

        assert_eq!(required, vec!["pattern"]);
        for name in [
            "pattern",
            "path",
            "glob",
            "output_mode",
            "-B",
            "-A",
            "-C",
            "context",
            "-n",
            "-i",
            "type",
            "head_limit",
            "offset",
            "multiline",
        ] {
            assert!(
                properties.contains_key(name),
                "missing schema property {name}"
            );
        }
    }

    #[tokio::test]
    async fn fs_grep_pattern_is_always_regex() {
        let tool = make_tool(vec![("a.rs", "funcXfoo\nfunction foo")]);
        let res = tool
            .execute(
                "c",
                json!({ "pattern": "func.*foo", "output_mode": "content" }),
                CancellationToken::new(),
                None,
            )
            .await
            .expect("execute");
        let text = res.content[0].extract_text().expect("text");
        assert!(text.contains("funcXfoo"), "regex .* should match: {text}");
    }

    #[tokio::test]
    async fn fs_grep_default_files_with_matches() {
        let tool = make_tool(vec![("a.rs", "foo"), ("b.rs", "foo bar")]);
        let res = tool
            .execute(
                "c",
                json!({ "pattern": "foo" }),
                CancellationToken::new(),
                None,
            )
            .await
            .expect("execute");
        let text = res.content[0].extract_text().expect("text");
        assert!(text.contains("a.rs"));
        assert!(text.contains("b.rs"));
        // 默认 FilesWithMatches 不输出命中行
        assert!(!text.contains("foo bar"));
    }

    #[tokio::test]
    async fn fs_grep_count_mode() {
        let tool = make_tool(vec![("a.rs", "foo\nfoo"), ("b.rs", "foo")]);
        let res = tool
            .execute(
                "c",
                json!({ "pattern": "foo", "output_mode": "count" }),
                CancellationToken::new(),
                None,
            )
            .await
            .expect("execute");
        let text = res.content[0].extract_text().expect("text");
        assert!(text.contains("a.rs:2"));
        assert!(text.contains("b.rs:1"));
    }

    #[tokio::test]
    async fn fs_grep_case_insensitive() {
        let tool = make_tool(vec![("a.rs", "Hello WORLD")]);
        let res = tool
            .execute(
                "c",
                json!({ "pattern": "world", "-i": true, "output_mode": "content" }),
                CancellationToken::new(),
                None,
            )
            .await
            .expect("execute");
        let text = res.content[0].extract_text().expect("text");
        assert!(text.contains("Hello WORLD"), "got: {text}");
    }

    #[tokio::test]
    async fn fs_grep_type_shortcut_filters_extension() {
        let tool = make_tool(vec![
            ("src/main.rs", "fn rust_target() {}"),
            ("docs/README.md", "fn rust_target() {}"),
        ]);
        let res = tool
            .execute(
                "c",
                json!({ "pattern": "rust_target", "type": "rust", "output_mode": "files_with_matches" }),
                CancellationToken::new(),
                None,
            )
            .await
            .expect("execute");
        let text = res.content[0].extract_text().expect("text");
        assert!(text.contains("src/main.rs"));
        assert!(!text.contains("docs/README.md"));
    }

    #[tokio::test]
    async fn fs_grep_unknown_type_invalid_arguments() {
        let tool = make_tool(vec![("a.rs", "x")]);
        let err = tool
            .execute(
                "c",
                json!({ "pattern": "x", "type": "elixir" }),
                CancellationToken::new(),
                None,
            )
            .await
            .expect_err("unknown type rejected");
        assert!(matches!(err, AgentToolError::InvalidArguments(_)));
    }

    #[tokio::test]
    async fn fs_grep_head_limit_offset_paginates() {
        let tool = make_tool(vec![
            ("a.rs", "x"),
            ("b.rs", "x"),
            ("c.rs", "x"),
            ("d.rs", "x"),
        ]);
        // 第 0 页：前 2 个
        let page0 = tool
            .execute(
                "c",
                json!({ "pattern": "x", "head_limit": 2, "offset": 0, "output_mode": "files_with_matches" }),
                CancellationToken::new(),
                None,
            )
            .await
            .expect("execute");
        let text0 = page0.content[0].extract_text().expect("text");
        // 第 2 页：跳过前 2 个
        let page1 = tool
            .execute(
                "c",
                json!({ "pattern": "x", "head_limit": 2, "offset": 2, "output_mode": "files_with_matches" }),
                CancellationToken::new(),
                None,
            )
            .await
            .expect("execute");
        let text1 = page1.content[0].extract_text().expect("text");
        // 两页内容不重合（忽略空行 + truncated 提示行）。
        let page0_files: Vec<&str> = text0
            .lines()
            .filter(|l| !l.is_empty() && !l.starts_with('('))
            .collect();
        let page1_files: Vec<&str> = text1
            .lines()
            .filter(|l| !l.is_empty() && !l.starts_with('('))
            .collect();
        for line in &page0_files {
            assert!(
                !page1_files.contains(line),
                "page1 不应该重复 page0 行 {line}"
            );
        }
    }

    #[tokio::test]
    async fn fs_grep_before_after_context() {
        let tool = make_tool(vec![("a.rs", "L1\nL2\nNEEDLE\nL4\nL5")]);
        let res = tool
            .execute(
                "c",
                json!({
                    "pattern": "NEEDLE",
                    "-B": 1,
                    "-A": 1,
                    "output_mode": "content",
                }),
                CancellationToken::new(),
                None,
            )
            .await
            .expect("execute");
        let text = res.content[0].extract_text().expect("text");
        assert!(text.contains("L2"), "应含 before context: {text}");
        assert!(text.contains("NEEDLE"));
        assert!(text.contains("L4"), "应含 after context: {text}");
        assert!(!text.contains("L1"));
        assert!(!text.contains("L5"));
    }

    #[tokio::test]
    async fn fs_grep_long_line_truncated() {
        let long_line = "x".repeat(800);
        let content = format!("a\n{}\nz", long_line);
        let tool = make_tool(vec![("min.js", content.as_str())]);
        let res = tool
            .execute(
                "c",
                json!({ "pattern": "x{500}", "output_mode": "content" }),
                CancellationToken::new(),
                None,
            )
            .await
            .expect("execute");
        let text = res.content[0].extract_text().expect("text");
        assert!(text.contains("...(truncated)"));
        // 命中行长度（不算前缀和换行）应在 ~500 + 后缀范围内
    }

    #[tokio::test]
    async fn fs_grep_excludes_vcs_dirs() {
        let tool = make_tool(vec![
            (".git/HEAD", "ref: refs/heads/main"),
            ("src/main.rs", "ref: refs/heads/main"),
        ]);
        let res = tool
            .execute(
                "c",
                json!({ "pattern": "ref:" , "output_mode": "files_with_matches"}),
                CancellationToken::new(),
                None,
            )
            .await
            .expect("execute");
        let text = res.content[0].extract_text().expect("text");
        assert!(text.contains("src/main.rs"));
        assert!(
            !text.contains(".git/HEAD"),
            "VCS path should be filtered: {text}"
        );
    }
}
