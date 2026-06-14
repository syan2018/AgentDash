# 收口 AgentRun mailbox 验收阻断项

## Goal

修复 `06-13-agentrun-session-queue-command-convergence` 归档后验收发现的 mailbox 阻断项，让 AgentRun mailbox 在命令幂等、claim recovery、hook delivery 收口和前端状态投影上达到可交付质量。

这次目标不是继续扩展 mailbox 能力，而是把已经切换到 mailbox 的控制面收紧：所有同属 mailbox 的用户可见 command 必须有稳定且可重复的 `client_command_id`；scheduler recovery 不能在崩溃窗口中重复投递；AgentRun-anchored hook delivery 必须遵守 stop-boundary/follow-up 契约；前端必须展示后端投影的 mailbox 状态而不是只显示一条预览文本。

## Confirmed Facts

- 旧 `PendingQueueService`、route-local `SendNext/Enqueue/Steer`、`accepted_receipt(...)`、旧 pending endpoint/DTO、`AgentRunPendingDispatcher` 已不再作为生产权威路径命中。
- 新增行数主要来自 durable mailbox 必要结构：application scheduler、domain mailbox model、Postgres repository、migration、runtime delegate 和 contract/frontend 投影。
- 当前阻断来自收口质量，而不是旧 pending 模型未删除：
  - promote/delete/resume 把 snapshot `command_id` 当 `client_command_id`，同类第二次操作会因 request digest 不同而冲突。
  - launch/steer 副作用先发生，`Dispatched/Steered` 后写回；expired `Consuming` 一律回到 `Queued`，存在重复投递窗口。
  - AfterTurn `follow_up` 被并入 `HookAfterTurn + AgentLoopTurnBoundary`，不符合 legacy follow-up 归一为 stop-boundary `ContinueOnStop` 的契约。
  - anchored HookAutoResume mailbox 写入失败会被 terminal effect 视为成功，导致 replay 丢失。
  - hook delivery dedup key 缺少 `session_id/turn_id/event_seq` 等事实字段，可追踪性不足。
  - 前端 mailbox row 未展示 `status/barrier/delivery`，也未使用根 `MailboxStateView` 的 pause message。
- `cargo check -p agentdash-api`、相关前端 typecheck/Vitest 已通过，但后端核心 scheduler/API mailbox 测试覆盖明显不足。

## Requirements

- 为 mailbox control commands 建立真正的命令幂等边界：
  - promote/delete/resume 请求必须携带调用级 `client_command_id`，不能复用 snapshot 的稳定 `command_id`。
  - 重复提交同一个 `client_command_id` + 相同 digest 必须 replay；同一 command kind 操作不同 message 必须使用不同 client id，不应互相冲突。
  - cancel 若继续作为 AgentRun 用户可见 command，必须纳入 durable command receipt 或被明确从 mailbox command receipt 契约中分层说明。
- 收紧 mailbox claim recovery：
  - 过期 `Consuming` 不能无条件恢复为 `Queued` 并重复 delivery。
  - recovery 必须优先识别已有 accepted refs/result，或把不确定 delivery 状态置为 `Blocked/Failed` 等需要显式处理的状态。
  - launch/steer delivery 的操作 id、accepted refs、receipt result 与 claim token 写回顺序必须可解释、可测试。
- 修正 hook delivery 收口：
  - legacy `follow_up` 必须归一为 `SteerActiveTurn { stop_effect=ContinueOnStop }`，并在 `AgentRunTurnBoundary` 消费。
  - `AfterTurn` 普通 steering 继续走 `AgentLoopTurnBoundary + DrainMode::All`。
  - Hook delivery `source_dedup_key` 必须包含 runtime session、turn/event facts 和 index，避免只用内容 digest 造成跨 turn 误去重。
  - anchored HookAutoResume mailbox envelope 创建失败时，terminal effect 不得标记成功，必须保留 replay 机会。
- 完善前端 mailbox 投影：
  - mailbox row 应展示后端投影的 `status/barrier/delivery` 关键信息。
  - mailbox pause banner 应消费根 `MailboxStateView.message/pause_reason/can_resume`，不能丢失后端 pause message。
  - 删除无实际行为的编辑/拖拽 affordance，避免误导用户。
- 补充聚焦回归测试：
  - command idempotency：delete/promote 多 message、resume duplicate replay。
  - scheduler/recovery：expired `Consuming`、accepted result replay、no duplicate launch/steer。
  - hook：AfterTurn steering、BeforeStop/follow_up stop-boundary、HookAutoResume failure replay。
  - frontend：mailbox row 状态展示、pause message、control command client id。

## Acceptance Criteria

- [ ] promote/delete/resume 的 request DTO 与前端调用携带独立 `client_command_id`；同一 mailbox 中对两条不同 message 执行同类操作不会发生 digest conflict。
- [ ] duplicate control command replay 返回已有 receipt/result，不重复 delete/promote/resume side effect。
- [ ] cancel command 的 receipt 边界被修复或在 spec/design 中明确证明不属于 mailbox command receipt 范围。
- [ ] expired `Consuming` recovery 不会盲目重复 launch/steer；有 accepted refs/result 的记录恢复到终态或 replayable result，不确定副作用状态进入可见 blocked/failed 状态。
- [ ] AgentRun-anchored `AfterTurn` steering 写入 `HookAfterTurn + AgentLoopTurnBoundary + DrainMode::All`。
- [ ] AgentRun-anchored legacy `follow_up` 写入 `HookBeforeStop/AgentRunTurnBoundary + ContinueOnStop` 或等价 stop-boundary continuation，不再提前按 AfterTurn internal boundary 注入。
- [ ] anchored HookAutoResume mailbox envelope 创建失败时 terminal effect 不标记 succeeded，并可由 outbox 重试。
- [ ] Hook delivery dedup key 包含 session/turn/event/index 事实，重复 replay 不重复创建 envelope，不同 turn 的同内容 delivery 不误折叠。
- [ ] mailbox row 展示 status/barrier/delivery 信息，pause banner 使用后端 pause message，编辑/拖拽空 affordance 移除。
- [ ] 后端新增或更新测试覆盖 mailbox control command 幂等、scheduler recovery、hook boundary 和 HookAutoResume failure replay。
- [ ] 前端 typecheck 与 mailbox 相关 Vitest 通过；后端 `cargo check -p agentdash-api` 与新增 targeted Rust tests 通过。

## Out of Scope

- 不重新设计 AgentRun workspace 页面布局。
- 不保留旧 pending endpoint/DTO 兼容路径。
- 不把非 AgentRun-owned runtime 的 hook direct delivery 强制迁入 mailbox。
- `draft start` 若仍是 ProjectAgent run bootstrap 路径，可暂不改为 mailbox envelope；若实现中发现它复用 mailbox command receipt 或会破坏同一幂等边界，则纳入修复。

## Open Questions

- 无需用户补充产品决策。建议按上述范围进入实现。
