# 执行计划

## Start Gate

当前任务仍处于 `planning`。进入实现前需要：

- 用户 review 并认可 mailbox/barrier 设计。
- 用户 review `current-state.md`，确认 cut-over gates 覆盖应切除的旧线条。
- 确认是否在本任务内把 public endpoint 名称从 pending 改为 mailbox。
- 执行 `python ./.trellis/scripts/task.py start 06-13-agentrun-session-queue-command-convergence`。

## Implementation Slices

### 1. Terminology, Protocol Alignment, And Mechanical Rename Baseline

Goal: 先把控制面词汇、Codex protocol 对齐关系和 mailbox/barrier 类型位置立起来，并批量重命名会继续制造歧义的旧事实。

Likely files:

- `crates/agentdash-contracts/src/workflow.rs`
- `packages/app-web/src/generated/workflow-contracts.ts`
- `.trellis/spec/backend/session/runtime-execution-state.md`
- `.trellis/spec/frontend/state-management.md`

Steps:

- 引入 `MailboxMessageView`、`MailboxMessageStatus`、`MailboxMessageOrigin`、`MailboxDelivery`、`SteeringStopEffect`、`ConsumptionBarrier` DTO。
- 引入 `MailboxDrainMode` DTO，明确 `one` 与 `all` 的消费数量策略。
- 引入或重命名目标词汇：
  - `AgentRunThread`
  - `AgentRunTurn`
  - `AgentLoopTurn`
  - `AgentLoopTurnBoundary`
  - `AgentRunTurnBoundary`
- 将 API response outcome 命名为 scheduler outcome：`launched | queued | steered | deleted | resumed | blocked | failed`。
- 保留 command receipt DTO，但从语义上只表示 command idempotency。
- 更新相关 spec，明确 AgentRunThread、AgentRunTurn 与 AgentLoopTurn 的命名边界，并写明控制面必须映射到 Codex app-server protocol 的 thread/turn 原语。
- 对旧 `pending` / route-local command kind / `TurnExecution` 命名做批量机械迁移，不保留兼容 alias，除非某个现存代码事实还没有完成语义迁移。

Validation:

```powershell
cargo check -p agentdash-contracts
```

### 2. Storage: AgentRun Mailbox

Goal: 建立 durable mailbox repository，替代 `PendingQueueService(HashMap)` 作为权威状态。

Likely files:

- `crates/agentdash-domain/src/workflow/*`
- `crates/agentdash-application/src/repository_set.rs`
- `crates/agentdash-infrastructure/src/persistence/postgres/mod.rs`
- new `agent_run_mailbox_repository.rs`
- `crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql`
- `crates/agentdash-infrastructure/src/migration.rs`
- `crates/agentdash-api/src/bootstrap/repositories.rs`

Steps:

- 新增 domain records：
  - `AgentRunMailboxMessage`
  - `MailboxMessageOrigin`
  - `MailboxMessageSource`
  - `MailboxDelivery`
  - `ConsumptionBarrier`
  - `MailboxDrainMode`
  - `MailboxMessageStatus`
  - `AgentRunMailboxState`
- 新增 repository trait：
  - create envelope
  - list by run/agent/runtime
  - create envelope idempotently by `source_dedup_key`
  - claim next eligible candidate
  - recover expired consuming claims
  - mark consuming/dispatched/steered/failed/deleted
  - pause/resume state
  - cleanup user-origin payload
- 新增 PostgreSQL migration 和 repository 实现。
- 将 `PendingQueueService` 改成 mailbox-backed facade 或直接替换为 `AgentRunMailboxService`。

Validation:

```powershell
cargo test -p agentdash-domain mailbox
cargo test -p agentdash-infrastructure mailbox
```

### 3. Command Receipt Generalization

Goal: receipt 从 delivery-only 改为 AgentRun command 幂等层，不再承载 message lifecycle。

Likely files:

- `crates/agentdash-domain/src/workflow/command_receipt.rs`
- `crates/agentdash-application/src/workflow/command_receipt.rs`
- `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_delivery_command_receipt_repository.rs`
- `crates/agentdash-application/src/workflow/agent_message.rs`
- `crates/agentdash-application/src/workflow/project_agent_run_start.rs`

Steps:

- 将 `agent_run_delivery_command_receipts` 命名与字段扩展为 `agent_run_command_receipts`。
- 增加 `command_kind`、`mailbox_message_id`、`result_json`。
- 保留 request digest conflict 行为。
- duplicate command 通过 receipt result 或 mailbox message 状态返回稳定响应。
- `AgentRunMessageService` 仍可复用 receipt，但它不再是唯一拥有 receipt 的路径。

Validation:

```powershell
cargo test -p agentdash-application agent_message
cargo test -p agentdash-infrastructure agent_run_command_receipt
```

### 4. Mailbox Scheduler

Goal: 把 route-local `send_next/enqueue/steer` 分支收敛到 scheduler。

Likely files:

