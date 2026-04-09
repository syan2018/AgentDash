# Context Compaction 上下文压缩

> 状态：in_progress
> 参考：`references/pi-mono/packages/coding-agent/src/core/compaction/`

## Goal

为长会话引入**可持久化、可恢复、可策略化**的上下文压缩能力，使 session 在长对话下仍能：

- 稳定控制进入模型窗口的历史体积
- 保留完整原始 transcript 以便恢复、调试、审计
- 允许通过 Hook / preset / 配置自定义压缩触发策略与摘要策略
- 为后续 `session-tree-branching` 提供可继承的历史折叠基础

## 当前问题

当前工作区实现已经引入了 `CompactionSummary`、`ContextTokenStats` 和 Hook 扩展，但仍存在几个结构性缺口：

1. 压缩结果只改写一次请求内的局部 `context.messages`，没有成为稳定的 session 持久状态
2. `BeforeCompact` / `AfterCompact` 只完成了类型铺设，运行时尚未形成真实生命周期
3. `transform_context()` 同时承担“上下文注入”和“触发压缩”两类职责，导致重入与状态同步复杂
4. 还没有定义“哪些历史不再进入模型窗口”的正式投影规则
5. 迭代压缩时，旧 summary 与待摘要消息的边界尚未完全收紧，存在重复输入风险

## 设计原则

- **窗口投影优先**：我们不直接删除历史，而是维护“进入模型窗口的逻辑历史投影”
- **持久 transcript 保真**：原始 user / assistant / tool_result 事件保留，压缩是额外事件，不回写旧事件
- **策略与执行分离**：Hook / preset 决定“要不要压、怎么压”；agent runtime 负责“如何正确执行”
- **唯一 token 真相**：仅使用 LLM response usage data，不使用 chars/4 等启发式估算
- **Hook 可插拔**：默认策略通过 builtin preset 提供，而不是硬编码在 agent loop
- **branch-aware**：当前先按 session 维度设计，但数据模型要为后续 branch lineage 预留作用域

---

## 核心概念

### 1. 原始历史 vs 模型窗口投影

我们同时维护两类历史：

1. **原始历史（persistent transcript）**
   - 全量 user / assistant / tool_result 事件
   - 用于恢复、调试、审计、前端完整回看

2. **模型窗口投影（projected messages for LLM）**
   - 当前有效的 `CompactionSummary`
   - checkpoint 之后保留的最近 tail 消息
   - 本轮运行时临时注入的 hook context
   - 当前用户输入与工具定义

结论：**compact 后“不进入模型窗口”的不是被删除，而是被窗口投影规则排除。**

### 2. Compaction Checkpoint

一次压缩应落为一个明确的 checkpoint，用来声明：

- 哪一段历史已经被折叠为 summary
- 哪个 summary 是当前有效摘要
- 下一轮构建模型输入时应该从哪里开始保留 tail

建议通过一个新的 session event 表达，而不是修改旧 transcript：

```rust
pub struct ContextCompacted {
    pub session_id: String,
    pub branch_scope: Option<String>,
    pub compacted_until_message_key: String,
    pub summary_message: CompactionSummaryMessage,
    pub replaced_message_keys: Vec<String>,
    pub tokens_before: u64,
    pub messages_compacted: u32,
    pub timestamp_ms: i64,
}
```

其中 `compacted_until_message_key` 是投影边界的核心字段。它比数组下标稳定，适合恢复链路和后续 branch lineage 复用。

### 3. CompactionSummaryMessage

```rust
pub struct CompactionSummaryMessage {
    pub summary: String,
    pub tokens_before: u64,
    pub messages_compacted: u32,
    pub timestamp_ms: i64,
}
```

职责：

- 作为“已压缩历史”的逻辑代表进入模型窗口
- 在前端以压缩卡片展示
- 在 continuation / restore 时作为投影产物重建

---

## 目标行为

### Agent Loop 正确流程

发送每次 LLM 请求前，固定执行以下顺序：

