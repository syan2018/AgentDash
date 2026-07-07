# Design

## Architecture

RuntimeSession terminal 仍先持久化 Backbone `turn_terminal` event，再经 `RuntimeTerminalBoundary` 提交 terminal evidence。AgentRun control-plane 接收到 evidence 后，只做两件事：

1. 幂等 materialize terminal 后续 control effects。
2. claim 并执行 effect rows。

`AgentRunDeliveryBinding` 继续是用户可见 running/terminal 状态的事实源；`agent_run_control_effects` 只表达 terminal 后续控制副作用的 durable execution progress。LifecycleGate 继续是 wait/review/resume 的事实 owner；gate wait policy 是 gate payload 的 typed declaration。

## Control Effect Outbox

新增或调整 outbox 合同：

- `dedup_key`: stable text key，覆盖 `delivery_runtime_session_id`, `turn_id`, `terminal_event_seq`, `effect_kind` 和必要 discriminator。
- claim fields: `claim_token`, `claim_owner`, `claim_expires_at_ms`。
- store method: `insert_or_get_control_effect`, `claim_control_effects`, `mark_control_effect_succeeded`, `mark_control_effect_failed`, `mark_control_effect_dead_letter`。
- unique index: `agent_run_control_effects(dedup_key)`。

`observe_runtime_terminal` 的顺序固定为：

```text
terminal evidence
-> resolve owner refs from RuntimeSessionExecutionAnchor
-> collect immediate hook outputs only when durable
-> materialize all required effect rows with dedup keys
-> claim/execute rows independently
```

执行失败只更新当前 effect row，不回滚 terminal event，也不阻止其他 rows 执行。

## Gate Wait Policy And Wake

Companion child dispatch 在打开 wait gate 时直接携带 `GateWaitPolicyEnvelope`。不再先创建 gate 再调用 `declare_child_wait_obligation` 补写 payload。

Gate producer terminal fallback 从 terminal effect 消费 `WaitProducerRef`，查询 matching `LifecycleGate`，通过 `LifecycleGateResolver` resolve open gate 或 ensure resolved gate wake。mailbox wake 作为 durable control effect 执行时，payload 使用 `GateMailboxWakeIntent` 的 bounded JSON；重复 wake 依赖 mailbox source identity dedup。

## Hook Effect Durability

Hook effects 分为 durable 和 immediate：

- durable hook effect 必须包含 handler identity，并通过 registry 恢复 handler。
- handler execution 返回 `Result<(), String>`，失败进入 outbox failed/dead-letter。
- 无 durable identity 的 effect 只在 terminal intake 当场执行，不写 durable outbox，并记录 diagnostic。

## Runtime / Relay Terminal Naming

保留 RuntimeSession terminal 主链路，但命名中明确 runtime/delivery terminal。interactive terminal / PTY terminal 使用独立 event/type/payload 名称，前端 projection 不再用裸 `terminal lost` 合并两类事实。

Backend disconnect 可以同时产生两类事实：

- delivery runtime lost: 驱动 RuntimeSession terminal / AgentRun control effect。
- interactive terminal lost: 驱动 terminal resource state projection。

二者在 Backbone payload 和 frontend reducer 中必须保持不同 discriminant。

## Migration Notes

项目未上线，migration 可以直接收正 schema。需要更新 `0053_agent_run_control_effects.sql` 或新增后续 migration，使已有 `agent_run_control_effects` 具备 dedup/claim 字段和索引。若删除 Noop effect kind，需要同步 Rust enum、repository parse、tests 和任何生成类型。
