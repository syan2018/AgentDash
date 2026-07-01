# Design: Memory Context In Unified Dynamic Context Path

## Problem Statement

Memory 当前已经被归类为动态发现资源，但运行模型仍偏 snapshot：每轮 launch 从 `MemoryDiscoveryOutput` 构造 `memory_context`，前端把它作为一块 markdown body 展示。用户希望它像 Skill 一样进入统一动态上下文路径，而不是作为特殊块单独处理。

设计目标是一个简单模型：

```text
memory_context snapshot       = 当前 Memory inventory 状态
memory_inventory_delta section = mount/discovery 变化导致的 index/source 级变化
```

不做 topic-level diff，不新增 Memory 专属 live update 通道，不追踪每次普通 VFS 文件写入。

## Current Evidence

- `crates/agentdash-application-runtime-session/src/session/memory_context_frame.rs` 负责构造 `memory_context`，当前 section 是 `SystemNotice`。
- `crates/agentdash-spi/src/platform/memory_discovery.rs` 定义 `MemoryDiscoveryOutput`、`MemoryDiscoveryCluster`、`DiscoveredMemorySource` 和 diagnostics。
- `.trellis/spec/backend/capability/integration-api.md` 规定 Memory discovery 只从 active VFS 的受控 mount 与 bounded index 文件派生，不扩大权限边界。
- `crates/agentdash-application-runtime-session/src/session/dimension/skill.rs` 已通过 `SkillDimensionDelta` 把 Skill 变化渲染为 `ContextFrameSection::SkillDelta`。
- `crates/agentdash-application-runtime-session/src/session/dimension/vfs.rs` 已通过 `VfsDimensionDelta` 表达 mount 增删与默认 mount 变化。
- `.trellis/spec/backend/session/execution-context-frames.md` 已规定 Memory 属于 `discovered_inventory`，不进入 PiAgent system prompt。
- `packages/app-web/src/features/session/model/contextFrame.ts` 尚无 Memory 专属 section union。

## Module Shape

建议新增一个与 `dimension/skill.rs` 同风格的 runtime-session 维度 helper：

```rust
dimension/memory.rs
```

核心 interface：

```rust
pub(crate) struct MemoryDimensionDelta {
    pub added: Vec<RuntimeMemorySourceEntry>,
    pub removed: Vec<RuntimeMemorySourceEntry>,
    pub changed: Vec<RuntimeMemorySourceEntry>,
}

impl MemoryDimensionDelta {
    pub fn from_inventory_delta(
        before: Option<&MemoryDiscoveryOutput>,
        after: &MemoryDiscoveryOutput,
    ) -> Option<Box<dyn DimensionDelta>>;
}
```

它和 `SkillDimensionDelta` 一样实现：

```rust
impl DimensionDelta for MemoryDimensionDelta {
    fn has_changes(&self) -> bool;
    fn to_section(&self) -> ContextFrameSection;
    fn render_text(&self, phase_node: Option<&str>) -> String;
}
```

这样 Memory delta 可以跟 Skill/VFS/Tool/MCP 一样进入现有动态 context frame，而不是产生新的 Memory 专属运行通道。

## Protocol Shape

### Snapshot Section

`memory_context` snapshot 继续作为独立 frame 存在，因为它是 launch/turn 当前状态投影。但 section 应从 `SystemNotice` 迁移为 typed Memory section，例如：

```rust
ContextFrameSection::MemoryInventory {
    title: String,
    summary: String,
    mode: MemoryInventorySectionMode,
    sources: Vec<RuntimeMemorySourceEntry>,
    diagnostics: Vec<RuntimeMemoryDiagnosticEntry>,
    added_sources: Vec<RuntimeMemorySourceEntry>,
    removed_sources: Vec<RuntimeMemorySourceEntry>,
    changed_sources: Vec<RuntimeMemorySourceEntry>,
}
```

`mode = snapshot` 时：

- `sources` 表达当前 inventory。
- `diagnostics` 表达当前 diagnostics。
- added / removed / changed 为空。

### Delta Section

同一个 section 在 `mode = delta` 时表达 index/source 级变化：

- `added_sources`
- `removed_sources`
- `changed_sources`
- `diagnostics`

不新增 `memory_inventory_delta` frame kind。首版把 delta section 放进现有 runtime context delta frame，和 `SkillDelta` 一样展示。

### RuntimeMemorySourceEntry

首版只需要保留 index/source 级字段：

```rust
pub struct RuntimeMemorySourceEntry {
    pub provider_key: String,
    pub source_key: String,
    pub display_name: String,
    pub source_uri: String,
    pub index_uri: String,
    pub mount_id: String,
    pub scope: String,
    pub index_status: String,
    pub revision: String,
    pub summary: Option<String>,
}
```

revision 基于模型可见的 normalized source projection 计算，例如：

```text
hash(provider_key/source_key/source_uri/index_uri/index_status/bounded_index_content)
```

不依赖文件 mtime，不解析 topic body。

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

首版复用 Skill/VFS 的动态上下文路径：

```text
runtime surface / mount 变化
  -> recompute skill discovery and memory discovery
  -> compare previous MemoryDiscoveryOutput with next MemoryDiscoveryOutput
  -> MemoryDimensionDelta
  -> existing runtime context delta frame sections
  -> session stream / frontend ContextFrame renderer
```

只要求 mount/runtime discovery surface 变化时处理 Memory discovery delta；普通 VFS 文件写入不在首版追踪范围内。

## Frontend Presentation

前端应按 delivery metadata 展示 discovered inventory，并把 Memory 和 Skill/VFS/Tool/MCP 放在同一层：

```text
Discovered Inventory
  TOOL SURFACE
  MCP UPDATE
  SKILL UPDATE
  MEMORY
```

Memory renderer 展示：

- source/index chips。
- snapshot 或 delta mode。
- revision。
- diagnostics list。
- delta 的 added / removed / changed source 列表。
- snapshot 的 bounded index 仍可通过 rendered text 或 typed field 预览；首版可以只展示 source/index/status/revision。

前端不从 markdown body 解析 Memory 信息；它消费 typed section。

## Trade-offs

- 复用 Skill/VFS 维度 delta 路径，能降低实现复杂度，也符合 Memory 当前“discovery output projection”的事实模型。
- 只做 index/source 级 delta，避免把实现绑定到当前 Memory markdown/topic 格式。
- 只在 mount/runtime discovery surface 变化时处理，能避免为普通 VFS 写入建立昂贵或过早的 watcher/indexer。

## Risks

- 如果用户在同一 mount 内直接改 `MEMORY.md`，首版可能要等下一次 mount/runtime discovery refresh 才显示 delta；这是本任务接受的简化。
- Memory 尚未进入 `CapabilityState`，实现时需要选择一个轻量位置保存上一份 `MemoryDiscoveryOutput`，但不应为此引入新的持久化模型。
- `capability_state_delta` 这个 frame kind 已经承载 Skill/VFS 等 runtime surface 变化，Memory 复用时要确保前端 label 不再误导为单纯 capability key 变化。
