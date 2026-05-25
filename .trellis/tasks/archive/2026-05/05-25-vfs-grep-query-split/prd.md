# VFS GrepQuery trait split（follow-up）

> **Parent task:** [05-25-fs-tools-optimization-review](../05-25-fs-tools-optimization-review/design.md)
> 这是 follow-up，**不与 4 个 rebuild 同期**。等 release branch merge 后再开工。
> 决策来源：parent design.md §0 命名分层段 + 决策矩阵 FU#1。

## Goal

把 `vfs-search-spi-fix` 中往 `SearchQuery` 加的 grep 特化字段（`is_regex` /
`include_glob` / `context_lines` / `multiline` / `output_mode` 等）拆出到独立
的 `GrepQuery`，让 `SearchQuery` 退回到通用搜索语义，承接当前 SPI 设计泄漏。

## Background

详细说明见 parent design.md §0 "命名分层"段。

**当前问题**：`SearchQuery` 扩展后承担了 grep 特有的字段，未来如果加
vector / semantic search，复用 `SearchQuery` 会让接口语义混乱。

**用户决议（parent S8 brainstorm）**：方向上认可拆 `GrepQuery extends SearchQuery`，
但不在 `vfs-search-spi-fix` 内做（避免 SPI 改动倍增 + 任务风险叠加），
作为 follow-up 单独处理。

## Requirements

### R1 — `SearchQuery` 退回通用语义

```rust
pub struct SearchQuery {
    pub pattern: String,
    pub path: Option<String>,
    pub max_results: Option<usize>,
    pub case_sensitive: bool,
}
```

### R2 — 抽出 `GrepQuery`

```rust
pub struct GrepQuery {
    pub base: SearchQuery,
    pub is_regex: bool,
    pub include_glob: Option<String>,
    pub context_lines: usize,
    pub before_lines: usize,
    pub after_lines: usize,
    pub multiline: bool,
    pub output_mode: SearchOutputMode,
}
```

### R3 — `MountProvider::grep_text` 默认实现

```rust
trait MountProvider {
    async fn search_text(...) -> SearchResult;  // 通用

    async fn grep_text(
        &self,
        mount: &Mount,
        query: &GrepQuery,
        ctx: &MountOperationContext,
    ) -> Result<GrepResult, MountError> {
        // default: forward to search_text；丢弃 grep-specific 字段
        // 带 tracing::warn!() 提示丢字段
    }
}
```

### R4 — 4 个 provider 重写 `grep_text`（保留 search_text 默认实现）

inline / canvas / lifecycle / skill_asset 的现有 `search_text` 实现迁移为
`grep_text`；`search_text` 退化为只用通用字段的简单实现（或保持丢字段的
默认）。

### R5 — `fs_grep` 工具改调 `grep_text`

`FsGrepTool::execute` 从调用 `service.search_text_extended` 改为
`service.grep_text_extended`（新 service 方法）。

### R6 — `relay_service` 加 `grep_text_extended`

类比 `search_text_extended`，但 dispatch 到 provider 的 `grep_text`。

## Acceptance Criteria

- [ ] `cargo build` / `cargo test` 通过。
- [ ] `SearchQuery` 字段集只剩通用 4 个，无 grep-specific 字段。
- [ ] 4 个 provider 都实现了 `grep_text`，行为与 split 前一致。
- [ ] `fs_grep` 行为与 split 前完全一致（用现有集成测试验证）。
- [ ] SPI 文档说明了 `search_text` vs `grep_text` 的语义边界。
- [ ] 未来如果加 vector / semantic search，可以直接定义新方法
      （如 `vector_search_text(VectorQuery)`）而不用扩 `GrepQuery`。

## Constraints / 不在范围

- **不**改 fs_grep 的 prompt / schema（外部行为完全不变）。
- **不**优化 grep 性能（仅做接口重排）。

## Notes

- 工时估算：1–3 day。
- **依赖**：4 个 child rebuild（`vfs-search-spi-fix` /
  `fs-read-rebuild` / `fs-grep-rebuild` / `fs-glob-rebuild`）全部 merge 到
  main 之后才能开工。
- 优先级：P2（不阻塞用户体验，纯架构清理）。
- 如果未来真要加 vector / semantic search，可以与那个任务合并；
  但单独做也合理。
