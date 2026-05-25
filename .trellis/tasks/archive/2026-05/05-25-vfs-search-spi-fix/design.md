# Design — VFS Search SPI 修复

> **Parent task:** [05-25-fs-tools-optimization-review](../05-25-fs-tools-optimization-review/design.md)
> **PRD:** [./prd.md](./prd.md)
>
> **CC 对齐目标：** 详见 [prd.md 顶部](./prd.md) 链接段。

## §1 目标与边界

补齐 `agentdash-spi::platform::mount` 的 SPI 字段与方法，使三个 fs 工具
（read / grep / glob）的 rebuild 任务能在不动 SPI 的前提下落地。

**本任务做：**

- `SearchQuery` / `SearchResult` / `ReadResult` 字段扩展（含 `SearchOutputMode` 枚举）。
- `MountProvider::read_text_range` / `suggest_paths` 默认实现。
- `relay_service::search_text_extended` 字段透传修复。
- 4 个 provider 接入新字段（默认行为不变 + warn-and-degrade）。
- 5 项集成测试 + SPI doccomment 补全。

**本任务不做（明确转给后续任务）：**

- `read_text_range` 的 lifecycle / relay_fs 真按 range 优化 → fs-read-rebuild。
- `search_text` 中 `is_regex` / `include_glob` / `output_mode` 的真实语义（除 inline 已具备的部分） → fs-grep-rebuild。
- `suggest_paths` 的工具层调用 → fs-read-rebuild。
- `SearchQuery` → `GrepQuery` 拆分 → vfs-grep-query-split (P2 follow-up)。
- `RuntimeFileEntry.modified_at` 的填充语义升级 → fs-glob-rebuild。

## §2 SearchQuery 字段集

### §2.1 完整 struct

```rust
#[derive(Debug, Clone)]
pub struct SearchQuery {
    pub pattern: String,
    pub path: Option<String>,
    pub case_sensitive: bool,
    pub max_results: Option<usize>,
    // === NEW ===
    pub is_regex: bool,
    pub include_glob: Option<String>,
    pub context_lines: usize,
    pub before_lines: usize,
    pub after_lines: usize,
    pub multiline: bool,
    pub output_mode: SearchOutputMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchOutputMode {
    #[default]
    Content,
    FilesWithMatches,
    Count,
}
```

### §2.2 字段语义表（对齐 CC GrepTool）

| 字段 | 语义 | CC 对应 | 默认 | 由谁兑现 |
|------|------|---------|------|----------|
| `pattern` | 始终是正则表达式（A7 决议）。 | `pattern` | 必填 | service / provider |
| `path` | mount://path 起点；空 = 全 mount。 | `path` | None | service |
| `case_sensitive` | false ⇒ smart-case；true ⇒ 严格大小写。 | `-i` 反义 | true | provider |
| `max_results` | 命中行数硬上限；超出截断 + `truncated=true`。 | `head_limit` | None ⇒ 50（service 兜底） | service |
| `is_regex` | **保留位**：当前所有调用方传 `true`（pattern 始终正则）；预留为后续 `GrepQuery` 拆分前的占位。 | n/a（CC 始终正则） | true | service |
| `include_glob` | 只对匹配此 glob 的文件应用搜索。 | `glob` | None | provider |
| `context_lines` | `-C` 等价：before/after 各 N 行（0 = 不输出上下文）。 | `-C` | 0 | provider |
| `before_lines` | `-B` 等价；与 `context_lines` 同时设置时取 `max(before_lines, context_lines)`。 | `-B` | 0 | provider |
| `after_lines` | `-A` 等价；与 `context_lines` 同时设置时取 `max(after_lines, context_lines)`。 | `-A` | 0 | provider |
| `multiline` | true ⇒ pattern `.` 跨行 + `^/$` 匹配每行；与 ripgrep `--multiline + --multiline-dotall` 等价。 | `multiline` | false | provider |
| `output_mode` | `Content` / `FilesWithMatches` / `Count`。 | `output_mode` | `Content` | service / provider |

**为什么保留 `is_regex`：** parent A7 决议是"pattern 始终正则"，但 SPI 字段保留以支持
后续 `GrepQuery` 拆分（FU#1）将其作为 grep-specific 字段拆出。本任务不删除字段，
service 层始终传 `true`。这避免了 SPI 字段两次 churn。

### §2.3 `Default` 实现

