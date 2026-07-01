# 收束 ContextDeliveryPlan 与 PiAgent 上下文消费

## Goal

建立一条统一的 `ContextDeliveryPlan` 链路，用来描述每轮 `ContextFrame` 的正式消费顺序、缓存策略、Agent 可见性和前端展示顺序。该计划需要把“上下文是什么”和“某个 Agent 如何消费它”拆开，让 PiAgent 可以把少量 section 映射到 system prompt，也允许其它 Agent 通过显式 connector profile 声明 system 覆盖、native consumption、忽略或其它消费方式。

## Background

当前 ContextFrame 已经承担 identity、guidelines、memory、capability delta、assignment、pending action、compaction summary 等结构化上下文，但消费路径仍然分散：

- `ContextFrame` 协议只有 `delivery_channel`、`message_role`、`created_at_ms` 等局部字段，不能表达正式顺序、cache 层、或不同 Agent 的消费策略。
- `TurnPreparer` 通过 push 顺序组装 `context.turn.context_frames`，但 PiAgent system prompt 又在 connector 内按 kind 硬编码 `identity -> system_guidelines -> memory_context`。
- `memory_context` 当前声明为 `connector_context/system`，并被 PiAgent 拼进 system prompt；这和 memory 的动态发现、索引指针、可变性语义不匹配。
- 前端 `ContextFrameStream` 当前按事件/数组顺序展示，无法告诉用户“正式进入模型的顺序”。
- `continuation_context` 字段链存在，但 `build_continuation_context_frame` 无调用点，frame construction 当前写死 `continuation_context_frame: None`；`compaction_summary` 才是当前活跃的历史摘要/压缩恢复语义。
- 历史研究 `.trellis/tasks/archive/2026-05/04-29-session-context-builder-unification/research/context-injection-map.md` 已确认旧注入系统曾分散在 assembler、prompt pipeline、hook delegate、PiAgent connector 和 bridge 层，重复注入与隐式顺序是长期问题。

## Requirements

### R1. ContextDeliveryPlan 成为 ContextFrame 消费的正式计划

`ContextDeliveryPlan` 必须表达每个 ContextFrame 或 section 的：

- `delivery_phase`
- `delivery_order`
- `cache_policy`
- `cache_key` 或 `cache_revision`
- `model_channel`
- `agent_consumption`
- `frontend_label`

计划应由 runtime/session 层在准备 turn 时生成，并作为 PiAgent connector 和前端展示的共同事实来源。

### R2. Agent 消费策略和 ContextFrame 语义分离

同一个 ContextFrame 的语义不应该天然等同于 system/user message。`system` 是某类 Agent 的消费策略，而不是 frame 自身的身份。

PiAgent 可以把 `stable_system` / `session_policy` 中允许进入 system 的条目拼成 system prompt；其它 Agent 不应被默认归为 audit-only，而应由 connector profile 明确声明支持的消费能力，例如覆盖 system、追加 system、走 connector-native prompt surface、忽略或仅审计展示。

### R3. PiAgent system prompt 移除 memory_context

`memory_context` 应被归类为动态发现资源，与 skill、tool schema、MCP、VFS 等同属 discovered inventory。它通过 discovery digest 驱动更新，不再作为 PiAgent system prompt 顶层规则。

### R4. continuation_context 明确为清理项

`continuation_context` 不进入正式顺序。实现阶段必须选择以下路径之一：

- 删除 `continuation_context` 字段链、builder、前端解析与测试夹具。
- 若确认仍需要冷 transcript rehydrate，则改名为 `restored_transcript` 或 `resume_transcript` 并补齐真实 producer。

当前推荐路径是删除。

### R5. compaction_summary 保持为现役历史摘要机制

`compaction_summary` 是当前活跃的压缩后历史摘要。ContextDeliveryPlan 应为它分配清晰位置，并保持现有 compaction message / context frame / projection 语义可见。

### R6. 前端展示正式顺序

前端必须展示后端给出的正式顺序和分层，而不是事件到达顺序。用户应能看到每个 frame 的 phase、order、cache policy、model channel、Agent 消费结果。

### R7. 设计支持并行落地

实现应能拆分为协议/计划器、PiAgent 消费、memory/cleanup、前端展示、验证五条相对独立的工作线，降低跨层修改互相等待的时间。

## Proposed Delivery Phases

初始 phase 建议如下，具体命名可在设计评审中收敛：

| Phase | 示例内容 | 主要消费方式 |
| --- | --- | --- |
| `stable_system` | identity / PiAgent base prompt | PiAgent system；其它 Agent 按 adapter 策略 |
| `session_policy` | system_guidelines / user preferences / project guidelines | PiAgent system 或 connector-native policy |
| `run_state` | compaction_summary / restored runtime state | model context / system-compatible summary |
| `assignment` | assignment_context / runtime task fragments | model context |
| `discovered_inventory` | capability/tool/MCP/VFS/skill/memory deltas | dynamic model context 或 audit |
| `turn_runtime` | pending_action / hook turn-start / auto_resume | per-turn model context |

## Acceptance Criteria

- [ ] 新增或扩展协议能够表达 `ContextDeliveryPlan` 的 phase、order、cache、model channel 与 agent consumption。
- [ ] PiAgent system prompt 不再硬编码消费 `memory_context`。
- [ ] `memory_context` 被归类为动态发现资源，并具备 discovery digest / cache policy 语义。
- [ ] 前端 ContextFrame 展示按正式 plan 排序，并展示 phase/cache/channel 信息。
- [ ] `continuation_context` 被删除，或被重命名并补齐真实 producer；不能继续以半接入状态留在正式链路里。
- [ ] `compaction_summary` 继续作为活跃历史摘要展示与进入模型上下文。
- [ ] 现有 ContextFrame、PiAgent connector、runtime-session、前端 parser/render 测试被更新。
- [ ] 任务完成前有针对 PiAgent、至少一种支持 system 覆盖或 connector-native consumption 的非 PiAgent 策略的设计或测试覆盖。

## Out Of Scope

- 不重新设计 compaction 算法。
- 不改数据库字段，除非实现中发现必须持久化新的 plan 结构；若需要持久化，应补 migration。
- 不为旧接口保留兼容路径；项目处于预研阶段，按正确模型收束。

## Open Question

`ContextDeliveryPlan` 的 connector profile 应先支持哪些 consumption mode 需要在实现中收敛；已确认不能把非 PiAgent 默认简化为 audit-only，因为存在支持 system 覆盖或原生 system surface 的 Agent。
