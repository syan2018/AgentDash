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
3. `AgentMessage` 同时混合了"原始事实消息"和"投影消息"（如 `CompactionSummary`）
4. 后续如果要支持 branch-aware compaction，会缺少可复用的 lineage 锚点

## 当前判断

当前更合理的定位是：

- `PersistedSessionEvent` = 原始持久化事实
- `AgentMessage` = runtime / restore / compaction 使用的 projected message model

因此本任务不是推翻 `AgentMessage`，而是为它补一层**身份与投影上下文**。

## 消费层分析

`AgentMessage` 跨 6 层使用（12 个源文件，~116 处引用），但**不是所有层都需要身份**：

| 层 | 需要 MessageRef？ | 需要 ProjectionKind？ | 说明 |
|----|---|---|---|
| Runtime Loop | 否 | 否 | 只关心消息内容和顺序 |
| LLM Bridge | 否 | 否 | 只做 AgentMessage → rig Message 转换 |
| Steering/Follow-up | 否 | 否 | 临时注入，生命周期极短 |
| **Compaction** | **是** | 部分 | cut boundary 需要精准引用 |
| **Restore/Continuation** | **是** | **是** | 对齐边界 + 稳定重建 + 区分原始/投影 |
| Stream Mapper/Persistence | 已有 | 不适用 | `turn_id` + `entry_index` 在 event 级别已存在 |

真正需要身份的消费者只有 **compaction** 和 **restore** 两个模块。

## 设计目标

### 1. 稳定消息身份

引入 `MessageRef`，用于：

- compaction cut boundary
- restore 后消息对齐
- branch lineage 继承
- 调试时定位某条消息来自哪段事件

### 2. 原始事实与投影语义分层

显式区分：

- 原始 transcript 事实
- 从 transcript 计算出来的 projected messages
- 只存在于投影层的 summary / checkpoint 代表消息

### 3. 保持现有 runtime 简洁性

- runtime 侧继续用 `Vec<AgentMessage>`，不引入 envelope 包装
- bridge / steering / follow-up 路径完全不受影响
- 身份层只在 compaction 和 restore 边界可见

## 方案评估

### 方案 A：为 AgentMessage 直接加 identity 字段

在每个 enum variant 上加 `message_ref: Option<MessageRef>`。

优点：
- 使用方最直接
- 现有 pattern match 都用了 `..`，加字段不破坏匹配

缺点：
- 语义混淆 — `AgentMessage` 同时承担内容载体和身份载体
- Runtime / Bridge / Steering 不需要 ref 却不得不携带
- 后续 identity / projection metadata 增加时 enum 持续膨胀

**结论：不采用。**

### 方案 B：全局 AgentMessageEnvelope 替换

将 `Vec<AgentMessage>` 全面替换为 `Vec<AgentMessageEnvelope>`。

优点：
- 概念最干净，身份与内容彻底分离

缺点：
- 迁移代价被严重低估：`AgentState.messages`、`AgentContext.messages`、`BridgeRequest.messages`、`TurnControlDecision.steering/follow_up` 全部需要改
- Bridge 根本不需要 ref，却被迫处理 envelope
- 不是"逐步迁移"能解决的 — 接口签名一改，上下游全要跟

**结论：不采用。过度设计。**

### 方案 C：各模块内部维护外部 key

优点：
- 代码改动最小

缺点：
- identity 仍然是隐式的、分散的
- 无法跨模块共享引用语义

**结论：不采用。但 `continuation.rs` 的 `restored_assistant_key` 模式值得复用。**

### 方案 D：分层身份模型（采用）

**不改 `AgentMessage` 本体，也不做全局 envelope 替换。** 在真正需要身份的两个边界（compaction + restore）各自用最自然的方式解决。

核心思路：
1. 定义共享的 `MessageRef` 类型
2. `CompactionSummary` variant 加 `compacted_until_ref` 字段（向后兼容）
3. 引入 `ProjectedTranscript` 作为 restore/compaction 专用的投影输出类型
4. Runtime / Bridge / Steering 路径完全不变

## 详细设计

### MessageRef

```rust
/// 消息稳定引用 — 对齐 PersistedSessionEvent 已有的 turn_id + entry_index
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MessageRef {
    pub turn_id: String,
    pub entry_index: u32,
}
```

设计决策：
- **组成为 `turn_id + entry_index`**，直接复用 `PersistedSessionEvent` 已有字段
- `continuation.rs` 的 `restored_assistant_key` 已经在用 `"assistant:turn:{turn_id}:{entry_index}"` 做去重，说明这对值天然稳定
- 不引入独立 `message_id` — 避免新的 ID 分配/同步开销

### CompactionSummary 升级

```rust
CompactionSummary {
    summary: String,
    tokens_before: u64,
    messages_compacted: u32,                     // 保留，向后兼容
    #[serde(default, skip_serializing_if = "Option::is_none")]
    compacted_until_ref: Option<MessageRef>,     // 新增
    timestamp: Option<u64>,
}
```

- `compacted_until_ref` 表示"此摘要覆盖了直到这条消息（含）的所有内容"
- Compaction 引擎优先用 `compacted_until_ref`，fallback 到 `messages_compacted` 计数
- `Option + default + skip_serializing_if` 保证已有 JSONL 数据向后兼容

### ProjectedTranscript

```rust
/// 从持久化事件重建的投影 transcript
pub struct ProjectedTranscript {
    pub entries: Vec<ProjectedEntry>,
}

pub struct ProjectedEntry {
    pub message_ref: MessageRef,
    pub projection_kind: ProjectionKind,
    pub message: AgentMessage,
}

pub enum ProjectionKind {
    /// 直接从原始 transcript 事件还原
    Transcript,
    /// 压缩摘要
    CompactionSummary,
}
```