- new `crates/agentdash-application/src/workflow/agent_run_mailbox.rs`
- `crates/agentdash-application/src/workflow/agent_message.rs`
- `crates/agentdash-application/src/workflow/agent_steering.rs`
- `crates/agentdash-api/src/agent_run_pending.rs`
- `crates/agentdash-api/src/routes/lifecycle_agents.rs`

Steps:

- 新增 `AgentRunMailboxService`：
  - `accept_user_message`
  - `accept_hook_message`
  - `accept_system_message`
  - `promote_message`
  - `delete_message`
  - `resume_mailbox`
  - `schedule`
- `schedule` 读取 `SessionExecutionState` 并按 barrier 判断：
  - idle/terminal -> launch one eligible `ImmediateIfIdle` or `AgentRunTurnBoundary`
  - AgentLoopTurn boundary -> consume all eligible `AgentLoopTurnBoundary + drain_mode=All`
  - before-stop trigger -> consume all eligible steering plus one `AgentRunTurnBoundary + LaunchOrContinueTurn + drain_mode=One`
  - terminal completed trigger -> consume one `AgentRunTurnBoundary + LaunchOrContinueTurn + drain_mode=One` only as fallback
  - failed/interrupted -> pause state
- 将 delivery 适配到现有服务：
  - launch -> `AgentRunMessageService` / `SessionLaunchService`，但控制语义必须可映射到 Codex `turn/start`
  - steer -> `AgentRunSteeringService` / `SessionControlService`，但控制语义必须可映射到 Codex `turn/steer(expected_turn_id)`
  - system resume -> 对应 `LaunchCommand` source
- 为 command receipt / mailbox result 记录 `expected_active_agent_run_turn_id`、observed/accepted turn id、protocol turn id 和 typed rejection/deferral reason。
- 确保 user-origin payload 在成功消费后清理。
- 确保 scheduler claim 支持按 trigger 选择 drain budget：AgentLoopTurn `all`，AgentRunTurn 边界 `one`。
- 确保 claim 使用 `claim_token/claim_expires_at`，并在 scheduler 启动时恢复 expired `Consuming` envelope。

Validation:

```powershell
cargo test -p agentdash-application mailbox_scheduler
cargo test -p agentdash-api agent_run_mailbox
```

### 5. API Surface Refactor

Goal: public command API 表达 mailbox，而不是 pending queue 特例。

Likely files:

- `crates/agentdash-api/src/routes/lifecycle_agents.rs`
- `crates/agentdash-contracts/src/workflow.rs`
- `packages/app-web/src/services/lifecycle.ts`
- generated TypeScript contracts

Steps:

- `composer-submit` 变为：
  - claim command receipt
  - create mailbox envelope
  - call scheduler
  - return `AgentRunMessageCommandResponse`
- 将 pending endpoints 改为 mailbox endpoints：
  - list mailbox messages
  - delete mailbox message
  - promote mailbox message
  - resume mailbox
- response 包含：
  - command receipt
  - scheduler outcome
  - mailbox message view
  - accepted refs
  - runtime state
- 预研期不保留 pending endpoint 兼容 alias。

Validation:

```powershell
cargo check -p agentdash-api
cargo test -p agentdash-api lifecycle_agents
```

### 6. Stop/Terminal And System Message Integration

Goal: before-stop、terminal callback 和系统 steering 都成为 scheduler trigger 或 internal mailbox intake。

Likely files:

- `crates/agentdash-api/src/agent_run_pending.rs`
- `crates/agentdash-api/src/bootstrap/session.rs`
- `crates/agentdash-application/src/session/terminal_effects.rs`
- `crates/agentdash-application/src/session/hub/hook_dispatch.rs`
- `crates/agentdash-application/src/session/launch/ingestion.rs`
- agent loop event adapter / connector ingestion 相关文件

Steps:

- 将 before-stop hook/loop boundary 接入 `schedule(trigger=agent_run_turn_boundary)`，在 active loop 尚可继续时消费 steering 和一条 turn 消息。
- 将 completed terminal callback 改为 `schedule(trigger=agent_run_turn_boundary)`，作为 before-stop 未消费时的 fallback。
- failed/interrupted terminal 写 mailbox pause state。
- 将 agent loop 内部 `TurnEnd` 事件接入 `schedule(trigger=agent_loop_turn_boundary)`，使 steer/hook AgentLoopTurn-boundary 消息在下一次 assistant response 前被批量消费。
- after-turn / before-stop 产出的 steering 在可解析 AgentRun anchor 时写 hook-origin envelope；现有 `follow_up` 输出归一为 `SteerActiveTurn { stop_effect=ContinueOnStop }`。
- before-stop 调度时除了 hook/user steering，还要消费一条 eligible turn pending message，并以 steering continuation 继续当前 loop。
- hook auto-resume 在可解析 AgentRun anchor 时写 system-origin envelope，并用 terminal effect id / event seq 作为 `source_dedup_key`。
- 保持非 AgentRun-owned runtime 的 direct auto-resume 路径，除非能可靠映射到 AgentRun mailbox。
- 对齐现有 `QueueMode::All` / `OneAtATime`：mailbox AgentLoopTurn-boundary 消息默认 drain all，普通 AgentRunTurn pending 默认 drain one。

