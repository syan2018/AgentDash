# 收束 Memory Context 动态更新与前端展示

## Goal

把 Memory 从“启动时单独摘出来的一块 ContextFrame 文本”收束为动态上下文体系中的一等资源，让用户能在前端清楚看到当前 Memory inventory 状态，以及 Agent 或运行时导致 Memory source/index 变化时产生的可见事件。

## Background

当前 `memory_context` 已经从 PiAgent system prompt 中移出，并归入 `discovered_inventory` / `discovery_digest` / `model_channel=context`。这一步解决了 Memory 被当成 system 顶层规则的问题，但它仍然主要表现为 launch/turn snapshot：

- 后端 `crates/agentdash-application-runtime-session/src/session/memory_context_frame.rs` 从 `MemoryDiscoveryOutput` 构造 `memory_context`，section 仍是 `SystemNotice { title: "Memory Context", body }`。
- Memory discovery provider 位于 Host Integration API，当前契约是从 active VFS 的受控 mount 与 bounded index 文件派生 `MemoryDiscoveryOutput`。
- 前端 `ContextFrameStream` 已按 `ContextDeliveryPlan` 排序并给 `memory_context` 标记 `MEMORY`，但没有把 Memory 与 tool/MCP/VFS/skill delta 作为同一动态上下文分组来呈现。
- 前端 `ContextFrameSection` 没有 Memory 专属 section；Memory 的 source、index、diagnostic、revision 或变化摘要无法结构化展示。
- Agent 通过 VFS 修改 `agent://MEMORY.md` 或相关 Memory topic 后，用户缺少一条“Memory 已变化”的 timeline/context-frame 事件，只能在下一轮 snapshot 中间接看到变化。

这个任务的核心不是把 Memory 放回 system prompt，而是补齐动态资源的状态与事件模型。

## Requirements

### R1. Memory 展示归入动态上下文

前端应把 `memory_context` 展示为 `discovered_inventory` 的一部分，与 tool schema、MCP、VFS、Skill、Companion roster 等动态上下文变化处于同一信息层级。

Memory 可以保留专门 renderer，因为它有 source URI、index URI、bounded index、diagnostic、scope、capability 等独有信息；但视觉和信息架构不应暗示它是 system prompt 或独立 launch context。

### R2. 区分 snapshot 与 delta

Memory 上下文需要明确两种 frame：

- `memory_context`：当前 turn 可见 Memory inventory snapshot，来源于 `LaunchPlan.discovered_memory`，表达当前有哪些 source/index、默认 source/index、diagnostics 与 bounded index 内容。
- `memory_inventory_delta`：运行中或两次 projection 间 Memory inventory/index/source 发生变化的事件，表达“发生了什么变化”。

两者可以在前端同时出现，但语义不同：snapshot 表达当前状态，delta 表达变化事件。

### R3. Memory delta 必须是用户可见事件

当 Agent 或运行时导致 Memory source/index 发生可观察变化时，应产生 `memory_inventory_delta` ContextFrame，并进入 session timeline/context stream。用户应能看到：

- 哪个 source/index 变化。
- 变化类型，例如 created / updated / removed / reindexed / diagnostics_changed。
- previous revision 与 next revision，或等价 digest。
- 变化摘要；若后端能可靠计算 topic 级 diff，则展示 added / updated / removed topics。

首版可以先实现 source/index 级 delta，不要求完整 topic body diff。

### R4. Memory 变化来源必须来自明确语义链路

不能把所有普通 VFS 写入都猜成 Memory 变化。Memory delta 应来自以下可验证入口之一：

- Memory provider / indexer 对受控 memory source 的重新发现结果与前一份 inventory 对比。
- Runtime VFS 写入后，由 memory discovery rule 命中的受控 memory index/source 触发 re-discovery。
- 后续若新增 Memory tool，则由该 tool 显式产出 `MemoryInventoryChange`。

任何入口都必须保持 VFS 权限边界：读写仍由 VFS mount capability 控制，Memory 不新增绕过 VFS 的数据库或专用读写通道。

### R5. Delivery metadata 继续保持动态资源语义

`memory_context` 与 `memory_inventory_delta` 都应继续归入：

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
- change kind 或 snapshot kind。
- status / diagnostics。
- previous / next revision。
- added / updated / removed topic 摘要，如后端提供。

`memory_context` 不应只作为 `system_notice` 的 markdown body 展示。

## Acceptance Criteria

- [ ] `ContextFrameSection` 协议新增 Memory snapshot/delta 的结构化 section，或新增能覆盖 snapshot 与 delta 的 Memory section union。
- [ ] `memory_context` 继续保留 snapshot 语义，但前端以动态上下文资源展示，而不是普通 system notice。
- [ ] 新增 `memory_inventory_delta` frame kind，delivery metadata 为 discovered inventory / discovery digest / context。
- [ ] 后端能够从 Memory inventory 前后状态生成 delta，至少覆盖 source/index created、updated、removed、diagnostics_changed、reindexed 这些变化。
- [ ] Agent 或运行时修改受控 Memory index/source 后，session stream 能看到 Memory delta ContextFrame。
- [ ] 前端 ContextFrameStream 在 discovered inventory 层展示 Memory snapshot/delta，并显示 revision、source/index、change kind 和 diagnostics。
- [ ] PiAgent system prompt 测试继续证明 Memory snapshot/delta 不进入 system prompt。
- [ ] 文档更新说明 Memory snapshot 与 Memory delta 的职责边界，以及 Memory 仍通过 VFS 权限边界读写。

## Out Of Scope

- 不重新设计 Memory 文件格式。
- 不要求首版做完整 topic body diff；可以先做 index/source 级变化摘要。
- 不引入绕过 VFS 的 Memory 数据库读写 API。
- 不改变 compaction summary 语义。
- 不把 Memory 放回 PiAgent system prompt。

## Open Questions

- 首版 Memory delta 的触发源应优先接在哪个位置：runtime VFS mutation 后的 re-discovery，还是 Memory provider/indexer 自身的 refresh path？
- 是否需要在首版展示 topic-level diff，还是只展示 index/source revision 和 summary？
