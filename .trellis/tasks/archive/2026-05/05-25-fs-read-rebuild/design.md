# Design — fs_read 重做

> **Parent:** [05-25-fs-tools-optimization-review](../05-25-fs-tools-optimization-review/design.md)
> **PRD:** [./prd.md](./prd.md)（含 CC 参考路径）

## §1 范围

把 [fs_read tool](../../../crates/agentdash-application/src/vfs/tools/fs/read.rs)
从草率实现升级到与 CC FileReadTool 对齐：schema (offset/limit) + 真按 range
读 + 字节/行数双阈值上限 + per-instance LRU 64 dedup + ENOENT 友好提示。

**不在范围（PRD §Constraints）：** PDF / notebook / 截图 thin-space / 跨 session
dedup / SPI 改动（已在 spi-fix archived）。

## §2 RelayVfsService 加两个封装方法

### §2.1 `read_text_range`

```rust
pub async fn read_text_range(
    &self,
    vfs: &Vfs,
    target: &ResourceRef,
    offset: usize,           // 0-based 行号，与 SPI 对齐
    limit: Option<usize>,
    overlay: Option<&InlineContentOverlay>,
    identity: Option<&AuthIdentity>,
) -> Result<ReadResult, String>;
```

- overlay 命中 ⇒ 拿到全文后切片（overlay content 通常很小，slice 成本低）。
- 否则 dispatch 到 `provider.read_text_range`（SPI 有默认实现，lifecycle/relay_fs
  本任务覆盖为真按 range 读；canvas/inline/skill_asset 走默认）。

### §2.2 `suggest_paths`

```rust
pub async fn suggest_paths(
    &self,
    vfs: &Vfs,
    target: &ResourceRef,
    limit: usize,
    identity: Option<&AuthIdentity>,
) -> Result<Vec<String>, String>;
```

- 直接 dispatch 到 `provider.suggest_paths(mount, basename(target.path), limit, ctx)`。
- mount 边界内查找；不跨 mount。

## §3 Provider 真按 range 读（lifecycle / relay_fs）

### §3.1 lifecycle

