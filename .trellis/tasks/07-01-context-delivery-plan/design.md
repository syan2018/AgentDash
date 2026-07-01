# Design: ContextDeliveryPlan

## Problem Statement

当前 ContextFrame 同时承担“结构化上下文载体”和“投递到模型的临时指令”两种职责。不同层通过 `kind`、`delivery_channel`、`message_role` 和局部 push 顺序猜测消费方式，导致 PiAgent、runtime-session、hook delegate、前端展示之间没有同一份正式顺序。

本设计引入 `ContextDeliveryPlan`，把“有哪些 ContextFrame”和“某个 Agent 如何消费这些 frame”拆开。

## Module Shape

建议新增深模块：

```rust
ContextDeliveryPlanner
```

外部 interface：

```rust
fn plan_context_delivery(input: ContextDeliveryInput) -> ContextDeliveryPlan
```

该 interface 隐藏以下实现细节：

- frame kind 到 delivery phase 的映射
- phase 内排序
- cache key / revision 计算
- connector/agent 消费能力与策略
- 哪些 frame 进入 PiAgent system prompt
- 哪些 frame 进入 turn context
- 哪些 frame 仅用于审计/前端展示

调用方不再通过硬编码 kind 数组决定顺序。

## Core Types

### ContextDeliveryPlan

```rust
pub struct ContextDeliveryPlan {
    pub plan_id: String,
    pub target_agent: ContextDeliveryTarget,
    pub entries: Vec<ContextDeliveryEntry>,
}
```

### ContextDeliveryEntry

```rust
pub struct ContextDeliveryEntry {
    pub frame_id: String,
    pub frame_kind: String,
    pub delivery_phase: ContextDeliveryPhase,
    pub delivery_order: u32,
    pub cache_policy: ContextCachePolicy,
    pub cache_key: Option<String>,
    pub cache_revision: Option<String>,
    pub model_channel: ContextModelChannel,
    pub agent_consumption: ContextAgentConsumption,
    pub frontend_label: String,
}
```

### ContextDeliveryPhase

初始枚举：

```rust
stable_system
session_policy
run_state
assignment
discovered_inventory
turn_runtime
```

### ContextCachePolicy

初始枚举：

```rust
static
session_digest
runtime_state_digest
assignment_revision
discovery_digest
turn_ephemeral
uncached
```

`uncached` 需要伴随原因字段或诊断记录。参考 `references/claude-code/src/constants/systemPromptSections.ts` 中 `DANGEROUS_uncachedSystemPromptSection` 的约束：破 cache 必须是显式决定。

### ContextModelChannel

初始枚举：

```rust
system
developer
context
user
audit_only
ignored
```

这里的 `system` 表示对某个目标 Agent 的消费方式，不代表 frame 自身是 system 语义。

### ContextAgentConsumption

初始结构建议：

```rust
pub struct ContextAgentConsumption {
    pub target: String,
    pub mode: ContextAgentConsumptionMode,
    pub reason: String,
}
```

模式：

```rust
consume
audit_only
ignore
connector_native
system_override
system_append
```

PiAgent 示例：

- `identity` -> `consume/system`
- `system_guidelines` -> `consume/system`
- `memory_context` -> `consume/context`

非 PiAgent 示例：

- `identity` -> `system_override` / `system_append` / `connector_native` / `ignore`
- `system_guidelines` -> `system_override` / `system_append` / `connector_native` / `ignore`
- `memory_context` -> `consume/context`

非 PiAgent 不默认 audit-only。是否消费 `stable_system` / `session_policy` 由 connector profile 声明；支持覆盖 system 的 Agent 可以选择 `system_override`，支持追加 system 的 Agent 可以选择 `system_append`，已有内嵌 prompt 且不允许覆盖的 Agent 才选择 `ignore` 或 `audit_only`。

## PiAgent Consumption

PiAgent connector 的 system prompt 只读取 plan 中：

```text
model_channel in [system, developer]
agent_consumption.mode == consume
```

并按：

```text
delivery_phase, delivery_order
```

稳定排序。

PiAgent 不再维护 `identity -> system_guidelines -> memory_context` 的本地 kind 列表。

## Memory Handling

`memory_context` 的语义是动态发现资源索引：

- source uri
- index uri
- bounded index digest
- diagnostics digest
- default source/index

它应归入：

```text
delivery_phase = discovered_inventory
cache_policy = discovery_digest
model_channel = context
```

它和 skill/tool/MCP/VFS 的共同点是：来自发现或 resolver 输出，可能在会话中变化，不能成为 PiAgent system prompt 的长期规则。

## continuation_context Cleanup

当前证据：

- `build_continuation_context_frame` 只有定义，无调用点。
- frame construction 写死 `continuation_context_frame: None`。
- 前端存在 parser/render/test fixture，但运行链路缺少 producer。
- `compaction_summary` 是活跃摘要机制。

设计默认删除 `continuation_context`。若实现前发现冷 transcript rehydrate 仍需要该能力，应改名为 `restored_transcript` 并补 producer，不能沿用 `continuation_context`。

## Frontend Projection

前端 `ContextFrameStream` 应消费 plan metadata：

- group by `delivery_phase`
- sort by `delivery_order`
- display `cache_policy`
- display `model_channel`
- display `agent_consumption.mode`

单个 ContextFrame 的 arrival order 仍可作为 event feed 顺序，但不再冒充正式模型上下文顺序。

## Cache Key Strategy

建议初始规则：

| Frame | Policy | Revision Source |
| --- | --- | --- |
| identity | `static` / `session_digest` | agent identity + executor system prompt digest |
| system_guidelines | `session_digest` | preferences revision + guideline file digest |
| compaction_summary | `runtime_state_digest` | compaction_id / projection_version |
| assignment_context | `assignment_revision` | runtime fragment digest |
| capability/tool/MCP/VFS/skill | `discovery_digest` | capability resolver / runtime context revision |
| memory_context | `discovery_digest` | memory inventory digest |
| pending_action / auto_resume / hook notices | `turn_ephemeral` | turn id / action revision |

## Parallelization Plan

建议并行分五条线：

| Lane | Focus | Depends On |
| --- | --- | --- |
| A | SPI/protocol and planner type design | none |
| B | Runtime-session planner integration and frame ordering | A draft |
| C | PiAgent consumption and memory reclassification | A draft |
| D | Frontend parser/render official order | A draft |
| E | continuation_context cleanup and compaction guard tests | none |

Lane A 输出最小协议草案后，B/C/D 可并行。E 可以立即开始，因为它主要是死链路清理和现役 compaction 保护。

## Risks

- `ContextFrame` 是 SPI 类型，新增字段会影响前端 generated/manual parser 与 tests。
- PiAgent 当前 system prompt 依赖本地 kind 列表，改造需要覆盖 connector tests。
- memory 从 system prompt 移出会改变模型行为，需要用精确测试确认它仍以动态 context 方式可见。
- 非 PiAgent connector 的 profile 需要明确声明 system 覆盖、追加、native consumption 或忽略能力，否则 `system` 语义容易再次泄漏为 PiAgent 的局部假设。
