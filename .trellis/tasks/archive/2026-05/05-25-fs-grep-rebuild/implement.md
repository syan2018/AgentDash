# Implement — fs_grep 重做

> 设计依据：[design.md](./design.md)。Branch: `release/fs-tools-rebuild`。

## S1 — Schema + OutputMode 枚举

- [ ] **S1.1** [grep.rs](../../../crates/agentdash-application/src/vfs/tools/fs/grep.rs)
      重写 `FsGrepParams`（design.md §2）+ `deny_unknown_fields`。
- [ ] **S1.2** 加 `OutputMode` 枚举（snake_case Deserialize；Default = FilesWithMatches）。
- [ ] **S1.3** 加 `default_true` helper for line_numbers 默认值。
- [ ] **Validation:** `cargo check`。

## S2 — type 快捷键 + glob 合并

- [ ] **S2.1** 加 `LANG_EXTENSIONS` 静态表（design.md §3）。
- [ ] **S2.2** 加 helper `build_combined_glob(glob, type) -> Option<String>`：
      - 仅 glob ⇒ 原样返回。
      - 仅 type ⇒ 翻译为 `**/*.{ext1,ext2,...}`。
      - 都有 ⇒ `{user_glob,**/*.{ext1,...}}`。
- [ ] **S2.3** unknown type ⇒ InvalidArguments，列出 supported types。

## S3 — 描述 prompt 改写

- [ ] **S3.1** 描述对齐 CC GrepTool 的措辞（参见
      [GrepTool/prompt.ts](../../../references/claude-code/src/tools/GrepTool/prompt.ts)）：
      "pattern is a regular expression"、"defaults to FilesWithMatches"、
      "supports glob/type to filter files"、"head_limit defaults to 250"。

## S4 — execute 主体重写

- [ ] **S4.1** 删除现有 execute 实现，按 design.md §8 流程重写。
- [ ] **S4.2** 调 `service.search_text_extended` 时填齐
      `case_sensitive / before_lines / after_lines / multiline / output_mode = Content`。
      max_results = head_limit.unwrap_or(250).max(offset + 1) + offset（buffer）。
      head_limit == 0 ⇒ 50000。
- [ ] **S4.3** 接收 hits 后：VCS 过滤 + 长行裁剪 + skip(offset).take(head_limit)。
- [ ] **S4.4** 按 OutputMode 转换：Content / FilesWithMatches / Count。
- [ ] **S4.5** truncated 后缀文案不变（仍 `(results truncated; ...)`）。

## S5 — service 层加 VCS 过滤 + 长行裁剪

- [ ] **S5.1** [relay_service.rs](../../../crates/agentdash-application/src/vfs/relay_service.rs)
      加 `const VCS_EXCLUDE_DIRS: &[&str]` + `const MAX_LINE_LEN: usize = 500`。
- [ ] **S5.2** 加 helper `is_vcs_path(path: &str) -> bool`（路径段包含黑名单）。
- [ ] **S5.3** 加 helper `trim_long_line(line: &str) -> String`。
- [ ] **S5.4** `search_text_extended` 非 inline 分支：在 hits 收集时过滤
      VCS path + 裁剪长行。
- [ ] **S5.5** `search_inline` 分支：在 file iteration 处过滤 VCS path +
      hit 格式化时裁剪长行。

## S6 — inline provider 升级为 regex

- [ ] **S6.1** [provider_inline.rs](../../../crates/agentdash-application/src/vfs/provider_inline.rs)
      `search_text` 重写：
      - is_regex = true（始终）⇒ 用 RegexBuilder（case_insensitive 反向 case_sensitive；
        multi_line + dot_matches_new_line 跟 multiline）。
      - 加 include_glob 过滤（globset 已是 dep）。
      - before_lines / after_lines 在命中后追加上下文行（用一个 BTreeSet 防重）。
- [ ] **S6.2** 保留 truncated 字段填充逻辑。

## S7 — 测试矩阵 13 项

按 design.md §9 落 13 项测试。fixture 用 inline mock，提供多文件 + 含
`.git/` 路径 + 含长行的 minified.js。

## S8 — 全量回归 + commit + archive

- [ ] **S8.1** `cargo test --workspace --lib`。
- [ ] **S8.2** `cargo clippy`（仅本任务文件 clean）。
- [ ] **S8.3** Commit: `feat(fs-grep): 重做 fs_grep 工具与 CC GrepTool 对齐`。
- [ ] **S8.4** `task.py finish && task.py archive`。

## 风险

| 风险 | 应对 |
|------|------|
| inline 升级 regex 后旧测试（如 fs-grep tool 既有 substring 测试）行为变 | 旧 tool-side 测试本就要重写（schema 已 breaking）；任何 inline mock 调用走新参数 |
| service 层 VCS 过滤可能漏掉某些 path 形式（如 `./.git/HEAD`） | helper 用 path segment iteration（Path::components），覆盖前缀 / 中间 |
| globset alternative 语法 `{a,b}` 兼容性 | globset 0.4 支持；测试覆盖 type+glob 联合 |
| head_limit 0 = 无限的 50000 上限可能太小 | 50000 行 grep 已远超 LLM 处理能力，合理保护 |

## 完成判定

- [ ] S1-S8 全 ✓，13 项测试全绿；cargo build/test/clippy workspace OK。
