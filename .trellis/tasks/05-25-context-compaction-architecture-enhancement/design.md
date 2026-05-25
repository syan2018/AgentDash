# 上下文压缩系统架构增强设计

## Design Goal

上下文压缩应成为 Session runtime 的一等 checkpoint 机制，而不是 agent loop 内部的临时消息裁剪。设计目标是把“何时压缩、压缩什么、如何恢复、如何展示、失败如何处理”拆成可组合边界，使后续可以继续吸收 Codex、Claude Code 或其它 Agent runtime 的策略，而不破坏 AgentDash 现有 Session / Bundle / Hook / Backbone 架构。

## Architecture Overview

建议引入三层模型：

```text
ContextPressureEvaluator
  -> CompactionStrategyPipeline
  -> CompactionCheckpointStore / ProjectedTranscript
```

### Agent Ownership Boundary

平台压缩系统只作用于 AgentDash 自己维护 canonical transcript 的 runtime，例如 Pi Agent / native AgentDash agent loop。Codex Bridge 等 connector 拥有自己的会话状态、压缩触发、恢复投影和 provider 特定约束，平台不应把这些内部历史纳入统一 checkpoint projection。

这个边界让 AgentDash 可以继续吸收 Codex 的 checkpoint 思路作为架构参考，同时不把 Codex Bridge 变成“双重压缩”。Bridge connector 的职责是把外部 runtime 已经确认的事件、usage、diagnostic 和最终可展示状态映射进 Backbone / ContextFrame，而不是被平台策略层裁剪其私有 transcript。

### ContextPressureEvaluator

职责：

- 估算即将发送给 provider 的有效请求大小。
- 汇总最近 provider usage、估算 usage、model context window、reserve tokens、connector capability、hook policy。
- 判断当前 connector 是否由平台拥有 transcript；非平台拥有的 connector 不进入平台压缩策略，只暴露事件映射和诊断。
- 输出 `ContextPressureDecision`：
  - `Noop`
  - `LightweightCleanup`
  - `SummaryCompaction`
  - `ReactiveRecovery`
  - `BlockWithDiagnostic`

决策需要携带：

- `phase`: `pre_turn | pre_provider | mid_turn | reactive_overflow | model_downshift | manual`
- `reason`: `context_limit | model_downshift | user_requested | provider_overflow | policy_requested`
- `budget_scope`: `total | body_after_prefix`
- `target_input_budget`
- `reserve_tokens`
- `estimated_request_tokens`
- `context_window`

### CompactionStrategyPipeline

职责：

- 按策略序执行上下文缩减。
- 每个策略输入 `ProjectedTranscript + RuntimeContextProjection + Budget`，输出新的 projection 与审计信息。
- 策略失败时返回 typed error，不允许写入成功 checkpoint。

初始策略建议：

1. `ToolResultMicrocompact`
   - 对超大工具结果、旧文件读取、shell 输出、媒体块做结构化缩减。
   - 产出可审计 replacement，不调用 LLM。

2. `SummaryPrefixCompaction`
   - 对 checkpoint 之后的历史前缀生成 handoff summary。
   - 保留由 token budget 决定的 recent tail。
   - 对 tool call / tool result、assistant message、user turn 边界做原子保留。

3. `CheckpointProjection`
   - 将 summary、retained tail、canonical context injection 组合成 replacement projection。
   - pre-turn / manual 可以不内嵌 bootstrap context，让下一轮 construction 重建。
   - mid-turn / reactive retry 必须注入当前 turn 所需 canonical context，保证同一 turn 继续执行。

后续可接入：

- `ContextCollapseCommit`：细粒度折叠多段历史，而不是一次大 summary。
- `ProviderRemoteCompaction`：由 provider 返回 compacted projection，但本地仍做规范化与 checkpoint 落库。
- `SessionMemoryCompaction`：把长期事实迁移到独立记忆层，checkpoint 只保留引用。

### CompactionCheckpoint

建议引入显式 checkpoint 结构，概念字段如下：

