# WI-04 Command Mailbox Queue

## Objective

把用户指令、AgentRun queue state、runtime delivery operation 三层事实拆清，并把 Mailbox owner 改为 AgentRun。

## Decisions

D-005, D-006, D-007, D-017

## Research Inputs

- `research/command-mailbox-delivery.md`
- `research/wi-04-command-mailbox-current-state.md`
- `research/aggregate-ownership.md`
- `research/database-physical-design.md`

## Scope

- mailbox message/state owner 收敛为 `run_id + agent_id + message_id`。
- 删除 mailbox 对 RuntimeSession 的 ownership / cascade 语义。
- 将 `runtime_session_id` 移为 nullable delivery ref、accepted ref 或 delivery attempt 关联。
- 收敛 `CommandReceipt` 与 mailbox 的关系：receipt 负责幂等和 outcome ref，mailbox 负责 queue state。
- 定义 `RuntimeDeliveryOperation` / `DeliveryAttempt`，表达一次 queue item 投递到 RuntimeSession 的执行尝试。
- move、promote、delete、resume、reorder 等用户可见 queue 操作进入 command receipt / stale guard。

## Out Of Scope

- tool approval 是否升格为产品事实由 WI-09 最终确认。
- current delivery selection 交给 WI-06。
- accepted commit 原子化交给 WI-05。

## Dependencies

依赖 WI-00 的 mailbox/receipt/runtime command 使用点清单。实施需对齐 WI-03 admission 产出的 initial mailbox envelope。

## Implementation Notes

- mailbox 领域上是 AgentRun child fact，但物理上大概率保留 child table，因为 queue claim、排序、recover、扫描都需要索引和锁。
- receipt -> mailbox result ref 是主方向；message -> receipt 若存在，只作为 nullable correlation。
- duplicate replay 读取 receipt outcome，不从 mailbox 反向推导外部命令结果。

## Acceptance

- 删除 RuntimeSession 不会删除未完成 mailbox item。
- 一条用户输入能画成 receipt -> queue item -> delivery attempt 的单线状态机。
- 所有状态字段都能归属到 instruction、queue item 或 delivery attempt。
- mailbox repository 如果仍保留物理实现，对外命名为 AgentRun queue capability，而不是同级聚合仓储。

## Validation

- queue claim/recover/order 单元测试覆盖 runtime session 轮换。
- idempotency duplicate replay 测试只依赖 command receipt。
- migration 验证 mailbox FK/cascade 已从 session ownership 改为 AgentRun ownership。

## Acceptance Record 2026-07-04 / Worker B2

### Ownership Convergence

- Mailbox durable owner is now represented in code and SQL as the AgentRun queue child: `run_id + agent_id + message_id`.
- `AgentRunMailboxMessage`, `NewAgentRunMailboxMessage`, `AgentRunMailboxState`, and `AgentRunMailboxClaimRequest` use `delivery_runtime_session_id` for the nullable RuntimeSession trace reference. This ref records the delivery target used by claim/intake paths and is not an ownership, permission, or cascade boundary.
- `agent_run_mailbox_messages` and `agent_run_mailbox_states` keep AgentRun owner FKs/cascade and migration `0047_agent_run_mailbox_delivery_runtime_ref.sql` renames the old RuntimeSession-shaped column to `delivery_runtime_session_id` with `ON DELETE SET NULL`.
- Mailbox claim remains owner-scoped by `run_id + agent_id`; RuntimeSession rotation does not participate in the claim predicate and only updates the nullable delivery trace ref on the claimed rows.

### Fact Split

- User instruction fact: `AgentRunCommandReceipt` owns idempotency, request digest, accepted refs, and command outcome replay.
- Queue item fact: `agent_run_mailbox_messages` owns ordering, claim lease, recovery status, payload retention, and queue lifecycle for the AgentRun agent.
- Delivery attempt evidence remains embedded in the queue row through `delivery_runtime_session_id`, `claim_token`, lease timestamps, `attempt_count`, and accepted turn refs. A dedicated DeliveryAttempt table was not added because this slice did not complete an atomic split of lease, attempt, and accepted refs.

### Validation

- `cargo test -p agentdash-domain mailbox_move` passed for command receipt kind roundtrip.
- `cargo test -p agentdash-infrastructure agent_run_mailbox` passed, including nullable delivery ref claim and RuntimeSession delete set-null coverage.
- `cargo test -p agentdash-application-agentrun mailbox` passed, including target stale guard, initial mailbox envelope, and fork-submit child mailbox coverage.
- `cargo test -p agentdash-application mailbox`, `cargo test -p agentdash-application routine`, and `cargo test -p agentdash-application wait_activity` passed for application consumers of mailbox refs.
- `cargo check -p agentdash-domain -p agentdash-infrastructure -p agentdash-application -p agentdash-application-agentrun -p agentdash-api`, `cargo fmt --check`, and `pnpm run migration:guard` passed.