这本质上是将 `continuation.rs` 现有的私有 `RestoredMessageEnvelope` 提升为公共类型，加上 `MessageRef` 和 `ProjectionKind`。

**命名决策：** 叫 `ProjectedTranscript` 而非 `AgentMessageEnvelope`，因为它不是要替代 `AgentMessage`，而是 restore 流水线的输出类型。

**使用范围：**
- `build_restored_session_messages_from_events` 返回 `ProjectedTranscript`
- `apply_compaction_checkpoint` 基于 `compacted_until_ref` 在 `ProjectedTranscript` 上做切割
- `CompactionResult` 可携带 `ProjectedTranscript`

**Runtime 注入时降级：**
```rust
let messages: Vec<AgentMessage> = transcript.entries
    .into_iter()
    .map(|e| e.message)
    .collect();
agent.replace_messages(messages);
```

### CompactionSummary 是否留在 AgentMessage enum 内

**保留。** 理由：
- LLM bridge 需要把它转成 `<summary>` user message 发给模型
- 如果从 enum 中拿出去，`agent_to_llm` 转换逻辑反而更复杂
- 用 `ProjectionKind::CompactionSummary` 在投影层区分语义即可

## Requirements

- `MessageRef` 类型定义，基于 `turn_id + entry_index`
- `CompactionSummary` variant 新增 `compacted_until_ref: Option<MessageRef>`
- compaction 引擎切换为 ref-based cut（fallback 到计数）
- restore / continuation 产出 `ProjectedTranscript`（带 ref + projection kind）
- compaction checkpoint 事件持久化时携带 `MessageRef`
- Runtime / Bridge / Steering 路径不变

## Acceptance Criteria

- [ ] `MessageRef` 类型定义并在 agent-types crate 导出
- [ ] `CompactionSummary` 新增 `compacted_until_ref` 字段，已有 JSONL 向后兼容
- [ ] `ProjectedTranscript` / `ProjectedEntry` / `ProjectionKind` 类型定义
- [ ] `build_restored_session_messages_from_events` 返回 `ProjectedTranscript`
- [ ] compaction cut boundary 从 `messages_compacted` 升级到 `compacted_until_ref`
- [ ] `context_compacted` 事件中持久化 `compacted_until_ref`
- [ ] Runtime loop / bridge / steering 代码无变化
- [ ] 已有 session 数据（无 `compacted_until_ref`）能正常 fallback

## 迁移路径

### P1：基础类型

- 定义 `MessageRef`
- 定义 `ProjectedTranscript` / `ProjectedEntry` / `ProjectionKind`
- `CompactionSummary` 加 `compacted_until_ref: Option<MessageRef>`

### P2：Restore + Compaction 核心升级

- `continuation.rs`：restore 产出 `ProjectedTranscript`，ref 从 event metadata 派生
- `compaction/mod.rs`：切换到 ref-based cut，fallback 计数
- `hook_delegate.rs`：`after_compaction` 构造和持久化 `compacted_until_ref`

### P3（未来，按需）：Runtime 携带 ref

- 只有当 session tree / branch-aware compaction 需要 runtime 内部引用具体消息时
- `AgentState.messages` 升级为 `Vec<ProjectedEntry>` 或类似结构
- Bridge 边界做 `map(|e| &e.message)` 降级
- 当前不需要

## 迁移影响评估

| 改动 | 影响文件 | 破坏性 |
|------|---------|--------|
| 定义 `MessageRef` | 新文件 in agent-types | 无 |
| `CompactionSummary` 加字段 | message.rs | `Option + default` 向后兼容 |
| `ProjectedTranscript` 类型 | 新文件 in agent-types | 无 |
| Restore 返回 `ProjectedTranscript` | continuation.rs + hub.rs | 局部重构，~2 文件 |
| Compaction 切换 ref-based cut | compaction/mod.rs + hook_delegate.rs | 局部重构，~2 文件 |
| Runtime / Bridge / Steering | **无变化** | **无** |

核心变更集中在 4 个文件，不触碰 runtime loop 和 bridge 层。

## Out Of Scope

- 前端最终展示形态
- 历史 session 数据迁移（通过 fallback 兼容）
- 多 provider 专用 message schema
- 全局 AgentMessageEnvelope 替换（明确不做）
- content-part-level ref（message-level 足够）
- P3 runtime 携带 ref（当前无具体场景驱动）

## Related Files

- `crates/agentdash-agent-types/src/message.rs` — AgentMessage 定义，CompactionSummary 升级
- `crates/agentdash-agent-types/src/content.rs` — ContentPart（不变）
- `crates/agentdash-application/src/session/continuation.rs` — restore 投影核心，RestoredMessageEnvelope 升级为 ProjectedTranscript
- `crates/agentdash-agent/src/compaction/mod.rs` — compaction 引擎，切换 ref-based cut
- `crates/agentdash-application/src/session/hook_delegate.rs` — after_compaction 事件构造
- `crates/agentdash-application/src/session/persistence.rs` — PersistedSessionEvent（turn_id + entry_index 来源）
- `crates/agentdash-executor/src/connectors/pi_agent/rig_bridge.rs` — LLM bridge（不变）

## Notes

- 当前 `04-08-context-compaction` 已经先用 `messages_compacted` 完成可工作的 checkpoint 投影
- 本任务用于追踪更长期、更稳固的消息身份模型升级，不阻塞当前 compaction 主任务收敛
- `continuation.rs` 已有的私有 `RestoredMessageEnvelope` + `restored_assistant_key` 模式验证了本方案的可行性
