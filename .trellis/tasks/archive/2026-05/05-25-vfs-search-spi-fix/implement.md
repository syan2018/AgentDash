# Implement — VFS Search SPI 修复

> 执行清单 + validation gate。设计依据见 [design.md](./design.md)。
> 在分支 `release/fs-tools-rebuild` 上完成；**不直接 merge 到 main**。

## S1 — SPI 类型扩展（[mount.rs](../../../crates/agentdash-spi/src/platform/mount.rs)）

- [ ] **S1.1** `SearchQuery` 加 7 个字段（design.md §2.1）。补 `Default` impl、`derive(Debug, Clone)`。
- [ ] **S1.2** 新增 `SearchOutputMode` 枚举 + `Default = Content`。
- [ ] **S1.3** `SearchResult` 加 `truncated: bool`，加 `derive(Debug, Clone, Default)`。
- [ ] **S1.4** `SearchMatch` 加 `derive(Debug, Clone)`（旧 type 没 derive）。
- [ ] **S1.5** `ReadResult` 加 `version_token: Option<String>` + `modified_at: Option<i64>`。
- [ ] **S1.6** `ReadResult::new` / `with_version_token(token: impl Into<String>)` /
      `with_modified_at(mtime: i64)` builder 方法。
- [ ] **Validation gate:** `cargo check -p agentdash-spi` 通过。

## S2 — `MountProvider` trait 默认方法

- [ ] **S2.1** `read_text_range`：design.md §5 的默认实现。
- [ ] **S2.2** `suggest_paths`：design.md §6 的默认实现 + `MAX_SCAN_FILES = 1000` 常量。
- [ ] **S2.3** `agentdash-spi/Cargo.toml` 加 `strsim = "0.11"` 依赖。
- [ ] **S2.4** SPI 文件头 doccomment 补 "Search 字段语义（A7：pattern 始终正则）"
      + version_token 协议（design.md §4.1）+ read_text_range 默认行为
      + suggest_paths 性能成本提示。
- [ ] **Validation gate:** `cargo build -p agentdash-spi` 通过。

## S3 — 4 个 Provider 接入（顺序 inline）

按 design.md §8 的策略。每个 provider 改动控制在 30 行内。

### S3.1 `inline_fs` provider

- [ ] 文件：`crates/agentdash-application/src/vfs/providers/inline_fs/`
- [ ] `search_text`：
  - 用 `RegexBuilder::new(&query.pattern).multi_line(query.multiline).dot_matches_new_line(query.multiline)`
  - 接 `before_lines = max(query.before_lines, query.context_lines)` / `after_lines = max(query.after_lines, query.context_lines)`
  - 加 `include_glob` 过滤（用 `globset::Glob` 编译）。
- [ ] `read_text` / `stat`：填 `version_token = format!("rev:{revision}")`、`modified_at = updated_at`。
- [ ] `truncated` 设置：当命中数 ≥ `max_results.unwrap_or(usize::MAX)` 时设为 true。

### S3.2 `canvas` provider

- [ ] 文件：`crates/agentdash-application/src/vfs/providers/canvas/`
- [ ] `search_text`：保留 substring 行为，新字段 warn 一次（用 `tracing::warn!` + `once_cell` 限频）。
- [ ] `read_text` / `stat`：填 `version_token = canvas_version_id.to_string()`、`modified_at = updated_at`。

### S3.3 `lifecycle` provider

- [ ] 文件：`crates/agentdash-application/src/vfs/providers/lifecycle/`
- [ ] `search_text`：调 ripgrep 时把新字段映射 CLI 参数（design.md §8.3）。
- [ ] `read_text` / `stat`：填 `version_token = format!("{mtime}:{size}")`、`modified_at = mtime`。

### S3.4 `relay_fs` provider

- [ ] 文件：`crates/agentdash-application/src/vfs/providers/relay_fs/`（如存在；否则是 lifecycle 的别名）
- [ ] 同 lifecycle 处理（共享代码可抽 helper 函数）。
- [ ] 如果 relay_fs 不是独立 crate，跳过本步。

### S3.5 `skill_asset` provider

- [ ] 文件：`crates/agentdash-application/src/vfs/providers/skill_asset/`
- [ ] `search_text`：保留 substring 行为，warn 一次。
- [ ] `read_text` / `stat`：填 `version_token = updated_at.to_string()`、`modified_at = updated_at`。

- [ ] **Validation gate:** `cargo build -p agentdash-application` 通过。