Validation:

```powershell
cargo test -p agentdash-application hook_auto_resume
cargo test -p agentdash-api agent_run_mailbox
```

### 7. Frontend Projection

Goal: UI 只消费 mailbox projection，不再理解 send-next/enqueue/steer 分支。

Likely files:

- `packages/app-web/src/services/lifecycle.ts`
- `packages/app-web/src/features/session/ui/SessionChatView.tsx`
- `packages/app-web/src/features/session/ui/composer/PendingMessageRow.tsx`
- `packages/app-web/src/pages/AgentRunWorkspacePage.tsx`
- related tests

Steps:

- 更新 service 方法和返回类型。
- 将 pending row 重命名或泛化为 mailbox message row。
- 根据 `MailboxMessageView.status/barrier/delivery` 展示标签和动作。
- composer submit 使用 backend outcome 刷新 workspace。
- promote/delete/resume 都处理 command receipt 和 duplicate response。

Validation:

```powershell
pnpm --filter app-web typecheck
pnpm --filter app-web test -- lifecycle
pnpm --filter app-web test -- SessionChatView
```

### 8. Final Check

Steps:

- 逐条执行 `current-state.md` 的 cut-line grep commands，并解释所有剩余命中。
- 检查没有进程内 pending queue 作为权威状态。
- 检查所有用户可见 command 都 claim receipt。
- 检查 mailbox scheduler 是唯一判断 runtime state -> 消费结果的地方。
- 检查 runtime control delivery 优先映射到 Codex app-server protocol 的 `thread`/`turn` 原语；所有有限偏移都必须在 backend envelope/domain enum 中显式表达，并有 typed adapter 与测试。
- 检查没有新增 route-local 或 connector-private 的平行 lifecycle protocol。
- 检查 steer/cancel 类 command 都有 active `AgentRunTurn` precondition，并记录 expected/accepted protocol turn id。
- 检查 before-stop 会先批量消费 steering，再消费一条普通 pending turn message 并继续当前 loop。
- 检查 completed terminal 只作为 fallback 自动消费一条普通 pending launch message。
- 检查 internal turn boundary 会批量消费所有 eligible steer/hook message。
- 检查 failed/interrupted pause 不阻止新用户消息 resume 新 AgentRunTurn。
- 检查 expired `Consuming` envelope 会在 recovery path 中恢复为 queued/blocked/terminal result。
- 检查 hook terminal effect replay 不会重复创建 mailbox envelope。
- 检查 user-origin payload cleanup。

Suggested final commands:

```powershell
cargo check -p agentdash-api
cargo test -p agentdash-application mailbox
cargo test -p agentdash-infrastructure mailbox
pnpm --filter app-web typecheck
```

## Sub-agent Execution Plan

实现开始后建议按以下 sub-agent 切片：

- Backend storage/domain：mailbox records、repository、migration、receipt generalization。
- Backend scheduler/API：`AgentRunMailboxService`、lifecycle routes、terminal callback trigger。
- Frontend projection：generated DTO、service、mailbox row UI、workspace refresh。
- Check：review barrier semantics、duplicate idempotency、payload retention、contract alignment。

Dependencies:

- Scheduler/API 依赖 storage/domain 和 receipt generalization。
- Frontend 依赖 API contract 与 generated types。
- Check 依赖至少 backend compile 和 frontend generated types。

## Risk Points

- `turn` 命名容易继续混淆；代码注释和 DTO 应明确 `agent_run_turn` 与 `agent_loop_turn`。
- Scheduler claim 必须避免并发 terminal/resume/promote 重复消费同一 envelope。
- Scheduler recovery 必须处理 expired claim 和 process restart 后的 `Consuming` envelope。
- 新用户消息在 failed/interrupted 后能 resume，但旧 paused envelope 不能被静默绕过消费。
- before-stop 与 terminal fallback 不能重复消费同一条 `AgentRunTurnBoundary` envelope。
- Codex protocol 偏移必须有明确存在理由、schema 字段、adapter mapping 和 projection 行为；不能只存在于业务分支里。
- Hook delivery message 只有在能映射 AgentRun anchor 时进入 mailbox，否则会错误污染普通 runtime。
- Hook `follow_up` 只能作为 stop-boundary steering continuation 建模，不能重新长成独立 delivery path。
- user-origin payload cleanup 不能早于 launch/steer accepted refs 写入。
- Endpoint 改名会触发前端较多调用点更新，generated contract diff 要单独 review。

## Review Checklist Before Start

- 是否接受 public endpoint 命名从 pending 改为 mailbox？
- 是否接受 `agent_loop_turn_boundary` 默认 drain all，AgentRunTurn `agent_run_turn_boundary` 默认 drain one？
- 是否接受 backend envelope/domain crates 作为控制面事实源，Codex protocol 作为优先对齐的可适配基线而非完整天花板？
- 是否接受 user-origin payload queued 时短期持久、消费成功后清理？
- 是否接受将 `send_next/enqueue/steer` 降级为 scheduler outcome，而不是 command kind？
