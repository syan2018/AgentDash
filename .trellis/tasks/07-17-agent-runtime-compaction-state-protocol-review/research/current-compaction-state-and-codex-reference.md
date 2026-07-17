# Agent Runtime 压缩状态与 Codex Reference 对照研究

> 研究状态：本文对当前压缩链与 `references/codex` 的源码证据继续有效；目标状态的 owner 已进一步收敛为 Hosted Agent / AgentSession，而不是独立 Runtime journal。实施以父任务 `design.md` 为准。

## 1. 结论摘要

当前实现已经具备 candidate/checkpoint/activation/head 的 durable saga 骨架，但没有把“压缩正在占用会话执行权”建模成 canonical conversation lifecycle。压缩 command 被接受后只创建 operation 与后台 work，Thread 仍保持 `Active`、`active_turn_id` 仍为空，也不产生 `contextCompaction` item 的 started/completed 事件。因此 command availability、API outcome、前端状态和 worker 实际执行之间没有共同事实源。

这不仅是展示缺失，还造成真实并发与恢复问题：

1. compaction accepted 后、worker prepare 前，普通 `TurnStart` 仍可被接受；
2. prepare 发现 active turn 后只返回错误并重试，无法表达“已排队”与“正在压缩”的区别；
3. preparation/activation 的普通失败没有 terminalization 入口，durable work 可无限 release/reclaim；
4. managed compaction 与 Native replay 对 typed thread item 的支持集合不一致，直到 activation side effect 边界才失败；
5. API 返回 `launched_compaction_turn`，但 Runtime 实际没有创建任何 Turn；
6. 成功只发布 ContextFrame summary，未发布 Codex App Server Protocol 的 canonical `contextCompaction` item lifecycle。

目标设计应保留现有 durable saga 的正确部分，但把它纳入 canonical maintenance turn/activity，并在同一 Runtime transaction 中生成 conversation lifecycle 与 Codex-shaped presentation。

## 2. 当前 AgentDash 状态模型

### 2.1 Thread status 只表达生命周期/健康状态

`RuntimeThreadStatus` 当前只有：

```text
Active | Suspended | Desynchronized | Closed | Lost
```

证据：

- `crates/agentdash-agent-runtime-contract/src/event.rs:35`

这里的 `Active` 表示 Thread 可执行，不等价于 UI 意义上的 idle/running。实际占用状态另由 `active_turn_id` 表达。因此直接把 `Compacting` 塞进 `RuntimeThreadStatus` 会把长期生命周期/健康状态与短期活动类型混在同一枚举中。

### 2.2 ContextCompact acceptance 不创建 Turn 或 Item

`ContextCompact` acceptance 会：

1. 冻结 `source_end_event_sequence`；
2. 创建 `OperationAccepted`；
3. 创建 `ContextPreparationWorkItem::Pending`；
4. 不创建普通 runtime outbox。

但 `apply_command_projection` 对 `RuntimeCommand::ContextCompact` 是空分支，不产生 `TurnStarted`、`ItemStarted` 或 thread activity 变化。

证据：

- `crates/agentdash-agent-runtime/src/gateway.rs:1487`
- `crates/agentdash-agent-runtime/src/gateway.rs:1542`
- `crates/agentdash-agent-runtime/src/gateway.rs:2159`

### 2.3 command availability 看不到 pending compaction

`AvailabilityState` 只有：

```text
thread_status
has_active_turn
has_pending_interaction
```

`TurnStart` 只在 `has_active_turn=true` 时不可用；pending/prepared compaction 不属于 availability 输入。

证据：

- `crates/agentdash-agent-runtime-contract/src/availability.rs:20`
- `crates/agentdash-agent-runtime-contract/src/availability.rs:71`

因此 compaction operation 已 accepted、但尚未开始 prepare 时，普通 `TurnStart` 仍可能通过。数据库 partial unique index只阻止同一 Thread 的第二个 nonterminal compaction，不能阻止普通 Turn。

证据：

- `crates/agentdash-infrastructure/migrations/0061_agent_runtime_managed_state.sql:180`

### 2.4 API 声称启动了 compaction turn，但该 Turn 不存在

API 仅根据 snapshot 当时是否存在 active turn，把 accepted compaction 返回为：

```text
scheduled_next_turn | launched_compaction_turn
```

它没有检查 canonical compaction lifecycle；`launched_compaction_turn` 只是同步请求时的推断。

证据：

- `crates/agentdash-api/src/routes/lifecycle_agents.rs:2101`

这违反“前端/API 不从间接信号推断 Runtime 状态”的项目跨层约束。

