# AgentRun Mailbox 验收阻断项收口设计

## Design Thesis

这次修复的边界是 mailbox 控制面硬化，而不是新增调度模型。现有 mailbox 已经是 queued/paused/blocked/consuming 的事实源；问题在于几个边界还没有达到这个事实源应有的幂等和恢复质量。

设计目标：

- command receipt 只表达命令幂等，不能复用 snapshot command id 作为调用实例 id。
- mailbox message claim 只表达调度尝试，不能在不确定副作用状态时盲目重放。
- hook delivery 必须按 AgentRun mailbox 契约区分 AgentLoopTurn 和 AgentRunTurn boundary。
- frontend 只展示后端 mailbox projection，不发明 pending 状态。

## Command Idempotency

### Current Problem

`AgentRunCommandOnlyRequest` 只有 `command: AgentRunCommandPreconditionView`。delete/promote/resume route 把 `body.command.command_id` 当作 `client_command_id`，而该值是 snapshot 里的稳定 command kind，例如 `delete_mailbox_message`。

由于 mailbox receipt digest 包含 `message_id`，同一个 AgentRun 中删除第二条 message 会复用同一个 client id 但 digest 不同，产生错误 conflict。

### Target Contract

新增或替换为控制命令 request DTO：

```rust
pub struct AgentRunCommandRequest {
    pub command: AgentRunCommandPreconditionView,
    pub client_command_id: String,
}
```

语义：

- `command.command_id` 是 snapshot stale guard，只验证用户看到的 command 是否仍可用。
- `client_command_id` 是调用实例 id，用于 durable receipt。
- frontend 每次触发 promote/delete/resume/cancel 生成新的 `crypto.randomUUID()`。
- retry 同一次 HTTP 调用可复用同一 client id；用户第二次点击另一条 message 必须生成新 id。

Composer submit 已经有独立 `client_command_id`，保持不变。

Cancel command 也应使用该 request 形态。若 cancel 继续返回轻量 JSON，需要独立说明它的 receipt 不属于 mailbox message receipt；更推荐纳入 `AgentRunMessageCommandResponse` 或同构 command receipt response，避免继续违反“用户可见 AgentRun command 有 receipt”。

## Claim Recovery

### Current Problem

`recover_expired_consuming` 把所有 expired `Consuming` 恢复为 `Queued`。但 `consume_as_launch` / `consume_as_steering` 的副作用发生在 mailbox row 写回 `Dispatched/Steered` 之前。进程在副作用成功后崩溃时，recovery 会再次 delivery。

### Target Behavior

Recovery 必须按证据分层：

| Evidence | Recovery |
| --- | --- |
| status=`Consuming` 且 accepted refs / receipt result 已写入 | 恢复为 `Dispatched` 或 `Steered`，并保留 result replay |
| status=`Consuming` 且无 accepted refs，但 attempt_count > 0 且 lease expired | 转为 `Blocked` 或 `Failed`，错误说明 `delivery_result_unknown` |
| status=`Consuming` 且明确还未进入 delivery side effect | 可恢复为 `Queued` |

实现时优先使用现有字段，避免新迁移：

- delivery 前若可以先记录 deterministic operation marker，则 recovery 能识别是否已越过副作用边界。
- 若无法可靠区分，宁可转 `Blocked(delivery_result_unknown)`，由用户/后续 retry command 处理，也不要自动重复 launch/steer。
- completion 写回仍必须比较 `claim_token`。

如果实现发现现有 schema 无法表达最小安全状态，可追加 migration 增加 `delivery_started_at` 或 `delivery_operation_id`。项目未上线，不做兼容路径，但 migration 必须完整。

## Hook Boundary

### AfterTurn

AfterTurn 普通 `steering`：

```text
source=HookAfterTurn
barrier=AgentLoopTurnBoundary
delivery=SteerActiveTurn { stop_effect=None }
drain_mode=All
```

AfterTurn legacy `follow_up` 不应并入上述消息。`follow_up` 的契约是继续当前 loop 的 stop-boundary 语义，应该归一为 stop-boundary continuation，或在 delegate 层延迟到 BeforeStop 消费。

### BeforeStop / Follow-up

BeforeStop `steering` 与 legacy `follow_up`：

```text
source=HookBeforeStop
barrier=AgentRunTurnBoundary
delivery=SteerActiveTurn { stop_effect=ContinueOnStop }
drain_mode=All
```

BeforeStop delegate 先写 hook-origin mailbox envelope，再 drain AgentRunTurnBoundary。消费到 message 时返回 `StopDecision::Continue`，避免先 terminal 再 launch。

### Dedup Key

Hook delivery key 使用事件事实而不是纯内容摘要：

```text
hook_delivery:{source}:{runtime_session_id}:{turn_id_or_event_key}:{event_seq_or_digest}:{index}
```

目标不是永久全局去重，而是保证同一 hook replay 不重复创建 envelope，同时不同 turn 的同内容 hook message 不误折叠。

## HookAutoResume Replay

anchored HookAutoResume 的 terminal effect 执行必须区分三种结果：

| Route result | Terminal effect result |
| --- | --- |
| Routed | succeeded |
| NoAnchor | execute unanchored fallback; fallback 成功/失败决定 effect 状态 |
| Failed | failed/dead-letter according to terminal effect retry policy |

`request_hook_auto_resume` 目前返回 `bool` 只能表示限流 decision，不足以表达 mailbox route failure。目标是让 terminal effect executor 能收到错误并保留 outbox replay。

## Frontend Projection

`MailboxMessageRow` 使用 generated `MailboxMessageView` 字段：

- `status` 显示为短标签。
- `barrier + delivery` 显示为紧凑说明。
- `last_error` 在 failed/blocked 时显示。
- 根 `AgentRunWorkspaceView.mailbox` 或等价后端 pause state 提供 banner message；conversation snapshot 仍可携带 resume command。
- 删除无行为的编辑按钮和拖拽手柄。

前端不得重新声明 mailbox status union。

## Test Strategy

Backend tests should be small and targeted. This is not a full end-to-end suite.

- Application/service tests use in-memory/test repository support where available to cover command receipt and scheduler branching.
- Postgres repository tests cover claim recovery behavior.
- Hook runtime tests cover delegate boundary conversion.
- Frontend Vitest covers generated DTO consumption and visible state text.

## Rollback Shape

如果某个 slice 失败，回滚该 slice 的 contract/API/frontend 同步改动，保留已通过的 backend hardening slice。不要恢复旧 pending queue 或 route-local classifier。