```rust
struct CompactionCheckpoint {
    checkpoint_id: Uuid,
    session_id: Uuid,
    created_at_ms: u64,
    phase: CompactionPhase,
    reason: CompactionReason,
    strategy: CompactionStrategyKind,
    budget_scope: CompactionBudgetScope,
    covered_until_ref: MessageRef,
    replacement_projection: ProjectedTranscript,
    summary: Option<String>,
    tokens_before: u64,
    tokens_after_estimated: u64,
    reserve_tokens: u64,
    context_window: u64,
    diagnostics: Vec<CompactionDiagnostic>,
}
```

落库形态可以是新的 session platform event，也可以是 session meta/checkpoint 表；关键是不再只靠 `messages_compacted` 推断边界。

## Session Branch And Repository Strategy

Codex 的 rollout 模型把 JSONL 当作事实日志，fork/rollback/resume 都通过 replay 还原。AgentDash 已经采用数据库仓储，应把同一类语义拆成三张事实形态：

- `session_events`：不可变审计日志，继续承载 Backbone / Platform feed。
- `session_checkpoints`：模型可恢复状态快照，用于 continuation、executor restore、fork base 和压缩恢复。
- `session_lineage`：会话分支拓扑索引，用于 parent/child、fork point、relation kind 和 edge status。

推荐 checkpoint 表字段：

```text
session_checkpoints
  checkpoint_id
  session_id
  created_event_seq
  covered_until_event_seq
  covered_until_ref
  base_checkpoint_id
  lineage_node_id
  status
  replacement_projection_json
  summary
  token_stats_json
  strategy
  phase
  created_at_ms
```

推荐 lineage 表字段：

```text
session_lineage
  child_session_id
  parent_session_id
  relation_kind
  fork_point_event_seq
  fork_point_ref
  fork_point_checkpoint_id
  status
  created_at_ms
  metadata_json
```

`session_lineage` 不替代 `session_bindings`：前者表达会话树和恢复继承关系，后者表达 session 归属于 project/story/task 等业务 owner。

Rollback 不应删除 `session_events`。推荐引入 active projection cursor：

```text
session_projection_heads
  session_id
  projection_kind
  head_event_seq
  active_checkpoint_id
  updated_by_event_seq
  updated_at_ms
```

这样 feed 仍然能展示完整审计历史，agent restore 只消费当前模型可见 projection。

恢复路径变为：

```text
active projection cursor
  -> latest valid checkpoint within cursor
  -> replay suffix after checkpoint and before active head
  -> ProjectedTranscript
```

Fork 路径变为：

```text
parent active projection at fork point
  -> create child session
  -> insert session_lineage edge with fork point
  -> child restore uses parent checkpoint/projection base + child suffix
```

这个设计比复制整段 parent event history 更适合数据库：事件保持不可变且可审计，branch 关系由索引表达，checkpoint 可以作为跨 fork 的稳定恢复基线。若需要让 child 完全独立于 parent retention，可以在 fork 时 materialize 一条 child initial checkpoint，把 parent fork projection 固化到 child 自己的 `session_checkpoints`。

## Data Flow

### Pre-provider path

该路径只适用于平台拥有 transcript 的 AgentDash native / Pi Agent runtime。

```text
AgentContext.messages
  -> evaluate_compaction(existing stats)
  -> transform_context
  -> estimate BridgeRequest(system + messages + tools)
  -> ContextPressureEvaluator
  -> CompactionStrategyPipeline
  -> persist checkpoint
  -> emit context_compacted + context_frame
  -> rebuild messages_for_llm from checkpoint projection
  -> provider request
```

当前代码先 compaction 后 transform_context；目标路径需要在最终 provider payload 可见后再做一次 pressure evaluation。可以保留 hook `BeforeCompact`，但它应参与 policy 决策，而不是唯一事实源。

### Mid-turn path

```text
assistant response / tool results appended
  -> token usage or estimate crosses budget
  -> compact with phase=mid_turn
  -> replacement projection includes canonical runtime context
  -> continue loop before draining unrelated pending input
```

mid-turn 压缩必须保持工具调用边界完整，不能压缩掉尚未配对的 tool call / tool result。

### Resume path