### 2.5 prepare 阶段把 active turn 当成错误，不是合法排队状态

`prepare_compaction` 要求：

```text
thread.active_turn_id == None
thread.status == Active
```

否则返回 `OperationNotActive`。worker 把普通错误向上返回，durable work 随后 release/reclaim。

证据：

- `crates/agentdash-agent-runtime/src/context.rs:375`
- `crates/agentdash-infrastructure/src/agent_runtime_workers.rs:936`

当前“在 active turn 中 schedule compaction”实际上依赖 worker 反复尝试，直到 active turn 恰好结束；没有 durable `Queued → Running` 调度迁移。

### 2.6 普通失败没有 terminalization

现有 public transition 主要是：

- `complete_compaction_without_changes`
- `prepare_compaction`
- `confirm_compaction_activation`
- `finalize_compaction`
- `recover_compaction`
- `desynchronize_compaction`

`CompactionTerminal::Failed` 虽然存在，但生产代码没有构造它。prepare engine error、typed projection error、driver Unsupported/Rejected 等 pre-apply 失败只会让 worker release；operation、preparation slot 与 UI 均不进入失败终态。

证据：

- `crates/agentdash-agent-runtime/src/context.rs:57`
- `crates/agentdash-agent-runtime/src/context.rs:198`
- `crates/agentdash-agent-runtime/src/context.rs:459`
- `crates/agentdash-agent-runtime/src/context.rs:541`
- `crates/agentdash-agent-runtime/src/context.rs:656`
- `crates/agentdash-agent-runtime/src/context.rs:815`

当前日志里的 unsupported typed thread item 因此会反复出现，而不是形成一次可观察的压缩失败。

## 3. typed thread item replay 缺陷

### 3.1 managed compaction preparation 的支持集合

infrastructure 的 `tool_compaction_messages` 已能把以下 item 转成 provider-neutral tool-call/result pair：

- Codex `CommandExecution`
- Codex `McpToolCall`
- Codex `DynamicToolCall`
- 全部 `AgentDashNativeThreadItem`

证据：

- `crates/agentdash-infrastructure/src/agent_runtime_workers.rs:701`
- `crates/agentdash-infrastructure/src/agent_runtime_workers.rs:781`

### 3.2 Native activation replay 的支持集合更窄

Native `context_block_to_message` 只支持：

- `UserMessage`
- `AgentMessage`
- `Reasoning`
- `DynamicToolCall`

其他 Codex item 与全部 AgentDash native item 都返回：

```text
native context replay encountered an unsupported typed thread item
```

证据：

- `crates/agentdash-integration-native-agent/src/mapping.rs:76`
- `crates/agentdash-integration-native-agent/src/mapping.rs:148`

这形成同一 candidate 在 preparation 阶段可接受、activation 阶段不可消费的协议集合不一致。

### 3.3 推荐修复边界

不应只在 Native mapper 里继续堆 variant match，因为 preparation 与 activation 会再次形成两份集合。目标应建立一个共享的、可测试的“canonical context replay projection”：

```text
typed RuntimeItem
  -> provider-neutral AgentMessage sequence
  -> MaterializedContext replay blocks
  -> Native exact replace_messages
```

推荐让 `MaterializedContext` 持久化已经通过 profile/capability 校验的 provider-neutral replay message，而不是让 driver 在 activation side effect 边界重新解释任意 presentation `ThreadItem`。原始 typed item 与 source item IDs 继续保留在 transcript/journal/recipe 作为审计事实。

这样能够：

- 一个工具 item 明确展开成 assistant tool-call + tool-result 两条消息；
- digest 覆盖 driver 真正应用的 message sequence；
- preparation 时即可拒绝无法 lossless 投影的 item；
- Native activation 只验证 digest/revision 并执行 exact replacement；
- cold rebind 与 managed activation 复用同一 replay projection。

若最终仍保留 `ContextBlock::RuntimeItem` 作为 driver 输入，则必须把 projection 函数提升到 adapter 可复用的共享边界，并建立全部 `AgentDashThreadItem` variant 的 exhaustiveness/parity test；不能保留 infrastructure 与 Native 各自维护的 match。

## 4. `references/codex` 的压缩状态设计

### 4.1 压缩是非 steerable active task

Codex Core 把手动压缩作为 `CompactTask`，其 `TaskKind` 为 `Compact`。active task 为 Compact 时，steer 明确返回 `ActiveTurnNotSteerable { Compact }`。

证据：

- `references/codex/codex-rs/core/src/tasks/compact.rs:16`
- `references/codex/codex-rs/core/src/state/turn.rs:69`
- `references/codex/codex-rs/core/src/session/mod.rs:3867`