```rust
impl Default for SearchQuery {
    fn default() -> Self {
        Self {
            pattern: String::new(),
            path: None,
            case_sensitive: true,
            max_results: None,
            is_regex: true,
            include_glob: None,
            context_lines: 0,
            before_lines: 0,
            after_lines: 0,
            multiline: false,
            output_mode: SearchOutputMode::Content,
        }
    }
}
```

测试和老调用方可用 `SearchQuery { pattern, ..Default::default() }` 简化构造，
减少 breaking 影响面。

## §3 SearchResult 扩展

```rust
#[derive(Debug, Clone, Default)]
pub struct SearchResult {
    pub matches: Vec<SearchMatch>,
    pub truncated: bool,  // NEW
}

#[derive(Debug, Clone)]
pub struct SearchMatch {
    pub path: String,
    pub line: Option<u32>,
    pub content: String,
}
```

**`truncated` 触发条件（写入 SPI doccomment）：**

- provider 内部按 `max_results` 截断结果（命中行 ≥ max_results）。
- provider 主动放弃（资源/超时）保护性截断。

provider 实现要求：每个 `search_text` 实现都要在自己的截断点写 `truncated = true`，
当前所有 provider 的截断逻辑要在本任务一并审计。

## §4 ReadResult 扩展 + version_token 协议

```rust
#[derive(Debug, Clone, Default)]
pub struct ReadResult {
    pub path: String,
    pub content: String,
    pub attributes: Option<serde_json::Map<String, serde_json::Value>>,
    // === NEW ===
    pub version_token: Option<String>,
    pub modified_at: Option<i64>,
}
```

### §4.1 `version_token` 语义（写进 doccomment）

- 不透明字符串。dedup 缓存比对 `==` 即可，**不解析内容**。
- provider 自由选择生成方式，但应满足：相同字节内容 ⇒ 相同 token；任何修改 ⇒ token 变化。
- `None` 表示 provider 暂时无法生成（旧版本 / 错误路径）；调用方按"不命中"处理，
  **不引入常量 fallback**（避免相同的 fallback 值被误判为命中）。

### §4.2 各 provider 生成策略

| Provider | version_token | modified_at | 备注 |
|----------|--------------|------------|------|
| `lifecycle` | `format!("{mtime}:{size}")` | filesystem `mtime` | mtime/size 取自 `tokio::fs::metadata` |
| `relay_fs` | `format!("{mtime}:{size}")` | 同上 | 同 lifecycle 走 std fs |
| `inline_fs` | inline_files 表 `revision` 字段（自增整数 → String） | inline_files 表 `updated_at` | revision 已存在，不需 schema 变更 |
| `canvas` | canvas page 的 `version_id` | canvas page `updated_at` | 已存在 metadata |
| `skill_asset` | skill 的 `updated_at`（同时复用为 token） | 同 token | skill 元数据无独立版本号；mtime 已具备等价语义 |

**本任务范围**：所有 provider 都要填好 `version_token` + `modified_at`。fs-read-rebuild 任务
在工具层用这两个字段做 LRU 64 dedup。

## §5 `MountProvider::read_text_range` 默认实现

```rust
async fn read_text_range(
    &self,
    mount: &Mount,
    path: &str,
    offset: usize,        // 0-based 行号；与 CC FileReadTool 的 offset 一致
    limit: Option<usize>, // 行数上限；None = 读到 EOF
    ctx: &MountOperationContext,
) -> Result<ReadResult, MountError> {
    let full = self.read_text(mount, path, ctx).await?;
    let mut lines = full.content.lines();
    let skipped: Vec<&str> = lines.by_ref().skip(offset).collect();
    let take_n = limit.unwrap_or(skipped.len());
    let sliced = skipped.into_iter().take(take_n).collect::<Vec<_>>().join("\n");
    Ok(ReadResult {
        path: full.path,
        content: sliced,
        attributes: full.attributes,
        version_token: full.version_token,
        modified_at: full.modified_at,
    })
}
```

**要点：**

- 默认实现 = 读全文 + 切片，与现状等价。
- offset 用 0-based 行号（不是字节）—— 与 CC `FileReadTool.offset` 对齐。
- `limit = None` ⇒ 读到 EOF。
- `version_token` 沿用全文读取的 token（range 不影响版本）。

**fs-read-rebuild 任务**会重写 lifecycle / relay_fs 的此方法以真正按 range 读
（用 `tokio::io::AsyncBufReadExt::lines` + skip + take），避免大文件全文加载。

