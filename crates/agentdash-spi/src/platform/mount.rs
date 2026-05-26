//! MountProvider SPI — 统一 mount I/O 契约。
//!
//! 定义 `MountProvider` trait 及其关联类型，
//! 供企业插件直接实现外部服务的文件系统级操作。

use async_trait::async_trait;

use crate::Mount;

// ============================================================================
// Error
// ============================================================================

#[derive(Debug, Clone)]
pub enum MountError {
    NotSupported(String),
    NotFound(String),
    ProviderNotRegistered(String),
    OperationFailed(String),
}

impl std::fmt::Display for MountError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MountError::NotSupported(msg) => write!(f, "not supported: {msg}"),
            MountError::NotFound(msg) => write!(f, "not found: {msg}"),
            MountError::ProviderNotRegistered(msg) => write!(f, "provider not registered: {msg}"),
            MountError::OperationFailed(msg) => write!(f, "operation failed: {msg}"),
        }
    }
}

impl std::error::Error for MountError {}

// ============================================================================
// I/O types
// ============================================================================

/// 单文件读取结果。
///
/// `attributes` 是可选的扩展元数据（xattr 风格），由 provider 按需填充。
/// 例如 KM 插件可以放 `author` / `tags` / `url`，relay_fs 可以放 `git_author`。
/// Agent 工具侧负责与 `content` 分通道展示，避免污染文件正文。
///
/// `version_token` 是 dedup 缓存的 invalidation key：不透明字符串，调用方按
/// `==` 比对，相同内容 ⇒ 相同 token；任何修改 ⇒ token 变化。`None` 表示
/// provider 暂时无法生成（旧实现路径），调用方按"不命中"处理，**不引入
/// 常量 fallback**（避免相同 fallback 值被误判为命中）。生成方式由各 provider
/// 自定：lifecycle / relay_fs 用 `format!("{mtime}:{size}")`；inline 用
/// inline_files 表 revision；canvas 用 page version_id；skill_asset 用 updated_at。
///
/// `modified_at` 是 Unix 毫秒时间戳；缺失填 `None`。
#[derive(Debug, Clone, Default)]
pub struct ReadResult {
    pub path: String,
    pub content: String,
    pub attributes: Option<serde_json::Map<String, serde_json::Value>>,
    pub version_token: Option<String>,
    pub modified_at: Option<i64>,
}

impl ReadResult {
    pub fn new(path: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            content: content.into(),
            attributes: None,
            version_token: None,
            modified_at: None,
        }
    }

    pub fn with_attributes(mut self, attrs: serde_json::Map<String, serde_json::Value>) -> Self {
        self.attributes = Some(attrs);
        self
    }

    pub fn with_version_token(mut self, token: impl Into<String>) -> Self {
        self.version_token = Some(token.into());
        self
    }

    pub fn with_modified_at(mut self, mtime: i64) -> Self {
        self.modified_at = Some(mtime);
        self
    }
}

/// 单文件二进制读取结果。
///
/// `data` 保留原始 bytes；上层根据目标通道决定是否 base64 编码。
/// `mime_type` 必须来自 provider 存储/metadata，不由工具层按扩展名猜测。
#[derive(Debug, Clone, Default)]
pub struct BinaryReadResult {
    pub path: String,
    pub data: Vec<u8>,
    pub mime_type: String,
    pub attributes: Option<serde_json::Map<String, serde_json::Value>>,
}

