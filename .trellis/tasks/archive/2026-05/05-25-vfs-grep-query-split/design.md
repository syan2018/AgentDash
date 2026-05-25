# Design — VFS GrepQuery trait split

> **Parent:** [05-25-fs-tools-optimization-review](../05-25-fs-tools-optimization-review/design.md)
> **PRD:** [./prd.md](./prd.md)

## §1 范围

把 `SearchQuery` 在 vfs-search-spi-fix 中扩出来的 grep-specific 字段
（`is_regex` / `include_glob` / `context_lines` / `before_lines` /
`after_lines` / `multiline` / `output_mode`）拆到独立的 `GrepQuery`，
让 `SearchQuery` 退回通用搜索语义，承接 SPI 设计泄漏。

**为什么同期做：** 用户决议 — 4 个 P0/P1 rebuild 已稳定 + 测试覆盖到位
+ 唯一调用 grep-specific 字段的入口是 fs_grep tool（其他调用方走 search_text 通用路径），
现在拆 SPI 影响面可控。避免未来再拉一次 release 周期。

## §2 SPI 拆分

### §2.1 退化的 `SearchQuery`

```rust
#[derive(Debug, Clone, Default)]
pub struct SearchQuery {
    pub pattern: String,
    pub path: Option<String>,
    pub max_results: Option<usize>,
    pub case_sensitive: bool,
}
```

去除 7 个 grep-specific 字段。`Default::case_sensitive = true`（与历史一致）。

### §2.2 新建 `GrepQuery`

```rust
#[derive(Debug, Clone)]
pub struct GrepQuery {
    pub base: SearchQuery,
    pub include_glob: Option<String>,
    pub context_lines: usize,
    pub before_lines: usize,
    pub after_lines: usize,
    pub multiline: bool,
    pub output_mode: SearchOutputMode,
}

impl Default for GrepQuery {
    fn default() -> Self {
        Self {
            base: SearchQuery::default(),
            include_glob: None,
            context_lines: 0,
            before_lines: 0,
            after_lines: 0,
            multiline: false,
            output_mode: SearchOutputMode::default(),
        }
    }
}
```

**`is_regex` 字段去除：** A7 决议"pattern 始终视为正则"，service 层不需要切换；
未来 `vector_search` 等场景用独立的 query 类型，不复用 SearchQuery。

### §2.3 trait 改动

```rust
pub trait MountProvider {
    /// 通用搜索（substring / 简单匹配）。各 provider 按 native 能力实现。
    async fn search_text(
        &self,
        mount: &Mount,
        query: &SearchQuery,
        ctx: &MountOperationContext,
    ) -> Result<SearchResult, MountError>;

    /// grep 风格搜索：pattern 始终正则；支持 include_glob / context / multiline /
    /// output_mode 等 grep-specific 字段。
    ///
    /// 默认实现：forward 给 `search_text`，丢弃 grep-specific 字段并
    /// `tracing::warn!()` 一次提示降级。
    async fn grep_text(
        &self,
        mount: &Mount,
        query: &GrepQuery,
        ctx: &MountOperationContext,
    ) -> Result<SearchResult, MountError> {
        if query.include_glob.is_some()
            || query.context_lines > 0
            || query.before_lines > 0
            || query.after_lines > 0
            || query.multiline
        {
            tracing::warn!(
                provider = self.provider_id(),
                "grep_text default impl: dropping grep-specific fields (forwarding to search_text)"
            );
        }
        self.search_text(mount, &query.base, ctx).await
    }
}
```

**`SearchResult` 不拆**：grep_text / search_text 共用同一返回类型（matches +
truncated），避免无收益的复制类型。

## §3 4 个 Provider 迁移

### §3.1 inline_fs

- **现状**：`search_text` 已是 regex + include_glob + before/after_lines（在
  fs-grep-rebuild 升级）。
- **迁移**：把现有 `search_text` 实现整体搬到 `grep_text`。
  新 `search_text` 退化为通用 substring（不识别 grep 字段）：

```rust
async fn search_text(
    &self,
    mount: &Mount,
    query: &SearchQuery,
    _ctx: &MountOperationContext,
) -> Result<SearchResult, MountError> {
    // 通用 substring 搜索（与拆前 substring 行为一致）。
    // 字段：pattern / path / case_sensitive / max_results。
    ...
}
```

### §3.2 canvas / skill_asset

- **现状**：substring，不识别 grep 字段。
- **迁移**：search_text 保持当前 substring 实现；不实现 grep_text（走默认 forward）。
  这样调用 grep_text 时会 warn-and-degrade 到 substring。

### §3.3 lifecycle

- **现状**：search_text 当前主要是 `search_projected_skill_files`（也是 substring）。
- **迁移**：search_text 不动；不实现 grep_text。
- 副作用：fs_grep 在 lifecycle mount 上现在走 substring 退化。这与 vfs-search-spi-fix
  之前的状态一致（lifecycle 也没真接 ripgrep），不是回退。

