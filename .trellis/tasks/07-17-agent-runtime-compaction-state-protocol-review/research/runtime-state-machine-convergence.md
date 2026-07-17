# Agent Runtime 状态机收敛研究

> 研究状态：本文的现状 inventory、代码位置与状态重复分析继续有效；目标状态结论以父
> `design.md` 为准：统一 Managed Runtime 保留为所有 Agent 的平台外层，完整 Agent
> 拥有自身 history/lifecycle，只有 history-maintained state 可以使用 Session 命名。

## 结论

本次应趁压缩缺陷暴露整个链路的机会，真正收敛 Agent Runtime 状态模型。这里的“收敛”不是把所有状态压进一个巨型枚举，而是：

1. 明确少量正交 state machine 的 aggregate ownership；
2. 让 Runtime domain journal 成为业务事实源；
3. 用一个深的 Runtime transition module 隐藏准入、事件排序、terminalization、幂等和恢复复杂度；
4. 让 Host、worker、driver、AgentRun、protocol 和 UI 分别只承担 adapter、delivery 或 projection 职责；
5. 删除从 `main` 搬运后仍保留的重复状态、旁路推断与 cutover 残留。

用户已明确允许破坏性重写本分支中的相关实现。设计无需保留旧内部 interface、schema、事件或投影路径的兼容行为；数据库仍通过 forward migration 达到唯一最终模型。

## 当前状态空间盘点

| 状态/模型 | 当前所有者 | 当前用途 | 主要问题 |
| --- | --- | --- | --- |
| `RuntimeThreadStatus` | `agentdash-agent-runtime-contract` / Runtime journal | Thread 生命周期与健康：Active、Suspended、Desynchronized、Closed、Lost | 基本维度合理，但消费者容易把 `Active` 误当成 idle/running |
| `active_turn_id` + `RuntimeTurnState.phase` | Runtime thread aggregate | 独占 Turn 与 active/terminal | 没有 `RuntimeTurnKind`，无法表达 Agent 与 ContextCompaction 的 activity 差异 |
| `RuntimeItemState` / `RuntimeInteractionState` | Runtime thread aggregate | Turn 子实体 lifecycle | 结构合理，但 compaction 没进入这条 canonical lifecycle |
| `RuntimeOperationRecord` / terminal | Runtime thread aggregate | command idempotency 与业务 terminal | compaction 普通失败无法稳定 terminalize |
| `ContextPreparationStatus` | Runtime context implementation / DB | Pending、Prepared、Terminal | 与 activation/operation 分裂，消费者无法直接得到 compaction aggregate phase |
| `ContextActivationStatus` | Runtime context implementation / DB | Prepared、Applied、Terminal | 与 preparation/head/operation 共同描述一个 saga，但没有一个 authoritative aggregate view |
| `CompactionTerminal` | Runtime context implementation | Succeeded、Failed、Lost | `Failed` 缺少生产 transition，普通错误落入 worker retry |
| `RuntimeBindingState` | Runtime Host aggregate | binding/generation 健康与 lifecycle | 与 Thread health 语义部分重叠，但两者的合法组合与驱动方向没有集中契约 |
| `AgentRunExecutionState` | application-agentrun | Idle、Running、Cancelling、Completed/Failed/Interrupted/Lost | 把 Runtime execution 再建模一次，容易与 canonical snapshot 漂移 |
| `ConversationExecutionStatusModel` | application-agentrun workspace | Draft、Ready、StartingClaimed、RunningActive、Cancelling、Terminal 等产品 read model | 产品前置条件与 Runtime activity 混合，需改成单点投影而非另一事实源 |
| Codex `TurnStatus` / ContextCompaction item | protocol/presentation | 外部协议 read model | 应由 Runtime journal确定性投影，不能反向承担状态 |
| `CompactionPhase` | agent-types decision metadata | PreProvider、StandaloneCompactTurn、OverflowRetry | 表达执行位置而非 lifecycle phase，名称会与 saga phase冲突 |
| `SessionCompactionStatus` | legacy SPI export | 旧 session projection compaction | 对应 runtime session compaction tables 已在 migration 0065 删除，疑似 cutover 残留 |
| durable work claim/release | infrastructure worker | 投递与重试 | 当前被迫替代业务 Queued/Failed 状态，导致无限重领与不可观察 |

代码证据：