```text
1. 读取最近一次 LLM usage -> ContextTokenStats
2. evaluate BeforeCompact
3. 如需压缩：
   - 规划 cut point
   - 生成或接受 summary
   - 持久化 ContextCompacted event
   - 更新当前 AgentState / projected messages
   - evaluate AfterCompact
4. evaluate transform_context()
5. 组装 projected messages + hook injections
6. 发送到 LLM
```

关键点：

- `transform_context()` 不再负责决定是否 compact
- compaction 在 `transform_context()` 之前完成
- compaction 结果必须影响本轮与后续 turn 的窗口投影

### 哪些内容不再进入模型窗口

当一个 `ContextCompacted` checkpoint 生效后：

- 被 `compacted_until_message_key` 覆盖的旧 user / assistant / tool_result 原文不再进入模型窗口
- 被新 summary 取代的旧 `CompactionSummary` 不再进入模型窗口
- 其他 session 的历史不进入模型窗口
- 其他 branch 的历史不进入模型窗口
- 上一轮临时 hook injection message 不持久化，也不进入后续模型窗口

### 哪些内容仍然进入模型窗口

- system prompt
- 当前有效的 `CompactionSummary`
- checkpoint 之后保留的 tail 消息
- 当前 turn 用户输入
- 当前 turn 动态 hook injection
- 当前工具定义

---

## 技术方案

### A. 生命周期拆分

#### `transform_context()` 回归单一职责

`transform_context()` 只负责：

- 根据当前 session snapshot 注入 hook context
- 注入 pending action steering / follow-up
- 对可发送给模型的消息做最终轻量改写

`transform_context()` 不再直接返回 `compaction_request`，避免：

- 双重执行导致 pending action 消费不一致
- 需要 cooldown 之类的防重入状态
- 压缩逻辑和注入逻辑相互耦合

#### 新增显式 compaction checkpoint

建议在 `AgentRuntimeDelegate` 或等价 runtime coordination 层新增独立方法：

```rust
pub trait AgentRuntimeDelegate {
    // ...已有...

    async fn evaluate_compaction(
        &self,
        input: EvaluateCompactionInput,
        cancel: CancellationToken,
    ) -> Result<CompactionDecision>;

    async fn notify_compaction_result(
        &self,
        result: CompactionExecutionResult,
        cancel: CancellationToken,
    ) -> Result<()>;
}
```

也可以不直接扩 trait，而是在 application runtime 内部新增独立 coordinator。关键不是接口名字，而是**compaction 生命周期不能再隐含在 `transform_context()` 里**。

### B. 策略层与执行层分离

#### 策略层：Hook / preset / config

策略层负责决定：

- 当前是否应该 compact
- `reserve_tokens`
- `keep_last_n`
- `custom_summary`
- `custom_prompt`
- 是否跳过本次压缩

建议保留 builtin preset：

- `context_compaction_trigger`
- trigger 改为 `BeforeCompact`
- 默认逻辑：`last_input_tokens > context_window - reserve_tokens`

#### 执行层：agent compaction engine

执行层负责：

- 读取 projected transcript
- 规划 cut point
- 识别当前有效 summary
- 生成 summary input
- 调用 LLM 生成摘要
- 产生 `ContextCompacted` event
- 将结果应用到当前内存态与后续 restore 投影

### C. 数据结构建议

#### Token 统计

```rust
pub struct ContextTokenStats {
    pub last_input_tokens: u64,
    pub last_output_tokens: u64,
    pub context_window: u64,
    pub last_updated_ms: Option<i64>,
}
```

#### 策略层决策

```rust
pub struct CompactionDecision {
    pub should_compact: bool,
    pub reserve_tokens: u64,
    pub keep_last_n: u32,
    pub custom_summary: Option<String>,
    pub custom_prompt: Option<String>,
}
```

注意：`previous_summary` 不应由策略层返回，它属于执行上下文，应由 compaction engine 从 projected history 中提取。

#### 执行规划

