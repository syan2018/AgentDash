# VFS Search SPI 修复

> **Parent task:** [05-25-fs-tools-optimization-review](../05-25-fs-tools-optimization-review/design.md)
> 决策来源：parent 的 design.md §4 决策矩阵（F1 / F2 / F4 三条 SPI 修复）。
>
> **CC 对齐目标（SPI 层无直接对应；以三个工具的字段/输出语义为准）：**
> - [GrepTool.ts](../../../references/claude-code/src/tools/GrepTool/GrepTool.ts)（驱动 SearchQuery 字段扩展）
> - [GlobTool.ts](../../../references/claude-code/src/tools/GlobTool/GlobTool.ts)（驱动 RuntimeFileEntry / SearchResult.truncated）
> - [FileReadTool.ts](../../../references/claude-code/src/tools/FileReadTool/FileReadTool.ts)（驱动 ReadResult 扩展 + read_text_range）
> - [utils/fileStateCache.ts](../../../references/claude-code/src/utils/fileStateCache.ts)（version_token 设计参考；CC 用 mtime/size）

## Goal

补齐 `agentdash-spi` 中 `MountProvider` SPI 的字段与方法，让 `SearchQuery` /
`SearchResult` / `ReadResult` 能承载 fs_grep / fs_glob / fs_read 重做所需的
全部信息。**这是基础设施层修复，必须最先 merge**，三个 rebuild 任务都依赖它。

## Background — 当前 SPI 缺口

核实自 parent 任务 design.md §0bis（关键发现 F1/F2/F3/F4）：

- **F1.** `SearchQuery` 字段不全。当前只有 `pattern/path/case_sensitive/max_results`，
  导致 `relay_service::search_text_extended` 的非 inline 分支
  ([relay_service.rs:724-757](../../../crates/agentdash-application/src/vfs/relay_service.rs#L724-L757))
  把 `is_regex` / `include_glob` / `context_lines` 静默丢弃。
- **F2.** `ReadResult` 没有 `version_token` / `modified_at` 字段，dedup 缓存
  没有 invalidation key 来源。
- **F3.** 非 inline 分支的 `truncated` 永远返回 `false`（`SearchResult` 也无
  `truncated` 字段）。
- **F4.** `MountProvider` 没有 `read_text_range` 方法，所有按行/字节的范围读
  都退化为整文件加载。
- **附加：** 没有 `suggest_paths` 方法，fs_read 的 ENOENT 友好提示无处落脚。

## Requirements

### R1 — `SearchQuery` 字段补齐

```rust
pub struct SearchQuery {
    pub pattern: String,
    pub path: Option<String>,
    pub case_sensitive: bool,
    pub max_results: Option<usize>,
    // NEW
    pub is_regex: bool,
    pub include_glob: Option<String>,
    pub context_lines: usize,
    pub before_lines: usize,
    pub after_lines: usize,
    pub multiline: bool,
    pub output_mode: SearchOutputMode,
}

pub enum SearchOutputMode {
    Content,           // 默认
    FilesWithMatches,
    Count,
}
```

### R2 — `SearchResult` 加 `truncated`

```rust
pub struct SearchResult {
    pub matches: Vec<SearchMatch>,
    pub truncated: bool,  // NEW
}
```

### R3 — `ReadResult` 扩展

```rust
pub struct ReadResult {
    pub path: String,
    pub content: String,
    pub attributes: Option<Map<String, Value>>,
    // NEW
    pub version_token: Option<String>,
    pub modified_at: Option<i64>,
}
```

`version_token` 语义（在 SPI 文档明确）：

- provider 各自决定生成方式：lifecycle/relay_fs 用 `format!("{mtime}:{size}")`；
  canvas 用 canvas 的 `version_id`；inline 用 inline_files 表 revision；
  skill_asset 用 skill 的 `updated_at`。
- 取不到时填 `None` —— 调用方（dedup）按"不命中"处理，**不引入常量 fallback**。

### R4 — `MountProvider::read_text_range`

```rust
trait MountProvider {
    async fn read_text_range(
        &self,
        mount: &Mount,
        path: &str,
        offset: usize,
        limit: Option<usize>,
        ctx: &MountOperationContext,
    ) -> Result<ReadResult, MountError> {
        // default impl: read_text + slice（与现状等价）
    }
}
```

**本任务范围内不优化 lifecycle/relay_fs 的真实 range 读**——默认实现即可，
fs-read-rebuild 任务里再各自重写优化。

### R5 — `MountProvider::suggest_paths`

```rust
trait MountProvider {
    async fn suggest_paths(
        &self,
        mount: &Mount,
        prefix: &str,
        limit: usize,
        ctx: &MountOperationContext,
    ) -> Result<Vec<String>, MountError> {
        // default impl: list(recursive=true) + levenshtein 排序 + take(limit)
    }
}
```

### R6 — `relay_service::search_text_extended` 修复字段透传

非 inline 分支必须把所有新字段都填进 `SearchQuery` 后再调
`provider.search_text`。`SearchResult.truncated` 也要正确反映。

### R7 — 4 个 provider 接入新字段（默认行为不变）

inline / canvas / lifecycle / skill_asset 的 `search_text` 实现至少在签名上
接入新字段，**默认行为保持现状**。真正使用新字段的语义在三个 rebuild 任务
里实现。

## Acceptance Criteria

- [ ] `cargo build` 整个 workspace 通过。
- [ ] `cargo test -p agentdash-spi -p agentdash-application` 全绿。
- [ ] 现有 fs_grep / fs_glob / fs_read 调用方**无需修改**即可工作。
- [ ] 新加 5 个集成测试：
  - `SearchQuery::is_regex=true` 在 inline / canvas / lifecycle 上分别工作。
  - `SearchQuery::include_glob` 在 inline / canvas 上分别工作。
  - `SearchResult::truncated` 在三个 provider 上正确反映截断状态。
  - `ReadResult::version_token` 在 lifecycle 和 canvas 两个 provider 上有非 None 值。
  - `read_text_range` 默认实现在 canvas / inline 上行为正确。
- [ ] SPI 文档（doccomment）明确 `version_token` 语义、`read_text_range`
      默认行为、`suggest_paths` 默认成本提示。

## Constraints / 不在范围

- **不**优化 lifecycle/relay_fs 的 `read_text_range` 真实 range 读 → fs-read-rebuild。
- **不**写 fs_grep / fs_glob / fs_read 工具层逻辑 → 三个 rebuild。
- **不**做 `vfs-grep-query-split`（trait split） → follow-up task。

## Notes

- 在 `release/fs-tools-rebuild` 分支上完成；**不直接 merge 到 main**。
- 工时估算：1–3 day。
- 依赖：无。
- 后续阻塞：`fs-read-rebuild` / `fs-grep-rebuild` / `fs-glob-rebuild` 都在
  本任务 merge（到 release branch）后才能开工。
