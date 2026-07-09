# 手动上下文压缩实际执行链路收敛设计

## Current Findings

数据库现象表明，手动 compact-only request 已经创建并被 maintenance turn 消费，但该 turn 只发出了 `context_compaction_noop`，reason 是 `no_eligible_messages`，没有任何 projection commit。该 payload 是 maintenance turn 的 system delivery 事件，不是压缩结果。

代码链路中有三处会把不同问题折叠到同一个表象：

- `AgentRunContextCompactionSessionRuntimePort::launch_compact_only_turn` 发起 `LaunchSource::ContextCompaction` 后只观察 request 750ms。若 request 状态变为 `Noop`，命令 receipt 直接记录 `NoEligibleMessages` 且 `turn_id = null`。
- `agent_loop::agent_loop_compact_only` 不追加 prompt，只拿当前 `AgentContext` 跑 `run_compaction_preflight`。因此维护轮能否压缩完全取决于 connector 传入的 `messages/message_refs`。
- `compaction::should_execute_compaction` 返回 bool，将“消息不足”、“messages/refs 长度不一致”、“boundary ref 缺失”、“first kept ref 缺失”都折叠成 false；上层统一发 `no_eligible_messages`。

`context-compaction-projection.md` 已定义目标契约：模型输入由 `ContextProjector` 从 durable facts 构建；`MessageRef` 是 runtime 输入和持久化 transcript 的共同坐标；成功 compact 必须携带显式边界并提交 projection checkpoint。

## Design Goals

本次修复把手动 compact-only 当成一次结构性 runtime maintenance turn，而不是普通 system delivery prompt。它需要先 materialize 当前模型上下文，再执行同一套 compact eligibility 与 summary flow。

参考 `references/codex` 与 `references/claude-code` 后，本任务采用的核心原则是：compaction 是一个受管 agent lifecycle，不是普通 prompt 的副作用。Codex 用 `Op::Compact -> CompactTask -> ContextCompactionItem -> CompactedItem(replacement_history)` 表达这一点；Claude Code 用 `/compact -> compact_start/end -> compact_boundary` 表达这一点。AgentDash 已经有 lifecycle item 与 projection checkpoint，因此不引入额外 window chain，只把现有边界串成一致状态机。

状态表达按以下语义收敛：

- `completed`: 生成 summary，提交 compaction record、segments、projection head。
- `noop`: 上下文完整且规则判断当前没有可压缩前缀。
- `failed`: 上下文恢复、引用边界、summary provider、projection commit 或 cancel/abort 导致无法完成结构性压缩。

## Lifecycle Boundary

手动 compact-only 的状态机应明确为：

```text
requested
  -> maintenance_turn_launched
  -> context_materialized
  -> eligibility_checked
  -> compact_item_started
  -> summary_generated
  -> projection_committed
  -> compact_item_completed
  -> request_completed
```

异常路径：

```text
requested
  -> maintenance_turn_launched
  -> context_materialized
  -> eligibility_checked(no_eligible_messages)
  -> compact_item_noop
  -> request_noop

requested
  -> maintenance_turn_launched
  -> restore_or_eligibility_or_summary_or_commit_failed
  -> compact_item_failed
  -> request_failed
```

AgentDash 不需要新建 compaction run 表；manual request id、maintenance turn id、lifecycle item id 和 committed compaction id 共同组成可诊断坐标。成功边界是 projection checkpoint commit，不是 summary 文本生成，也不是 `ContextCompacted` marker 本身。

## Proposed Changes

### 1. Compact-only launch restore

在 session launch planning 中为 `LaunchSource::ContextCompaction` 明确选择 compact maintenance restore 策略：

- 如果存在 live Pi runtime，继续使用 live runtime state。
- 如果没有 live runtime 且 session 有历史事件，强制走 `RepositoryRehydrate(ExecutorState)`，即使 `RuntimeTraceLaunchState.executor_session_id` 存在。
- restore 得到的 `RestoredSessionState` 必须包含非空 messages，并且 refs 数量与 messages 数量一致；否则在 launch/preflight 阶段生成 failed diagnostic。

原因：compact-only 不会请求 provider 主回答，也不应依赖外部 follow-up session。它需要 AgentDash 自己的 durable model context 和 `MessageRef` 坐标来提交 projection。

### 1.5 Compact-only terminal semantics

`run_compaction_preflight` 需要返回 compaction preflight outcome，而不只是 context window numbers。普通 provider turn 可以把 auto-compaction failure 表达为 failed lifecycle item 后继续主请求；compact-only maintenance turn 中，compaction 就是本 turn 的全部目的，因此 invalid input、summary failure、projection commit failure 和 cancel/abort 应让 maintenance turn 进入失败终态。

建议形态：

```rust
pub enum CompactionPreflightOutcome {
    NotRequested,
    Noop { reason: String },
    Completed,
    Failed { reason: String },
}
```

`agent_loop_compact_only` 消费该 outcome：

