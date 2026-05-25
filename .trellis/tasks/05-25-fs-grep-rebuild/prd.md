# fs_grep 重做

> **Parent task:** [05-25-fs-tools-optimization-review](../05-25-fs-tools-optimization-review/design.md)
> 决策来源：parent 的 design.md §0 对齐 diff 表 + §1 P0#3 + §2 P1#4-#6 + §3.2 P2#10。

## Goal

把 `fs_grep` 工具从草率实现升级为与 Claude Code `GrepTool` 行为对齐的版本：
schema 改名 + output_mode + 全部 grep 行为开关 + VCS 黑名单 + 长行裁剪 +
type 快捷键。

## Background

详细评估见 parent design.md。Sketch：

- **现状缺陷**：永远返回 `path:line: content` 一种格式（无 output_mode）；
  无 case-insensitive / multiline / 独立 -A/-B；无 VCS 排除；无长行裁剪；
  字段命名（query / regex / include / max_results）与 CC 全部不同。
- **CC 对齐基线**（项目原则）：参数 pattern / glob / head_limit / output_mode；
  pattern 始终是正则（无 regex 字段）；六个 VCS 目录硬编码黑名单；--max-columns 500；
  type 快捷键。

## Requirements

### R1 — Schema 对齐 CC（breaking）

```rust
pub struct FsGrepParams {
    pub pattern: String,                          // 始终是正则；与 CC 一致
    pub path: Option<String>,                     // 保留 mount://path
    pub glob: Option<String>,                     // BEFORE: include
    pub r#type: Option<String>,                   // NEW（5–10 种语言映射）
    pub output_mode: Option<OutputMode>,          // NEW；默认 FilesWithMatches
    pub before_context: Option<usize>,            // -B
    pub after_context: Option<usize>,             // -A
    pub context: Option<usize>,                   // -C 或 context（统一字段）
    pub case_insensitive: Option<bool>,           // -i
    pub line_numbers: Option<bool>,               // -n；默认 true
    pub multiline: Option<bool>,                  // -U
    pub head_limit: Option<usize>,                // BEFORE: max_results；默认 250；0 = 无限
    pub offset: Option<usize>,                    // NEW；与 head_limit 配合分页
}
```

**去除的字段**：`query`（→ pattern）、`regex`（pattern 始终是正则）、
`include`（→ glob）、`max_results`（→ head_limit）、`context_lines`（→ context/-A/-B）。

prompt 描述对齐 CC GrepTool 的措辞，明确 "pattern is a regular expression"。

### R2 — `output_mode` 三档实现

- `Content`（旧默认）：返回 `path:line:content` 命中行。
- `FilesWithMatches`（**新默认**）：仅返回去重的文件名列表。
- `Count`：每个文件返回 `path:N` 计数。

实施位置：parent design.md §1 P0#3 决议 = **Tool 层方案 A**——service
仍返回完整命中，FsGrepTool 在序列化前去重/计数。

### R3 — VCS 默认黑名单

硬编码 6 个目录到 fs_grep tool 层（在 glob 前缀拼上 `!**/.git`、`!**/.svn`、
`!**/.hg`、`!**/.bzr`、`!**/.jj`、`!**/.sl`）。

**A3 待定**（parent open question）：是否做成可配置（mount metadata 的
`vfs_search_excludes`）。本任务 brainstorm 阶段拍板，**默认硬编码**。

### R4 — 长行裁剪

service 层（含 inline 路径 + 各 provider 的 search_text）加 `MAX_LINE_LEN = 500`，
超长 line trim 到 500 + 后缀 `...(truncated)`。

### R5 — 行为开关接入

`-i`（case_insensitive）/ `multiline`（regex `(?s)` 修饰）/ 独立 `-A`/`-B`/`-C` /
`-n`（line_numbers）—— 都映射到 `vfs-search-spi-fix` 已扩展好的 SearchQuery 字段。

### R6 — `type` 快捷键（最小集）

支持 5–10 种核心语言：

```rust
const LANG_EXTENSIONS: &[(&str, &[&str])] = &[
    ("js",   &["js","jsx","mjs","cjs"]),
    ("ts",   &["ts","tsx","mts","cts"]),
    ("py",   &["py","pyi"]),
    ("rust", &["rs"]),
    ("go",   &["go"]),
    ("java", &["java"]),
    ("c",    &["c","h"]),
    ("cpp",  &["cc","cpp","cxx","hpp","hxx"]),
    ("cs",   &["cs"]),
    ("rb",   &["rb"]),
];
```

`type` 字段翻译为 glob 模式，与 `glob` 字段叠加（取并集）。

### R7 — `head_limit` + `offset` 默认值

- 默认 `head_limit = 250`；`0` = 无限。
- 默认 `offset = 0`。
- truncated 标志通过 SearchResult.truncated 透传。

## Acceptance Criteria

- [ ] `cargo build` / `cargo test` 通过。
- [ ] schema 改名通过：旧调用方传 `query/regex/include/max_results` 报清晰错误。
- [ ] pattern 始终视为正则：传 literal 字符串如 `function foo` 走正则路径
      （应该正常匹配，因为没有正则元字符）；传 `func.*foo` 也走正则路径
      （应该匹配 `funcXfoo`）。
- [ ] output_mode 三档分别在 inline / canvas / lifecycle 上正确工作。
- [ ] VCS 黑名单：在含 `.git/` 的真实工程目录上 grep 不会返回 `.git/` 内
      的命中。
- [ ] 长行裁剪：含 5KB 单行的 minified.js 命中 → 返回的 line ≤ 500 字符 +
      `...(truncated)` 后缀。
- [ ] case_insensitive / multiline / -A/-B/-C 各加单元测试。
- [ ] type 快捷键：传 `type: "rust"` 仅命中 `.rs` 文件。
- [ ] head_limit + offset 分页：连续两次 `head_limit=10, offset=0/10` 不重复。
- [ ] agent 端 prompt + tool schema 已同步更新到 release branch。

## Constraints / 不在范围

- **不**改 SPI（依赖 `vfs-search-spi-fix` 已 merge）。
- **不**做 SPI trait split（→ follow-up `vfs-grep-query-split`）。
- **不**引入 ripgrep 后端（白名单：未来重大架构调整）。

## Open Questions

- A3（继承自 parent）：VCS 黑名单可配置 vs 硬编码？brainstorm 阶段定，
  **默认硬编码**。

## Notes

- 在 `release/fs-tools-rebuild` 分支上完成；**不直接 merge 到 main**。
- 工时估算：1–3 day。
- 依赖：`vfs-search-spi-fix` 已在 release branch 上 merge。