## S4 — `relay_service::search_text_extended` 字段透传

- [ ] **S4.1** `TextSearchParams`（service 层 type）扩展同样的 7 个字段。
- [ ] **S4.2** [relay_service.rs:724-757](../../../crates/agentdash-application/src/vfs/relay_service.rs#L724-L757)
      非 inline 分支：构造 `SearchQuery` 时填全部新字段。
- [ ] **S4.3** 返回值改为 `(matches, truncated)` 真实反映；当前是写死 `false`。
- [ ] **S4.4** [relay_service.rs:763-851](../../../crates/agentdash-application/src/vfs/relay_service.rs#L763-L851)
      `search_inline` 分支加 `include_glob` 过滤（用 `globset::GlobMatcher`）。
- [ ] **S4.5** 调用方（fs_grep tool 层）暂不调整 —— 仍用旧的 4 字段构造，
      新字段走 default。fs-grep-rebuild 任务负责真用上。
- [ ] **Validation gate:** `cargo build -p agentdash-application` + 既有 fs_grep / fs_glob / fs_read 集成测试不退化。

## S5 — 集成测试 5 项（design.md §9）

- [ ] **T1** `tests/vfs/search_spi.rs`：`SearchQuery::is_regex=true`
      在 inline / canvas / lifecycle 各跑一道（canvas 的预期是 warn + substring 退化）。
- [ ] **T2** `SearchQuery::include_glob` 在 inline 过滤；在 canvas 上 list 阶段过滤。
- [ ] **T3** `SearchResult::truncated` 在三个 provider 上：构造 ≥ max_results+1 命中，断言 truncated=true。
- [ ] **T4** `ReadResult::version_token` 在 lifecycle / canvas 上非 None；连续 read 同一文件，token 一致；
      文件改后 token 变化。
- [ ] **T5** `read_text_range` 默认实现：在 canvas / inline 上 offset=2, limit=3 ⇒ 第 3-5 行。

- [ ] **Validation gate:** `cargo test -p agentdash-application --test vfs` 全绿。

## S6 — 全量回归

- [ ] **S6.1** `cargo build` 整 workspace。
- [ ] **S6.2** `cargo test -p agentdash-spi -p agentdash-application`。
- [ ] **S6.3** `cargo clippy -p agentdash-spi -p agentdash-application -- -D warnings`（仅本任务文件）。
- [ ] **S6.4** 手抽烟雾：在 release branch 上跑现有 `fs_grep` / `fs_read` / `fs_glob`
      工具的端到端测试，确认无行为退化。

## S7 — 用户 review + commit

- [ ] **S7.1** 把 design.md / implement.md / 上述代码改动 review 一遍。
- [ ] **S7.2** 单 commit，message 模板：
      `feat(vfs-spi): 补齐 Search/Read SPI 字段，承接 fs 工具重做`。
- [ ] **S7.3** `task.py finish` → archive。

## 风险与回滚

| 风险 | 应对 |
|------|------|
| S3 某个 provider 接入失败（如 canvas 没有现成 substring search） | 该 provider 留 warn + return empty result，工具层在 rebuild 任务里再补；不阻塞本任务 |
| S5 测试矩阵 provider mock 复杂 | T1-T5 各用最小 fixture（inline 用 in-memory store，lifecycle 用 tempdir，canvas 用 mock provider） |
| `strsim` 引入冲突 workspace 已有 | 先 `cargo tree -i strsim` 查；如已有版本则复用；否则加 `0.11` |
| relay_fs ripgrep 不在测试环境 | T1 lifecycle 用 ripgrep 真二进制；如 CI 缺就跳测（`#[cfg_attr(not(feature="ripgrep-test"), ignore)]`） |

## 完成判定

- [ ] S1-S6 全部 ✓
- [ ] S5 测试矩阵 5 项全绿
- [ ] cargo build / clippy 全 workspace 通过
- [ ] design.md / implement.md 与最终代码一致（本任务执行过程中如有调整，回 design.md 同步）
- [ ] 用户 review 通过 → archive

## 执行策略

**全部 inline 处理**，不调度 subagent：
- 每个 provider 接入 < 30 行；
- subagent dispatch overhead（spec/research/check 三轮上下文）> 收益。
- 如果 S3.3 / S3.4 lifecycle/relay_fs ripgrep 参数映射比预期复杂，单独切 trellis-research
  跑一次 ripgrep CLI 参数映射调研（短路径 medium 范围），再回来 inline 实施。