## §4 Service 层

### §4.1 加 `grep_text_extended`

```rust
pub async fn grep_text_extended(
    &self,
    vfs: &Vfs,
    params: &TextSearchParams<'_>,
) -> Result<(Vec<String>, bool), String> {
    // 类比 search_text_extended，但 dispatch 到 provider.grep_text
    // 把 TextSearchParams 翻译为 GrepQuery
}
```

### §4.2 简化 `search_text_extended`

`search_text_extended` 当前 dispatch 到 `provider.search_text` 时填了 7 个 grep
字段；现在 SearchQuery 没有这些字段，要剥掉。**但是** TextSearchParams 现状里有
grep 字段 — 我们要保留 TextSearchParams 的兼容性？

**决策：** TextSearchParams 是 service 层 type，不是 SPI。可以保留兼容字段，但
`search_text_extended` 只用通用字段；`grep_text_extended` 用全部字段。

**实施**：search_text_extended 简化（去掉 grep 字段透传，只填 SearchQuery 通用
4 字段），仅供"通用搜索"调用方。fs_grep tool 改调 grep_text_extended。

### §4.3 search_inline 分支

`search_inline` 是 service 内部的"绕过 provider"路径，专门处理 inline overlay。
现在它使用 grep 字段（regex / include_glob / context_lines / before/after / multiline）
也是直接读 TextSearchParams，不走 SearchQuery。

**决策：** 搬到 `grep_inline`，作为 grep_text_extended 的 inline 分支。
search_text_extended 的 inline 分支退化为 substring（与 canvas/skill_asset 保持
一致的通用语义）。

## §5 fs_grep tool 改调点

```rust
// BEFORE (fs-grep-rebuild 后):
self.service.search_text_extended(&vfs, &TextSearchParams { ... }).await

// AFTER:
self.service.grep_text_extended(&vfs, &TextSearchParams { ... }).await
```

行为完全不变（service 层把 TextSearchParams 翻译为 GrepQuery）。fs-grep 11 项
测试全部应继续通过。

## §6 兼容性 / 影响面

| 调用点 | 现状字段 | 拆分后行为 |
|--------|---------|-----------|
| fs_grep tool | search_text_extended + 全部字段 | grep_text_extended（行为不变） |
| relay 协议 search 请求（`api/mount_providers/relay_fs.rs`） | provider.search_text（仅通用 4 字段） | 无变化（本来就只用通用字段） |
| 其它 service 内部调 search_text（如有） | 通用 4 字段 | 无变化 |
| vfs-search-spi-fix 集成测试 T1 (is_regex=true) | inline regex | T1 改为通过 grep_text 验证；inline search_text 退化 substring |

## §7 测试矩阵

- T1 SearchQuery 退化字段：编译时确保 SearchQuery 没有 grep 字段（构造时不传）。
- T2 GrepQuery 默认值：默认 forward 行为正确（构造一个 default GrepQuery 调
  inline grep_text，应等价于 substring 搜索 base.pattern）。
- T3 inline grep_text 完整路径：现有 fs-grep 11 项测试**完全不动**地通过。
- T4 canvas grep_text 默认实现：传带 include_glob 的 GrepQuery，warn 后 forward
  到 search_text。
- T5 search_text 退化语义：新 SearchQuery 不再支持 regex；inline.search_text
  传 `pattern: "func.*"` 应按 substring 不匹配 `funcXfoo`（仅匹配字面 `func.*`）。
- T6 grep_text_extended 行为等价 search_text_extended（拆分前）：复用 fs-grep
  11 项测试覆盖即可。

由于 fs_grep tool 测试覆盖了端到端行为（pattern 正则 / case_insensitive /
output_mode / context / VCS / 长行 / type / 分页），它们的全绿即是 T6 的最强证据。

新增测试只补 T2（grep_text 默认实现 forward）+ T5（search_text 退化为 substring）。

## §8 决策矩阵

| ID | 决策 | 状态 |
|----|------|------|
| D1 | is_regex 字段去除（A7 决议无需开关） | accept |
| D2 | GrepResult 复用 SearchResult，不造新类型 | accept |
| D3 | grep_text 默认实现 forward + warn 一次 | accept |
| D4 | inline 唯一全实现 grep_text 的 provider；canvas/lifecycle/skill_asset 走默认 | accept |
| D5 | search_text_extended 退化为通用搜索（inline overlay 分支也退化） | accept |
| D6 | fs_grep tool 切到 grep_text_extended | accept |
| D7 | TextSearchParams 保留所有字段，但 search/grep 两条路径各用各的 | accept |
| D8 | vfs-search-spi-fix 的 T1 (is_regex) 测试需要 adapt 到 grep 路径 | accept |
