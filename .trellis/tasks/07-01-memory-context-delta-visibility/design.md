# Design: Memory Context Snapshot And Delta

## Problem Statement

Memory 当前已经被归类为动态发现资源，但运行模型仍偏 snapshot：每轮 launch 从 `MemoryDiscoveryOutput` 构造 `memory_context`，前端把它作为一块 markdown body 展示。缺少 Memory 变化事件会让用户无法看见 Agent 对长期上下文的修改，也让 `discovery_digest` 的变化缺少可解释的 timeline 证据。

设计目标是建立“状态 + 事件”双模型：

```text
memory_context            = 当前 Memory inventory snapshot
memory_inventory_delta    = Memory inventory/source/index 变化事件
```

## Current Evidence

- `crates/agentdash-application-runtime-session/src/session/memory_context_frame.rs` 负责构造 `memory_context`，当前 section 是 `SystemNotice`。
- `crates/agentdash-spi/src/platform/memory_discovery.rs` 定义 `MemoryDiscoveryOutput`、`MemoryDiscoveryCluster`、`DiscoveredMemorySource` 和 diagnostics。
- `.trellis/spec/backend/capability/integration-api.md` 规定 Memory discovery 只从 active VFS 的受控 mount 与 bounded index 文件派生，不扩大权限边界。
- `.trellis/spec/backend/session/execution-context-frames.md` 已规定 Memory 属于 `discovered_inventory`，不进入 PiAgent system prompt。
- `packages/app-web/src/features/session/model/contextFrame.ts` 尚无 Memory 专属 section union。
- `packages/app-web/src/features/session/ui/ContextFrameStream.tsx` 只按 kind 给 `memory_context` 标记 `MEMORY`，尚未按 discovered inventory 分组呈现。

## Module Shape

建议在 runtime-session 中新增一个窄模块：

```rust
memory_inventory_delta
```

外部 interface：

```rust
fn diff_memory_inventory(
    before: &MemoryDiscoveryOutput,
    after: &MemoryDiscoveryOutput,
) -> Option<MemoryInventoryDelta>

fn build_memory_inventory_delta_frame(
    input: MemoryInventoryDeltaFrameInput,
) -> Option<ContextFrame>
```

这个模块隐藏 source 去重、digest/revision 计算、diagnostic diff、change kind 归类和 frame section 构造。调用方只需要提供前后 inventory。

## Protocol Shape

### Frame Kind

新增：

```text
memory_inventory_delta
```

delivery metadata：

```text
delivery_phase = discovered_inventory
delivery_order = 61
cache_policy = discovery_digest
model_channel = context
agent_consumption.mode = consume
frontend_label = Memory Inventory Delta
```

`memory_context` 保持 order 60；delta 放在 61，表示它紧随当前 Memory snapshot 或其它 discovered inventory 项展示。

### Section

建议新增 section：

```rust
ContextFrameSection::MemoryInventory {
    title: String,
    summary: String,
    mode: MemoryInventorySectionMode,
    sources: Vec<MemoryInventorySourceView>,
    diagnostics: Vec<MemoryInventoryDiagnosticView>,
    changes: Vec<MemoryInventoryChangeView>,
    previous_revision: Option<String>,
    next_revision: Option<String>,
}
```

其中：

```rust
enum MemoryInventorySectionMode {
    Snapshot,
    Delta,
}

enum MemoryInventoryChangeKind {
    Created,
    Updated,
    Removed,
    Reindexed,
    DiagnosticsChanged,
}
```

首版 `changes` 可以只表达 source/index 级变化：

```rust
struct MemoryInventoryChangeView {
    kind: MemoryInventoryChangeKind,
    source_key: String,
    source_uri: String,
    index_uri: String,
    previous_revision: Option<String>,
    next_revision: Option<String>,
    summary: String,
}
```

后续如果 index parser 能稳定识别 topics，再追加：

```rust
added_topics: Vec<MemoryTopicSummary>
updated_topics: Vec<MemoryTopicSummary>
removed_topics: Vec<MemoryTopicSummary>
```

## Revision Strategy

首版 revision 可以基于已规范化的 discovery output 计算稳定 digest：

```text
memory_inventory_revision = hash(normalized provider_key/source_key/source_uri/index_uri/index_status/bounded_index_content/diagnostics)
source_revision = hash(normalized single source + source diagnostics)
```

这样不要求新增数据库字段，也不依赖文件系统 mtime。因为 Memory 的模型可见面来自 `MemoryDiscoveryOutput`，revision 应绑定这份模型可见 projection，而不是原始文件路径状态。

## Data Flow

### Snapshot

```text
FrameConstruction / LaunchPlan
  -> discovered_memory: MemoryDiscoveryOutput
  -> TurnPreparer
  -> build_memory_context_frame
  -> ContextFrame(kind=memory_context, section=MemoryInventory(snapshot))
  -> ContextDeliveryPlan(discovered_inventory)
  -> frontend dynamic context group
```

### Delta

```text
Memory-relevant VFS mutation or memory refresh
  -> derive_runtime_memory_inventory(after)
  -> read previous active memory inventory projection
  -> diff_memory_inventory(before, after)
  -> build_memory_inventory_delta_frame
  -> emit context_frame event
  -> enqueue turn-start/context notification for live connector when applicable
  -> update active memory inventory projection
```

## Active Projection Ownership

Active turn/session already tracks capability state and context frames. Memory delta needs a small current-memory projection cache keyed by session/runtime target. Candidate owners:

- `TurnExecution` stores latest `MemoryDiscoveryOutput` or revision-only summary.
- Existing runtime surface update path reuses active turn snapshot and updates context frames.

首版优先让 active runtime projection 保存 latest memory inventory projection；这样 delta diff 的 before/after 事实来自同一 runtime surface，而不是从前端事件或 JSONL 历史倒推。

## Frontend Presentation

前端应按 delivery metadata 先分层，再在 discovered inventory 中展示 Memory：

```text
Discovered Inventory
  TOOL SURFACE
  MCP UPDATE
  SKILL UPDATE
  MEMORY SNAPSHOT
  MEMORY DELTA
```

Memory renderer 展示：

- source/index chips。
- change kind 与 revision。
- diagnostics list。
- bounded index preview 仅对 snapshot 展示，并继续可滚动。
- delta 的 added/updated/removed/reindexed 变化列表。

前端不从 markdown body 解析 Memory 信息；它消费 typed section。

## Trade-offs

- 先做 source/index 级 delta，而不是 topic body diff，能尽快让用户看到 Memory 变化事件，同时避免绑定尚未稳定的 Memory 文件格式。
- revision 绑定 `MemoryDiscoveryOutput` 而不是文件 mtime，能保证模型可见上下文变化和前端事件一致。
- 让普通 VFS 写入先触发 re-discovery 再生成 delta，比在 VFS 工具里硬编码 `MEMORY.md` 更深，原因是 provider 才知道哪些 URI 是 Memory source/index。

## Risks

- 如果 active runtime 没有保存 previous memory inventory，就只能在下一轮 launch 看见 snapshot，无法生成 delta。
- 如果 VFS mutation hook 过粗，可能导致过多 re-discovery；需要用 memory discovery rules 或 affected URI 过滤。
- 如果 section 协议一次设计过细，会被当前 Memory markdown 格式反向锁死；首版应避免要求完整 topic diff。
