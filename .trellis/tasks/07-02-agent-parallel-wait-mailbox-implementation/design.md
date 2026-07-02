# Agent 并行等待与 mailbox 回传能力设计

## Architecture

能力模型基于 AgentDashboard 自有事实源：

```text
spawn / exec / companion request
  -> LifecycleGate 或 wait record 记录等待事实
  -> event resolved
  -> wake adapter 写 AgentRunMailboxMessage
  -> scheduler 按 barrier/drain delivery 消费
  -> session Backbone / mailbox notification
  -> frontend workspace projection refresh
```

Codex 的参考价值是闭环语义，而不是协议形状：

- spawn/send/close 改变目标 agent/mailbox 状态。
- wait 等 activity/mailbox/state change，不搬运大结果。
- result 通过 mailbox/notification 返回等待方。

## Wait Owner

等待事实不放在 command receipt，也不放在 transient hook pending action 中。

- `LifecycleGate` 或同级 lifecycle wait record 是 durable wait owner。
- `AgentRunMailboxMessage` 是 wake/result envelope。
- `AgentRunCommandReceipt` 只做用户/API 命令幂等。
- `HookPendingAction` 只做 runtime injection、stop blocking 和 UI提示，可引用 wait owner id。

建议 wait owner 字段：

- `wait_id`
- `run_id`
- `agent_id`
- `runtime_session_id`
- `kind`: companion/subagent/human/exec/workflow
- `source_ref`
- `correlation_ref`
- `status`: open/resolved/cancelled/failed/expired
- `created_turn_id`
- `resolved_turn_id`
- `payload_summary`

若现有 `LifecycleGate` 足以表达某类等待，优先扩展 projection，不新建重复表。

## Wake Envelope

事件完成后由通用 wake adapter 写 mailbox：

- `origin`: companion/system/workflow。
- `source.namespace`: companion/exec/workflow/platform。
- `source.kind`: result/response/exec_result/cancelled/failed。
- `source_ref`: wait/gate/exec id。
- `correlation_ref`: request/dispatch id。
- `source_dedup_key`: 稳定来自 wait id + event kind，保证重试幂等。
- `delivery`: 默认 `LaunchOrContinueTurn`；仅明确 active-loop steering 时使用 `SteerActiveTurn`。
- `barrier`: idle 可 immediate；running 默认 AgentRunTurnBoundary；人工恢复为 ManualResume。
- `drain_mode`: result 默认 One。

不要把 companion/exec result 塞进 hook-only `ResumeLaunchSource`。

## Wait Tool Semantics

wait 是 activity watcher：

- 如果已有 pending mailbox/gate resolved activity，立即返回摘要和 refs。
- 如果未来 activity 到达，通知后返回摘要和 refs。
- 如果 timeout，返回 timed_out，不改变 durable result。
- 如果 target closed/cancelled，返回对应 status。

结果正文由 mailbox content/projection 查询。这样可避免 wait 工具成为大结果传输通道，也和 AgentRun mailbox 的 durable/recovery 模型一致。

## Scheduler And Runtime

- scheduler 继续作为 delivery authority，route 和 tool handler 不直接 launch/steer。
- runtime adapter 在 after-turn/before-stop/terminal fallback 继续触发 mailbox boundary drain。
- wake adapter 写入 mailbox 后必须发 mailbox state changed notification，等待方和前端都通过 durable query 看到一致状态。
- expired consuming 无 accepted refs 仍进入 `delivery_result_unknown` blocked，不自动重排。

## Frontend Projection

AgentRun workspace projection 增加 waiting items：

- open wait list：等待类型、来源 label、preview、created time、status、可操作项。
- mailbox result list：复用现有 mailbox row/source label。
- notification：继续消费 mailbox state changed、companion events，未来 exec wait event 也走同一刷新 plan。

UI 不直接消费 RuntimeSession command owner；所有 workspace 命令仍归 AgentRun routes。

## Migration

若新增 wait record 表，需要 PostgreSQL migration 和 repository trait；如果复用 LifecycleGate，仅增加 projection DTO 和必要索引/metadata 字段。预研阶段不做兼容 fallback，schema 按正确模型推进。

## Constraints

- 不引入 Codex runtime dependency。
- 不新增旧 Session 形态对外端点。
- 不让 RuntimeSession 重新成为 workspace command owner。
- 不用 route-local queue 替代 AgentRun mailbox。
