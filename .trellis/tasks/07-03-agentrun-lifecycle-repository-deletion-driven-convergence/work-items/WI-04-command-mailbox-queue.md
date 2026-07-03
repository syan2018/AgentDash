# WI-04 Command Mailbox Queue

## Objective

把用户指令、AgentRun queue state、runtime delivery operation 三层事实拆清，并把 Mailbox owner 改为 AgentRun。

## Decisions

D-005, D-006, D-007, D-017

## Research Inputs

- `research/command-mailbox-delivery.md`
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
