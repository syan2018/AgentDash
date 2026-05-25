# fs_read 重做

> **Parent task:** [05-25-fs-tools-optimization-review](../05-25-fs-tools-optimization-review/design.md)
> 决策来源：parent 的 design.md §0 对齐 diff 表 + §1 P0#1/P0#2 + §3.1 P2#8。
>
> **CC 参考实现（对齐基线）：**
> - [FileReadTool.ts](../../../references/claude-code/src/tools/FileReadTool/FileReadTool.ts) — 主体（offset/limit schema、ENOENT 分支、dedup 集成）
> - [FileReadTool/prompt.ts](../../../references/claude-code/src/tools/FileReadTool/prompt.ts) — prompt 措辞参考
> - [FileReadTool/limits.ts](../../../references/claude-code/src/tools/FileReadTool/limits.ts) — MAX_BYTES / MAX_LINES 常量来源
> - [utils/fileRead.ts](../../../references/claude-code/src/utils/fileRead.ts) — `readFileInRange` 真按 range 读的实现样式
> - [utils/fileStateCache.ts](../../../references/claude-code/src/utils/fileStateCache.ts) — readFileState dedup 的 LRU 设计
> - [utils/file.ts](../../../references/claude-code/src/utils/file.ts) — `findSimilarFile` / `suggestPathUnderCwd` ENOENT 友好提示

## Goal

把 `fs_read` 工具从"草率实现"升级为与 Claude Code `FileReadTool` 行为对齐的
版本：schema 改名（offset/limit）+ 真按 range 读 + 字节/行数上限 + dedup +
ENOENT 友好提示。

## Background

详细评估见 parent 任务 design.md。Sketch：

- **现状缺陷**：[read.rs:108-119](../../../crates/agentdash-application/src/vfs/tools/fs/read.rs#L108-L119)
  全文加载后 `.lines().enumerate().filter()` slice，任何 `start_line/end_line`
  都全量搬运，无上限保护。
- **CC 对齐基线**（项目原则）：参数命名 `offset/limit`、按 range 真读、
  超限拒绝、同 path/range 未变 → 短桩、ENOENT 友好提示。

## Requirements

### R1 — Schema 对齐 CC（breaking）

```rust
// BEFORE
pub struct FsReadParams {
    pub path: String,
    pub start_line: Option<usize>,
    pub end_line: Option<usize>,
}

// AFTER
pub struct FsReadParams {
    pub path: String,                   // 保留 mount://path 协议（白名单）
    pub offset: Option<usize>,          // 1-based 起始行
    pub limit: Option<usize>,           // 行数；省略则读到末尾或上限触发
}
```

prompt 描述对齐 CC FileReadTool 的措辞。

### R2 — 真按 range 读

调用 SPI `read_text_range(mount, path, offset, limit, ctx)` 替代
`read_text + slice`。

**额外**：本任务里要把 lifecycle / relay_fs 两个 provider 的
`read_text_range` 重写为真按 range 读：

- lifecycle：`BufRead::lines().skip(offset-1).take(limit)`，避免全文加载。
- relay_fs：同上；如有更高效的字节定位 + line counter 也可。
- canvas / inline / skill_asset：保留 SPI 默认实现（in-DB 数据已在内存）。

### R3 — 字节 / 行数双阈值上限

```rust
const MAX_BYTES: usize = 256 * 1024;  // 256KB
const MAX_LINES: usize = 5000;
```

任一超限 → 返回 `is_error: true` 的 ToolResult，文案明确建议用
`offset/limit` 分段读。**不引入近似 token counter**——字节阈值就是项目侧的
代理。

### R4 — Dedup（per-session LRU）

- 缓存 key：`(mount_id, path, offset, limit)`。
- 缓存 value：`version_token`（来自 ReadResult）。
- 命中条件：相同 key + version_token 一致（取不到任一方则不命中）。
- 命中行为：返回短桩 ToolResult `"file unchanged since previous read"`，
  保留 path + range 元信息。
- 容量：**LRU 64 entries**（parent A1 决议）。
- 作用域：**每 session 一份**，不跨 session 共享（parent S1 决议）。

### R5 — ENOENT 友好提示

`MountError::NotFound` 时调一次 `provider.suggest_paths(mount, basename(path), 3, ctx)`，
拼进错误消息：`File not found: {path}. Did you mean: {top3.join(', ')}?`

**仅在调用路径所属 mount 内搜索**，不跨 mount（mount 抽象限制）。

### R6 — 二进制图片返回保持现状

`ContentPart::Image` + 文本元数据已基本对齐 CC，差异只在元数据呈现，
本任务**不动**。

## Acceptance Criteria

- [ ] `cargo build` / `cargo test` 通过。
- [ ] schema 改名通过：旧调用方传 `start_line/end_line` 应该报清晰错误。
- [ ] range 读：传 `offset=10000, limit=10` 读 50MB 文本文件，进程内存峰值
      不超过 50MB（验证非全文加载）。
- [ ] 字节超限：读 300KB 文本文件不带 limit → 返回 is_error 提示用 offset/limit。
- [ ] 行数超限：读 6000 行文件不带 limit → 返回 is_error 提示用 offset/limit。
- [ ] dedup 命中：连续两次相同 `(path, offset, limit)` 调用，第二次命中时
      返回短桩 + 不重新调 `read_text_range`（用 mock 验证）。
- [ ] dedup 失效：mock 改 version_token 后第二次调用走完整路径。
- [ ] ENOENT 友好提示：同 mount 内拼错文件名 → 错误消息包含 top-3 候选。
- [ ] agent 端 prompt + tool schema 已同步更新到 release branch。

## Constraints / 不在范围

- **不**做 PDF / notebook 解析（白名单：与 mount 抽象不对齐）。
- **不**做 macOS 截图 thin-space fallback（白名单：场景太窄）。
- **不**改 SPI（依赖 `vfs-search-spi-fix` 已 merge）。
- **不**做 dedup 跨 session 共享。

## Open Questions（继承自 parent A3/A8 之外）

- B1. 短桩文案精确措辞（CC 是 `FILE_UNCHANGED_STUB`，我们要不要复刻同样
  内容？）—— 在 brainstorm 阶段定。

## Notes

- 在 `release/fs-tools-rebuild` 分支上完成；**不直接 merge 到 main**。
- 工时估算：1–3 day。
- 依赖：`vfs-search-spi-fix` 已在 release branch 上 merge。
