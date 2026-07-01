# 收束 Memory Context 动态更新与前端展示

## Goal

把 Memory 从“启动时单独摘出来的一块 ContextFrame 文本”收束到与 Skill 基本一致的动态上下文路径里，让用户能在前端看到当前 Memory inventory 状态，以及 mount / discovery 变化导致的 Memory index 级变更事件。

## Background

当前 `memory_context` 已经从 PiAgent system prompt 中移出，并归入 `discovered_inventory` / `discovery_digest` / `model_channel=context`。这一步解决了 Memory 被当成 system 顶层规则的问题，但它仍然主要表现为 launch/turn snapshot：

- 后端 `crates/agentdash-application-runtime-session/src/session/memory_context_frame.rs` 从 `MemoryDiscoveryOutput` 构造 `memory_context`，section 仍是 `SystemNotice { title: "Memory Context", body }`。
- Memory discovery provider 位于 Host Integration API，当前契约是从 active VFS 的受控 mount 与 bounded index 文件派生 `MemoryDiscoveryOutput`。
- Skill 已经通过 `SkillDimensionDelta` 进入现有 runtime context delta section；VFS 也已有 `VfsDimensionDelta`，可作为 Memory 简化接入的参考路径。
- 前端 `ContextFrameStream` 已按 `ContextDeliveryPlan` 排序并给 `memory_context` 标记 `MEMORY`，但没有把 Memory 与 tool/MCP/VFS/skill delta 作为同一动态上下文分组来呈现。
- 前端 `ContextFrameSection` 没有 Memory 专属 section；Memory 的 source、index、diagnostic、revision 或变化摘要无法结构化展示。
- 当 runtime VFS mount 或 discovery surface 变化导致 Memory inventory/index 变化时，用户缺少一条“Memory 已变化”的 timeline/context-frame 事件，只能在下一轮 snapshot 中间接看到变化。

这个任务的核心不是把 Memory 放回 system prompt，也不是设计大型 Memory 子系统，而是把 Memory 纳入现有动态发现 / delta / 前端渲染路径。

## Requirements

### R1. Memory 展示归入动态上下文

前端应把 `memory_context` 展示为 `discovered_inventory` 的一部分，与 tool schema、MCP、VFS、Skill、Companion roster 等动态上下文变化处于同一信息层级。

Memory 可以保留专门 renderer，因为它有 source URI、index URI、bounded index、diagnostic、scope、capability 等独有信息；但视觉和信息架构不应暗示它是 system prompt 或独立 launch context。

### R2. 区分 snapshot 与 delta section

Memory 上下文需要明确两种表现：

- `memory_context`：当前 turn 可见 Memory inventory snapshot，来源于 `LaunchPlan.discovered_memory`，表达当前有哪些 source/index、默认 source/index、diagnostics 与 bounded index 内容。
- `memory_inventory_delta` section：运行中或两次 projection 间 Memory inventory/index/source 发生变化的事件，表达“发生了什么变化”。

delta 应尽量复用 Skill/VFS 现有动态上下文 frame 路径，而不是为 Memory 单独造一条运行通道。

### R3. Memory delta 只做 index/source 级简单模型

当 runtime VFS mount 或 discovery refresh 导致 Memory source/index 发生可观察变化时，应产生 `memory_inventory_delta` section，并进入 session timeline/context stream。用户应能看到：

- 哪个 source/index 变化。
- 变化类型：added / removed / changed。
- previous revision 与 next revision，或等价 digest。
- index 级变化摘要与 diagnostics。

首版不解析 topic-level diff，不追踪每次普通文件写入，也不引入 Memory 专属 watch 机制。

### R4. 触发源复用动态发现路径

不能把所有普通 VFS 写入都猜成 Memory 变化。Memory delta 首版只需要跟随现有动态发现路径，在 mount / runtime surface 变化时处理即可：

- VFS mount 增删或默认 mount 变化后，和 Skill 一样重新跑 discovery。
- Memory provider 对受控 memory source 的重新发现结果与前一份 inventory 对比。

任何入口都必须保持 VFS 权限边界：读写仍由 VFS mount capability 控制，Memory 不新增绕过 VFS 的数据库或专用读写通道。

### R5. Delivery metadata 继续保持动态资源语义

`memory_context` 与承载 `memory_inventory_delta` section 的动态上下文 frame 都应继续归入：

```text
delivery_phase = discovered_inventory
cache_policy = discovery_digest
model_channel = context
agent_consumption = consume
```

PiAgent system prompt 不应重新消费 Memory。

### R6. 前端需要结构化 renderer

前端应支持 Memory 专属 section 类型，至少覆盖：

- source URI / index URI。
- snapshot 或 delta mode。
- status / diagnostics。
- previous / next revision。
- added / removed / changed source/index 摘要。

`memory_context` 不应只作为 `system_notice` 的 markdown body 展示。

## Acceptance Criteria

- [ ] `ContextFrameSection` 协议新增 Memory snapshot/delta 的结构化 section，或新增能覆盖 snapshot 与 index 级 delta 的 Memory section union。
- [ ] `memory_context` 继续保留 snapshot 语义，但前端以动态上下文资源展示，而不是普通 system notice。
- [ ] Memory delta 像 Skill delta 一样进入现有动态上下文 frame/section 路径，不新增独立 Memory 运行通道。
- [ ] 后端能够从 Memory inventory 前后状态生成 index/source 级 delta，至少覆盖 added、removed、changed。
- [ ] VFS mount 或 runtime discovery surface 变化导致受控 Memory index/source 变化后，session stream 能看到 Memory delta section。
- [ ] 前端 ContextFrameStream 在 discovered inventory 层展示 Memory snapshot/delta，并显示 revision、source/index、change kind 和 diagnostics。
- [ ] PiAgent system prompt 测试继续证明 Memory snapshot/delta 不进入 system prompt。
- [ ] 文档更新说明 Memory snapshot 与 Memory delta 的职责边界，以及 Memory 仍通过 VFS 权限边界读写。

## Out Of Scope

- 不重新设计 Memory 文件格式。
- 不做 topic-level diff，只做 index/source 级变化摘要。
- 不引入绕过 VFS 的 Memory 数据库读写 API。
- 不新增 Memory 专属 live update 通道；复用 Skill/VFS 所在的动态上下文路径。
- 不追踪每次普通 VFS 文件写入；mount / runtime discovery surface 变化时处理 discovery delta 即可。
- 不改变 compaction summary 语义。
- 不把 Memory 放回 PiAgent system prompt。