Codex 的 Thread status 仍是 `Active`，没有单独的 `Compacting` Thread lifecycle enum；具体活动类型由 active turn/task 与 item 表达。

证据：

- `references/codex/codex-rs/app-server-protocol/src/protocol/v2/thread.rs:1279`

这支持 AgentDash 采用“Thread lifecycle 保持 Active + active maintenance turn kind=ContextCompaction + 对外派生 activity=compacting”，而不是把 transient activity混入 `RuntimeThreadStatus`。

### 4.2 canonical presentation 是 ContextCompaction item lifecycle

local、remote 与 token-budget compact 都会：

1. 创建 `ContextCompactionItem::new()`；
2. 发 `emit_turn_item_started`；
3. 完成 history replacement/new context window；
4. 用同一个 item ID 发 `emit_turn_item_completed`。

证据：

- `references/codex/codex-rs/core/src/compact.rs:228`
- `references/codex/codex-rs/core/src/compact.rs:371`
- `references/codex/codex-rs/core/src/compact_remote.rs:194`
- `references/codex/codex-rs/core/src/compact_remote.rs:300`
- `references/codex/codex-rs/core/src/compact_remote_v2.rs:207`
- `references/codex/codex-rs/core/src/compact_remote_v2.rs:322`
- `references/codex/codex-rs/core/src/compact_token_budget.rs:76`

App Server E2E 明确断言自动与手动压缩都产生：

```text
item/started  { item: { type: "contextCompaction", id } }
item/completed { item: { type: "contextCompaction", same id } }
```

证据：

- `references/codex/codex-rs/app-server/tests/suite/v2/compaction.rs:46`
- `references/codex/codex-rs/app-server/tests/suite/v2/compaction.rs:98`
- `references/codex/codex-rs/app-server/tests/suite/v2/compaction.rs:245`

### 4.3 `thread/compacted` 已废弃

Codex 协议明确标注 `thread/compacted` deprecated，v2 client 应消费 canonical `ContextCompaction` item。App Server 对 Core 的 legacy `ContextCompacted` event 不再向 v2 发 notification。

证据：

- `references/codex/codex-rs/app-server-protocol/src/protocol/v2/thread.rs:1547`
- `references/codex/codex-rs/app-server/src/bespoke_event_handling.rs:883`

AgentDash 不应把 `ExecutorContextCompacted` 或 `SessionMetaUpdate(key="context_compacted")` 当成 managed lifecycle 的替代品。前者只适合 opaque executor telemetry；后者可以保留 summary/archive 投影，但不能承担 started/completed 状态。

### 4.4 失败语义

Codex 只有在 history replacement 成功后才发 `item/completed`。失败路径发 `Error`，随后结束 compact turn；不会伪造成功的 ContextCompaction completed item。

证据：

- `references/codex/codex-rs/core/tests/suite/compact_remote.rs:2350`
- `references/codex/codex-rs/core/tests/suite/compact.rs:3711`

AgentDash 内部仍应把 Runtime Item terminalize 为 typed Failed/Lost，以保证 canonical state 收敛；对外 Codex-shaped presentation则应遵循：

```text
success: item/started -> item/completed -> turn/completed(completed)
failure: item/started -> error -> turn/completed(failed/lost)
```

前端必须用 event lifecycle 或 canonical Runtime item phase，而不是把所有 `contextCompaction` item 固定解释为 completed。

## 5. 前端与 projection 现状

前端已经注册 `ContextCompactionCardBody`，但 `getThreadItemStatus` 对任何 `contextCompaction` 都固定返回 `"completed"`。即使未来发出 `item/started`，当前 UI 仍无法显示真正的进行中状态。

证据：

- `packages/app-web/src/features/session/ui/bodies/ContextCompactionCardBody.tsx:7`
- `packages/app-web/src/features/session/model/types.ts:587`

当前 projection refresh 同时识别：

- `SessionMetaUpdate(key="context_compacted")`
- compaction summary ContextFrame
- turn lifecycle

managed Runtime 成功路径目前主要发布 compaction summary ContextFrame，而旧 Native stream mapper仍能发布 `context_compacted` meta + `ItemCompleted`。这两条路径没有共同 started 状态，且 ownership 不清晰。

证据：

- `crates/agentdash-agent-runtime/src/context.rs:757`
- `crates/agentdash-integration-native-agent/src/presentation.rs:1721`
- `crates/agentdash-integration-native-agent/src/presentation.rs:1790`
- `packages/app-web/src/features/session/ui/SessionChatViewModel.ts:266`

