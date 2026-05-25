# fs_glob 重做

> **Parent task:** [05-25-fs-tools-optimization-review](../05-25-fs-tools-optimization-review/design.md)
> 决策来源：parent 的 design.md §0 对齐 diff 表 + §2 P1#7 + §3 P2#9。
>
> **CC 参考实现（对齐基线）：**
> - [GlobTool.ts](../../../references/claude-code/src/tools/GlobTool/GlobTool.ts) — 主体（pattern 必填、mtime desc 排序、默认 100 上限、trailing slash 输出）
> - [GlobTool/prompt.ts](../../../references/claude-code/src/tools/GlobTool/prompt.ts) — prompt 措辞参考

## Goal

把 `fs_glob` 工具从草率实现升级为与 Claude Code `GlobTool` 行为对齐的版本：
schema 对齐（去 `recursive`、pattern 必填、去 substring 退化）+ 默认上限 +
mtime 排序 + 输出去 `[dir]/[file]` 前缀（用 trailing slash 表达目录）。

## Background

详细评估见 parent design.md。Sketch：

- **现状缺陷**：[glob.rs:83-110](../../../crates/agentdash-application/src/vfs/tools/fs/glob.rs#L83-L110)
  无 max_results 默认上限；无 mtime 排序；输出加 `[dir]/[file]` 前缀；
  `recursive` 字段独立于 pattern；`pattern` 缺失时退化为列目录、无 glob 字符
  时退化为 substring filter——这两条都偏离 CC 标准 glob 语义。
- **CC 对齐基线**：pattern 必填且始终为 glob；递归通过 `**` 表达；默认 100 上限；
  mtime desc 排序；输出仅路径列表。

## Requirements

### R1 — Schema 对齐 CC（breaking）

```rust
// BEFORE
pub struct FsGlobParams {
    pub path: Option<String>,
    pub recursive: Option<bool>,
    pub pattern: Option<String>,
}

// AFTER
pub struct FsGlobParams {
    pub pattern: String,                   // 必填；始终是 glob
    pub path: Option<String>,              // 保留 mount://path
    pub max_results: Option<usize>,        // NEW；默认 100
}
```

**去除的字段**：

- `recursive`：递归通过 `pattern = "**/foo"` 表达，不再有独立开关。旧调用方
  传 `recursive: true` → **报错**（parent A8 决议 = 报错而非静默忽略）。
- pattern 的"无 glob 字符当 substring filter"语义：去除，统一 glob 语义。
  调用方需 substring filter 时显式写 `*foo*`。
- pattern 的"缺失则列目录"语义：去除，列目录用 `pattern: "*"` 显式表达。

prompt 描述对齐 CC GlobTool 的措辞。

### R2 — 默认上限 + truncated

- 默认 `max_results = 100`。
- 服务返回 entries > 100 → 截断 + 输出末尾追加：
  `(N more entries; refine pattern or raise max_results)`。
- 输出加 `truncated: bool`（在 ToolResult.details 里暴露）。

### R3 — mtime desc 排序

`entries.sort_by_key(|e| std::cmp::Reverse(e.modified_at.unwrap_or(0)))`。

`modified_at` 缺失的统一沉底（用 0 作为 key），二级排序按 path 字典序保证
确定性。

### R4 — 输出格式对齐 CC

```
src/lib.rs
src/utils/         <-- 目录用 trailing slash 表达
src/utils/foo.rs
README.md
```

替代当前的 `[file] / [dir]` 前缀格式，节省每行 6 字节 + 与 CC 输出一致。

## Acceptance Criteria

- [ ] `cargo build` / `cargo test` 通过。
- [ ] schema 改名通过：旧调用方传 `recursive` → 报错（带友好建议
      "use pattern '**/foo' to express recursion"）。
- [ ] schema 改名通过：旧调用方省略 `pattern` → 报错（带友好建议
      "pattern is required; use '*' for current dir contents"）。
- [ ] pattern 必为 glob：传 `foo`（无通配符）只匹配文件名为 `foo` 的文件，
      不匹配 `foobar`（与现状的 substring filter 行为不同，breaking）。
- [ ] mtime 排序：在测试 mount 上构造 a.rs（旧）/ b.rs（中）/ c.rs（新），
      列出顺序为 c.rs / b.rs / a.rs。
- [ ] mtime 缺失：在 canvas mount 上（modified_at 为 None）能正常返回，
      按 path 字典序兜底。
- [ ] 默认上限：构造 200 个文件的 mount，不传 max_results → 返回 100 + 截断
      提示。
- [ ] 输出格式：目录用 `path/` 形式，文件无后缀；不再有 `[dir]/[file]` 前缀。
- [ ] agent 端 prompt + tool schema 已同步更新到 release branch。

## Constraints / 不在范围

- **不**改 SPI（依赖 `vfs-search-spi-fix` 已 merge，主要用 `RuntimeFileEntry.modified_at`）。
- **不**改 list 接口（默认在 service 层完成排序、tool 层完成截断）。

## Open Questions

- A8（继承自 parent）：去除 `recursive` 字段的兼容性策略 = **报错**
  （parent brainstorm S8 决议）。本任务 brainstorm 阶段确认错误消息措辞。

## Notes

- 在 `release/fs-tools-rebuild` 分支上完成；**不直接 merge 到 main**。
- 工时估算：< 1 day。
- 依赖：`vfs-search-spi-fix` 已在 release branch 上 merge（用其
  `modified_at` 字段）。