[provider_lifecycle.rs:151-356](../../../crates/agentdash-application/src/vfs/provider_lifecycle.rs#L151-L356)
现有 read_text 大量走 journey/run repository projection（virtual content），
对这类 virtual 内容**不重写 read_text_range**——它们本就是动态 JSON，slice
后无意义。继承 SPI 默认实现即可。

但 `read_projected_skill_file` 调用的 `load_projected_skill_files` 会从
SkillAssetRepository 拉文件 — 这部分内容是真实文本文件，可以加 range 优化。
**本任务先不做**（skill 文件通常很小 < 10KB，range 收益不明显），保留默认。

### §3.2 relay_fs（[crates/agentdash-api/src/mount_providers/relay_fs.rs](../../../crates/agentdash-api/src/mount_providers/relay_fs.rs)）

走 RelayMessage 通道，调用远端 backend 的 file_read。当前 relay 协议
（`ToolFileReadPayload`）是否支持 offset/limit 参数？需要先 grep 协议定义：

- 如果 relay 协议已支持 ⇒ 在 `read_text_range` 里走 range 通道。
- 如果不支持 ⇒ 保留 SPI 默认（全文 + slice），并在 prompt 注释为 follow-up。

**本任务决策：** relay 协议改造超范围，relay_fs 保留默认实现，**接受**
全文加载在远端 fetch 后才 slice 的成本。真正受益的是 lifecycle 物理文件
（如果有的话），但那条路径同样大多是 projection。

**结论：** 本任务**不**重写任何 provider 的 `read_text_range`，所有 provider
统一走 SPI 默认（全文 + slice）。Range 真优化推迟到证明性能瓶颈后再做。

> 这是相对 PRD §R2 的**降级**：parent design.md 当时假设 lifecycle/relay_fs 有
> 物理文件路径需要按行流式读，但实际现状两者都是远端/projection。降级理由：
> - lifecycle = projection（无实体文件）
> - relay_fs = 远端 file_read（无现成 range 协议字段）
> 真受益场景在 fs 工具被海量大文件压榨时再做 follow-up；当前 256KB 字节
> 上限 + 5000 行上限本身就限制了全文加载的最大成本（256KB 一次 read）。

## §4 字节 / 行数上限

```rust
const MAX_BYTES: usize = 256 * 1024;  // 256KB
const MAX_LINES: usize = 5000;
```

判定时机：拿到 `read_text_range` 返回的 `ReadResult` 后：

1. `result.content.len() > MAX_BYTES` ⇒ is_error，文案：
   `File too large ({n} bytes > {MAX_BYTES}). Use offset/limit to read in chunks.`
2. line_count > MAX_LINES ⇒ is_error，类似文案。

**只在用户**没传 offset/limit 时才检查上限**——传了 limit ≤ 5000 的请求即使
内容更大也放行（用户已知道自己在做什么）。

实际判定逻辑：

```rust
let limit_specified = params.limit.is_some();
if !limit_specified {
    if result.content.len() > MAX_BYTES { return is_error_too_large(...); }
    if line_count > MAX_LINES { return is_error_too_many_lines(...); }
}
```

## §5 LRU 64 Dedup

### §5.1 数据结构

```rust
type DedupKey = (String /* mount_id */, String /* path */, Option<usize> /* offset */, Option<usize> /* limit */);

#[derive(Clone)]
struct DedupCache {
    inner: Arc<Mutex<lru::LruCache<DedupKey, String /* version_token */>>>,
}

impl DedupCache {
    fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(lru::LruCache::new(NonZeroUsize::new(64).unwrap()))),
        }
    }
}
```

`FsReadTool` 在 `new()` 时调 `DedupCache::new()`，所有 clone 共享同一 Arc。
provider 层每个 session 构造新 FsReadTool ⇒ 自然 per-session。

### §5.2 命中流程

1. 计算 key = (mount_id, path, offset, limit)。
2. 先查 cache：若有 cached_token ⇒ 调 `read_text_range` 拿当前 token：
   - 当前 token == cached ⇒ 短桩返回（不再走完整 ToolResult 路径）。
   - 当前 token != cached（或 None）⇒ 完整 ToolResult + 更新 cache。
3. 无 cache ⇒ 完整路径 + 写入 cache。

**注意：** 步骤 2 仍要调一次 `read_text_range` 才能拿到当前 token。这意味着
dedup 不省调用，**只省 LLM 端 token**（短桩 ToolResult 比完整内容短）。

### §5.3 短桩文案（B1 brainstorm）

```
file: {path}
[unchanged since previous read of {offset}..{offset+limit}]
```

CC 用的是 `FILE_UNCHANGED_STUB` 常量，同样语义。我们就用这个简明形式。

### §5.4 lru crate 引入

`lru = "0.12"` 加入 `agentdash-application/Cargo.toml`。

## §6 ENOENT 友好提示

`MountError::NotFound` 时调 `service.suggest_paths(vfs, target, 3)`，拼到
错误消息：

```
File not found: {path}.
Did you mean: {top3.join(', ')}?
```

实现：catch `service.read_text_range` 的 `Err`，匹配 `MountError::NotFound`
（透过 String — service 层把 MountError 转 String 时丢了类型）。

**问题**：service.read_text_range 的 Result<_, String> 把错误类型扁了。
要么保留原 MountError（改 service 签名），要么用 string 前缀匹配。

**决策：** service 层改返回 `Result<ReadResult, MountError>` 而非
`Result<ReadResult, String>`。这是个更广泛改动，但保留类型信息，对 ENOENT
友好提示这种 case 是必需的。**本任务范围内仅改 read_text_range 一个方法**，
read_text / read_binary / search_text_extended 保留 String 错误（不破其他调用方）。

## §7 Schema 改名（breaking）

```rust
#[derive(Debug, Deserialize, JsonSchema)]
pub struct FsReadParams {
    pub path: String,
    /// 0-based 起始行（CC FileReadTool 是 1-based，但 SPI read_text_range
    /// 用 0-based）。tool 层接收 1-based offset，内部转 0-based 传给 SPI。
    /// 与 CC 完全对齐：tool 入参 = 1-based。
    pub offset: Option<usize>,
    pub limit: Option<usize>,
}
```

**1-based vs 0-based 调和：**
- tool 入参（user-facing）= 1-based（CC 对齐）；offset=0 视为同 omit。
- SPI read_text_range = 0-based。
- 转换：`spi_offset = params.offset.map(|n| n.saturating_sub(1)).unwrap_or(0)`。

旧调用方传 `start_line/end_line` ⇒ serde Deserialize 失败（unknown field）⇒
返回清晰的 `InvalidArguments`。

prompt 描述更新（参考 [FileReadTool/prompt.ts](../../../references/claude-code/src/tools/FileReadTool/prompt.ts)）：

```
Reads a file from the local filesystem. The path parameter must be in `mount_id://relative/path` format.

Usage:
- The offset parameter (1-based) skips that many lines from the file start.
- The limit parameter caps how many lines to return.
- For files larger than 256KB or 5000 lines, you MUST use offset/limit.
- Output uses cat -n format: `   N | content`.
- Image files (PNG/JPEG/etc) return as image blocks plus metadata.
```

## §8 测试矩阵

| 测试 | 验证 |
|------|------|
| T1 schema 改名 | 旧 `start_line` → InvalidArguments；新 `offset/limit` → ok |
| T2 1-based 转换 | `offset=2` 跳过第 1 行 |
| T3 字节超限 | 构造 300KB 内容 + 不传 limit → is_error |
| T4 行数超限 | 构造 6000 行 + 不传 limit → is_error |
| T5 上限 bypass | 同上但传 `limit: 100` → ok |
| T6 dedup 命中 | 连续调两次 → 第二次返回短桩 |
| T7 dedup 失效 | mock token 变化 → 第二次走完整路径 |
| T8 ENOENT 友好 | NotFound → 错误含候选 paths |

测试 fixture 复用现有 `MemoryReadProvider`，扩展支持 `read_text_range` /
`suggest_paths` mock。

## §9 决策矩阵

| ID | 决策 | 状态 |
|----|------|------|
| D1 | provider 真按 range 读暂不做（lifecycle/relay_fs 路径都不是物理文件） | accept（降级 PRD §R2） |
| D2 | tool 入参 1-based offset（CC 对齐）；SPI 0-based 内部转换 | accept |
| D3 | 上限只在用户没传 limit 时才检查 | accept |
| D4 | dedup key 包含 (mount_id, path, offset, limit) 四元组 | accept |
| D5 | dedup 不省调用，只省 LLM token | accept（与 CC 等价） |
| D6 | service.read_text_range 改用 MountError 错误类型（其他方法保 String） | accept |
| D7 | lru crate 0.12 引入 | accept |
| D8 | 短桩文案极简 `[unchanged since previous read of {range}]` | accept |
