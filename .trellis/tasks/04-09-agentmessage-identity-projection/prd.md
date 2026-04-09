# AgentMessage Identity And Projection Model

> 状态：planning
> 关联任务：`04-08-context-compaction`

## Goal

为 `AgentMessage` 体系补齐**稳定消息身份**与**投影语义边界**，使其既能继续作为 agent runtime 的统一消息 IR，又不会因为缺少 message ref / projection metadata 而限制后续的：

- context compaction checkpoint 精准边界
- session continuation / restore 稳定重建
- session tree / branch lineage 继承
- replay / debug / audit

## 背景

当前 `AgentMessage` 已经承担了多条主链路的统一消息模型：

- agent loop 内部上下文
- connector 到 LLM 的消息转换
- session restore 后的 projected messages
- compaction summary 注入后的模型窗口历史

这套模型足够轻量，也已经能支撑当前功能，但有一个核心短板：**消息没有稳定身份，只是带 role/content 的投影对象**。

当前的直接影响：

1. compaction checkpoint 只能先用 `messages_compacted` 数量表达边界，而不是更稳的 message ref
2. restore 时需要从事件流重新推断消息归并边界
3. `AgentMessage` 同时混合了“原始事实消息”和“投影消息”（如 `CompactionSummary`）
4. 后续如果要支持 branch-aware compaction，会缺少可复用的 lineage 锚点

## 当前判断

当前更合理的定位是：

- `PersistedSessionEvent` = 原始持久化事实
- `AgentMessage` = runtime / restore / compaction 使用的 projected message model

因此本任务不是推翻 `AgentMessage`，而是为它补一层**身份与投影上下文**。

## 设计目标

### 1. 稳定消息身份

需要引入稳定的 message reference，用于：

- compaction cut boundary
- restore 后消息对齐
- branch lineage 继承
- 调试时定位某条消息来自哪段事件

推荐方向：引入 envelope，而不是把所有 identity 字段直接塞进 `AgentMessage` 每个变体。

### 2. 原始事实与投影语义分层

需要显式区分：

- 原始 transcript 事实
- 从 transcript 计算出来的 projected messages
- 只存在于投影层的 summary / checkpoint 代表消息

### 3. 保持现有 runtime 简洁性

不希望把当前 `AgentMessage` 变成一个非常重的跨层 DTO。目标是：

- runtime 侧仍然能轻量使用
- connector 适配仍然直接
- 新增 identity 层后不破坏现有大部分调用方式

## 候选方案

### 方案 A：为 `AgentMessage` 直接加 identity 字段

示意：

```rust
pub enum AgentMessage {
    User {
        message_id: String,
        origin: MessageOrigin,
        content: Vec<ContentPart>,
        ...
    },
    ...
}
```

优点：

- 使用方最直接
- 不需要额外 envelope 解包

缺点：

- 每个变体都会变重
- 原始事实与投影语义容易继续混在一起
- 后续如果 identity / projection metadata 继续增加，enum 会持续膨胀

### 方案 B：引入 `AgentMessageEnvelope`（推荐）

示意：

```rust
pub struct AgentMessageEnvelope {
    pub message_ref: MessageRef,
    pub projection_kind: MessageProjectionKind,
    pub message: AgentMessage,
}
```

优点：

- `AgentMessage` 本体保持轻量
- identity / projection metadata 独立演进
- compaction checkpoint、restore、branch lineage 都能基于 `message_ref`

缺点：

- 现有使用点需要从 `Vec<AgentMessage>` 逐步升级到 `Vec<AgentMessageEnvelope>` 或在边界做适配
- 初期会多一层转换

### 方案 C：维持 `AgentMessage` 不变，只在 continuation / compaction 内部维护外部 key

优点：

- 代码改动最小

缺点：

- identity 仍然是隐式的、分散的
- 无法从根本上解决 checkpoint 边界与 lineage 稳定性问题
- 容易继续在不同模块重复发明 key 规则

## 推荐方向

推荐采用**方案 B**，即：

1. 保留 `AgentMessage` 作为轻量消息体
2. 新增 `AgentMessageEnvelope` / `MessageRef`
3. 将 compaction checkpoint 从“计数边界”升级为“message_ref 边界”
4. 明确 `CompactionSummary` 属于 projection message，而不是原始 transcript 事实

## Requirements

- `AgentMessage` 相关主链路需要具备稳定 message ref
- compaction checkpoint 需要能引用具体消息边界，而不是仅靠计数
- restore / continuation 需要产出带 identity 的 projected messages
- branch-aware compaction 需要可复用同一套消息引用模型
- 不要求一次性重写所有调用点，但需要定义清晰迁移路径

## Acceptance Criteria

- [ ] 形成 `AgentMessage` / `AgentMessageEnvelope` / `MessageRef` 的最终分层方案
- [ ] 明确 compaction checkpoint 如何从 `messages_compacted` 升级到 message ref 边界
- [ ] 明确 restore / continuation 如何生成稳定 ref
- [ ] 明确 connector / runtime / persistence 三层的适配边界
- [ ] 输出可执行迁移顺序，避免一次性大爆改

## 初步迁移路径

### P1

- 定义 `MessageRef`
- 定义 `AgentMessageEnvelope`
- 明确 `MessageProjectionKind`

### P2

- continuation / restore 先产出 envelope
- compaction checkpoint 改用 `compacted_until_ref`

### P3

- agent loop / runtime delegate 在内部逐步改用 envelope
- 需要时在 connector 边界降级回裸 `AgentMessage`

### P4

- 为 session tree / branching 接入 lineage-aware compaction

## Open Questions

1. `MessageRef` 的最小组成是什么？
   - 候选：`turn_id + entry_index + role`
   - 候选：独立 `message_id` + source event linkage

2. `CompactionSummary` 是否继续保留在 `AgentMessage` enum 内？
   - 推荐：短期保留
   - 长期可评估是否作为带 `projection_kind=compaction_summary` 的 envelope 消息

3. restore 是否应直接返回 `Vec<AgentMessageEnvelope>`？
   - 推荐：中期是
   - 短期可先保留 `Vec<AgentMessage>`，同时增加一条 envelope 版本链路

4. 是否需要为 reasoning / image 片段单独建立更细粒度 ref？
   - 当前建议：先只做 message-level ref，不做 content-part-level ref

## Out Of Scope

- 前端最终展示形态
- 历史 session 数据迁移
- 多 provider 专用 message schema
- 本轮直接重写全部 runtime 调用点

## Related Files

- `crates/agentdash-agent-types/src/message.rs`
- `crates/agentdash-agent-types/src/content.rs`
- `crates/agentdash-application/src/session/continuation.rs`
- `crates/agentdash-agent/src/compaction/mod.rs`
- `crates/agentdash-executor/src/connectors/pi_agent/rig_bridge.rs`

## Notes

- 当前 `04-08-context-compaction` 已经先用 `messages_compacted` 完成可工作的 checkpoint 投影
- 本任务用于追踪更长期、更稳固的消息身份模型升级，不阻塞当前 compaction 主任务收敛
