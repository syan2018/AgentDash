# Implement — VFS GrepQuery trait split

> 设计依据：[design.md](./design.md)。Branch: `release/fs-tools-rebuild`（与 4 个
> rebuild 同期发布；本任务用户决议同期处理）。

## S1 — SPI 拆分（[mount.rs](../../../crates/agentdash-spi/src/platform/mount.rs)）

- [ ] **S1.1** SearchQuery 退化：去掉 7 个 grep 字段（`is_regex` /
      `include_glob` / `context_lines` / `before_lines` / `after_lines` /
      `multiline` / `output_mode`）。Default impl 同步精简。
- [ ] **S1.2** 新建 `GrepQuery { base: SearchQuery, include_glob, context_lines,
      before_lines, after_lines, multiline, output_mode }` + Default。
- [ ] **S1.3** trait 加 `async fn grep_text(&self, mount, &GrepQuery, ctx) ->
      Result<SearchResult, MountError>` 默认实现 = forward 给 search_text +
      warn 一次（design.md §2.3）。
- [ ] **Validation:** `cargo check -p agentdash-spi`。

## S2 — Service 层加 grep_text_extended

- [ ] **S2.1** [relay_service.rs](../../../crates/agentdash-application/src/vfs/relay_service.rs)
      加 `pub async fn grep_text_extended(...)`。
- [ ] **S2.2** 实现：把 TextSearchParams 翻译为 GrepQuery（含
      include_glob/context_lines/before_lines/after_lines/multiline/output_mode）；
      非 inline 分支调 `provider.grep_text`；inline 分支调 grep_inline。
- [ ] **S2.3** 把 search_inline 改名为 grep_inline，归 grep_text_extended 用。
- [ ] **S2.4** 简化 search_text_extended：构造 SearchQuery 只填通用 4 字段；
      新建 search_inline 通用版（substring + base_path 过滤；不走 regex）。
- [ ] **S2.5** VCS 黑名单 + 长行裁剪：放在 grep_text_extended（grep 路径）；
      search_text_extended 退化路径不需要这些保护（通用搜索调用方自行处理）。

## S3 — 4 Provider 适配

### S3.1 inline_fs

- [ ] [provider_inline.rs](../../../crates/agentdash-application/src/vfs/provider_inline.rs)
      把现有 `search_text` 实现整体搬到 `grep_text`，签名改为 `&GrepQuery`，
      内部访问 `query.base.pattern` 等。
- [ ] 新写一个简化的 `search_text`：substring 匹配（仅用 `query.pattern` /
      `path` / `case_sensitive` / `max_results`）。
- [ ] 现有 7 项 inline 集成测试（spi-fix 引入）：
      - search_truncated_when_max_results_reached：保留（用 search_text 的
        substring 行为也满足 max_results 截断）。
      - read_text_returns_version_token_and_modified_at：保留。
      - read_text_range_default_impl_*：保留。
      - read_text_rejects_binary_and_search_skips_it：保留（substring 仍跳过 binary）。

### S3.2 canvas / skill_asset / lifecycle

- 现有 `search_text` 都是 substring，无需改 `search_text` 实现。
- 不实现 `grep_text`，走默认 forward + warn。
- 接受副作用：fs_grep 在这三个 provider 上 grep 行为退化（warn 一次后 substring）。

## S4 — fs_grep tool 切到 grep_text_extended

- [ ] **S4.1** [grep.rs](../../../crates/agentdash-application/src/vfs/tools/fs/grep.rs)
      `service.search_text_extended` → `service.grep_text_extended`。
      行为完全不变（11 项 fs_grep 测试应自动通过）。

## S5 — vfs-search-spi-fix 集成测试 adapt

- [ ] **S5.1** [provider_inline.rs](../../../crates/agentdash-application/src/vfs/provider_inline.rs)
      4 项 inline 集成测试：把 `search_truncated_when_max_results_reached` 保留
      （substring 也支持 max_results）；其它 search_text 调用点确保字段集兼容。

## S6 — 新增小测试

- [ ] **T2** GrepQuery 默认行为 = inline.grep_text 调用与 inline.search_text 等价
      （都是 substring base.pattern）。
- [ ] **T5** SearchQuery 退化语义：inline.search_text(`pattern: "func.*"`) 不应
      匹配 `funcXfoo`（substring 不识别 regex）。

## S7 — relay_fs / 其他 search_text 调用方

- [ ] [api/mount_providers/relay_fs.rs](../../../crates/agentdash-api/src/mount_providers/relay_fs.rs)
      它的 `search_text` 实现不接 GrepQuery 字段，无需变化。
- [ ] grep tool 之外是否有 service.search_text_extended 调用方？
      grep workspace 检查；如有，行为退化为 substring（这是预期）。

## S8 — 全量回归 + commit + archive

- [ ] cargo build --workspace --lib + cargo test --workspace --lib 全绿。
- [ ] commit: `refactor(vfs-spi): SearchQuery → SearchQuery + GrepQuery 拆分`。
- [ ] task.py finish + archive。

## 风险

| 风险 | 应对 |
|------|------|
| fs_grep 11 项行为变化 | 测试是端到端覆盖；grep_text_extended 实现要完整继承 search_text_extended grep 行为 |
| canvas/lifecycle/skill_asset 上 fs_grep 退化为 substring | 这是预期；warn 一次 + 不破坏 |
| TextSearchParams 字段集留着会让人困惑 | service 层 doc 注明 search_*_extended 用通用字段，grep_*_extended 用全部 |

## 完成判定

- [ ] S1-S8 全 ✓
- [ ] cargo test --workspace --lib 全绿
- [ ] fs_grep 11 项测试不变行为通过