- `crates/agentdash-agent-runtime-contract/src/event.rs:35`
- `crates/agentdash-agent-runtime/src/model.rs:21`
- `crates/agentdash-agent-runtime/src/model.rs:69`
- `crates/agentdash-agent-runtime/src/context.rs:59`
- `crates/agentdash-agent-runtime/src/context.rs:79`
- `crates/agentdash-agent-runtime/src/context.rs:116`
- `crates/agentdash-agent-runtime-host/src/model.rs:132`
- `crates/agentdash-application-agentrun/src/agent_run/execution_state.rs:2`
- `crates/agentdash-application-agentrun/src/agent_run/conversation_snapshot.rs:50`
- `crates/agentdash-agent-types/src/runtime/decisions.rs:200`
- `crates/agentdash-spi/src/session_persistence.rs:465`
- `crates/agentdash-infrastructure/migrations/0065_agent_runtime_cutover.sql:60`

## 推荐的 canonical 组合

```text
RuntimeThreadAggregate
├─ lifecycle/health: Active | Suspended | Desynchronized | Closed | Lost
├─ active_activity: None | Turn(turn_id)
├─ turns[turn_id]
│  ├─ kind: Agent | ContextCompaction
│  ├─ phase: Active | Terminal
│  ├─ items
│  └─ interactions
├─ operations[operation_id]
├─ context
│  ├─ active_head
│  └─ compactions[compaction_id]
└─ binding_coordinate: binding_id + epoch + driver_generation

RuntimeHostBindingAggregate
└─ binding lifecycle/health + lease/driver observation

Infrastructure delivery
└─ outbox/work claim/attempt/settlement（不是业务状态）

Read models
├─ AgentRun workspace execution
├─ Codex App Server Protocol
└─ Web UI
```

`RuntimeThreadAggregate` 与 `RuntimeHostBindingAggregate` 不合并。它们通过稳定 coordinate、generation fence 和少量明确 transition 对齐：Host 提供外部事实观察，Runtime 决定业务 terminal 与 Thread health；driver/worker 不直接发明 Runtime 状态。

## 深模块目标

Runtime 应形成一个小 interface、深 implementation 的 transition module。调用者只需提交 command 或外部 effect observation，并获得 durable commit/settlement；调用者不需要了解“先改 preparation、再改 activation、再终结 operation、再拼 presentation”的顺序。

目标 interface 应覆盖三类意图：

```text
execute(command envelope) -> durable RuntimeCommit
settle(effect observation) -> durable RuntimeCommit + delivery settlement
snapshot/query -> canonical state + derived availability
```

具体 context preparation、activation inspect、head CAS、Turn/Item terminal 和 presentation 顺序属于 implementation。Postgres 与 in-memory repository 是该 module 的本地可替换 adapter；Native/Codex driver 是 effect seam 的真实 adapter。

## 核心组合不变量

1. 一个 Thread 同时最多只有一个 active Turn；active activity 由该 Turn 的 kind 定义。
2. active ContextCompaction Turn 不可 steer；普通 TurnStart 不可越过它。
3. Turn terminal 前不得存在 active Item 或 Interaction。
4. 每个 accepted Operation 必须且只能到达一个 terminal。
5. compaction phase、Turn/Item phase、Operation terminal 与 active context head 必须在 Runtime transition 中成组收敛，不能由 worker 猜测。
6. driver side effect 前必须验证 binding/generation、expected context revision、candidate digest 与 replay capability。
7. side effect 后不可验证时只能进入 Lost/Desynchronized，不能回退为旧 head 可继续。
8. worker 重投只能重放同一稳定 identity 的事实，不得复制 Turn、Item、activation 或 protocol lifecycle。
9. AgentRun、协议和 UI 不持有业务状态，只消费 canonical snapshot/journal projection。
10. mailbox 可在 maintenance Turn active 时 durable 接受消息，但只有 Runtime 给出可继续判定后才能 promote。

## 推荐实施形态

不建议一次性把所有 crate 同时推倒后再集成。应使用一个父级架构任务统领最终模型，再按依赖明确的垂直切片替换：

1. canonical state kernel：Turn kind、active activity、availability、transition/terminal invariants、schema；
2. compaction tracer bullet：queued maintenance Turn、context saga、failure settlement、mailbox；
3. replay contract：provider-neutral materialized messages、Native activation 与 cold rebind parity；
4. Codex-shaped presentation：item/started、item/completed、error、turn terminal 与前端 lifecycle；
5. Host/recovery 收敛：binding/thread health 组合、generation/restart recovery；
6. 删除旧 read-model 推断、legacy SPI/status 与 cutover 残留。

每个切片完成后都必须能从 Runtime interface 做端到端断言；不保留旧链路与新链路并行运行。