```rust
pub struct CompactionPlan {
    pub summary_message_key: Option<String>,
    pub messages_to_compact: Vec<ProjectedMessageRef>,
    pub compacted_until_message_key: String,
    pub messages_to_keep: Vec<ProjectedMessageRef>,
}
```

### D. 窗口投影规则

建议把“构建发给模型的消息列表”收敛成一个单独模块：

```rust
pub fn build_projected_messages(
    raw_transcript: &[PersistedSessionEvent],
    checkpoints: &[ContextCompacted],
) -> Vec<AgentMessage>
```

规则：

1. 先恢复原始 transcript
2. 应用最新有效 checkpoint
3. 产出一个逻辑上的 `CompactionSummary`
4. 保留 checkpoint 之后的 tail
5. 再在 runtime 阶段追加当前轮 hook injection

### E. 迭代压缩规则

迭代压缩时：

- 当前有效 summary 仅通过 `previous_summary` 单独喂给摘要模型
- 已有 summary 不再混入 `messages_to_compact` 文本序列列化
- 进入模型窗口的永远只保留**最新有效 summary**

这条规则用于避免：

- 旧 summary 重复进入 prompt
- summary 逐轮膨胀
- 压缩前后边界不清晰

---

## Hook 设计

### 新增 HookTrigger

```rust
pub enum HookTrigger {
    // ...已有...
    BeforeCompact,
    AfterCompact,
}
```

### BeforeCompact 输入

```json
{
  "token_stats": {
    "last_input_tokens": 45000,
    "last_output_tokens": 2200,
    "context_window": 64000
  },
  "message_count": 85,
  "has_existing_summary": true,
  "default_decision": {
    "reserve_tokens": 16384,
    "keep_last_n": 20
  }
}
```

### BeforeCompact 输出

```json
{
  "compaction": {
    "cancel": false,
    "reserve_tokens": 16384,
    "keep_last_n": 20,
    "custom_summary": null,
    "custom_prompt": null
  }
}
```

### AfterCompact 输入

```json
{
  "tokens_before": 45000,
  "messages_compacted": 65,
  "summary_length": 1200,
  "used_custom_summary": false,
  "compacted_until_message_key": "assistant:turn:t-42:3"
}
```

---

## 持久化与恢复

### 推荐方案

推荐新增 `ContextCompacted` session event，而不是物化覆盖 message snapshot。

原因：

- 更符合当前 session event / continuation 的投影模型
- 易于回放、调试、审计
- 不会破坏现有 transcript 事件语义
- 后续 branch lineage 继承更自然

### 恢复链路要求

`build_restored_session_messages_from_events()` 需要扩展为：

1. 识别 transcript 事件
2. 识别 `ContextCompacted`
3. 计算最新有效 checkpoint
4. 输出 projected messages，而不是简单拼接全量 transcript

### Agent 内存态要求

当前 run 中的 `AgentState.messages` 必须与 projected messages 保持一致，否则会出现：

- 本轮请求已压缩
- 下一轮继续运行又从未压缩历史出发

因此 compaction 执行成功后，必须同步更新：

- 当前 agent 内存态
- session persistence
- 后续 continuation 投影结果

---

## Requirements

- 支持基于真实 token stats 的自动 compaction 检查
- 支持通过 Hook / preset / config 定义 compaction 策略
- compaction 结果必须成为持久状态，而非一次请求内的临时改写
- session restore 后，模型输入仍应遵守相同窗口投影规则
- 迭代 compaction 时只保留最新有效 summary 进入模型窗口
- `BeforeCompact` / `AfterCompact` 必须具备真实运行时调用点
- `transform_context()` 不再承担 compaction 调度职责
- 实现需要为未来 branch-aware compaction 预留作用域字段

## Acceptance Criteria