```text
persisted events
  -> active projection cursor
  -> latest valid CompactionCheckpoint within cursor
  -> replacement_projection
  -> replay suffix after covered_until_ref and before active head
  -> ProjectedTranscript
  -> connector restored_session_state or continuation ContextFrame
```

当前 `continuation.rs` 已有 `ProjectedTranscript` 和 `CompactionSummary` 投影，可在此基础上把 checkpoint 作为权威输入，而不是只读取 `context_compacted` summary payload。

## Hook And Bundle Boundaries

- Hook runtime 继续提供 `BeforeCompact` / `AfterCompact` 生命周期点。
- Hook 决策只表达 policy、custom prompt、custom summary、cancel、策略偏好，不直接替代 checkpoint store。
- `SessionContextBundle` 仍是 owner/workspace/VFS/tool/workflow context 主数据面。
- 压缩 projection 不应把 bundle bootstrap 内容长期复制成 user message；pre-turn 恢复由 construction 重新投影，mid-turn 才注入当前继续执行必需 context。
- Bridge connector 可以把外部 runtime 的压缩摘要或 diagnostic 映射成平台事件，但不能让平台 Hook 直接修改外部 runtime 的私有压缩决策。

## Backbone And ContextFrame Contract

成功压缩：

- 继续发送 `context_compacted`，但 payload 扩展为结构化字段：
  - `checkpoint_id`
  - `phase`
  - `reason`
  - `strategy`
  - `budget_scope`
  - `tokens_before`
  - `tokens_after_estimated`
  - `reserve_tokens`
  - `context_window`
  - `covered_until_ref`
  - `retained_message_count`
  - `summary`
- 继续生成 `ContextFrame(kind="compaction_summary")`。

失败压缩：

- 发送 hook trace 或 platform event，例如 `context_compaction_failed`。
- 不生成成功语义的 `compaction_summary` frame。
- 不替换 runtime messages，不写成功 checkpoint。

如果涉及跨层 DTO，必须同步 `agentdash-agent-protocol` TS 生成与前端模型。

## Failure Semantics

- `NoSummary`: 摘要为空，失败，不落地。
- `SummaryApiError`: 摘要内容是 API error 或 bridge error，失败，不落地。
- `Cancelled`: 用户或系统取消，失败，不落地。
- `CheckpointPersistFailed`: checkpoint 未写入，runtime history 不替换。
- `CompactionRequestTooLarge`: 对压缩请求自身做有限 retry；重试仍失败则保持原历史。
- `ConsecutiveFailuresExceeded`: 自动压缩熔断，只发诊断，不重复调用 summarizer。

## Migration Notes

项目处于预研阶段，可以直接引入目标 checkpoint schema，不做旧字段兼容。若新增持久化表或 event payload，需要通过 migration 明确收正，并同步 SQLite / PostgreSQL。

建议优先选择 `event + repository` 双写：`context_compacted` / `ContextFrame` 保持 UI 审计，`session_checkpoints` 作为 restore/fork/rollback 的查询事实源。branch-aware 字段应在 checkpoint schema 初版就出现，即使完整 session tree API 后续实现。

## Trade-offs

- checkpoint 持久化比单个 summary payload 更重，但能让 resume/fork/rollback 有稳定事实源。
- pre-provider 估算无法完全等于 provider 计费 token，但比只看上一轮 usage 更接近真实风险。
- lightweight cleanup 增加策略复杂度，但能减少昂贵 summary compaction 的频率，也保留更多细粒度上下文。
- `BodyAfterPrefix` 需要维护 baseline，但能避免 bootstrap context 造成重复压缩。
- 不接管 Codex Bridge 内部压缩会让平台 checkpoint 不覆盖所有 connector 的私有状态，但可以避免双重压缩、重复恢复投影和 provider 专属行为互相干扰；跨 connector 的统一性应体现在事件和可视化契约，而不是统一内部 transcript 管理。
- 数据库 lineage 比 Codex 复制 rollout history 更复杂，但能避免大段事件复制，也能让 fork point、rollback head、checkpoint 继承关系保持可查询。
- 如果 fork 时不 materialize child initial checkpoint，child 恢复会依赖 parent retention；如果 materialize，则写入更重，但 child 独立性更强。
