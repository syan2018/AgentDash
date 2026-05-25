# Implement — fs_read 重做

> 设计依据：[design.md](./design.md) §1-§9。Branch: `release/fs-tools-rebuild`。

## S1 — Service 层封装（[relay_service.rs](../../../crates/agentdash-application/src/vfs/relay_service.rs)）

- [ ] **S1.1** 加 `RelayVfsService::read_text_range(vfs, target, offset, limit, overlay, identity) -> Result<ReadResult, MountError>`。
      - overlay 命中 ⇒ overlay 全文 + slice。
      - 否则 dispatch `provider.read_text_range`。
      - 返回 MountError 而非 String（design.md §6 决议）。
- [ ] **S1.2** 加 `RelayVfsService::suggest_paths(vfs, target, limit, identity) -> Result<Vec<String>, String>`。
      - dispatch `provider.suggest_paths(mount, basename(target.path), limit, ctx)`。
- [ ] **Validation:** `cargo check -p agentdash-application`。

## S2 — fs_read tool schema 改名 + range 调用（[fs/read.rs](../../../crates/agentdash-application/src/vfs/tools/fs/read.rs)）

- [ ] **S2.1** `FsReadParams` 改字段：`start_line/end_line` → `offset/limit`。
      doc comment 用 design.md §7 措辞。
- [ ] **S2.2** `description` prompt 改写为 design.md §7 文案。
- [ ] **S2.3** `execute` 改调 `service.read_text_range`，offset 1-based → 0-based 转换。
- [ ] **S2.4** 输出格式保留：`file: {path}\n   {n} | {line}` cat -n 风格；`n` 用
      实际行号（offset + relative_idx + 1，从 1 开始）。
- [ ] **Validation:** `cargo build -p agentdash-application`。

## S3 — 字节 / 行数上限

- [ ] **S3.1** 在 [fs/read.rs](../../../crates/agentdash-application/src/vfs/tools/fs/read.rs)
      文件头加 `const MAX_BYTES: usize = 256 * 1024;` `const MAX_LINES: usize = 5000;`。
- [ ] **S3.2** `execute` 拿到 ReadResult 后，仅在 `params.limit.is_none()` 时检查上限。
- [ ] **S3.3** 超限 ⇒ 返回 is_error ToolResult，文案：
      - bytes: `File too large ({n} bytes > {MAX_BYTES} max). Use offset/limit to read in chunks.`
      - lines: `File too long ({n} lines > {MAX_LINES} max). Use offset/limit to read in chunks.`
- [ ] **Validation:** unit test T3/T4。

## S4 — Dedup LRU 64

- [ ] **S4.1** Cargo.toml 加 `lru = "0.12"`。
- [ ] **S4.2** 加 `DedupCache` struct + `Arc<Mutex<LruCache<DedupKey, String>>>`。
      key = (mount_id, path, offset, limit)。
- [ ] **S4.3** `FsReadTool::new` 多构造 `DedupCache::new()` 一份。
- [ ] **S4.4** execute 流程：先调 `read_text_range`（要拿当前 token），然后比对：
      - cache 命中 + token 一致 ⇒ 短桩 ToolResult `[unchanged since previous read of L{a}-L{b}]`，
        is_error = false。
      - 否则更新 cache 后返回完整结果。
- [ ] **S4.5** token = None 视为 cache miss（不命中，符合 SPI 协议）。
- [ ] **Validation:** unit test T6/T7。

## S5 — ENOENT 友好提示

- [ ] **S5.1** match `service.read_text_range` 的 `Err(MountError::NotFound(_))`：
      调 `service.suggest_paths(vfs, target, 3, identity)`。
- [ ] **S5.2** 拼错误消息：`File not found: {path}. Did you mean: {top3.join(', ')}?`
      （top3 为空时 `Did you mean: <no candidates>?`）
- [ ] **S5.3** suggest_paths 内部失败不阻塞 ⇒ fallback 到不带候选的纯文案。
- [ ] **Validation:** unit test T8。

## S6 — 测试矩阵（[fs/read.rs](../../../crates/agentdash-application/src/vfs/tools/fs/read.rs) 末尾）

- [ ] **T1** schema 改名：旧 `start_line` ⇒ InvalidArguments；新 `offset/limit` ⇒ ok。
- [ ] **T2** 1-based 转换：构造 5 行文件，`offset=2` ⇒ 输出从第 2 行开始。
- [ ] **T3** 字节超限：构造 300KB 内容 + 无 limit ⇒ is_error。
- [ ] **T4** 行数超限：构造 6000 行 + 无 limit ⇒ is_error。
- [ ] **T5** 上限 bypass：T3 同样内容 + `limit: 100` ⇒ ok。
- [ ] **T6** dedup 命中：连续两次相同参数 ⇒ 第二次短桩。
- [ ] **T7** dedup 失效：mock provider 改 token ⇒ 第二次完整路径。
- [ ] **T8** ENOENT 友好：拼错文件名 ⇒ 错误含候选。
- [ ] **Validation:** `cargo test -p agentdash-application --lib vfs::tools::fs::read`。

## S7 — 全量回归

- [ ] **S7.1** `cargo build --workspace --lib`。
- [ ] **S7.2** `cargo test --workspace --lib`。
- [ ] **S7.3** clippy check（仅本任务文件）。

## S8 — Commit + archive

- [ ] **S8.1** 单 commit：`feat(fs-read): 重做 fs_read 工具与 CC FileReadTool 对齐`。
- [ ] **S8.2** `task.py finish && task.py archive`。

## 风险

| 风险 | 应对 |
|------|------|
| service.read_text_range 改 MountError 类型，调用方需改 | 本任务唯一新方法，无现存调用方破坏 |
| MAX_BYTES 在 inline mount 上太严（inline 可能有 2MB 文档） | inline 实测多在 KB 级别，超限分段读是合理交互；如反例多见再调阈值 |
| dedup token = None 时频繁 cache miss（lifecycle virtual content） | 是预期行为：projection 内容每次都"变"，dedup 不该误命中 |
| lru 0.12 与现有 cargo lock 冲突 | `cargo tree -i lru` 检查；冲突就升级 |

## 完成判定

- [ ] S1-S7 全 ✓
- [ ] 8 项测试全绿
- [ ] cargo build/test/clippy 全 workspace 通过
- [ ] commit + archive

## 执行策略

全 inline 处理；测试 fixture 在现有 `MemoryReadProvider` 上扩展（实现
`read_text_range` + `suggest_paths` mock）。