## §6 `MountProvider::suggest_paths` 默认实现

```rust
async fn suggest_paths(
    &self,
    mount: &Mount,
    prefix: &str,
    limit: usize,
    ctx: &MountOperationContext,
) -> Result<Vec<String>, MountError> {
    const MAX_SCAN_FILES: usize = 1000;  // 性能护栏
    let listing = self.list(
        mount,
        &ListOptions {
            path: String::new(),
            pattern: None,
            recursive: true,
        },
        ctx,
    ).await?;
    let mut scored: Vec<(usize, String)> = listing.entries.into_iter()
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
```

**性能护栏：**

- `MAX_SCAN_FILES = 1000`：扫描前 N 个条目即停。fs-read-rebuild 调用方传 `limit ≤ 5`。
- 默认实现成本 = O(N · |prefix|)，N ≤ 1000 ⇒ 单次调用 < 5ms（粗估）。
- 大 mount 上的 provider（如 lifecycle 巨型 git repo）应在 fs-read-rebuild 之后
  覆盖此方法，用更高效的 prefix 索引（trigram / fst）。

**依赖新增：** `strsim` crate（cargo 生态主流，纯 Rust，无 transitive dep）。
加在 `agentdash-spi/Cargo.toml`。

## §7 `relay_service::search_text_extended` 字段透传修复

### §7.1 当前问题（F1）