- `Completed` / `Noop` / `NotRequested` 可正常 `AgentEnd`。
- `Failed` 返回 `AgentError`，让 session terminal diagnostic 与 request failed 对齐。

普通 `stream_assistant_response` 可继续沿用现有“compaction failed 不必终止 provider turn”的策略，除非错误是 cancel。

### 2. Eligibility diagnostic type

用结构化结果替代 `should_execute_compaction(...) -> bool`：

```rust
pub enum CompactionEligibility {
    Eligible,
    NoEligibleMessages { message_count: usize, keep_last_n: u32 },
    InvalidInput { reason: CompactionEligibilityFailure, message_count: usize, ref_count: usize },
}

pub enum CompactionEligibilityFailure {
    MessageRefLengthMismatch,
    CompactedUntilRefMissing,
    FirstKeptRefMissing,
}
```

`find_cut_point <= start_index` 或消息数量不足归类为 `NoEligibleMessages`。refs 数量或边界问题归类为 `InvalidInput`。`execute_compaction` 保留内部校验，作为最后防线。

`run_compaction_preflight` 根据分类处理：

- `Eligible`: 继续 `execute_compaction`。
- `NoEligibleMessages`: 发 `ContextCompactionNoop` 并调用 `after_compaction_noop`。
- `InvalidInput`: 发 `ContextCompactionFailed` 并调用 `after_compaction_failed`，错误 code 使用稳定 reason，例如 `compaction_message_ref_len_mismatch`、`compaction_boundary_ref_missing`、`compaction_first_kept_ref_missing`。

原因：eligibility 是压缩业务判断，refs 完整性是 runtime 不变量。两者必须在 request lifecycle 中分开。

### 3. Manual request finalization

`ManualContextCompactionDelegate` 已经负责把 manual noop/failed/completed 写回 request；需要补齐 consumed request 的异常终结语义：

- `after_compaction_failed` 对 manual request 始终写 failed，包括 cancel/abort；metadata 中保留 `reason`、`lifecycle_item_id`、`error`、`metadata`。
- 如果需要区分用户取消，可在 result metadata 中记录 `reason = "cancelled"`，当前 domain 状态仍可用 `failed` 表示本次请求没有完成结构性 compact。
- compact-only turn 若在 preflight 前就发现 restore state 无效，应通过相同 failed path 写 request。

原因：request 已经被 consumed 后必须进入终态，否则 command receipt、session terminal diagnostic 和 request repository 会分裂。

### 4. Command receipt semantics

维护轮 launch 成功后，command receipt 应始终记录 maintenance `turn_id`。短轮询只用于在极快终结时补充当前状态，不应把 turn id 清空。

结果建议：

- request `Completed`: outcome `LaunchedCompactionTurn` 或新增 `Completed` 结果，带 `turn_id` 和 `request_id`。
- request `Noop`: outcome `NoEligibleMessages`，带 `turn_id`、`request_id` 和 noop reason。
- request `Failed`: outcome `Failed`，带 `turn_id`、`request_id` 和 failed reason。
- request `Consumed` 或仍 `Requested`: outcome `LaunchedCompactionTurn`，带 `turn_id`。

原因：手动命令触发的是一个 runtime maintenance turn。即使 turn 极快完成，用户和诊断工具也需要同一个 turn id 追踪事件。

### 4.5 Avoiding over-design

不采用的参考实现复杂度：

- 不引入 Codex 的 window id chain；AgentDash 的 projection version/head 已经承担 checkpoint identity。
- 不引入 Claude Code 的 microcompact/snip/context-collapse 多策略栈；本任务只收敛 summary-prefix structural compact。
- 不新增 parallel compaction run store；manual request repository 已经是命令级 lifecycle record。
- 不让 UI 通过 system delivery payload 推断结果；结果事实只来自 request state、lifecycle event 和 projection commit。

### 5. Tests

新增或调整以下测试层级：

- `agentdash-agent` compaction eligibility 单元测试：refs 长度不一致、boundary ref missing、first kept ref missing、true no eligible。
- `agentdash-agent` preflight 测试：manual metadata 下 invalid input 发 failed 而不是 noop。
- `agentdash-application-runtime-session` launch planner 测试：`LaunchSource::ContextCompaction` 在无 live runtime、有历史事件、有 executor follow-up metadata 时仍恢复 executor state。
- `agentdash-application-runtime-session` manual delegate 测试：cancel/invalid input 会 mark failed。
- 应用层成功集成测试：历史事件 -> projected transcript -> compact-only turn -> context_compacted -> projection commit -> request completed。

## Open Decisions

- 是否为 `AgentRunContextCompactionOutcome` 增加 `CompletedCompaction`，让同步完成的 compact-only 命令在 receipt 中直接表达完成，而不是复用 `LaunchedCompactionTurn`。
- 是否为 manual request domain status 增加 `cancelled`。当前设计先用 `failed + reason=cancelled`，因为这次需求核心是避免 consumed/noop 误报。