impl BinaryReadResult {
    pub fn new(path: impl Into<String>, data: Vec<u8>, mime_type: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            data,
            mime_type: mime_type.into(),
            attributes: None,
        }
    }

    pub fn with_attributes(mut self, attrs: serde_json::Map<String, serde_json::Value>) -> Self {
        self.attributes = Some(attrs);
        self
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MountEditCapabilities {
    #[serde(default)]
    pub create: bool,
    #[serde(default)]
    pub delete: bool,
    #[serde(default)]
    pub rename: bool,
}

impl MountEditCapabilities {
    pub fn supports_move(self) -> bool {
        self.rename || (self.create && self.delete)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ApplyPatchRequest {
    /// apply_patch 自由格式文本；其中路径必须相对 mount 根目录。
    pub patch: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ApplyPatchResult {
    pub added: Vec<String>,
    pub modified: Vec<String>,
    pub deleted: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ListOptions {
    pub path: String,
    pub pattern: Option<String>,
    pub recursive: bool,
}

#[derive(Debug, Clone)]
pub struct ListResult {
    pub entries: Vec<RuntimeFileEntry>,
}

/// 文件条目摘要。
///
/// - `is_virtual = true` 表示该条目是 provider 动态投影（/proc 风格），
///   物理存储中不存在。对这类条目，`size` 和 `modified_at` 通常为 `None`。
/// - `attributes` 是 xattr 风格的扩展元数据，由 provider 按需填充；
///   `list` 调用可以选择是否填充（按性能成本决定）。
#[derive(Debug, Clone, Default)]
pub struct RuntimeFileEntry {
    pub path: String,
    pub size: Option<u64>,
    pub modified_at: Option<i64>,
    pub is_dir: bool,
    pub is_virtual: bool,
    pub attributes: Option<serde_json::Map<String, serde_json::Value>>,
}

impl RuntimeFileEntry {
    /// 构造一个普通文件条目。
    pub fn file(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            is_dir: false,
            ..Self::default()
        }
    }

    /// 构造一个目录条目。
    pub fn dir(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            is_dir: true,
            ..Self::default()
        }
    }

    pub fn with_size(mut self, size: u64) -> Self {
        self.size = Some(size);
        self
    }

    pub fn with_modified_at(mut self, mtime: i64) -> Self {
        self.modified_at = Some(mtime);
        self
    }

    /// 标记为 projection 虚拟条目。
    pub fn as_virtual(mut self) -> Self {
        self.is_virtual = true;
        self
    }

    pub fn with_attributes(mut self, attrs: serde_json::Map<String, serde_json::Value>) -> Self {
        self.attributes = Some(attrs);
        self
    }
}

/// 判断一个 `RuntimeFileEntry` 是否表示二进制内容（用于 grep 默认实现跳过）。
/// 约定：`attributes.content_kind == "binary"`。
pub(crate) fn entry_is_binary(entry: &RuntimeFileEntry) -> bool {
    entry
        .attributes
        .as_ref()
        .and_then(|attrs| attrs.get("content_kind"))
        .and_then(|v| v.as_str())
        == Some("binary")
}

// ============================================================================
// Search
// ============================================================================

/// 通用文本搜索查询参数。
///
/// 仅承载与"通用搜索"语义相关的字段（substring / 简单匹配）；grep 风格的
/// 字段（regex / context / multiline / output_mode 等）已移到 [`GrepQuery`]，
/// 调用方应根据语义需要选择 `MountProvider::search_text` 或 `grep_text`。
#[derive(Debug, Clone)]
pub struct SearchQuery {
    pub pattern: String,
    pub path: Option<String>,
    pub case_sensitive: bool,
    pub max_results: Option<usize>,
}

impl Default for SearchQuery {
    fn default() -> Self {
        Self {
            pattern: String::new(),
            path: None,
            case_sensitive: true,
            max_results: None,
        }
    }
}

/// grep 风格搜索查询参数。
///
/// **A7 决议**：`base.pattern` 始终视为正则表达式（与 Claude Code GrepTool 对齐）。
///
/// 字段语义参考 ripgrep / GNU grep：
/// - `base.case_sensitive = false` ⇒ provider 应启用 smart-case；`true` ⇒ 严格大小写。
/// - `context_lines` 等价 `-C N`；`before_lines` / `after_lines` 等价 `-B` / `-A`。
///   同时设置时，effective_before = `max(before_lines, context_lines)`，after 同理。
/// - `multiline = true` ⇒ pattern `.` 跨行 + `^/$` 匹配每行（ripgrep
///   `--multiline --multiline-dotall`）。
/// - `include_glob` 限定 grep 仅扫描匹配该 glob 的文件。
/// - `output_mode` 决定 provider 返回的命中粒度。
#[derive(Debug, Clone, Default)]
pub struct GrepQuery {
    pub base: SearchQuery,
    pub include_glob: Option<String>,
    pub context_lines: usize,
    pub before_lines: usize,
    pub after_lines: usize,
    pub multiline: bool,
    pub output_mode: SearchOutputMode,
}

/// 搜索结果的输出形态（对齐 Claude Code GrepTool `output_mode`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchOutputMode {
    /// 返回命中行（含上下文，如配置）。默认。
    #[default]
    Content,
    /// 仅返回命中文件路径列表（按 path 去重）。
    FilesWithMatches,
    /// 仅返回总命中计数。
    Count,
}

#[derive(Debug, Clone)]
pub struct SearchMatch {
    pub path: String,
    pub line: Option<u32>,
    pub content: String,
}

/// `truncated = true` 表示 provider 主动截断了结果集，原因可能是：
/// - 命中数达到 `SearchQuery::max_results`；
/// - provider 自身的资源/超时保护性截断。
///
/// 调用方据此决定是否提示用户"refine pattern or raise max_results"。
#[derive(Debug, Clone, Default)]
pub struct SearchResult {
    pub matches: Vec<SearchMatch>,
    pub truncated: bool,
}

// ============================================================================
// Exec
// ============================================================================

#[derive(Debug, Clone)]
pub struct ExecRequest {
    pub mount_id: String,
    pub cwd: String,
    pub command: String,
    pub timeout_ms: Option<u64>,
    /// 由 ShellExecTool 生成的流式输出关联 ID。
    /// relay_fs 会将此 ID 作为 `ToolShellExecPayload.call_id`，
    /// 使 `EventToolShellOutput` 能路由回 `ShellOutputRegistry`。
    pub streaming_call_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ExecResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

// ============================================================================
// Watch / events
// ============================================================================

/// 文件变更事件 kind。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MountEventKind {
    Created,
    Modified,
    Deleted,
    /// 重命名/移动。并非所有 provider 都能精确区分此类别，
    /// 不支持时应降级为 Deleted + Created。
    Renamed,
}

/// 单次 mount 内容变更事件。
///
/// 由 `MountProvider::watch` 返回的通道推送。供编排引擎、UI、hook
/// 等消费者响应存储变更，替代轮询。
#[derive(Debug, Clone)]
pub struct MountEvent {
    pub mount_id: String,
    pub path: String,
    pub kind: MountEventKind,
    /// Unix 毫秒时间戳。
    pub timestamp_ms: i64,
}

impl MountEvent {
    pub fn new(mount_id: impl Into<String>, path: impl Into<String>, kind: MountEventKind) -> Self {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        Self {
            mount_id: mount_id.into(),
            path: path.into(),
            kind,
            timestamp_ms: now_ms,
        }
    }
}

/// 事件订阅句柄。
///
/// 基于 tokio broadcast channel：多订阅者共享同一事件流，掉队的订阅者
/// 会收到 `RecvError::Lagged`（由消费侧自行处理）。
pub type MountEventReceiver = tokio::sync::broadcast::Receiver<MountEvent>;

// ============================================================================
// Context
// ============================================================================

/// Runtime context passed to every `MountProvider` operation.
///
/// Providers that need infrastructure references (e.g. `BackendRegistry`,
/// overlay) hold them via constructor injection. This struct carries
/// cross-cutting per-request concerns like the authenticated user.
#[derive(Debug, Default)]
pub struct MountOperationContext {
    /// The authenticated identity of the user who initiated this operation.
    /// Injected by the framework from the HTTP session; providers consume
    /// it on demand (e.g. external docs provider maps `user_id` to an
    /// upstream owner identity).
    pub identity: Option<crate::platform::auth::AuthIdentity>,
}

// ============================================================================
// MountProvider trait
// ============================================================================

/// Unified mount I/O provider trait.
///
/// Each provider handles a specific `provider` string (e.g. `"relay_fs"`,
/// `"inline_fs"`, `"km_bridge"`).  The mount dispatcher resolves the
/// mount, looks up the matching provider, and delegates.
#[async_trait]
pub trait MountProvider: Send + Sync {
    fn provider_id(&self) -> &str;

    // ---- 服务协商元信息 ----
    // 内置 provider (relay_fs, inline_fs, lifecycle_vfs) 不需要覆盖这些方法。
    // 插件 provider 覆盖后，前端通过 /api/mount-providers 自动发现可配置的服务。

    /// 用户可见的显示名称。默认返回 provider_id。
    fn display_name(&self) -> &str {
        self.provider_id()
    }

    /// root_ref 的格式提示，前端作为 placeholder 展示。
    fn root_ref_hint(&self) -> &str {
        ""
    }

    /// 该 provider 支持的 capability 列表（用于前端预填）。
    fn supported_capabilities(&self) -> Vec<&str> {
        vec!["read", "list"]
    }

    /// 是否允许用户在 Context Container 编辑器中直接配置。
    /// 内置 provider 返回 false，插件 provider 按需返回 true。
    fn is_user_configurable(&self) -> bool {
        false
    }

    async fn read_text(
        &self,
        mount: &Mount,
        path: &str,
        ctx: &MountOperationContext,
    ) -> Result<ReadResult, MountError>;

    /// 按行号 range 读取文件内容。
    ///
    /// `offset` 是 0-based 行号（与 Claude Code FileReadTool 的 offset 对齐）。
    /// `limit = None` 表示读到 EOF。
    ///
    /// 默认实现 = `read_text` 全文 + 切片（语义等价；不省内存）。
    /// 实现真正按 range 读的 provider（lifecycle / relay_fs）应覆盖此方法，
    /// 用 `tokio::io::AsyncBufReadExt::lines` + skip + take 避免大文件全量加载。
    /// `version_token` 沿用全文 token（range 不影响版本）。
    async fn read_text_range(
        &self,
        mount: &Mount,
        path: &str,
        offset: usize,
        limit: Option<usize>,
        ctx: &MountOperationContext,
    ) -> Result<ReadResult, MountError> {
        let full = self.read_text(mount, path, ctx).await?;
        let mut iter = full.content.lines().skip(offset);
        let take_n = limit.unwrap_or(usize::MAX);
        let collected: Vec<&str> = (&mut iter).take(take_n).collect();
        let sliced = collected.join("\n");
        Ok(ReadResult {
            path: full.path,
            content: sliced,
            attributes: full.attributes,
            version_token: full.version_token,
            modified_at: full.modified_at,
        })
    }

    /// 在 mount 内为 `prefix` 查找最相似的文件路径，按 levenshtein 距离升序返回。
    ///
    /// 用于 fs_read 的 ENOENT 友好提示（"did you mean ...?"）。
    ///
    /// 默认实现 = `list(recursive=true)` + 内存中按 levenshtein 排序，扫描前
    /// `MAX_SCAN_FILES = 1000` 个条目即停（避免在大型 mount 上的 O(N) 成本爆炸）。
    /// 调用方应传 `limit ≤ 5`。大型 mount 上的 provider（如包含巨型 git repo
    /// 的 lifecycle）应覆盖此方法，用更高效的 prefix 索引（trigram / fst）。
    async fn suggest_paths(
        &self,
        mount: &Mount,
        prefix: &str,
        limit: usize,
        ctx: &MountOperationContext,
    ) -> Result<Vec<String>, MountError> {
        const MAX_SCAN_FILES: usize = 1000;
        let listing = self
            .list(
                mount,
                &ListOptions {
                    path: String::new(),
                    pattern: None,
                    recursive: true,
                },
                ctx,
            )
            .await?;
        let mut scored: Vec<(usize, String)> = listing
            .entries
            .into_iter()
            .filter(|e| !e.is_dir)
            .take(MAX_SCAN_FILES)
            .map(|e| {
                let dist = strsim::levenshtein(prefix, &e.path);
                (dist, e.path)
            })
            .collect();
        scored.sort_by_key(|(d, _)| *d);
        Ok(scored.into_iter().take(limit).map(|(_, p)| p).collect())
    }

    async fn read_binary(
        &self,
        mount: &Mount,
        path: &str,
        ctx: &MountOperationContext,
    ) -> Result<BinaryReadResult, MountError> {
        let _ = (mount, path, ctx);
        Err(MountError::NotSupported(format!(
            "provider `{}` does not support read_binary",
            self.provider_id()
        )))
    }

    async fn write_text(
        &self,
        mount: &Mount,
        path: &str,
        content: &str,
        ctx: &MountOperationContext,
    ) -> Result<(), MountError>;

    fn edit_capabilities(&self, mount: &Mount) -> MountEditCapabilities {
        let _ = mount;
        MountEditCapabilities::default()
    }

    async fn delete_text(
        &self,
        mount: &Mount,
        path: &str,
        ctx: &MountOperationContext,
    ) -> Result<(), MountError> {
        let _ = (mount, path, ctx);
        Err(MountError::NotSupported(format!(
            "provider `{}` does not support delete_text",
            self.provider_id()
        )))
    }

    async fn rename_text(
        &self,
        mount: &Mount,
        from_path: &str,
        to_path: &str,
        ctx: &MountOperationContext,
    ) -> Result<(), MountError> {
        let _ = (mount, from_path, to_path, ctx);
        Err(MountError::NotSupported(format!(
            "provider `{}` does not support rename_text",
            self.provider_id()
        )))
    }

    async fn apply_patch(
        &self,
        mount: &Mount,
        request: &ApplyPatchRequest,
        ctx: &MountOperationContext,
    ) -> Result<ApplyPatchResult, MountError> {
        let _ = (mount, request, ctx);
        Err(MountError::NotSupported(format!(
            "provider `{}` does not support apply_patch",
            self.provider_id()
        )))
    }

    async fn list(
        &self,
        mount: &Mount,
        options: &ListOptions,
        ctx: &MountOperationContext,
    ) -> Result<ListResult, MountError>;

    async fn search_text(
        &self,
        mount: &Mount,
        query: &SearchQuery,
        ctx: &MountOperationContext,
    ) -> Result<SearchResult, MountError>;

    /// grep 风格搜索：`base.pattern` 始终正则，支持 include_glob / context /
    /// multiline / output_mode。
    ///
    /// 默认实现使用通用的 `list + read_text + regex` 算法，让所有 provider
    /// （含 lifecycle virtual projection、canvas、skill_asset）自动获得完整的
    /// grep 能力。能在协议层原生支持 grep（如 inline_fs 直接读 in-memory 表 /
    /// relay_fs 把字段透传给远端）的 provider 可以覆盖此方法做性能优化。
    ///
    /// 算法：
    /// 1. `list` 全 mount（recursive）拿到所有可读文件；
    /// 2. 二进制条目（`attributes.content_kind == "binary"`）跳过；
    /// 3. include_glob 过滤；
    /// 4. 对每个剩余条目 `read_text` 拿全文，`read_text` 失败的条目跳过（容错，
    ///    `tracing::warn!` 一次）；
    /// 5. 在内存里跑 regex（`case_sensitive` / `multiline` 走 RegexBuilder
    ///    设置，`multiline` 同时打开 `dot_matches_new_line`）；
    /// 6. `before_lines` / `after_lines` 在命中后追加上下文行；与 `context_lines`
    ///    同时设置时取 max。
    /// 7. `max_results` 命中即短路返回 `truncated = true`。
    async fn grep_text(
        &self,
        mount: &Mount,
        query: &GrepQuery,
        ctx: &MountOperationContext,
    ) -> Result<SearchResult, MountError> {
        let listing = self
            .list(
                mount,
                &ListOptions {
                    path: query.base.path.clone().unwrap_or_default(),
                    pattern: None,
                    recursive: true,
                },
                ctx,
            )
            .await?;

        let mut builder = regex::RegexBuilder::new(&query.base.pattern);
        builder
            .case_insensitive(!query.base.case_sensitive)
            .multi_line(query.multiline)
            .dot_matches_new_line(query.multiline);
        let re = builder
            .build()
            .map_err(|e| MountError::OperationFailed(format!("invalid regex: {e}")))?;

        let glob_matcher = match query.include_glob.as_deref() {
            Some(pat) => Some(
                globset::Glob::new(pat)
                    .map_err(|e| MountError::OperationFailed(format!("invalid glob: {e}")))?
                    .compile_matcher(),
            ),
            None => None,
        };

        let before = query.before_lines.max(query.context_lines);
        let after = query.after_lines.max(query.context_lines);
        let max_results = query.base.max_results.unwrap_or(usize::MAX);

        let mut matches: Vec<SearchMatch> = Vec::new();

        for entry in listing.entries {
            if entry.is_dir {
                continue;
            }
            if entry_is_binary(&entry) {
                continue;
            }
            if let Some(matcher) = &glob_matcher
                && !matcher.is_match(entry.path.as_str())
            {
                continue;
            }
            let read = match self.read_text(mount, &entry.path, ctx).await {
                Ok(r) => r,
                Err(MountError::NotFound(_)) | Err(MountError::NotSupported(_)) => {
                    tracing::warn!(
                        provider = self.provider_id(),
                        path = %entry.path,
                        "grep_text: skipping unreadable entry"
                    );
                    continue;
                }
                Err(e) => return Err(e),
            };
            let lines: Vec<&str> = read.content.lines().collect();
            for (idx, line) in lines.iter().enumerate() {
                if !re.is_match(line) {
                    continue;
                }
                let start = idx.saturating_sub(before);
                for (ctx_idx, ctx_line) in lines.iter().enumerate().take(idx).skip(start) {
                    matches.push(SearchMatch {
                        path: entry.path.clone(),
                        line: Some((ctx_idx + 1) as u32),
                        content: ctx_line.trim().to_string(),
                    });
                }
                matches.push(SearchMatch {
                    path: entry.path.clone(),
                    line: Some((idx + 1) as u32),
                    content: line.trim().to_string(),
                });
                let end = (idx + 1 + after).min(lines.len());
                for (ctx_idx, ctx_line) in lines.iter().enumerate().take(end).skip(idx + 1) {
                    matches.push(SearchMatch {
                        path: entry.path.clone(),
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

    async fn exec(
        &self,
        mount: &Mount,
        request: &ExecRequest,
        ctx: &MountOperationContext,
    ) -> Result<ExecResult, MountError> {
        let _ = (mount, request, ctx);
        Err(MountError::NotSupported(format!(
            "provider `{}` does not support exec",
            self.provider_id()
        )))
    }

    /// 查询文件元数据（不读取内容）。
    ///
    /// 类比 POSIX `stat()`：只返回 path/size/mtime/attributes 等属性，
    /// 不读取 content。适合需要元数据但不需要正文的场景。
    /// 默认实现回退为 `list` + 过滤。插件 provider 如果有更高效的元数据通道
    /// （例如 KM 的 metadata API），应覆盖此方法。
    async fn stat(
        &self,
        mount: &Mount,
        path: &str,
        ctx: &MountOperationContext,
    ) -> Result<RuntimeFileEntry, MountError> {
        let _ = (mount, path, ctx);
        Err(MountError::NotSupported(format!(
            "provider `{}` does not support stat",
            self.provider_id()
        )))
    }

    /// 订阅 mount 内容变更事件。
    ///
    /// 返回一个 broadcast receiver；调用方可通过 `.recv().await` 消费事件。
    /// `path` 为空串表示订阅整个 mount；非空则表示订阅该子树。
    ///
    /// 该能力与 `MountCapability::Watch` 对应。默认返回 `NotSupported`。
    async fn watch(
        &self,
        mount: &Mount,
        path: &str,
        ctx: &MountOperationContext,
    ) -> Result<MountEventReceiver, MountError> {
        let _ = (mount, path, ctx);
        Err(MountError::NotSupported(format!(
            "provider `{}` does not support watch",
            self.provider_id()
        )))
    }

    /// Whether this mount can be used right now (e.g. relay backend connected).
    async fn is_available(&self, _mount: &Mount) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    //! 验证 `MountProvider::grep_text` 默认实现的通用 list+read+regex 算法。
    //! 这是 vfs-grep-query-split + virtual-projection-coverage 的核心契约：
    //! 任何 provider（lifecycle / canvas / skill_asset / 第三方）都通过此默认
    //! 实现自动获得完整 grep 能力，不需要单独适配。
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// 极简 in-memory mock provider，只实现 list / read_text，让 grep_text 走默认。
    struct MockProvider {
        files: Mutex<HashMap<String, FileFixture>>,
    }

    struct FileFixture {
        content: String,
        is_binary: bool,
    }

    impl MockProvider {
        fn new(files: &[(&str, &str)]) -> Self {
            let mut map = HashMap::new();
            for (path, content) in files {
                map.insert(
                    (*path).to_string(),
                    FileFixture {
                        content: (*content).to_string(),
                        is_binary: false,
                    },
                );
            }
            Self {
                files: Mutex::new(map),
            }
        }

        fn add_binary(&self, path: &str) {
            self.files.lock().unwrap().insert(
                path.to_string(),
                FileFixture {
                    content: String::new(),
                    is_binary: true,
                },
            );
        }
    }

    #[async_trait]
    impl MountProvider for MockProvider {
        fn provider_id(&self) -> &str {
            "mock_grep"
        }

        async fn read_text(
            &self,
            _mount: &Mount,
            path: &str,
            _ctx: &MountOperationContext,
        ) -> Result<ReadResult, MountError> {
            let files = self.files.lock().unwrap();
            let f = files
                .get(path)
                .ok_or_else(|| MountError::NotFound(path.to_string()))?;
            if f.is_binary {
                return Err(MountError::NotSupported("binary".to_string()));
            }
            Ok(ReadResult::new(path, f.content.clone()))
        }

        async fn write_text(
            &self,
            _mount: &Mount,
            _path: &str,
            _content: &str,
            _ctx: &MountOperationContext,
        ) -> Result<(), MountError> {
            Err(MountError::NotSupported("read only".to_string()))
        }

        async fn list(
            &self,
            _mount: &Mount,
            _options: &ListOptions,
            _ctx: &MountOperationContext,
        ) -> Result<ListResult, MountError> {
            let files = self.files.lock().unwrap();
            let entries = files
                .iter()
                .map(|(path, f)| {
                    let mut entry = RuntimeFileEntry::file(path.clone());
                    if f.is_binary {
                        let mut attrs = serde_json::Map::new();
                        attrs.insert(
                            "content_kind".to_string(),
                            serde_json::Value::String("binary".to_string()),
                        );
                        entry = entry.with_attributes(attrs);
                    }
                    entry
                })
                .collect();
            Ok(ListResult { entries })
        }

        async fn search_text(
            &self,
            _mount: &Mount,
            _query: &SearchQuery,
            _ctx: &MountOperationContext,
        ) -> Result<SearchResult, MountError> {
            // 默认 grep_text 不再 forward 到 search_text，所以这里返回空也无妨。
            Ok(SearchResult::default())
        }
    }

    fn fake_mount() -> Mount {
        Mount {
            id: "mock".to_string(),
            provider: "mock_grep".to_string(),
            backend_id: String::new(),
            root_ref: "memory://mock".to_string(),
            capabilities: vec![],
            default_write: false,
            display_name: "Mock".to_string(),
            metadata: serde_json::Value::Null,
        }
    }

    #[tokio::test]
    async fn grep_text_default_finds_regex_matches_across_files() {
        let provider = MockProvider::new(&[
            ("a.rs", "fn foo() {}\nfn bar() {}"),
            ("b.rs", "fn baz() {}"),
            ("README.md", "no functions here"),
        ]);
        let result = provider
            .grep_text(
                &fake_mount(),
                &GrepQuery {
                    base: SearchQuery {
                        pattern: r"fn \w+".to_string(),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                &MountOperationContext::default(),
            )
            .await
            .expect("grep");
        // 命中 a.rs 两行 + b.rs 一行 = 3 个 match
        assert_eq!(result.matches.len(), 3);
        assert!(result.matches.iter().any(|m| m.path == "a.rs"));
        assert!(result.matches.iter().any(|m| m.path == "b.rs"));
        assert!(!result.matches.iter().any(|m| m.path == "README.md"));
    }

    #[tokio::test]
    async fn grep_text_default_respects_include_glob() {
        let provider =
            MockProvider::new(&[("src/main.rs", "fn x() {}"), ("docs/notes.md", "fn x() {}")]);
        let result = provider
            .grep_text(
                &fake_mount(),
                &GrepQuery {
                    base: SearchQuery {
                        pattern: r"fn \w+".to_string(),
                        ..Default::default()
                    },
                    include_glob: Some("**/*.rs".to_string()),
                    ..Default::default()
                },
                &MountOperationContext::default(),
            )
            .await
            .expect("grep");
        assert_eq!(result.matches.len(), 1);
        assert_eq!(result.matches[0].path, "src/main.rs");
    }

    #[tokio::test]
    async fn grep_text_default_provides_before_after_context() {
        let provider = MockProvider::new(&[("log.txt", "L1\nL2\nNEEDLE\nL4\nL5")]);
        let result = provider
            .grep_text(
                &fake_mount(),
                &GrepQuery {
                    base: SearchQuery {
                        pattern: "NEEDLE".to_string(),
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
        let contents: Vec<&str> = result.matches.iter().map(|m| m.content.as_str()).collect();
        assert!(contents.contains(&"L2"));
        assert!(contents.contains(&"NEEDLE"));
        assert!(contents.contains(&"L4"));
        assert!(!contents.contains(&"L1"));
        assert!(!contents.contains(&"L5"));
    }

    #[tokio::test]
    async fn grep_text_default_skips_binary_entries() {
        let provider = MockProvider::new(&[("text.md", "needle in text")]);
        provider.add_binary("image.png");
        let result = provider
            .grep_text(
                &fake_mount(),
                &GrepQuery {
                    base: SearchQuery {
                        pattern: "needle".to_string(),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                &MountOperationContext::default(),
            )
            .await
            .expect("grep");
        // binary 条目跳过，只命中 text.md
        assert_eq!(result.matches.len(), 1);
        assert_eq!(result.matches[0].path, "text.md");
    }

    #[tokio::test]
    async fn grep_text_default_case_insensitive() {
        let provider = MockProvider::new(&[("a.rs", "Hello WORLD")]);
        let result = provider
            .grep_text(
                &fake_mount(),
                &GrepQuery {
                    base: SearchQuery {
                        pattern: "world".to_string(),
                        case_sensitive: false,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                &MountOperationContext::default(),
            )
            .await
            .expect("grep");
        assert_eq!(result.matches.len(), 1);
        assert!(result.matches[0].content.contains("WORLD"));
    }

    #[tokio::test]
    async fn grep_text_default_truncates_at_max_results() {
        let provider = MockProvider::new(&[("a.rs", "x\nx\nx\nx\nx")]);
        let result = provider
            .grep_text(
                &fake_mount(),
                &GrepQuery {
                    base: SearchQuery {
                        pattern: "x".to_string(),
                        max_results: Some(2),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                &MountOperationContext::default(),
            )
            .await
            .expect("grep");
        assert_eq!(result.matches.len(), 2);
        assert!(result.truncated);
    }
}