[relay_service.rs:724-757](../../../crates/agentdash-application/src/vfs/relay_service.rs#L724-L757)
非 inline 分支构造 `SearchQuery` 时只填 4 个字段，丢弃 `is_regex` /
`include_glob` / `context_lines` / `multiline` / `output_mode`。

### §7.2 修复

```rust
let query = SearchQuery {
    pattern: params.pattern.clone(),
    path: params.path.clone(),
    case_sensitive: params.case_sensitive,
    max_results: params.max_results,
    is_regex: params.is_regex,
    include_glob: params.include_glob.clone(),
    context_lines: params.context_lines,
    before_lines: params.before_lines,
    after_lines: params.after_lines,
    multiline: params.multiline,
    output_mode: params.output_mode,
};
let result = provider.search_text(&mount, &query, &ctx).await?;
Ok((result.matches, result.truncated))
```

注意 `TextSearchParams`（service 层入参）也要同步扩展同样的字段。这是 service
层 type，不在 SPI 包里 —— 与 SPI 字段一一映射。

### §7.3 inline 分支审计（F1 部分修复）

`search_inline` (relay_service.rs:763-851) 现在已经支持 regex / context_lines，
但是不支持 `include_glob`。本任务在 inline 分支也加 `include_glob` 过滤
（在已 walk 出的文件列表上加一道 glob 匹配；用 `globset` crate，已是项目依赖）。

## §8 4 个 Provider 接入策略

**统一原则（warn-and-degrade）：** 接入新字段后，能支持的语义直接支持；
不能支持的字段 `tracing::warn!()` 一行提示后忽略。这样 fs 工具调用任意 provider
都不会失败，只是 grep/glob 高级能力在某些 provider 上降级。

### §8.1 `inline` provider

- `is_regex = true`：现有实现已是 regex（`regex::Regex::new(pattern)`），无变化。
- `include_glob`：在 inline_files 表 query 之后加一道 globset 过滤。
- `context_lines / before_lines / after_lines`：现有实现已支持 context；
  本任务把字段从单一 context 拆成 before/after 双字段，按 §2.2 优先级合并。
- `multiline`：在 RegexBuilder 上加 `.multi_line(true).dot_matches_new_line(true)`。
- `output_mode = Content`：现状默认；其他两个模式由 fs-grep-rebuild 在工具层降级处理
  （从 Content 结果聚合）。本任务 inline 实现仅识别字段，不改输出形态。

### §8.2 `canvas` provider

- canvas 现行 `search_text` 是 substring 搜索（不识别 regex）。
- `is_regex = true`（默认）：tracing::warn!("canvas provider 暂不支持 regex 搜索，
  退化为 substring") 一次（用 `tracing::warn_span` 或限频日志避免刷屏）。
- 其他高级字段同样 warn-and-degrade。
- 本任务内 canvas provider 仅 = 接受字段、不处理；fs-grep-rebuild 评估是否值得在
  canvas 上加 regex（用 `regex::Regex` + canvas page text 全量扫）。

### §8.3 `lifecycle` / `relay_fs` provider

- 走 ripgrep（外部进程）。
- 把新字段映射到 ripgrep CLI 参数：
  - `is_regex = true` ⇒ 默认（ripgrep 默认正则）；`is_regex = false` ⇒ `--fixed-strings`。
  - `include_glob` ⇒ `--glob <pattern>`。
  - `context_lines` ⇒ `-C N`；`before_lines` ⇒ `-B N`；`after_lines` ⇒ `-A N`。
    同时设置时取 max（在 query 处理层合并）。
  - `multiline` ⇒ `--multiline --multiline-dotall`。
  - `case_sensitive = false` ⇒ `--smart-case`；`true` ⇒ 默认（不传 `-i`）。
- `output_mode` 在工具层处理（fs-grep-rebuild），SPI 不区分 —— provider 始终返回 Content。

### §8.4 `skill_asset` provider

- 与 canvas 类似，substring 搜索。warn-and-degrade。

### §8.5 进入策略：4 provider 接入是否并行？

PRD 注明工时 1–3 day。4 provider 接入实际只是签名扩展（每个 provider 不到 30 行 diff）。
**inline 顺序处理即可**，不需要 subagent 并行（subagent overhead > 4 个 provider 的小改动）。

## §9 测试矩阵

| 测试 | Provider | 验证 |
|------|----------|------|
| T1 `is_regex=true` | inline / canvas / lifecycle | inline regex 工作；canvas warn 后 substring；lifecycle ripgrep regex |
| T2 `include_glob="*.rs"` | inline / canvas | inline 过滤生效；canvas 受 list 阶段过滤 |
| T3 `truncated` | inline / canvas / lifecycle | 命中数 ≥ max_results 时 truncated=true |
| T4 `version_token` 非 None | lifecycle / canvas | 两个 provider 各自的 token 生成路径走通 |
| T5 `read_text_range` 默认实现 | canvas / inline | offset=2, limit=3 时正确返回第 3-5 行 |

集成测试落 `crates/agentdash-application/tests/`（已有 vfs 集成测试目录）。

## §10 兼容性与发布

- **branch:** `release/fs-tools-rebuild`（已切 + 已落 PRD）。
- **不直接 merge 到 main**：与三个 rebuild 任务同 release。
- **API breaking：** SearchQuery 加字段、SearchResult.truncated 新字段、ReadResult 新字段。
  外部插件 provider（如有第三方实现 `MountProvider`）需要适配；release notes 写明。
- **测试矩阵：** 所有 5 项 + `cargo test -p agentdash-spi -p agentdash-application` 全绿。
- **doccomment：** SPI 文件头加一段 "Search 字段语义（A7 决议：pattern 始终正则）"。

## §11 风险

| 风险 | 触发 | 应对 |
|------|------|------|
| inline 现有 search_text 语义微变（context_lines 字段拆 before/after） | 旧测试 | 保留向后兼容：context_lines=N 同时填 before/after=N；测试不动 |
| `strsim` crate 拉一道额外 dep | 编译时间 | strsim 是纯 Rust 单文件 ~200 行，可忽略 |
| `version_token` provider 实现遗漏 | 接入分支不全 | 测试矩阵 T4 覆盖两个 provider；其他在 fs-read-rebuild 校验 |
| relay_fs ripgrep 进程参数顺序 | shell escape | 用 `tokio::process::Command::args` 数组传参，不拼字符串 |

## §12 决策矩阵（开放点）

| ID | 决策 | 状态 |
|----|------|------|
| D1 | SearchOutputMode 命名 = `Content / FilesWithMatches / Count` | accept（CC 对齐） |
| D2 | `is_regex` 字段保留还是直接删 | accept 保留（FU#1 拆分前的占位） |
| D3 | `before_lines` / `after_lines` 与 `context_lines` 同时设置时取 max | accept（GNU grep / CC 行为） |
| D4 | `read_text_range.offset` = 行号还是字节 | accept 行号（CC 对齐） |
| D5 | `suggest_paths` 默认实现的 MAX_SCAN_FILES = 1000 | accept（性能护栏，可在覆盖实现里去除） |
| D6 | strsim crate 引入 | accept |
| D7 | provider 接入并行 vs 顺序 | accept 顺序（小改动 + subagent overhead） |
| D8 | inline 分支 include_glob 用 globset crate 过滤 | accept（已是依赖） |