## 6. 推荐目标状态模型

### 6.1 两层状态，不混淆 Thread lifecycle 与当前 activity

```text
RuntimeThreadStatus
  Active | Suspended | Desynchronized | Closed | Lost

RuntimeActivity / active turn
  Idle
  AgentTurn(turn_id)
  ContextCompaction {
      turn_id,
      item_id,
      compaction_id,
      phase
  }
```

建议给 canonical Turn 增加 `RuntimeTurnKind::{Agent, ContextCompaction}`，并让 snapshot 提供 typed active turn/activity view。产品层可把它稳定显示为“正在压缩”，但 command admission 直接读取 canonical kind/phase，不读取 UI 派生字符串。

### 6.2 Compaction aggregate phase

建议收敛为一个权威 phase，而不是让 preparation status、activation status 与 operation terminal分别被消费者猜测：

```text
Queued
  -> Preparing
  -> CandidatePrepared
  -> Activating
  -> Applied
  -> Succeeded

Queued|Preparing|CandidatePrepared
  -> Failed | Cancelled

Activating|Applied
  -> Succeeded | Lost
```

持久表仍可拆 preparation/candidate/activation/head，但 `ContextCompactionView.phase` 必须由 Runtime 在 transition 中明确写入/投影，并能从单一 aggregate snapshot恢复。

### 6.3 conversation 与协议事件顺序

真正开始占用会话执行权时，同一 transaction：

```text
CompactionStarted
TurnStarted(kind=ContextCompaction)
ItemStarted(content=contextCompaction{id})
```

成功 commit 同一 transaction：

```text
ContextCheckpointActivated
ContextCompactionSucceeded
ItemTerminal(Completed)
TurnTerminal(Completed)
OperationTerminal(Succeeded)
ContextFrameChanged(compaction summary)
presentation item/completed
presentation turn/completed
```

pre-apply 失败同一 transaction：

```text
ContextCompactionFailed
ItemTerminal(Failed)
TurnTerminal(Failed)
OperationTerminal(Failed)
presentation error
presentation turn/completed(failed)
```

post-apply 不可验证：

```text
ContextCompactionLost
ItemTerminal(Lost)
TurnTerminal(Lost)
ThreadStatusChanged(Desynchronized)
OperationTerminal(Lost)
```

### 6.4 command admission

- queued compaction 尚未取得执行权时，当前 agent turn可继续到 terminal；
- compaction 取得执行权后，Runtime `TurnStart` 与 `TurnSteer` 不可执行；
- 新用户消息可由 AgentRun mailbox 接受为 deferred，但不能提前创建 Runtime Turn；
- compaction terminal 后，只有 Succeeded/clean Failed 才能启动 deferred message；
- post-apply Lost/Desynchronized 必须阻止 deferred message启动；
- cancel 只允许发生在 driver activation side effect 之前；进入 Activating 后必须完成恢复判定，不能假装取消。

## 7. 实施影响范围

预计涉及：

- `agentdash-agent-runtime-contract`
  - turn kind/activity/compaction phase/snapshot/event/availability
  - context replay block contract
- `agentdash-agent-runtime`
  - compaction start/fail/cancel/finalize transition
  - canonical Turn/Item lifecycle与 presentation
- `agentdash-infrastructure`
  - worker settlement、attempt policy、terminalization
  - PostgreSQL schema/migration/repository
  - context materialization/replay projection
- `agentdash-integration-native-agent`
  - exact replay只消费已验证的 canonical message blocks
  - activation idempotency与 inspect
- `agentdash-integration-codex`
  - native opaque compaction继续保持 observed，不冒充 managed head
  - v2 `ContextCompaction` item 与 deprecated notification的边界审计
- `agentdash-application-agentrun` / API
  - mailbox deferred admission
  - 删除由 snapshot 瞬时信号推断 `launched_compaction_turn`
- `agentdash-agent-protocol`
  - 复用 pinned Codex-shaped `contextCompaction` item lifecycle
- `packages/app-web`
  - active compaction状态、card lifecycle、失败与 replay恢复
- migration
  - 为 turn kind/compaction phase等新 durable facts追加 migration；项目未上线，不保留旧错误模型兼容路径。

## 8. 需要产品确认的边界

压缩运行期间收到新用户消息时，推荐由 AgentRun mailbox durable 接受并标记 deferred，在 compaction terminal 后基于新 context head启动；Runtime 本身在 compaction active期间拒绝创建普通 Turn。另一种选择是直接拒绝用户提交，但会损失 mailbox 已具备的排队能力和更平滑的交互体验。