- [ ] 长会话在触发 compaction 后，下一轮请求不再重新带入已折叠的旧 transcript
- [ ] session 重启或 restore 后，模型窗口投影与 compaction 前一致
- [ ] `BeforeCompact` preset 可以关闭、覆盖 `keep_last_n`、覆盖 summary prompt
- [ ] `AfterCompact` hook 能收到真实执行结果载荷
- [ ] 迭代压缩时不会把旧 summary 作为普通待压缩消息再次输入摘要模型
- [ ] hook injection 仍仅影响当前请求，不被误持久化为 transcript
- [ ] 新增的 compaction 测试覆盖 agent loop、hook lifecycle、continuation restore 三条主链路

## Out of Scope

- 手动 `/compact` 命令与前端按钮
- 前端压缩卡片的详细交互样式
- 多模型/多 provider 差异化 prompt 优化
- branch tree 完整产品能力
- 已经落库旧 session 的历史迁移脚本

---

## 实施分期

| 阶段 | 内容 | Crate | 优先级 |
|------|------|-------|--------|
| P1 | 从 `transform_context()` 中拆出 compaction 生命周期 | agent-types, agent, application | 本次 |
| P2 | 定义 `ContextCompacted` event 与 continuation 投影规则 | application | 本次 |
| P3 | 重构 compaction engine：plan / summarize / apply 分层 | agent | 本次 |
| P4 | 打通 `BeforeCompact` / `AfterCompact` 真实运行时调用 | spi, application | 本次 |
| P5 | agent 内存态、session persistence、restore 三处状态对齐 | agent, application | 本次 |
| P6 | 增加 compaction 专项测试 | agent, application, executor | 本次 |
| P7 | 手动触发入口与前端渲染 | application, frontend | 后续 |

---

## Decision (ADR-lite)

**Context**

当前实现已经证明“只在当前请求中临时替换消息”不够稳固；要同时满足策略灵活性与状态正确性，必须让 compaction 成为显式生命周期和持久化事实。

**Decision**

- 采用“原始 transcript + 窗口投影”的双层模型
- 采用显式 compaction checkpoint，而不是直接覆盖旧 transcript
- 将 compaction 从 `transform_context()` 生命周期中拆出
- 保留 Hook 驱动的策略灵活性，但执行和持久化由 runtime 统一负责

**Consequences**

- 需要改造 session continuation / restore 链路
- 需要新增一类 session event 或等价 checkpoint 表达
- 运行时结构更清晰，后续手动 compact、branch-aware compaction 和 UI 展示都会更自然

---

## 待确认问题

以下问题当前有推荐方向，但仍建议实现前与你确认：

1. **压缩持久化表达**
   - 推荐：新增 `ContextCompacted` session event
   - 备选：维护一份独立 materialized projected snapshot
   - 影响：前者更适合 replay/debug，后者实现路径可能更短但更容易与事件历史分叉

2. **checkpoint 作用域字段**
   - 推荐：现在先写成 `branch_scope: Option<String>` 预留字段，当前未启用 branch 时填空
   - 影响：可以避免未来 session-tree-branching 再次改 compaction 数据模型

3. **前端历史展示语义**
   - 推荐：前端默认展示 `CompactionSummary` 卡片，原始被压缩 transcript 仍可从完整事件视图恢复，但不在普通会话视图展开
   - 影响：关系到用户看到的是“历史被折叠”还是“历史被替换”

4. **摘要模型选择**
   - 推荐：默认沿用当前 session 模型，保留后续切 `compaction_model` 的扩展点
   - 影响：先减少模型矩阵复杂度，但超长摘要场景下成本未必最优

5. **旧 session 兼容策略**
   - 推荐：本任务不做数据迁移，仅保证新 compaction event 生效；旧 session 继续按现有 restore 逻辑处理
   - 影响：实现简单，但需要接受“新旧 session 行为不同”的过渡期

---

## Technical Notes

- 当前工作区已有 `CompactionSummary`、`ContextTokenStats`、Hook 枚举扩展和默认 preset 雏形
- 当前主要结构性风险在于：压缩未持久化、`BeforeCompact/AfterCompact` 没有真实调用点、窗口投影规则尚未落地
- 后续实现时应优先补齐测试：
  - agent loop compaction 流程测试
  - hook lifecycle 测试
  - continuation / restore 投影测试
